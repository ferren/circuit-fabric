//! The managed-document open gate.
//!
//! [`ProjectStorage::prepare_document_open`] is the only path from a `(project ID, document
//! ID)` pair to an opener request. It reuses the existing authorization record, the scan
//! status, and the SHA-256 integrity gate before any bytes are released, and refuses with an
//! explicit [`DocumentOpenDenial`] otherwise. The request it returns carries only verified
//! managed bytes — never a filesystem path — so opener plugins cannot reach anything else.

use circuitfabric_plugin_api::{DocumentOpenDenial, DocumentOpenerRequest, VerifiedDocumentCopy};

use crate::{ProjectStorage, ProjectStorageError, ScanStatus};

impl ProjectStorage {
    /// Prepares an opener request for one managed document after every gate passes.
    ///
    /// Gates, in order: the request must address this storage's own project (cross-project
    /// opens are refused), the document must exist, be authorized, have passed its content
    /// scan, and its managed copy must still match the recorded SHA-256 hash. Only then are
    /// the verified bytes wrapped into a [`DocumentOpenerRequest`].
    ///
    /// # Errors
    ///
    /// Returns a [`DocumentOpenDenial`] describing the first gate that refused the request.
    pub fn prepare_document_open(
        &self,
        project_id: &str,
        document_id: &str,
    ) -> Result<DocumentOpenerRequest, DocumentOpenDenial> {
        if project_id != self.manifest().project.id {
            return Err(DocumentOpenDenial::CrossProject);
        }
        let index = self.load_document_index().map_err(|_| DocumentOpenDenial::NotFound)?;
        let document = index
            .documents
            .iter()
            .find(|document| document.id == document_id)
            .ok_or(DocumentOpenDenial::NotFound)?;
        if !document.authorized {
            return Err(DocumentOpenDenial::Unauthorized);
        }
        match document.scan_status {
            ScanStatus::Scanned => {}
            ScanStatus::Pending => return Err(DocumentOpenDenial::NotScanned),
            ScanStatus::Rejected => return Err(DocumentOpenDenial::ScanRejected),
        }
        let bytes = self.read_verified_document_content(document).map_err(|error| match error {
            ProjectStorageError::ManagedCopyConflict { .. } => {
                DocumentOpenDenial::IntegrityMismatch
            }
            _ => DocumentOpenDenial::MissingManagedCopy,
        })?;
        let managed_copy =
            VerifiedDocumentCopy::from_verified(bytes, document.content_hash.clone())
                .ok_or(DocumentOpenDenial::IntegrityMismatch)?;
        Ok(DocumentOpenerRequest {
            project_id: project_id.to_owned(),
            document_id: document.id.clone(),
            file_name: document.original_file_name.clone(),
            document_kind: document.document_kind,
            content_hash: document.content_hash.clone(),
            source_locator: document.source_locator.clone(),
            managed_copy,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use circuitfabric_contracts::Project;
    use circuitfabric_plugin_api::{DocumentLoadState, DocumentOpenerOutcome};

    use super::*;
    use crate::{DocumentCategory, ProjectStorage};

    fn project(id: &str) -> Project {
        Project { id: id.to_owned(), name: format!("Project {id}"), description: None }
    }

    fn test_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir()
            .join(format!("circuitfabric-project-opening-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn imported_markdown(
        storage: &ProjectStorage,
        root: &std::path::Path,
    ) -> crate::ProjectDocument {
        let source = root.join("notes.md");
        fs::write(&source, "# Notes\nUse a 1uF capacitor.\n").expect("write source");
        storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("import document")
            .document
    }

    #[test]
    fn an_authorized_scanned_document_prepares_a_verified_request() {
        let root = test_root("happy-path");
        let storage = ProjectStorage::create(&root, project("gate")).expect("create project");
        let document = imported_markdown(&storage, &root);

        let request = storage
            .prepare_document_open("gate", &document.id)
            .expect("authorized and scanned document opens");

        assert_eq!(request.project_id, "gate");
        assert_eq!(request.document_id, document.id);
        assert_eq!(request.file_name, "notes.md");
        assert_eq!(request.managed_copy.content_hash(), document.content_hash);
        assert_eq!(request.managed_copy.data(), b"# Notes\nUse a 1uF capacitor.\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_denial_gate_refuses_with_its_reason() {
        let root = test_root("denials");
        let storage = ProjectStorage::create(&root, project("gate")).expect("create project");
        let document = imported_markdown(&storage, &root);

        let cross = storage
            .prepare_document_open("other-project", &document.id)
            .expect_err("a foreign project ID is refused");
        assert_eq!(cross, DocumentOpenDenial::CrossProject);

        let unknown = storage
            .prepare_document_open("gate", "doc-does-not-exist")
            .expect_err("an unknown document ID is refused");
        assert_eq!(unknown, DocumentOpenDenial::NotFound);

        storage.set_document_authorized(&document.id, false).expect("revoke authorization");
        assert_eq!(
            storage.prepare_document_open("gate", &document.id),
            Err(DocumentOpenDenial::Unauthorized)
        );
        storage.set_document_authorized(&document.id, true).expect("grant authorization");

        storage.set_document_scan_status(&document.id, ScanStatus::Pending).expect("mark pending");
        assert_eq!(
            storage.prepare_document_open("gate", &document.id),
            Err(DocumentOpenDenial::NotScanned)
        );
        storage
            .set_document_scan_status(&document.id, ScanStatus::Rejected)
            .expect("mark rejected");
        assert_eq!(
            storage.prepare_document_open("gate", &document.id),
            Err(DocumentOpenDenial::ScanRejected)
        );
        storage.set_document_scan_status(&document.id, ScanStatus::Scanned).expect("mark scanned");

        fs::remove_file(storage.root().join(&document.relative_path)).expect("remove copy");
        assert_eq!(
            storage.prepare_document_open("gate", &document.id),
            Err(DocumentOpenDenial::MissingManagedCopy)
        );
        fs::write(storage.root().join(&document.relative_path), "tampered").expect("tamper copy");
        assert_eq!(
            storage.prepare_document_open("gate", &document.id),
            Err(DocumentOpenDenial::IntegrityMismatch)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_document_of_one_project_cannot_be_opened_through_another() {
        let alpha_root = test_root("cross-alpha");
        let beta_root = test_root("cross-beta");
        let alpha = ProjectStorage::create(&alpha_root, project("alpha")).expect("alpha project");
        let beta = ProjectStorage::create(&beta_root, project("beta")).expect("beta project");
        let document = imported_markdown(&alpha, &alpha_root);

        assert_eq!(
            beta.prepare_document_open("beta", &document.id),
            Err(DocumentOpenDenial::NotFound),
            "the document ID is unknown in the other project's index"
        );
        assert_eq!(
            beta.prepare_document_open("alpha", &document.id),
            Err(DocumentOpenDenial::CrossProject),
            "addressing project alpha through beta's storage is refused"
        );
        let _ = fs::remove_dir_all(&alpha_root);
        let _ = fs::remove_dir_all(&beta_root);
    }

    /// End-to-end: the gate's request feeds a real opener registry (dev-dependency) and comes
    /// back as a loaded read-only view.
    #[test]
    fn a_prepared_request_loads_through_the_builtin_opener_registry() {
        let root = test_root("registry");
        let storage = ProjectStorage::create(&root, project("gate")).expect("create project");
        let document = imported_markdown(&storage, &root);

        let registry =
            circuitfabric_document_opener::DocumentOpenerRegistry::with_builtin_openers();
        let request = storage.prepare_document_open("gate", &document.id).expect("gate passes");
        let outcome = registry.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = &outcome else {
            panic!("markdown must load, got {outcome:?}");
        };
        assert_eq!(outcome.load_state(), DocumentLoadState::Loaded);
        assert_eq!(view.document_id, document.id);
        assert_eq!(view.content_hash, document.content_hash);
        assert_eq!(view.title, "notes", "the display title strips the extension");
        let _ = fs::remove_dir_all(&root);
    }
}
