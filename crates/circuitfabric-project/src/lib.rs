//! Project-scoped resource organization for `CircuitFabric`.
//!
//! [`ProjectStorage`] owns the persistent workspace layout inside a user-selected root
//! (documents, sessions, logic, schematics), while [`ProjectWorkspace`] layers the in-memory
//! project boundary — configuration, document authorization, and project-scoped evidence
//! retrieval — on top of that persisted state. The desktop control plane, EDA bridge, and
//! agent runtime share the same authorization boundary from the outset.

use std::{collections::BTreeMap, path::Path};

use circuitfabric_contracts::{
    DatasheetExtraction, DocumentFragment, DocumentKind, DocumentRecord, EvidencePackage, Project,
    ProjectId,
};
use circuitfabric_document::{DocumentError, DocumentService};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod changesets;
mod datasheets;
mod directories;
mod documents;
mod opening;
mod sessions;
mod storage;

pub use changesets::{
    ChangeSetAuditEntry, ChangeSetEvidenceRef, ChangeSetExecutionReport, ChangeSetStage,
    ChangeSetStageStatus, StoredChangeSet,
};
pub use circuitfabric_document::{
    EvidenceCorpus, EvidenceHit, EvidenceScope, EvidenceSearch, FragmentAnchor,
};
pub use directories::{DocumentConsistencyReport, DocumentDirectoryNode, DocumentDirectoryTree};
pub use documents::{
    DATASHEETS_DIRECTORY_ID, DOCUMENT_INDEX_SCHEMA_VERSION, DocumentCategory, DocumentDirectory,
    DocumentImport, ProjectDocument, REFERENCE_DESIGNS_DIRECTORY_ID, ScanStatus,
    classify_document_kind, is_evidence_indexable, is_text_extractable,
};
pub use sessions::{
    SESSION_MARKDOWN_SCHEMA_VERSION, SessionActor, SessionCategory, SessionEvent, SessionEventKind,
    SessionListing, SessionMetadata, SessionReplay, SessionSeed, SessionStatus, SessionSummary,
    SessionUsage, rfc3339,
};
pub use storage::{
    CHANGESET_SCHEMA_VERSION, PROJECT_REGISTRY_SCHEMA_VERSION, PROJECT_STORAGE_SCHEMA_VERSION,
    ProjectLayoutDiagnostics, ProjectManifest, ProjectRegistry, ProjectRegistryEntry,
    ProjectStorage, ProjectStorageError,
};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("a project identifier cannot be empty")]
    EmptyProjectId,
    #[error("a project name cannot be empty")]
    EmptyProjectName,
    #[error("project `{0}` already exists")]
    AlreadyExists(ProjectId),
    #[error("project `{0}` does not exist")]
    NotFound(ProjectId),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Storage(#[from] ProjectStorageError),
}

/// Configuration deliberately owned by one project rather than the global runtime.
///
/// Runtime connection details and provider credentials belong to the application-wide runtime
/// settings. This contract contains only the allow-lists and instructions that can change from
/// one design workspace to another.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfiguration {
    pub agent_instructions: Option<String>,
    pub enabled_skill_ids: Vec<String>,
    pub enabled_mcp_server_ids: Vec<String>,
}

#[derive(Default)]
pub struct ProjectWorkspace {
    projects: BTreeMap<ProjectId, Project>,
    configurations: BTreeMap<ProjectId, ProjectConfiguration>,
    documents: DocumentService,
}

impl ProjectWorkspace {
    /// Creates a project that owns its documents, agent settings, and EDA sessions.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier or name is empty, or when the identifier is already
    /// registered.
    pub fn create_project(&mut self, project: Project) -> Result<(), ProjectError> {
        if project.id.trim().is_empty() {
            return Err(ProjectError::EmptyProjectId);
        }
        if project.name.trim().is_empty() {
            return Err(ProjectError::EmptyProjectName);
        }
        if self.projects.contains_key(&project.id) {
            return Err(ProjectError::AlreadyExists(project.id));
        }

        self.projects.insert(project.id.clone(), project);
        Ok(())
    }

    #[must_use]
    pub fn project(&self, project_id: &str) -> Option<&Project> {
        self.projects.get(project_id)
    }

    #[must_use]
    pub fn projects(&self) -> Vec<&Project> {
        self.projects.values().collect()
    }

    /// Returns configuration scoped to this project only.
    #[must_use]
    pub fn configuration(&self, project_id: &str) -> Option<&ProjectConfiguration> {
        self.configurations.get(project_id)
    }

    /// Replaces configuration scoped to an existing project.
    ///
    /// This intentionally has no access to global runtime endpoint or provider settings.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] when the project does not exist.
    pub fn set_configuration(
        &mut self,
        project_id: &str,
        configuration: ProjectConfiguration,
    ) -> Result<(), ProjectError> {
        self.require_project(project_id)?;
        self.configurations.insert(project_id.to_owned(), configuration);
        Ok(())
    }

    /// Registers extracted text only after its target project has been authorized.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project, or propagates document
    /// registration errors.
    pub fn register_document_text(
        &mut self,
        project_id: &str,
        id: String,
        kind: DocumentKind,
        title: String,
        source_locator: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<DocumentRecord, ProjectError> {
        self.require_project(project_id)?;
        Ok(self.documents.register_text(
            project_id.to_owned(),
            id,
            kind,
            title,
            source_locator,
            text,
        )?)
    }

    /// Retrieves citation-ready fragments from documents authorized by exactly one project.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] when the project is unknown.
    pub fn retrieve_document_evidence(
        &self,
        project_id: &str,
        query: &str,
    ) -> Result<EvidencePackage, ProjectError> {
        self.require_project(project_id)?;
        Ok(self.documents.retrieve(project_id, query))
    }

    /// Snapshots the citable fragments of exactly one project for a ranked search that can
    /// run off the UI thread ([`EvidenceCorpus::search`]).
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] when the project is unknown.
    pub fn evidence_corpus(&self, project_id: &str) -> Result<EvidenceCorpus, ProjectError> {
        self.require_project(project_id)?;
        Ok(self.documents.corpus(project_id))
    }

    /// Returns whether an authorized document passed integrity verification and is citable in
    /// this project: its full text is indexed, or verified datasheet rows are registered.
    #[must_use]
    pub fn is_document_evidence_available(&self, project_id: &str, document_id: &str) -> bool {
        self.documents.is_indexed(project_id, document_id)
            || self.documents.has_verified_fragments(project_id, document_id)
    }

    /// Returns whether verified datasheet rows of this document are registered as evidence.
    #[must_use]
    pub fn has_verified_datasheet_evidence(&self, project_id: &str, document_id: &str) -> bool {
        self.documents.has_verified_fragments(project_id, document_id)
    }

    /// Authorized, scanned PDFs of this project whose full text is not indexed yet.
    ///
    /// Their text is extracted off the UI thread with [`extract_pdf_pages`] and then
    /// registered through [`Self::register_pdf_pages`].
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project, or propagates storage errors.
    pub fn pending_pdf_documents(
        &self,
        project_id: &str,
        storage: &ProjectStorage,
    ) -> Result<Vec<ProjectDocument>, ProjectError> {
        self.require_project(project_id)?;
        Ok(storage
            .list_documents()?
            .into_iter()
            .filter(|document| {
                document.document_kind == DocumentKind::Pdf
                    && document.authorized
                    && document.scan_status == ScanStatus::Scanned
                    && !self.documents.is_indexed(project_id, &document.id)
            })
            .collect())
    }

    /// Registers the page texts produced by [`extract_pdf_pages`] as citable fragments
    /// (`#page=<n>&line=<m>`). Registering an already indexed document is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project.
    pub fn register_pdf_pages(
        &mut self,
        project_id: &str,
        document: &ProjectDocument,
        pages: &[String],
    ) -> Result<(), ProjectError> {
        self.require_project(project_id)?;
        match self.documents.register_pages(
            project_id.to_owned(),
            document.id.clone(),
            document.document_kind,
            document.original_file_name.clone(),
            document.relative_path.to_string_lossy().replace('\\', "/"),
            pages,
        ) {
            Ok(_) | Err(DocumentError::AlreadyExists(_)) => Ok(()),
        }
    }

    /// Registers the verified rows of a datasheet extraction as evidence, replacing any
    /// earlier rows of this document, and returns how many were registered.
    ///
    /// Only rows carrying their verified source line are used, and only that line becomes
    /// the fragment text (cited as `#datasheet=<section>/<row>`); derived cells never do.
    /// An extraction that does not match the document's current content hash withdraws the
    /// document's rows instead. Callers are responsible for the managed copy's integrity.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project.
    pub fn register_datasheet_evidence(
        &mut self,
        project_id: &str,
        document: &ProjectDocument,
        extraction: &DatasheetExtraction,
    ) -> Result<usize, ProjectError> {
        self.require_project(project_id)?;
        let fragments = if extraction.document_id == document.id
            && extraction.content_hash == document.content_hash
        {
            datasheet_fragments(document, extraction)
        } else {
            Vec::new()
        };
        let count = fragments.len();
        self.documents.set_verified_fragments(project_id, &document.id, fragments);
        Ok(count)
    }

    /// Withdraws the verified datasheet rows of one document.
    pub fn clear_datasheet_evidence(&mut self, project_id: &str, document_id: &str) {
        self.documents.set_verified_fragments(project_id, document_id, Vec::new());
    }

    /// Imports a user-selected file as an authorized document of this project's root and
    /// registers its extractable text for evidence retrieval.
    ///
    /// The managed copy and its index record are written by [`ProjectStorage`]; provenance is
    /// the original absolute path of the source file. Re-importing the same file returns the
    /// existing record with `created: false` instead of adding a new one.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project, or propagates storage errors.
    pub fn import_project_document(
        &mut self,
        project_id: &str,
        storage: &ProjectStorage,
        source: impl AsRef<Path>,
        category: DocumentCategory,
    ) -> Result<DocumentImport, ProjectError> {
        self.import_project_document_into(project_id, storage, source, category, None)
    }

    /// Imports a user-selected file into a chosen directory of the project's `documents/` tree.
    ///
    /// `directory_id: None` places the copy in the category's built-in directory; `Some(id)`
    /// must name a directory created through [`ProjectStorage::create_document_directory`].
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project or directory, or propagates
    /// storage errors.
    pub fn import_project_document_into(
        &mut self,
        project_id: &str,
        storage: &ProjectStorage,
        source: impl AsRef<Path>,
        category: DocumentCategory,
        directory_id: Option<&str>,
    ) -> Result<DocumentImport, ProjectError> {
        self.require_project(project_id)?;
        let source_locator = source.as_ref().display().to_string();
        let imported =
            storage.import_document_into(&source, category, directory_id, source_locator)?;
        self.register_indexed_text(project_id, storage, &imported.document);
        Ok(imported)
    }

    /// Rehydrates evidence retrieval from a project's persisted, authorized documents.
    ///
    /// Idempotent: documents whose text is already registered are counted as available rather
    /// than re-registered. Returns the number of documents whose text is currently available
    /// for citation within this project.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::NotFound`] for an unknown project, or propagates storage errors.
    pub fn hydrate_project_documents(
        &mut self,
        project_id: &str,
        storage: &ProjectStorage,
    ) -> Result<usize, ProjectError> {
        self.require_project(project_id)?;
        let mut available = 0;
        for document in storage.list_documents()? {
            // Authorization and a completed content scan are both prerequisites for a
            // document to act as an evidence source.
            if !document.authorized || document.scan_status != ScanStatus::Scanned {
                continue;
            }
            let text = self.register_indexed_text(project_id, storage, &document);
            let rows = self.register_stored_datasheet_rows(project_id, storage, &document)?;
            if text || rows {
                available += 1;
            }
        }
        Ok(available)
    }

    /// Registers one indexed document's text; `false` means no citable text is available.
    fn register_indexed_text(
        &mut self,
        project_id: &str,
        storage: &ProjectStorage,
        document: &ProjectDocument,
    ) -> bool {
        if !is_text_extractable(&document.document_kind) {
            return false;
        }
        // An unreadable or non-UTF-8 copy simply has no citable text; the index record keeps
        // the document visible with its hash and provenance.
        // Hash verification is deliberately part of the indexing boundary.  A document whose
        // managed copy was changed after authorization is retained in the registry for audit,
        // but cannot be surfaced through evidence retrieval.
        let Ok(bytes) = storage.read_verified_document_content(document) else {
            return false;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return false;
        };
        matches!(
            self.documents.register_text(
                project_id.to_owned(),
                document.id.clone(),
                document.document_kind,
                document.original_file_name.clone(),
                document.relative_path.to_string_lossy().replace('\\', "/"),
                text,
            ),
            Ok(_) | Err(DocumentError::AlreadyExists(_))
        )
    }

    /// Registers the verified rows of a persisted extraction; `false` when there is none,
    /// it is stale, or the managed copy fails verification.
    fn register_stored_datasheet_rows(
        &mut self,
        project_id: &str,
        storage: &ProjectStorage,
        document: &ProjectDocument,
    ) -> Result<bool, ProjectError> {
        let Ok(Some(extraction)) = storage.load_datasheet_extraction(&document.id) else {
            return Ok(false);
        };
        if storage.read_verified_document_content(document).is_err() {
            return Ok(false);
        }
        Ok(self.register_datasheet_evidence(project_id, document, &extraction)? > 0)
    }

    fn require_project(&self, project_id: &str) -> Result<(), ProjectError> {
        self.projects
            .contains_key(project_id)
            .then_some(())
            .ok_or_else(|| ProjectError::NotFound(project_id.to_owned()))
    }
}

/// Extracts the per-page text of an authorized PDF from its hash-verified managed copy.
///
/// CPU-heavy (seconds for large datasheets), so call it off the UI thread and hand the
/// result to [`ProjectWorkspace::register_pdf_pages`]. `None` when the document is not an
/// authorized, scanned PDF, fails verification, has no readable text, or cannot be parsed;
/// parser panics on malformed files are contained.
#[must_use]
pub fn extract_pdf_pages(
    storage: &ProjectStorage,
    document: &ProjectDocument,
) -> Option<Vec<String>> {
    if document.document_kind != DocumentKind::Pdf
        || !document.authorized
        || document.scan_status != ScanStatus::Scanned
    {
        return None;
    }
    let bytes = storage.read_verified_document_content(document).ok()?;
    circuitfabric_document_opener::extract_pdf_text_pages(&bytes).ok()
}

/// One fragment per extraction row that carries its verified source line.
fn datasheet_fragments(
    document: &ProjectDocument,
    extraction: &DatasheetExtraction,
) -> Vec<DocumentFragment> {
    let locator = document.relative_path.to_string_lossy().replace('\\', "/");
    let parameters = |rows: &[circuitfabric_contracts::DatasheetParameter]| {
        rows.iter().map(|row| row.evidence.clone()).collect::<Vec<_>>()
    };
    let sections = [
        ("pins", extraction.pins.iter().map(|row| row.evidence.clone()).collect()),
        ("absoluteMaximumRatings", parameters(&extraction.absolute_maximum_ratings)),
        ("electricalCharacteristics", parameters(&extraction.electrical_characteristics)),
        ("operatingConditions", parameters(&extraction.operating_conditions)),
    ];
    sections
        .into_iter()
        .flat_map(|(section, lines)| {
            let locator = &locator;
            lines.into_iter().enumerate().filter_map(move |(index, line)| {
                let text = line?.trim().to_owned();
                (!text.is_empty()).then(|| DocumentFragment {
                    document_id: document.id.clone(),
                    content_hash: extraction.content_hash.clone(),
                    locator: format!("{locator}#datasheet={section}/{}", index + 1),
                    text,
                })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use circuitfabric_contracts::DocumentKind;

    use super::*;

    fn project(id: &str) -> Project {
        Project { id: id.to_owned(), name: format!("Project {id}"), description: None }
    }

    fn test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("circuitfabric-project-workspace-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    #[test]
    fn a_document_must_belong_to_an_existing_project() {
        let mut workspace = ProjectWorkspace::default();

        let error = workspace
            .register_document_text(
                "missing",
                "document-1".to_owned(),
                DocumentKind::Markdown,
                "Datasheet notes".to_owned(),
                "notes.md".to_owned(),
                "Use a 1uF capacitor.".to_owned(),
            )
            .expect_err("unknown project is rejected");

        assert!(matches!(error, ProjectError::NotFound(id) if id == "missing"));
    }

    #[test]
    fn evidence_is_retrieved_only_within_its_project() {
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("alpha")).expect("new project");
        workspace.create_project(project("beta")).expect("new project");
        workspace
            .register_document_text(
                "alpha",
                "alpha-datasheet".to_owned(),
                DocumentKind::Markdown,
                "Alpha datasheet".to_owned(),
                "alpha.md".to_owned(),
                "The regulator requires a 1uF capacitor.".to_owned(),
            )
            .expect("authorized document");

        let alpha =
            workspace.retrieve_document_evidence("alpha", "capacitor").expect("existing project");
        let beta =
            workspace.retrieve_document_evidence("beta", "capacitor").expect("existing project");

        assert_eq!(alpha.fragments.len(), 1);
        assert!(beta.fragments.is_empty());
    }

    #[test]
    fn projects_have_unique_identifiers() {
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("alpha")).expect("new project");

        let error =
            workspace.create_project(project("alpha")).expect_err("duplicate project must fail");

        assert!(matches!(error, ProjectError::AlreadyExists(id) if id == "alpha"));
    }

    #[test]
    fn configuration_is_scoped_to_its_project() {
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("alpha")).expect("new project");
        workspace.create_project(project("beta")).expect("new project");

        workspace
            .set_configuration(
                "alpha",
                ProjectConfiguration {
                    enabled_skill_ids: vec!["document-search".to_owned()],
                    ..ProjectConfiguration::default()
                },
            )
            .expect("existing project");

        assert_eq!(
            workspace.configuration("alpha").expect("alpha configuration").enabled_skill_ids,
            ["document-search"]
        );
        assert!(workspace.configuration("beta").is_none());
    }

    #[test]
    fn configuration_requires_an_existing_project() {
        let error = ProjectWorkspace::default()
            .set_configuration("missing", ProjectConfiguration::default())
            .expect_err("unknown projects cannot receive configuration");

        assert!(matches!(error, ProjectError::NotFound(id) if id == "missing"));
    }

    #[test]
    fn imported_documents_stay_retrievable_across_a_restart() {
        let root = test_root("evidence-persist");
        let storage = ProjectStorage::create(&root, project("power-supply")).expect("project");
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("power-supply")).expect("project");
        let source = root.join("incoming-notes.md");
        fs::write(&source, "# LM317\nThe regulator requires a 1uF capacitor.\n")
            .expect("write source");

        let document = workspace
            .import_project_document("power-supply", &storage, &source, DocumentCategory::Datasheet)
            .expect("import")
            .document;
        assert_eq!(document.document_kind, DocumentKind::Markdown);
        let evidence =
            workspace.retrieve_document_evidence("power-supply", "capacitor").expect("retrieve");
        assert_eq!(evidence.fragments.len(), 1);
        assert_eq!(evidence.fragments[0].document_id, document.id);

        // Re-importing the very same file must not add a second record.
        let again = workspace
            .import_project_document("power-supply", &storage, &source, DocumentCategory::Datasheet)
            .expect("re-import");
        assert!(!again.created);
        assert_eq!(again.document, document);
        assert_eq!(storage.list_documents().expect("list").len(), 1);

        // Simulate an application restart: a fresh workspace hydrates from the persisted index.
        let mut restarted = ProjectWorkspace::default();
        restarted.create_project(project("power-supply")).expect("project");
        let reopened = ProjectStorage::open(&root).expect("reopen");
        let available =
            restarted.hydrate_project_documents("power-supply", &reopened).expect("hydrate");
        assert_eq!(available, 1);
        let evidence =
            restarted.retrieve_document_evidence("power-supply", "capacitor").expect("retrieve");
        assert_eq!(evidence.fragments.len(), 1);
        assert_eq!(evidence.fragments[0].document_id, document.id);
        assert!(
            evidence.fragments[0]
                .locator
                .starts_with(&document.relative_path.to_string_lossy().replace('\\', "/"))
        );
        // Hydration is idempotent.
        assert_eq!(
            restarted.hydrate_project_documents("power-supply", &reopened).expect("hydrate"),
            1
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn non_text_documents_are_indexed_without_citable_fragments() {
        let root = test_root("evidence-binary");
        let storage = ProjectStorage::create(&root, project("binary")).expect("project");
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("binary")).expect("project");
        let source = root.join("schematic.pdf");
        fs::write(&source, b"%PDF-1.4 fake").expect("write source");

        let document = workspace
            .import_project_document("binary", &storage, &source, DocumentCategory::Datasheet)
            .expect("import")
            .document;

        assert_eq!(document.document_kind, DocumentKind::Pdf);
        assert_eq!(workspace.hydrate_project_documents("binary", &storage).expect("hydrate"), 0);
        let evidence = workspace.retrieve_document_evidence("binary", "PDF").expect("retrieve");
        assert!(evidence.fragments.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// A small but structurally valid single-page PDF with the given text lines.
    fn minimal_pdf(lines: &[&str]) -> Vec<u8> {
        use std::fmt::Write as _;
        let mut content = String::from("BT /F1 12 Tf 72 720 Td\n");
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                content.push_str("0 -18 Td\n");
            }
            let _ = writeln!(content, "({line}) Tj");
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

    #[test]
    fn pdf_full_text_is_extracted_and_cited_by_page() {
        let root = test_root("evidence-pdf");
        let storage = ProjectStorage::create(&root, project("pdf")).expect("project");
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("pdf")).expect("project");
        let source = root.join("lm317.pdf");
        fs::write(&source, minimal_pdf(&["LM317 regulator", "Requires a 1uF capacitor"]))
            .expect("write source");
        let document = workspace
            .import_project_document("pdf", &storage, &source, DocumentCategory::Datasheet)
            .expect("import")
            .document;
        assert!(!workspace.is_document_evidence_available("pdf", &document.id));

        let pending = workspace.pending_pdf_documents("pdf", &storage).expect("pending");
        assert_eq!(pending, std::slice::from_ref(&document));
        let pages = extract_pdf_pages(&storage, &document).expect("readable PDF text");
        workspace.register_pdf_pages("pdf", &document, &pages).expect("register pages");

        assert!(workspace.is_document_evidence_available("pdf", &document.id));
        assert!(workspace.pending_pdf_documents("pdf", &storage).expect("pending").is_empty());
        let evidence = workspace.retrieve_document_evidence("pdf", "capacitor").expect("retrieve");
        assert_eq!(evidence.fragments.len(), 1);
        assert!(evidence.fragments[0].locator.contains("#page=1&line="));

        fs::write(storage.root().join(&document.relative_path), b"tampered").expect("tamper");
        assert!(extract_pdf_pages(&storage, &document).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verified_datasheet_rows_become_evidence_and_survive_a_restart() {
        use circuitfabric_contracts::{
            DATASHEET_EXTRACTION_SCHEMA_VERSION, DatasheetOverview, DatasheetParameter,
            DatasheetPin, DatasheetPinKind,
        };
        let root = test_root("evidence-datasheet");
        let storage = ProjectStorage::create(&root, project("rows")).expect("project");
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("rows")).expect("project");
        let source = root.join("lm317.pdf");
        fs::write(&source, b"%PDF-1.4 fixture").expect("write source");
        let document = workspace
            .import_project_document("rows", &storage, &source, DocumentCategory::Datasheet)
            .expect("import")
            .document;
        let extraction = DatasheetExtraction {
            schema_version: DATASHEET_EXTRACTION_SCHEMA_VERSION,
            document_id: document.id.clone(),
            content_hash: document.content_hash.clone(),
            extracted_at_unix_seconds: 1,
            overview: DatasheetOverview::default(),
            pins: vec![DatasheetPin {
                number: "1".to_owned(),
                name: "VIN".to_owned(),
                kind: DatasheetPinKind::Power,
                description: "Power supply input".to_owned(),
                evidence: Some("1 VIN Power supply input".to_owned()),
            }],
            absolute_maximum_ratings: vec![DatasheetParameter {
                parameter: "Unverified legacy row".to_owned(),
                ..DatasheetParameter::default()
            }],
            electrical_characteristics: Vec::new(),
            operating_conditions: Vec::new(),
            notes: Vec::new(),
        };

        assert_eq!(
            workspace.register_datasheet_evidence("rows", &document, &extraction).expect("rows"),
            1
        );
        assert!(workspace.has_verified_datasheet_evidence("rows", &document.id));
        assert!(workspace.is_document_evidence_available("rows", &document.id));
        let evidence = workspace.retrieve_document_evidence("rows", "vin").expect("retrieve");
        assert_eq!(evidence.fragments.len(), 1);
        assert_eq!(evidence.fragments[0].text, "1 VIN Power supply input");
        assert!(evidence.fragments[0].locator.ends_with("#datasheet=pins/1"));
        assert!(
            workspace.retrieve_document_evidence("rows", "legacy").unwrap().fragments.is_empty()
        );

        let mut stale = extraction.clone();
        stale.content_hash = "sha256:other".to_owned();
        assert_eq!(workspace.register_datasheet_evidence("rows", &document, &stale).unwrap(), 0);
        assert!(!workspace.is_document_evidence_available("rows", &document.id));

        storage.save_datasheet_extraction(&extraction).expect("persist extraction");
        let mut restarted = ProjectWorkspace::default();
        restarted.create_project(project("rows")).expect("project");
        assert_eq!(restarted.hydrate_project_documents("rows", &storage).expect("hydrate"), 1);
        assert!(restarted.has_verified_datasheet_evidence("rows", &document.id));
        restarted.clear_datasheet_evidence("rows", &document.id);
        assert!(!restarted.is_document_evidence_available("rows", &document.id));

        fs::write(storage.root().join(&document.relative_path), b"tampered").expect("tamper");
        let mut tampered = ProjectWorkspace::default();
        tampered.create_project(project("rows")).expect("project");
        assert_eq!(tampered.hydrate_project_documents("rows", &storage).expect("hydrate"), 0);
        assert!(!tampered.has_verified_datasheet_evidence("rows", &document.id));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tampered_managed_copy_is_never_rehydrated_as_evidence() {
        let root = test_root("evidence-tamper");
        let storage = ProjectStorage::create(&root, project("tamper")).expect("project");
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("tamper")).expect("project");
        let source = root.join("reference.md");
        fs::write(&source, "The reference requires a 1uF capacitor.").expect("write source");
        let document = workspace
            .import_project_document("tamper", &storage, &source, DocumentCategory::Datasheet)
            .expect("import")
            .document;

        fs::write(storage.root().join(&document.relative_path), "tampered contents")
            .expect("tamper managed copy");

        let mut restarted = ProjectWorkspace::default();
        restarted.create_project(project("tamper")).expect("project");
        assert_eq!(restarted.hydrate_project_documents("tamper", &storage).expect("hydrate"), 0);
        assert!(!restarted.is_document_evidence_available("tamper", &document.id));
        assert!(
            restarted
                .retrieve_document_evidence("tamper", "capacitor")
                .expect("retrieve")
                .fragments
                .is_empty()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_same_document_imported_by_two_projects_stays_isolated() {
        let alpha_root = test_root("isolate-alpha");
        let beta_root = test_root("isolate-beta");
        let alpha_storage = ProjectStorage::create(&alpha_root, project("alpha")).expect("project");
        let beta_storage = ProjectStorage::create(&beta_root, project("beta")).expect("project");
        let mut workspace = ProjectWorkspace::default();
        workspace.create_project(project("alpha")).expect("project");
        workspace.create_project(project("beta")).expect("project");
        let source = alpha_root.join("shared.md");
        fs::write(&source, "Shared reference design mentions a 1uF capacitor.").expect("write");

        workspace
            .import_project_document("alpha", &alpha_storage, &source, DocumentCategory::Datasheet)
            .expect("import alpha");
        workspace
            .import_project_document("beta", &beta_storage, &source, DocumentCategory::Datasheet)
            .expect("import beta");

        assert_eq!(
            workspace
                .retrieve_document_evidence("alpha", "capacitor")
                .expect("alpha")
                .fragments
                .len(),
            1
        );
        assert_eq!(
            workspace
                .retrieve_document_evidence("beta", "capacitor")
                .expect("beta")
                .fragments
                .len(),
            1
        );
        let corpus = workspace.evidence_corpus("alpha").expect("alpha corpus");
        assert_eq!(corpus.fragment_count(), 1);
        let found = corpus.search("1uf capacitor", EvidenceScope::All, |_| true, 10);
        assert_eq!(found.hits[0].anchor, FragmentAnchor::Line { line: 1 });
        assert!(workspace.evidence_corpus("missing").is_err());
        assert_ne!(alpha_storage.root(), beta_storage.root());
        let _ = fs::remove_dir_all(&alpha_root);
        let _ = fs::remove_dir_all(&beta_root);
    }
}
