//! Persistent structured datasheet extractions.
//!
//! One extraction lives beside the document index as
//! `.circuitfabric/datasheets/<document-id>.json`, written atomically. Saving refuses
//! extractions whose content hash no longer matches the indexed document, so a stored
//! projection can never masquerade as current data; loading keeps the check with the
//! caller (the extraction carries its hash for comparison).

use std::{
    fs,
    path::{Path, PathBuf},
};

use circuitfabric_contracts::{DATASHEET_EXTRACTION_SCHEMA_VERSION, DatasheetExtraction};

use crate::ProjectStorageError;

impl crate::ProjectStorage {
    /// Persists one datasheet extraction atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when the document is unknown, the extraction's content hash no
    /// longer matches the indexed document, the schema version is unsupported, or the
    /// file cannot be written.
    pub fn save_datasheet_extraction(
        &self,
        extraction: &DatasheetExtraction,
    ) -> Result<(), ProjectStorageError> {
        if extraction.schema_version != DATASHEET_EXTRACTION_SCHEMA_VERSION {
            return Err(ProjectStorageError::UnsupportedDatasheetExtractionSchema {
                path: self.datasheet_extraction_path(&extraction.document_id)?,
                found: extraction.schema_version,
                expected: DATASHEET_EXTRACTION_SCHEMA_VERSION,
            });
        }
        let index = self.load_document_index()?;
        let document = index
            .documents
            .iter()
            .find(|document| document.id == extraction.document_id)
            .ok_or_else(|| ProjectStorageError::DocumentNotFound {
                id: extraction.document_id.clone(),
            })?;
        if document.content_hash != extraction.content_hash {
            return Err(ProjectStorageError::DatasheetExtractionStale {
                document_id: extraction.document_id.clone(),
            });
        }
        let path = self.datasheet_extraction_path(&extraction.document_id)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ProjectStorageError::Io {
                action: "create datasheet extraction directory",
                path: parent.to_owned(),
                source,
            })?;
        }
        crate::ProjectStorage::write_json_atomically(&path, extraction)
    }

    /// Loads the persisted extraction for one document, if any.
    ///
    /// Staleness (content hash drift since extraction) is deliberately not hidden: the
    /// returned extraction carries its hash so callers can compare it with the live
    /// document record and prompt for a re-extraction.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is unreadable, unparsable, or uses an unsupported
    /// schema version. A missing file is `Ok(None)`.
    pub fn load_datasheet_extraction(
        &self,
        document_id: &str,
    ) -> Result<Option<DatasheetExtraction>, ProjectStorageError> {
        let path = self.datasheet_extraction_path(document_id)?;
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).map_err(|source| ProjectStorageError::Io {
            action: "read datasheet extraction",
            path: path.clone(),
            source,
        })?;
        let extraction = serde_json::from_str::<DatasheetExtraction>(&raw).map_err(|source| {
            ProjectStorageError::ParseDatasheetExtraction { path: path.clone(), source }
        })?;
        if extraction.schema_version != DATASHEET_EXTRACTION_SCHEMA_VERSION {
            return Err(ProjectStorageError::UnsupportedDatasheetExtractionSchema {
                path,
                found: extraction.schema_version,
                expected: DATASHEET_EXTRACTION_SCHEMA_VERSION,
            });
        }
        Ok(Some(extraction))
    }

    /// Removes a stored extraction for a document in this project. The managed document and
    /// its index record are left intact, so the data can be extracted again.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or unknown document ID, an escaping sidecar path,
    /// or a filesystem failure other than an already missing extraction.
    pub fn clear_datasheet_extraction(
        &self,
        document_id: &str,
    ) -> Result<bool, ProjectStorageError> {
        let path = self.datasheet_extraction_path(document_id)?;
        let index = self.load_document_index()?;
        if !index.documents.iter().any(|document| document.id == document_id) {
            return Err(ProjectStorageError::DocumentNotFound { id: document_id.to_owned() });
        }
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(source) => {
                Err(ProjectStorageError::Io { action: "clear datasheet extraction", path, source })
            }
        }
    }

    fn datasheet_extraction_path(&self, document_id: &str) -> Result<PathBuf, ProjectStorageError> {
        let valid = !document_id.is_empty()
            && document_id.len() <= 64
            && document_id
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphanumeric())
            && document_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            });
        if !valid {
            return Err(ProjectStorageError::InvalidDatasheetDocumentId {
                id: document_id.to_owned(),
            });
        }
        self.resolve_relative_path(
            Path::new(".circuitfabric/datasheets").join(format!("{document_id}.json")),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use circuitfabric_contracts::{
        DATASHEET_EXTRACTION_SCHEMA_VERSION, DatasheetExtraction, DatasheetOverview,
    };

    use super::*;
    use crate::{DocumentCategory, ProjectStorage};

    fn project(id: &str) -> circuitfabric_contracts::Project {
        circuitfabric_contracts::Project {
            id: id.to_owned(),
            name: format!("Project {id}"),
            description: None,
        }
    }

    fn test_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir()
            .join(format!("circuitfabric-project-datasheets-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn imported_document(
        storage: &ProjectStorage,
        root: &std::path::Path,
    ) -> crate::ProjectDocument {
        let source = root.join("lm317.pdf");
        fs::write(&source, b"%PDF-1.4 fixture").expect("write source");
        storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("import document")
            .document
    }

    fn extraction_for(document: &crate::ProjectDocument) -> DatasheetExtraction {
        DatasheetExtraction {
            schema_version: DATASHEET_EXTRACTION_SCHEMA_VERSION,
            document_id: document.id.clone(),
            content_hash: document.content_hash.clone(),
            extracted_at_unix_seconds: 1,
            overview: DatasheetOverview {
                title: "LM317".to_owned(),
                part_numbers: vec!["LM317".to_owned()],
                manufacturer: None,
                packages: Vec::new(),
                features: Vec::new(),
                description: String::new(),
            },
            pins: Vec::new(),
            absolute_maximum_ratings: Vec::new(),
            electrical_characteristics: Vec::new(),
            operating_conditions: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[test]
    fn extractions_round_trip_and_survive_a_restart() {
        let root = test_root("round-trip");
        let storage = ProjectStorage::create(&root, project("ds")).expect("create project");
        let document = imported_document(&storage, &root);

        storage.save_datasheet_extraction(&extraction_for(&document)).expect("save extraction");

        let loaded = storage
            .load_datasheet_extraction(&document.id)
            .expect("load extraction")
            .expect("extraction exists");
        assert_eq!(loaded.document_id, document.id);
        assert_eq!(loaded.overview.title, "LM317");
        assert!(
            storage
                .root()
                .join(format!(".circuitfabric/datasheets/{}.json", document.id))
                .is_file()
        );

        let reopened = ProjectStorage::open(&root).expect("reopen project");
        let reloaded = reopened
            .load_datasheet_extraction(&document.id)
            .expect("load after restart")
            .expect("extraction persists");
        assert_eq!(reloaded, loaded);
        assert!(
            reopened
                .load_datasheet_extraction("doc-missing")
                .expect("load missing document")
                .is_none()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clearing_then_reextracting_replaces_the_persisted_result() {
        let root = test_root("clear-and-reextract");
        let storage = ProjectStorage::create(&root, project("refresh")).expect("create project");
        let document = imported_document(&storage, &root);
        storage.save_datasheet_extraction(&extraction_for(&document)).expect("save old result");

        let mut direct_refresh = extraction_for(&document);
        direct_refresh.overview.title = "LM317 updated directly".to_owned();
        storage.save_datasheet_extraction(&direct_refresh).expect("overwrite old result");
        assert_eq!(
            storage.load_datasheet_extraction(&document.id).expect("load overwritten result"),
            Some(direct_refresh)
        );

        assert!(storage.clear_datasheet_extraction(&document.id).expect("clear old result"));
        assert!(
            storage.load_datasheet_extraction(&document.id).expect("load cleared result").is_none()
        );
        assert!(
            !storage.clear_datasheet_extraction(&document.id).expect("clear already absent result")
        );
        assert!(
            storage
                .load_document_index()
                .expect("load index")
                .documents
                .iter()
                .any(|entry| entry.id == document.id)
        );

        let mut refreshed = extraction_for(&document);
        refreshed.overview.title = "LM317 refreshed".to_owned();
        storage.save_datasheet_extraction(&refreshed).expect("save refreshed result");
        let reopened = ProjectStorage::open(&root).expect("reopen project");
        assert_eq!(
            reopened.load_datasheet_extraction(&document.id).expect("load refreshed result"),
            Some(refreshed)
        );
        assert!(matches!(
            storage.clear_datasheet_extraction("../escape"),
            Err(ProjectStorageError::InvalidDatasheetDocumentId { .. })
        ));
        assert!(matches!(
            storage.clear_datasheet_extraction("doc-foreign"),
            Err(ProjectStorageError::DocumentNotFound { .. })
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn saving_refuses_stale_and_unknown_documents() {
        let root = test_root("stale");
        let storage = ProjectStorage::create(&root, project("stale")).expect("create project");
        let document = imported_document(&storage, &root);

        let mut stale = extraction_for(&document);
        stale.content_hash = "sha256:different".to_owned();
        assert!(matches!(
            storage.save_datasheet_extraction(&stale),
            Err(ProjectStorageError::DatasheetExtractionStale { .. })
        ));

        let mut unknown = extraction_for(&document);
        unknown.document_id = "doc-does-not-exist".to_owned();
        assert!(matches!(
            storage.save_datasheet_extraction(&unknown),
            Err(ProjectStorageError::DocumentNotFound { .. })
        ));

        let mut future = extraction_for(&document);
        future.schema_version = DATASHEET_EXTRACTION_SCHEMA_VERSION + 1;
        assert!(matches!(
            storage.save_datasheet_extraction(&future),
            Err(ProjectStorageError::UnsupportedDatasheetExtractionSchema { .. })
        ));

        assert!(matches!(
            storage.load_datasheet_extraction("../escape"),
            Err(ProjectStorageError::InvalidDatasheetDocumentId { .. })
        ));
        let _ = fs::remove_dir_all(&root);
    }
}
