//! Managed document opener plugins and their registry.
//!
//! ## Rendering scheme (selected)
//!
//! All built-in openers are pure-Rust, in-process parsers, chosen for permissive licensing,
//! cross-platform availability, and sandboxability:
//!
//! | Format | Library | License |
//! |--------|---------|---------|
//! | PDF | [`pdf-extract`] | MIT |
//! | Word `.docx` | [`zip`] + [`roxmltree`] | MIT / MIT-OR-Apache-2.0 |
//! | Excel `.xls`/`.xlsx` | [`calamine`] | MIT |
//! | Markdown | [`pulldown-cmark`] | MIT |
//!
//! This is deliberately an in-app parsing pipeline, never an OS-associated viewer: the crate
//! spawns no processes, opens no files of its own, evaluates no macros or formulas, and
//! follows no embedded links or external references. Openers receive only the in-memory
//! verified managed copy (see [`VerifiedDocumentCopy`]) and return structured read-only view
//! data. Legacy binary Word `.doc` is reported as explicitly unsupported rather than shelled
//! out to an external application.
//!
//! [`pdf-extract`]: https://crates.io/crates/pdf-extract
//! [`zip`]: https://crates.io/crates/zip
//! [`roxmltree`]: https://crates.io/crates/roxmltree
//! [`calamine`]: https://crates.io/crates/calamine
//! [`pulldown-cmark`]: https://crates.io/crates/pulldown-cmark

use std::panic::{AssertUnwindSafe, catch_unwind};

use circuitfabric_plugin_api::{
    DocumentBlock, DocumentBlockKind, DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest,
    DocumentSpan, DocumentSpanStyle, DocumentView, MAX_DOCUMENT_VIEW_TEXT,
};

pub mod datasheet;

mod excel;
mod markdown;
mod pdf;
mod word;

pub use datasheet::{
    extract_datasheet, extract_datasheet_by_category, extract_datasheet_with_agent,
    extract_datasheet_with_agent_with_pages,
};

pub use excel::ExcelOpener;
pub use markdown::MarkdownOpener;
pub use pdf::PdfOpener;
#[cfg(feature = "raster-pdf")]
pub use pdf::{pdf_page_sizes, render_pdf_pages};
pub use word::WordOpener;

/// Truncates view text to the protocol's per-fragment cap.
pub(crate) fn capped_text(text: &str) -> String {
    text.chars().take(MAX_DOCUMENT_VIEW_TEXT).collect()
}

/// Accumulates inline styled runs for one block.
///
/// Capping happens on whole spans so the block's `text` stays the exact concatenation of
/// its `spans` — hosts can map span offsets to the plain text one-to-one.
#[derive(Default)]
pub(crate) struct Spans {
    spans: Vec<DocumentSpan>,
}

impl Spans {
    pub(crate) fn push(&mut self, style: DocumentSpanStyle, text: &str) {
        if !text.is_empty() {
            self.spans.push(DocumentSpan { text: text.to_owned(), style });
        }
    }

    /// Whether every accumulated run is whitespace.
    pub(crate) fn is_blank(&self) -> bool {
        self.spans.iter().all(|span| span.text.trim().is_empty())
    }

    pub(crate) fn clear(&mut self) {
        self.spans.clear();
    }

    /// Joins the runs into capped plain text, dropping the styling.
    pub(crate) fn take_text(&mut self) -> String {
        let mut text = String::new();
        for span in &self.spans {
            if text.chars().count() >= MAX_DOCUMENT_VIEW_TEXT {
                break;
            }
            text.push_str(&span.text);
        }
        capped_text(&text)
    }

    /// Builds the finished block from the accumulated runs.
    pub(crate) fn take_block(&mut self, kind: &DocumentBlockKind) -> DocumentBlock {
        let mut spans = Vec::new();
        let mut total = 0_usize;
        for span in self.spans.drain(..) {
            if total >= MAX_DOCUMENT_VIEW_TEXT {
                break;
            }
            total += span.text.chars().count();
            spans.push(span);
        }
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        DocumentBlock { kind: kind.clone(), text, spans }
    }
}

/// Derives a display title from the original file name (extension stripped).
pub(crate) fn title_from_file_name(file_name: &str) -> String {
    let stem = match file_name.rfind('.') {
        Some(dot) if dot > 0 => &file_name[..dot],
        _ => file_name,
    };
    capped_text(stem)
}

/// Shared view provenance: the opener must keep document identity, hash, and source locator
/// attached to whatever it renders.
pub(crate) fn view(
    opener_id: &str,
    request: &DocumentOpenerRequest,
    body: circuitfabric_plugin_api::DocumentViewBody,
) -> DocumentView {
    DocumentView {
        opener_id: opener_id.to_owned(),
        document_id: request.document_id.clone(),
        content_hash: request.content_hash.clone(),
        source_locator: request.source_locator.clone(),
        title: title_from_file_name(&request.file_name),
        body,
    }
}

/// Registry of opener plugins with extension-based routing and unified capability reporting.
///
/// Headless by construction: any caller (desktop or otherwise) can query
/// [`Self::capabilities`] and [`Self::open`] without any UI state.
#[derive(Default)]
pub struct DocumentOpenerRegistry {
    openers: Vec<Box<dyn DocumentOpener>>,
}

impl DocumentOpenerRegistry {
    /// An empty registry; extend it with [`Self::register`] or start from
    /// [`Self::with_builtin_openers`].
    #[must_use]
    pub fn new() -> Self {
        Self { openers: Vec::new() }
    }

    /// A registry preloaded with the built-in PDF, Word, Excel, and Markdown openers.
    #[must_use]
    pub fn with_builtin_openers() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(PdfOpener));
        registry.register(Box::new(WordOpener));
        registry.register(Box::new(ExcelOpener));
        registry.register(Box::new(MarkdownOpener));
        registry
    }

    /// Registers one opener plugin. The last opener registered for an extension wins routing.
    pub fn register(&mut self, opener: Box<dyn DocumentOpener>) {
        self.openers.push(opener);
    }

    /// The unified capability declarations of every registered opener.
    #[must_use]
    pub fn capabilities(&self) -> Vec<circuitfabric_plugin_api::OpenerCapability> {
        self.openers.iter().map(|opener| opener.capability()).collect()
    }

    /// Whether any registered opener claims one of these extensions or MIME types.
    #[must_use]
    pub fn can_open(&self, file_name: &str) -> bool {
        self.opener_for(file_name).is_some()
    }

    fn opener_for(&self, file_name: &str) -> Option<&dyn DocumentOpener> {
        let extension = extension_of(file_name)?;
        self.openers
            .iter()
            .rev()
            .find(|opener| opener.capability().supports_extension(extension))
            .map(std::convert::AsRef::as_ref)
    }

    /// Opens a verified managed copy with the opener that claims its extension.
    ///
    /// Unknown extensions return an explicit `Unsupported` outcome; a parsing plugin that
    /// panics is contained and reported as `Failed` instead of crashing the host.
    #[must_use]
    pub fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let Some(opener) = self.opener_for(&request.file_name) else {
            return DocumentOpenerOutcome::Unsupported {
                reason: format!(
                    "no registered opener supports the extension of `{}`",
                    request.file_name
                ),
            };
        };
        match catch_unwind(AssertUnwindSafe(|| opener.open(request))) {
            Ok(outcome) => outcome,
            Err(_) => DocumentOpenerOutcome::Failed {
                reason: format!(
                    "opener `{}` stopped unexpectedly while parsing",
                    opener.capability().opener_id
                ),
            },
        }
    }
}

/// Convenience wrapper: opens a request with a fresh registry of the built-in openers.
#[must_use]
pub fn open_document_with_builtin_openers(
    request: &DocumentOpenerRequest,
) -> DocumentOpenerOutcome {
    DocumentOpenerRegistry::with_builtin_openers().open(request)
}

pub(crate) fn extension_of(file_name: &str) -> Option<&str> {
    let dot = file_name.rfind('.')?;
    if dot == 0 || dot == file_name.len() - 1 {
        return None;
    }
    Some(&file_name[dot + 1..])
}

#[cfg(test)]
pub(crate) mod testing {
    //! Fixture builders shared by the adapter tests: real PDF/ZIP packages assembled in memory.

    use std::fmt::Write as _;
    use std::io::{Cursor, Write};

    use sha2::{Digest, Sha256};

    /// A request whose copy hash is computed from the bytes, as the storage gate would issue.
    pub fn request_for(
        file_name: &str,
        bytes: &[u8],
    ) -> circuitfabric_plugin_api::DocumentOpenerRequest {
        let content_hash = format!("sha256:{:x}", Sha256::digest(bytes));
        circuitfabric_plugin_api::DocumentOpenerRequest {
            project_id: "proj".to_owned(),
            document_id: "doc-test".to_owned(),
            file_name: file_name.to_owned(),
            document_kind: circuitfabric_contracts::DocumentKind::Text,
            content_hash: content_hash.clone(),
            source_locator: format!("source/{file_name}"),
            managed_copy: circuitfabric_plugin_api::VerifiedDocumentCopy::from_verified(
                bytes.to_vec(),
                content_hash,
            )
            .expect("fixture hash matches its bytes"),
        }
    }

    /// Packs entries into a real ZIP archive in memory.
    pub fn zip_bytes(entries: &[(&str, String)]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (name, content) in entries {
                writer
                    .start_file((*name).to_owned(), zip::write::SimpleFileOptions::default())
                    .expect("start zip entry");
                writer.write_all(content.as_bytes()).expect("write zip entry");
            }
            writer.finish().expect("finish zip archive");
        }
        cursor.into_inner()
    }

    /// Assembles a small but structurally valid PDF with correct cross-reference offsets.
    pub fn minimal_pdf(lines: &[&str]) -> Vec<u8> {
        let escaped =
            |text: &str| text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
        let mut content = String::from("BT /F1 12 Tf 72 720 Td\n");
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                content.push_str("0 -18 Td\n");
            }
            let _ = writeln!(content, "({}) Tj", escaped(line));
        }
        content.push_str("ET");

        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];

        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            let _ = writeln!(pdf, "{} 0 obj\n{object}\nendobj", index + 1);
        }
        let xref_start = pdf.len();
        let _ = writeln!(pdf, "xref\n0 {}", objects.len() + 1);
        pdf.push_str("0000000000 65535 f \n");
        for offset in offsets {
            let _ = writeln!(pdf, "{offset:010} 00000 n ");
        }
        let _ = writeln!(
            pdf,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF",
            objects.len() + 1
        );
        pdf.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuitfabric_plugin_api::{DocumentLoadState, DocumentViewBody};

    struct RejectingOpener;

    impl DocumentOpener for RejectingOpener {
        fn capability(&self) -> circuitfabric_plugin_api::OpenerCapability {
            circuitfabric_plugin_api::OpenerCapability {
                opener_id: "test-rejecting".to_owned(),
                label: "Test".to_owned(),
                extensions: vec!["never".to_owned()],
                mime_types: Vec::new(),
            }
        }

        fn open(&self, _request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
            DocumentOpenerOutcome::Failed { reason: "always fails".to_owned() }
        }
    }

    struct PanickingOpener;

    impl DocumentOpener for PanickingOpener {
        fn capability(&self) -> circuitfabric_plugin_api::OpenerCapability {
            circuitfabric_plugin_api::OpenerCapability {
                opener_id: "test-panicking".to_owned(),
                label: "Test".to_owned(),
                extensions: vec!["boom".to_owned()],
                mime_types: Vec::new(),
            }
        }

        fn open(&self, _request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
            panic!("parser exploded");
        }
    }

    #[test]
    fn the_registry_reports_capabilities_without_ui_state() {
        let registry = DocumentOpenerRegistry::with_builtin_openers();
        let capabilities = registry.capabilities();

        let extensions: Vec<String> =
            capabilities.iter().flat_map(|capability| capability.extensions.clone()).collect();
        assert!(extensions.contains(&"pdf".to_owned()));
        assert!(extensions.contains(&"docx".to_owned()));
        assert!(extensions.contains(&"xls".to_owned()));
        assert!(extensions.contains(&"xlsx".to_owned()));
        assert!(extensions.contains(&"md".to_owned()));
        assert!(registry.can_open("MCU.PDF"), "routing is case-insensitive");
        assert!(!registry.can_open("unknown.xyz"));
    }

    #[test]
    fn unclaimed_extensions_get_an_explicit_unsupported_outcome() {
        let registry = DocumentOpenerRegistry::with_builtin_openers();
        let request = testing::request_for("unknown.xyz", b"whatever");

        let outcome = registry.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Unsupported);
        let DocumentOpenerOutcome::Unsupported { reason } = outcome else {
            panic!("expected unsupported, got {outcome:?}");
        };
        assert!(reason.contains("no registered opener"), "{reason}");
    }

    #[test]
    fn custom_plugins_can_be_registered_and_route_by_extension() {
        let mut registry = DocumentOpenerRegistry::new();
        registry.register(Box::new(RejectingOpener));
        let request = testing::request_for("fixture.never", b"content");

        let outcome = registry.open(&request);

        let DocumentOpenerOutcome::Failed { reason } = outcome else {
            panic!("expected the custom opener to run, got {outcome:?}");
        };
        assert_eq!(reason, "always fails");
    }

    #[test]
    fn a_panicking_opener_is_contained_as_a_failed_outcome() {
        let mut registry = DocumentOpenerRegistry::new();
        registry.register(Box::new(PanickingOpener));
        let request = testing::request_for("fixture.boom", b"content");

        let outcome = registry.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Failed);
    }

    #[test]
    fn a_markdown_fixture_round_trips_through_the_convenience_wrapper() {
        let request = testing::request_for("notes.md", b"# Title\nBody text\n");
        let outcome = open_document_with_builtin_openers(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("markdown fixture must load");
        };
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("markdown renders as blocks");
        };
        assert!(blocks.iter().any(|block| matches!(
            block.kind,
            circuitfabric_plugin_api::DocumentBlockKind::Heading { level: 1 }
        )));
    }
}
