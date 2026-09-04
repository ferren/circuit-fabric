//! Project-scoped resource organization for `CircuitFabric`.
//!
//! [`ProjectStorage`] owns the persistent workspace layout inside a user-selected root
//! (documents, sessions, logic, schematics), while [`ProjectWorkspace`] layers the in-memory
//! project boundary — configuration, document authorization, and project-scoped evidence
//! retrieval — on top of that persisted state. The desktop control plane, EDA bridge, and
//! agent runtime share the same authorization boundary from the outset.

use std::{collections::BTreeMap, path::Path};

use circuitfabric_contracts::{DocumentKind, DocumentRecord, EvidencePackage, Project, ProjectId};
use circuitfabric_document::{DocumentError, DocumentService};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod documents;
mod sessions;
mod storage;

pub use documents::{
    DocumentCategory, ProjectDocument, classify_document_kind, is_text_extractable,
};
pub use sessions::{
    SESSION_MARKDOWN_SCHEMA_VERSION, SessionActor, SessionEvent, SessionEventKind, SessionListing,
    SessionMetadata, SessionReplay, SessionSeed, SessionStatus, SessionSummary, SessionUsage,
    rfc3339,
};
pub use storage::{
    PROJECT_REGISTRY_SCHEMA_VERSION, PROJECT_STORAGE_SCHEMA_VERSION, ProjectLayoutDiagnostics,
    ProjectManifest, ProjectRegistry, ProjectRegistryEntry, ProjectStorage, ProjectStorageError,
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

    /// Imports a user-selected file as an authorized document of this project's root and
    /// registers its extractable text for evidence retrieval.
    ///
    /// The managed copy and its index record are written by [`ProjectStorage`]; provenance is
    /// the original absolute path of the source file.
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
    ) -> Result<ProjectDocument, ProjectError> {
        self.require_project(project_id)?;
        let source_locator = source.as_ref().display().to_string();
        let document = storage.import_document(&source, category, source_locator)?;
        self.register_indexed_text(project_id, storage, &document);
        Ok(document)
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
            if document.authorized && self.register_indexed_text(project_id, storage, &document) {
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
        let Ok(bytes) = storage.read_document_content(document) else {
            return false;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return false;
        };
        matches!(
            self.documents.register_text(
                project_id.to_owned(),
                document.id.clone(),
                document.document_kind.clone(),
                document.original_file_name.clone(),
                document.relative_path.to_string_lossy().replace('\\', "/"),
                text,
            ),
            Ok(_) | Err(DocumentError::AlreadyExists(_))
        )
    }

    fn require_project(&self, project_id: &str) -> Result<(), ProjectError> {
        self.projects
            .contains_key(project_id)
            .then_some(())
            .ok_or_else(|| ProjectError::NotFound(project_id.to_owned()))
    }
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
            .expect("import");
        assert_eq!(document.document_kind, DocumentKind::Markdown);
        let evidence =
            workspace.retrieve_document_evidence("power-supply", "capacitor").expect("retrieve");
        assert_eq!(evidence.fragments.len(), 1);
        assert_eq!(evidence.fragments[0].document_id, document.id);

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
            .expect("import");

        assert_eq!(document.document_kind, DocumentKind::Pdf);
        assert_eq!(workspace.hydrate_project_documents("binary", &storage).expect("hydrate"), 0);
        let evidence = workspace.retrieve_document_evidence("binary", "PDF").expect("retrieve");
        assert!(evidence.fragments.is_empty());
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
        assert_ne!(alpha_storage.root(), beta_storage.root());
        let _ = fs::remove_dir_all(&alpha_root);
        let _ = fs::remove_dir_all(&beta_root);
    }
}
