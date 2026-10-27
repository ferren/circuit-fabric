//! Presentation opener: `.pptx` slide text via ZIP + XML parsing.
//!
//! Only the `ppt/slides/slideN.xml` parts are read, in numeric slide order: each
//! slide's paragraphs become one page of a paged-text view. Speaker notes, embedded
//! media, macros (`ppt/vbaProject.bin`), and every external relationship are never
//! opened, so no active content can execute. Legacy binary `PowerPoint` `.ppt` files are
//! explicitly unsupported; the capability list reports only `.pptx`.

use std::io::Read;

use circuitfabric_plugin_api::{
    DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest, DocumentPage, DocumentViewBody,
    MAX_DOCUMENT_VIEW_PAGES, OpenerCapability,
};
use roxmltree::Node;

use crate::{capped_text, view};

/// Opens `PowerPoint` managed copies as a paged, read-only text view.
#[derive(Clone, Copy, Debug, Default)]
pub struct PresentationOpener;

/// The OOXML `DrawingML` namespace, matched leniently on the tail.
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

impl DocumentOpener for PresentationOpener {
    fn capability(&self) -> OpenerCapability {
        OpenerCapability {
            opener_id: "builtin-presentation".to_owned(),
            label: "Presentation".to_owned(),
            extensions: vec!["pptx".to_owned()],
            mime_types: vec![
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                    .to_owned(),
            ],
        }
    }

    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let mut archive =
            match zip::ZipArchive::new(std::io::Cursor::new(request.managed_copy.data())) {
                Ok(archive) => archive,
                Err(error) => {
                    return DocumentOpenerOutcome::Failed {
                        reason: format!("the presentation is not a readable package: {error}"),
                    };
                }
            };
        let mut slides = slide_entry_names(&archive);
        if slides.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the package has no ppt/slides parts".to_owned(),
            };
        }
        let truncated = slides.len() > MAX_DOCUMENT_VIEW_PAGES;
        slides.truncate(MAX_DOCUMENT_VIEW_PAGES);
        let mut pages = Vec::new();
        for (number, name) in slides {
            let Ok(mut entry) = archive.by_name(&name) else {
                continue;
            };
            if entry.size() > 32 * 1024 * 1024 {
                return DocumentOpenerOutcome::Failed {
                    reason: "a slide part is implausibly large".to_owned(),
                };
            }
            let mut xml = String::new();
            if let Err(error) = entry.read_to_string(&mut xml) {
                return DocumentOpenerOutcome::Failed {
                    reason: format!("a slide part cannot be decoded: {error}"),
                };
            }
            let document = match roxmltree::Document::parse(&xml) {
                Ok(document) => document,
                Err(error) => {
                    return DocumentOpenerOutcome::Failed {
                        reason: format!("a slide part is malformed: {error}"),
                    };
                }
            };
            // Image-only slides surface no text; their page number still counts, so the
            // remaining pages keep matching their real slide positions.
            if let Some(text) = slide_text(&document)
                && !text.trim().is_empty()
            {
                pages.push(DocumentPage { number, text: capped_text(text.trim()) });
            }
        }
        if pages.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the presentation contains no readable slide text".to_owned(),
            };
        }
        DocumentOpenerOutcome::Loaded {
            view: view(
                "builtin-presentation",
                request,
                DocumentViewBody::PagedText { pages, truncated },
            ),
        }
    }
}

/// The package's slide parts as `(1-based slide number, entry name)`, in slide order —
/// numeric, not lexicographic, so `slide10` follows `slide2`. Parsing the trailing
/// digits also rejects sibling parts like `ppt/slides/_rels/` and `slideLayouts`.
fn slide_entry_names<R: std::io::Read + std::io::Seek>(
    archive: &zip::ZipArchive<R>,
) -> Vec<(u32, String)> {
    const PREFIX: &str = "ppt/slides/slide";
    let mut slides: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(|name| {
            let digits = name.strip_prefix(PREFIX)?.strip_suffix(".xml")?;
            Some((digits.parse::<u32>().ok()?, name.to_owned()))
        })
        .collect();
    slides.sort_unstable_by_key(|(number, _)| *number);
    slides
}

/// One slide's paragraph lines joined with newlines; `None` when the slide has none.
fn slide_text(document: &roxmltree::Document<'_>) -> Option<String> {
    let mut lines = Vec::new();
    for paragraph in
        document.root_element().descendants().filter(|node| is_drawing_element(node, "p"))
    {
        let line: String = paragraph
            .descendants()
            .filter(|node| is_drawing_element(node, "t"))
            .filter_map(|node| node.text())
            .collect();
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn is_drawing_element(node: &Node<'_, '_>, local_name: &str) -> bool {
    node.tag_name().name() == local_name
        && node.tag_name().namespace().is_some_and(|namespace| namespace.starts_with(DRAWING_NS))
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{DocumentLoadState, DocumentOpenerOutcome, DocumentViewBody};

    use super::*;
    use crate::testing;

    fn slide_xml(paragraphs: &[&str]) -> String {
        let mut body = String::new();
        for paragraph in paragraphs {
            body.push_str("<a:p><a:r><a:t>");
            body.push_str(paragraph);
            body.push_str("</a:t></a:r></a:p>");
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" \
             xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
             <p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:sld>"
        )
    }

    #[test]
    fn a_pptx_loads_slides_in_numeric_order_as_paged_text() {
        // Written out of order and named so lexicographic order would differ.
        let bytes = testing::zip_bytes(&[
            ("ppt/slides/slide2.xml", slide_xml(&["Bullet: check load", "Bullet: check heat"])),
            ("ppt/slides/slide10.xml", slide_xml(&["Closing"])),
            ("ppt/slides/slide1.xml", slide_xml(&["Power budget"])),
            ("ppt/slides/_rels/slide1.xml.rels", "<Relationships/>".to_owned()),
            ("ppt/notesSlides/notesSlide1.xml", slide_xml(&["speaker notes must not leak"])),
        ]);
        let request = testing::request_for("review.pptx", &bytes);

        let outcome = PresentationOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("a valid pptx must load, got {outcome:?}");
        };
        assert_eq!(view.title, "review");
        let DocumentViewBody::PagedText { pages, truncated } = view.body else {
            panic!("pptx renders as paged text");
        };
        assert!(!truncated);
        let rendered: Vec<(u32, &str)> =
            pages.iter().map(|page| (page.number, page.text.as_str())).collect();
        assert_eq!(
            rendered,
            vec![
                (1, "Power budget"),
                (2, "Bullet: check load\nBullet: check heat"),
                (10, "Closing"),
            ],
            "slides are ordered numerically; page numbers match real slide positions"
        );
        let all: String = pages.iter().map(|page| page.text.as_str()).collect();
        assert!(!all.contains("speaker notes"), "notes parts are never read: {all}");
    }

    #[test]
    fn an_image_only_slide_keeps_the_remaining_slide_numbers_aligned() {
        let bytes = testing::zip_bytes(&[
            ("ppt/slides/slide1.xml", slide_xml(&["Title"])),
            ("ppt/slides/slide2.xml", slide_xml(&[])),
            ("ppt/slides/slide3.xml", slide_xml(&["Wrap up"])),
        ]);
        let request = testing::request_for("deck.pptx", &bytes);

        let outcome = PresentationOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("a valid pptx must load, got {outcome:?}");
        };
        let DocumentViewBody::PagedText { pages, .. } = view.body else {
            panic!("pptx renders as paged text");
        };
        let numbers: Vec<u32> = pages.iter().map(|page| page.number).collect();
        assert_eq!(numbers, vec![1, 3], "empty slides are skipped but keep numbering");
    }

    #[test]
    fn a_pptx_without_slide_parts_fails_explicitly() {
        let bytes = testing::zip_bytes(&[("unrelated.txt", "not a presentation".to_owned())]);
        let request = testing::request_for("broken.pptx", &bytes);

        let outcome = PresentationOpener.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Failed);
    }

    #[test]
    fn non_zip_bytes_fail_explicitly() {
        let request = testing::request_for("broken.pptx", b"plain text pretending to be pptx");

        assert_eq!(PresentationOpener.open(&request).load_state(), DocumentLoadState::Failed);
    }
}
