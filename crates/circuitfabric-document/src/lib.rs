//! Document access is project-scoped and returns citation-ready fragments.

use std::collections::BTreeMap;

use circuitfabric_contracts::{
    DocumentFragment, DocumentKind, DocumentRecord, EvidencePackage, ProjectId,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DocumentError {
    #[error("document `{0}` already exists")]
    AlreadyExists(String),
}

#[derive(Clone, Debug)]
struct IndexedDocument {
    record: DocumentRecord,
    fragments: Vec<DocumentFragment>,
}

#[derive(Default)]
pub struct DocumentService {
    documents: BTreeMap<(ProjectId, String), IndexedDocument>,
    /// Individually verified source lines (e.g. checked datasheet rows), kept apart from
    /// the full-text index so they can be replaced or withdrawn on their own.
    verified: BTreeMap<(ProjectId, String), Vec<DocumentFragment>>,
}

impl DocumentService {
    /// Returns whether this project has an indexed, citation-ready document with this id.
    #[must_use]
    pub fn is_indexed(&self, project_id: &str, document_id: &str) -> bool {
        self.documents.contains_key(&(project_id.to_owned(), document_id.to_owned()))
    }

    /// Returns whether this project holds verified fragments for this document.
    #[must_use]
    pub fn has_verified_fragments(&self, project_id: &str, document_id: &str) -> bool {
        self.verified.contains_key(&(project_id.to_owned(), document_id.to_owned()))
    }

    /// Replaces the verified fragments of one document; an empty list withdraws them.
    pub fn set_verified_fragments(
        &mut self,
        project_id: &str,
        document_id: &str,
        fragments: Vec<DocumentFragment>,
    ) {
        let key = (project_id.to_owned(), document_id.to_owned());
        if fragments.is_empty() {
            self.verified.remove(&key);
        } else {
            self.verified.insert(key, fragments);
        }
    }

    /// Registers already-extracted text split into pages; fragments cite
    /// `#page=<n>&line=<m>` with the line counted within its page.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentError::AlreadyExists`] when `id` is already registered for this
    /// project.
    pub fn register_pages(
        &mut self,
        project_id: ProjectId,
        id: String,
        kind: DocumentKind,
        title: String,
        source_locator: impl Into<String>,
        pages: &[String],
    ) -> Result<DocumentRecord, DocumentError> {
        let content_hash = format!("sha256:{:x}", Sha256::digest(pages.join("\u{c}").as_bytes()));
        let lines = pages.iter().enumerate().flat_map(|(page, text)| {
            text.lines()
                .enumerate()
                .map(move |(line, text)| (format!("page={}&line={}", page + 1, line + 1), text))
        });
        self.insert(project_id, id, kind, title, &source_locator.into(), &content_hash, lines)
    }

    /// Registers already-extracted text and derives citation-ready line fragments.
    ///
    /// Document identifiers are scoped per project: two projects may register the same `id`
    /// (for example after importing the same file) without sharing or overwriting each other.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentError::AlreadyExists`] when `id` is already registered for this
    /// project.
    pub fn register_text(
        &mut self,
        project_id: ProjectId,
        id: String,
        kind: DocumentKind,
        title: String,
        source_locator: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<DocumentRecord, DocumentError> {
        let text = text.into();
        let content_hash = format!("sha256:{:x}", Sha256::digest(text.as_bytes()));
        let lines =
            text.lines().enumerate().map(|(index, line)| (format!("line={}", index + 1), line));
        self.insert(project_id, id, kind, title, &source_locator.into(), &content_hash, lines)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert<'a>(
        &mut self,
        project_id: ProjectId,
        id: String,
        kind: DocumentKind,
        title: String,
        source_locator: &str,
        content_hash: &str,
        lines: impl Iterator<Item = (String, &'a str)>,
    ) -> Result<DocumentRecord, DocumentError> {
        let key = (project_id.clone(), id.clone());
        if self.documents.contains_key(&key) {
            return Err(DocumentError::AlreadyExists(id));
        }
        let record = DocumentRecord {
            id: id.clone(),
            project_id,
            kind,
            title,
            source_locator: source_locator.to_owned(),
            content_hash: content_hash.to_owned(),
        };
        let fragments = lines
            .filter_map(|(anchor, line)| {
                let text = line.trim();
                (!text.is_empty()).then(|| DocumentFragment {
                    document_id: id.clone(),
                    content_hash: content_hash.to_owned(),
                    locator: format!("{source_locator}#{anchor}"),
                    text: text.to_owned(),
                })
            })
            .collect();

        self.documents.insert(key, IndexedDocument { record: record.clone(), fragments });
        Ok(record)
    }

    #[must_use]
    pub fn retrieve(&self, project_id: &str, query: &str) -> EvidencePackage {
        let query_folded = query.to_lowercase();
        let fragments = self
            .documents
            .values()
            .filter(|document| document.record.project_id == project_id)
            .flat_map(|document| &document.fragments)
            .chain(
                self.verified
                    .iter()
                    .filter(|((owner, _), _)| owner == project_id)
                    .flat_map(|(_, fragments)| fragments),
            )
            .filter(|fragment| fragment.text.to_lowercase().contains(&query_folded))
            .cloned()
            .collect();

        EvidencePackage { project_id: project_id.to_owned(), query: query.to_owned(), fragments }
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::DocumentKind;

    use super::*;

    #[test]
    fn identical_document_ids_belong_to_their_own_projects() {
        let mut service = DocumentService::default();
        service
            .register_text(
                "project-a".to_owned(),
                "doc-shared".to_owned(),
                DocumentKind::Markdown,
                "A".to_owned(),
                "a.md".to_owned(),
                "Alpha capacitor note.".to_owned(),
            )
            .expect("register for project A");

        let error = service
            .register_text(
                "project-a".to_owned(),
                "doc-shared".to_owned(),
                DocumentKind::Markdown,
                "A again".to_owned(),
                "a2.md".to_owned(),
                "Duplicate id within one project.".to_owned(),
            )
            .expect_err("duplicate within a project is rejected");
        assert_eq!(error, DocumentError::AlreadyExists("doc-shared".to_owned()));

        service
            .register_text(
                "project-b".to_owned(),
                "doc-shared".to_owned(),
                DocumentKind::Markdown,
                "B".to_owned(),
                "b.md".to_owned(),
                "Beta capacitor note.".to_owned(),
            )
            .expect("same id in another project is allowed");

        let alpha = service.retrieve("project-a", "capacitor");
        let beta = service.retrieve("project-b", "capacitor");
        assert_eq!(alpha.fragments.len(), 1);
        assert_eq!(beta.fragments.len(), 1);
        assert_eq!(alpha.fragments[0].text, "Alpha capacitor note.");
        assert_eq!(beta.fragments[0].text, "Beta capacitor note.");
    }

    #[test]
    fn retrieval_is_limited_to_the_requesting_project() {
        let mut service = DocumentService::default();
        service
            .register_text(
                "project-a".to_owned(),
                "datasheet-a".to_owned(),
                DocumentKind::Markdown,
                "A".to_owned(),
                "a.md".to_owned(),
                "The regulator requires a 1uF capacitor.".to_owned(),
            )
            .expect("new document");

        let visible = service.retrieve("project-a", "capacitor");
        let hidden = service.retrieve("project-b", "capacitor");

        assert_eq!(visible.fragments.len(), 1);
        assert!(hidden.fragments.is_empty());
        assert_eq!(visible.fragments[0].locator, "a.md#line=1");
    }

    #[test]
    fn paged_documents_cite_page_and_line() {
        let mut service = DocumentService::default();
        service
            .register_pages(
                "project-a".to_owned(),
                "datasheet-a".to_owned(),
                DocumentKind::Pdf,
                "A".to_owned(),
                "a.pdf",
                &["Title\n".to_owned(), "\nVIN  Input voltage  6 V\n".to_owned()],
            )
            .expect("new paged document");
        assert!(service.is_indexed("project-a", "datasheet-a"));
        let found = service.retrieve("project-a", "input voltage");
        assert_eq!(found.fragments.len(), 1);
        assert_eq!(found.fragments[0].locator, "a.pdf#page=2&line=2");
        assert_eq!(found.fragments[0].text, "VIN  Input voltage  6 V");
    }

    #[test]
    fn verified_fragments_are_retrievable_replaceable_and_project_scoped() {
        let mut service = DocumentService::default();
        let fragment = |text: &str| DocumentFragment {
            document_id: "datasheet-a".to_owned(),
            content_hash: "sha256:file".to_owned(),
            locator: "a.pdf#datasheet=pins/1".to_owned(),
            text: text.to_owned(),
        };
        service.set_verified_fragments("project-a", "datasheet-a", vec![fragment("1 VIN power")]);
        assert!(service.has_verified_fragments("project-a", "datasheet-a"));
        assert!(!service.is_indexed("project-a", "datasheet-a"));
        assert_eq!(service.retrieve("project-a", "vin").fragments.len(), 1);
        assert!(service.retrieve("project-b", "vin").fragments.is_empty());

        service.set_verified_fragments("project-a", "datasheet-a", vec![fragment("2 GND ground")]);
        assert!(service.retrieve("project-a", "vin").fragments.is_empty());
        assert_eq!(service.retrieve("project-a", "gnd").fragments.len(), 1);

        service.set_verified_fragments("project-a", "datasheet-a", Vec::new());
        assert!(!service.has_verified_fragments("project-a", "datasheet-a"));
        assert!(service.retrieve("project-a", "gnd").fragments.is_empty());
    }
}
