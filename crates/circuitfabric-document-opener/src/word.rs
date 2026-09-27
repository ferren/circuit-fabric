//! Word opener: `.docx` body extraction via ZIP + XML parsing.
//!
//! Only the `word/document.xml` part is read: paragraph and table text. Macros
//! (`word/vbaProject.bin`), embedded objects, headers/footers, and every external
//! relationship are never opened, so no active content can execute. Legacy binary `.doc`
//! files are explicitly unsupported; the capability list reports only `.docx`.

use std::io::Read;

use circuitfabric_plugin_api::{
    DocumentBlock, DocumentBlockKind, DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest,
    DocumentViewBody, MAX_DOCUMENT_VIEW_BLOCKS, OpenerCapability,
};
use roxmltree::Node;

use crate::{capped_text, view};

/// Opens Word managed copies as a block-oriented, read-only text view.
#[derive(Clone, Copy, Debug, Default)]
pub struct WordOpener;

/// The OOXML wordprocessing namespace (matched leniently on the tail).
const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

impl DocumentOpener for WordOpener {
    fn capability(&self) -> OpenerCapability {
        OpenerCapability {
            opener_id: "builtin-word".to_owned(),
            label: "Word document".to_owned(),
            extensions: vec!["docx".to_owned()],
            mime_types: vec![
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    .to_owned(),
            ],
        }
    }

    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let xml = match read_document_xml(request.managed_copy.data()) {
            Ok(xml) => xml,
            Err(reason) => return DocumentOpenerOutcome::Failed { reason },
        };
        let document = match roxmltree::Document::parse(&xml) {
            Ok(document) => document,
            Err(error) => {
                return DocumentOpenerOutcome::Failed {
                    reason: format!("the Word document body is malformed: {error}"),
                };
            }
        };
        let mut blocks = Vec::new();
        collect_body_blocks(&document, &mut blocks);
        if blocks.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the Word document contains no readable body text".to_owned(),
            };
        }
        let truncated = blocks.len() > MAX_DOCUMENT_VIEW_BLOCKS;
        blocks.truncate(MAX_DOCUMENT_VIEW_BLOCKS);
        DocumentOpenerOutcome::Loaded {
            view: view("builtin-word", request, DocumentViewBody::Blocks { blocks, truncated }),
        }
    }
}

/// Reads and decodes the single `word/document.xml` entry.
fn read_document_xml(data: &[u8]) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|error| format!("the Word document is not a readable package: {error}"))?;
    let mut entry = archive
        .by_name("word/document.xml")
        .map_err(|_| "the package has no word/document.xml part".to_owned())?;
    if entry.size() > 32 * 1024 * 1024 {
        return Err("the Word document body is implausibly large".to_owned());
    }
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|error| format!("the Word document body cannot be decoded: {error}"))?;
    Ok(xml)
}

/// Direct child elements of a node, in document order.
fn element_children<'a>(node: Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children().filter(roxmltree::Node::is_element)
}

/// Walks the document body, emitting one block per paragraph and per flattened table row.
fn collect_body_blocks<'input>(
    document: &'input roxmltree::Document<'input>,
    blocks: &mut Vec<DocumentBlock>,
) {
    let Some(body) =
        element_children(document.root_element()).find(|element| is_word_element(element, "body"))
    else {
        return;
    };
    for element in element_children(body) {
        if is_word_element(&element, "p") {
            if let Some(block) = paragraph_block(element) {
                blocks.push(block);
            }
        } else if is_word_element(&element, "tbl") {
            for row in element_children(element).filter(|row| is_word_element(row, "tr")) {
                let cells: Vec<String> = element_children(row)
                    .filter(|cell| is_word_element(cell, "tc"))
                    .map(descendant_text)
                    .collect();
                let text = cells.join(" | ");
                if !text.trim().is_empty() {
                    blocks.push(DocumentBlock {
                        kind: DocumentBlockKind::Paragraph,
                        text: capped_text(&text),
                        spans: Vec::new(),
                    });
                }
            }
        }
    }
}

/// Builds one block from a `w:p` element: heading, list item, or plain paragraph.
fn paragraph_block(paragraph: Node<'_, '_>) -> Option<DocumentBlock> {
    let runs = descendant_spans(paragraph);
    if runs.iter().all(|span| span.text.trim().is_empty()) {
        return None;
    }
    let properties = element_children(paragraph).find(|element| is_word_element(element, "pPr"));
    let kind = properties.map_or(DocumentBlockKind::Paragraph, |properties| {
        let style = element_children(properties)
            .find(|element| is_word_element(element, "pStyle"))
            .and_then(|style| style.attribute((WORD_NS, "val")));
        if let Some(style) = style
            && let Some(level) = style
                .strip_prefix("Heading")
                .or_else(|| style.strip_prefix("heading"))
                .and_then(|level| level.parse::<u8>().ok())
            && (1..=6).contains(&level)
        {
            return DocumentBlockKind::Heading { level };
        }
        let numbered =
            element_children(properties).any(|element| is_word_element(&element, "numPr"));
        if numbered {
            return DocumentBlockKind::ListItem { depth: 0 };
        }
        DocumentBlockKind::Paragraph
    });
    let mut spans = crate::Spans::default();
    for run in runs {
        spans.push(run.style, &run.text);
    }
    Some(spans.take_block(&kind))
}

/// Collects `w:t` text below an element as styled runs, in document order.
///
/// Bold, italic, and strikethrough run properties become span styles; every `w:r` is
/// visited wherever it appears, so text inside field constructs is still covered.
fn descendant_spans(element: Node<'_, '_>) -> Vec<circuitfabric_plugin_api::DocumentSpan> {
    let mut spans = Vec::new();
    for node in element.descendants() {
        if is_word_element(&node, "r") {
            let style = run_style(&node);
            for text_node in node.descendants() {
                if is_word_element(&text_node, "t")
                    && let Some(chunk) = text_node.text()
                {
                    spans.push(circuitfabric_plugin_api::DocumentSpan {
                        text: chunk.to_owned(),
                        style,
                    });
                }
            }
        }
    }
    spans
}

/// Reads one run's `w:rPr` into a span style (bold > italic > strikethrough > plain).
fn run_style(run: &Node<'_, '_>) -> circuitfabric_plugin_api::DocumentSpanStyle {
    let Some(properties) = element_children(*run).find(|element| is_word_element(element, "rPr"))
    else {
        return circuitfabric_plugin_api::DocumentSpanStyle::Plain;
    };
    let has =
        |local: &str| element_children(properties).any(|element| is_word_element(&element, local));
    if has("b") {
        circuitfabric_plugin_api::DocumentSpanStyle::Strong
    } else if has("i") {
        circuitfabric_plugin_api::DocumentSpanStyle::Emphasis
    } else if has("strike") {
        circuitfabric_plugin_api::DocumentSpanStyle::Strikethrough
    } else {
        circuitfabric_plugin_api::DocumentSpanStyle::Plain
    }
}

/// Concatenates all `w:t` text below an element, in document order, without styling.
fn descendant_text(element: Node<'_, '_>) -> String {
    descendant_spans(element).iter().map(|span| span.text.as_str()).collect()
}

fn is_word_element(node: &Node<'_, '_>, local_name: &str) -> bool {
    node.tag_name().name() == local_name
        && node.tag_name().namespace().is_some_and(|namespace| namespace.starts_with(WORD_NS))
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{DocumentBlockKind, DocumentLoadState, DocumentOpenerOutcome};

    use super::*;
    use crate::testing;

    fn document_xml(body: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <w:document xmlns:w=\"{WORD_NS}\"><w:body>{body}</w:body></w:document>"
        )
    }

    #[test]
    fn a_docx_loads_headings_paragraphs_and_tables_as_blocks() {
        let body = "\
            <w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Power budget</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Input is </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>12 V</w:t></w:r><w:r><w:t> at 2 A.</w:t></w:r></w:p>\
            <w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>Check the fuse rating</w:t></w:r></w:p>\
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>R7</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>10k</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
        let bytes = testing::zip_bytes(&[("word/document.xml", document_xml(body))]);
        let request = testing::request_for("budget.docx", &bytes);

        let outcome = WordOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("a valid docx must load, got {outcome:?}");
        };
        assert_eq!(view.title, "budget");
        let DocumentViewBody::Blocks { blocks, truncated } = view.body else {
            panic!("docx renders as blocks");
        };
        assert!(!truncated);
        assert_eq!(blocks[0].kind, DocumentBlockKind::Heading { level: 1 });
        assert_eq!(blocks[0].text, "Power budget");
        assert_eq!(blocks[1].kind, DocumentBlockKind::Paragraph);
        assert_eq!(blocks[1].text, "Input is 12 V at 2 A.");
        assert!(
            blocks[1].spans.iter().any(|span| {
                span.style == circuitfabric_plugin_api::DocumentSpanStyle::Strong
                    && span.text == "12 V"
            }),
            "bold runs become strong spans: {:?}",
            blocks[1].spans
        );
        assert_eq!(blocks[2].kind, DocumentBlockKind::ListItem { depth: 0 });
        assert_eq!(blocks[3].text, "R7 | 10k", "tables are flattened per row");
    }

    #[test]
    fn a_docx_without_the_body_part_fails_explicitly() {
        let bytes = testing::zip_bytes(&[("unrelated.txt", "not a word document".to_owned())]);
        let request = testing::request_for("broken.docx", &bytes);

        let outcome = WordOpener.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Failed);
    }

    #[test]
    fn non_zip_bytes_fail_explicitly() {
        let request = testing::request_for("broken.docx", b"plain text pretending to be docx");

        assert_eq!(WordOpener.open(&request).load_state(), DocumentLoadState::Failed);
    }
}
