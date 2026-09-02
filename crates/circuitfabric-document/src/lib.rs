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
    documents: BTreeMap<String, IndexedDocument>,
}

impl DocumentService {
    /// Registers already-extracted text and derives citation-ready line fragments.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentError::AlreadyExists`] when `id` is already registered.
    pub fn register_text(
        &mut self,
        project_id: ProjectId,
        id: String,
        kind: DocumentKind,
        title: String,
        source_locator: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<DocumentRecord, DocumentError> {
        if self.documents.contains_key(&id) {
            return Err(DocumentError::AlreadyExists(id));
        }

        let source_locator = source_locator.into();
        let text = text.into();
        let content_hash = format!("sha256:{:x}", Sha256::digest(text.as_bytes()));
        let record = DocumentRecord {
            id: id.clone(),
            project_id,
            kind,
            title,
            source_locator: source_locator.clone(),
            content_hash: content_hash.clone(),
        };
        let fragments = text
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let text = line.trim();
                (!text.is_empty()).then(|| DocumentFragment {
                    document_id: id.clone(),
                    content_hash: content_hash.clone(),
                    locator: format!("{source_locator}#line={}", index + 1),
                    text: text.to_owned(),
                })
            })
            .collect();

        self.documents.insert(id, IndexedDocument { record: record.clone(), fragments });
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
}
