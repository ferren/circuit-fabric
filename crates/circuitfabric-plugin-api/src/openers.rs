//! Managed document opener contracts.
//!
//! Opener plugins render an already-verified, content-addressed managed copy into embeddable
//! read-only view data. A [`DocumentOpenerRequest`] deliberately carries only the project ID,
//! the document ID, and the verified bytes: openers never receive filesystem paths and
//! therefore cannot reach anything outside the managed copy. Macros, scripts, embedded links,
//! and other active content are never executed by this contract — openers parse static
//! content only and return structured view data for the host to render.

use std::fmt;

use circuitfabric_contracts::{DocumentId, DocumentKind, ProjectId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Upper bounds applied while building view data so a hostile document cannot exhaust memory.
pub const MAX_DOCUMENT_VIEW_PAGES: usize = 200;
/// Maximum number of blocks (headings, paragraphs, rows) kept in one block view.
pub const MAX_DOCUMENT_VIEW_BLOCKS: usize = 5_000;
/// Maximum number of rows surfaced per spreadsheet sheet.
pub const MAX_DOCUMENT_VIEW_ROWS_PER_SHEET: usize = 500;
/// Maximum length of one cell or block text fragment, in characters.
pub const MAX_DOCUMENT_VIEW_TEXT: usize = 4_096;

/// Reason a document open request was refused before any opener plugin ran.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentOpenDenial {
    /// The document ID is unknown in the addressed project.
    NotFound,
    /// The request addresses a project other than the one that owns the managed copy.
    CrossProject,
    /// The document exists but is not authorized for use in this project.
    Unauthorized,
    /// The document has not completed the content scan required before opening.
    NotScanned,
    /// The content scan explicitly rejected this document.
    ScanRejected,
    /// The managed copy no longer matches the authorized content hash.
    IntegrityMismatch,
    /// The managed copy is missing from the managed documents tree.
    MissingManagedCopy,
}

impl fmt::Display for DocumentOpenDenial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::NotFound => "document not found in this project",
            Self::CrossProject => "the document belongs to a different project",
            Self::Unauthorized => "the document is not authorized",
            Self::NotScanned => "the document has not completed its content scan",
            Self::ScanRejected => "the content scan rejected this document",
            Self::IntegrityMismatch => "the managed copy no longer matches its content hash",
            Self::MissingManagedCopy => "the managed copy is missing",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for DocumentOpenDenial {}

/// A managed copy whose bytes were just re-verified against the recorded SHA-256 hash.
///
/// The only constructor recomputes the digest, so an unverified or tampered copy can never
/// enter an opener request. Openers get the bytes and the hash — never a filesystem path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDocumentCopy {
    data: Vec<u8>,
    content_hash: String,
}

impl VerifiedDocumentCopy {
    /// Wraps bytes that are expected to already be verified, re-checking the digest.
    ///
    /// Returns `None` when the bytes do not match `content_hash`, which keeps unverified
    /// content out of opener requests by construction.
    #[must_use]
    pub fn from_verified(data: Vec<u8>, content_hash: String) -> Option<Self> {
        let actual = format!("sha256:{:x}", Sha256::digest(&data));
        (actual == content_hash).then_some(Self { data, content_hash })
    }

    /// The verified managed-copy bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The `sha256:<hex>` digest these bytes were verified against.
    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

/// Everything an opener plugin may know about the document being opened.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentOpenerRequest {
    pub project_id: ProjectId,
    pub document_id: DocumentId,
    /// Original file name; openers use only its extension to confirm routing.
    pub file_name: String,
    pub document_kind: DocumentKind,
    pub content_hash: String,
    /// Provenance locator of the original source file.
    pub source_locator: String,
    pub managed_copy: VerifiedDocumentCopy,
}

/// Coarse load state reported for every opener invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentLoadState {
    /// A read-only view was produced.
    Loaded,
    /// No opener claims this content, by format policy.
    Unsupported,
    /// The claimed format could not be parsed (corrupt or truncated content).
    Failed,
}

/// One page of extracted text in a paged view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPage {
    /// 1-based page number.
    pub number: u32,
    pub text: String,
}

/// The structural role of one block in a block-oriented view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentBlockKind {
    Heading { level: u8 },
    Paragraph,
    Code { language: Option<String> },
    ListItem { depth: u8 },
    Quote,
}

/// Inline emphasis for one run of text inside a block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentSpanStyle {
    Plain,
    Strong,
    Emphasis,
    Code,
    Strikethrough,
}

/// One styled run of inline text; consecutive spans make up a block's text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSpan {
    pub text: String,
    pub style: DocumentSpanStyle,
}

/// One read-only block of extracted content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentBlock {
    pub kind: DocumentBlockKind,
    /// Plain concatenation of the block's text; the fallback when `spans` is empty.
    pub text: String,
    /// Inline styling runs. Empty means render `text` without inline formatting.
    #[serde(default)]
    pub spans: Vec<DocumentSpan>,
}

/// One cell of a surfaced spreadsheet row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentCell {
    Empty,
    Text(String),
    Number(f64),
    Boolean(bool),
}

/// One surfaced worksheet of a spreadsheet view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSheet {
    pub name: String,
    pub rows: Vec<Vec<DocumentCell>>,
    /// Total row count in the sheet, which may exceed the surfaced `rows`.
    pub row_count: usize,
    /// `true` when `rows` was capped by [`MAX_DOCUMENT_VIEW_ROWS_PER_SHEET`].
    pub truncated: bool,
}

/// One page rasterized to a tight RGBA buffer: row-major, `width * height * 4` bytes, no
/// padding. Produced by rasterizing openers (for example pdfium-backed PDF rendering);
/// hosts convert to their platform image format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRasterPage {
    /// 1-based page number.
    pub number: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The embeddable, read-only body of a rendered document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentViewBody {
    /// Pre-rendered page bitmaps, in document order.
    RasterPages {
        pages: Vec<DocumentRasterPage>,
        /// Total page count of the document, which may exceed the rendered `pages`.
        page_count: usize,
        truncated: bool,
    },
    PagedText {
        pages: Vec<DocumentPage>,
        truncated: bool,
    },
    Blocks {
        blocks: Vec<DocumentBlock>,
        truncated: bool,
    },
    Sheets {
        sheets: Vec<DocumentSheet>,
    },
    PlainText {
        text: String,
    },
}

/// The complete opener result: what to render plus the provenance it must keep attached.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    /// ID of the opener plugin that produced this view.
    pub opener_id: String,
    pub document_id: DocumentId,
    pub content_hash: String,
    pub source_locator: String,
    /// Display title, derived from the original file name.
    pub title: String,
    pub body: DocumentViewBody,
}

/// The outcome of one opener invocation.
///
/// `Unsupported` and `Failed` are explicit, inspectable states — a corrupt or policy-excluded
/// document never silently renders as empty content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum DocumentOpenerOutcome {
    Loaded { view: DocumentView },
    Unsupported { reason: String },
    Failed { reason: String },
}

impl DocumentOpenerOutcome {
    /// The coarse load state of this outcome.
    #[must_use]
    pub const fn load_state(&self) -> DocumentLoadState {
        match self {
            Self::Loaded { .. } => DocumentLoadState::Loaded,
            Self::Unsupported { .. } => DocumentLoadState::Unsupported,
            Self::Failed { .. } => DocumentLoadState::Failed,
        }
    }
}

/// A unified capability declaration: which extensions and MIME types one opener can preview.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenerCapability {
    /// ID of the opener plugin this declaration describes.
    pub opener_id: String,
    /// Human-readable format label.
    pub label: String,
    /// Lowercase file extensions without the leading dot.
    pub extensions: Vec<String>,
    /// MIME types this opener can preview.
    pub mime_types: Vec<String>,
}

impl OpenerCapability {
    /// Whether this opener claims a file extension (case-insensitive, no leading dot).
    #[must_use]
    pub fn supports_extension(&self, extension: &str) -> bool {
        let normalized = extension.trim().trim_start_matches('.').to_ascii_lowercase();
        self.extensions.iter().any(|claimed| claimed.eq_ignore_ascii_case(&normalized))
    }

    /// Whether this opener claims a MIME type (case-insensitive).
    #[must_use]
    pub fn supports_mime(&self, mime_type: &str) -> bool {
        self.mime_types.iter().any(|claimed| claimed.eq_ignore_ascii_case(mime_type.trim()))
    }
}

/// A managed-document opener plugin.
///
/// Implementations receive only the request's verified bytes and must not touch the
/// filesystem, spawn processes, evaluate macros or formulas, or follow embedded links.
pub trait DocumentOpener: Send + Sync {
    /// The unified capability declaration for this opener.
    fn capability(&self) -> OpenerCapability;

    /// Renders the verified managed copy into a read-only view, or an explicit
    /// unsupported/failed state.
    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verified_copy(contents: &[u8]) -> Option<VerifiedDocumentCopy> {
        VerifiedDocumentCopy::from_verified(
            contents.to_vec(),
            format!("sha256:{:x}", Sha256::digest(contents)),
        )
    }

    #[test]
    fn a_verified_copy_rejects_bytes_that_do_not_match_the_hash() {
        let copy = verified_copy(b"managed bytes").expect("matching hash is accepted");
        assert_eq!(copy.data(), b"managed bytes");
        assert!(copy.content_hash().starts_with("sha256:"));

        let mismatch = VerifiedDocumentCopy::from_verified(
            b"different bytes".to_vec(),
            copy.content_hash().to_owned(),
        );
        assert!(mismatch.is_none(), "a tampered copy can never become verified");
    }

    #[test]
    fn capability_matching_is_case_insensitive() {
        let capability = OpenerCapability {
            opener_id: "builtin-pdf".to_owned(),
            label: "PDF".to_owned(),
            extensions: vec!["pdf".to_owned()],
            mime_types: vec!["application/pdf".to_owned()],
        };

        assert!(capability.supports_extension("PDF"));
        assert!(capability.supports_extension(".pdf"));
        assert!(!capability.supports_extension("docx"));
        assert!(capability.supports_mime("APPLICATION/PDF"));
        assert!(!capability.supports_mime("text/markdown"));
    }

    #[test]
    fn outcome_states_map_to_their_load_state() {
        let loaded = DocumentOpenerOutcome::Loaded {
            view: DocumentView {
                opener_id: "builtin-markdown".to_owned(),
                document_id: "doc-1".to_owned(),
                content_hash: "sha256:0".to_owned(),
                source_locator: "notes.md".to_owned(),
                title: "notes.md".to_owned(),
                body: DocumentViewBody::PlainText { text: "hello".to_owned() },
            },
        };
        assert_eq!(loaded.load_state(), DocumentLoadState::Loaded);
        assert_eq!(
            DocumentOpenerOutcome::Unsupported { reason: "no opener".to_owned() }.load_state(),
            DocumentLoadState::Unsupported
        );
        assert_eq!(
            DocumentOpenerOutcome::Failed { reason: "corrupt".to_owned() }.load_state(),
            DocumentLoadState::Failed
        );
    }
}
