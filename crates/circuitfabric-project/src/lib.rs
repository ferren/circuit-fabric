//! Project-scoped resource organization for `CircuitFabric`.
//!
//! This first implementation is in-memory. Its API is intentionally independent of the future
//! persistent store so the desktop control plane, EDA bridge, and agent runtime share the same
//! authorization boundary from the outset.

use std::collections::BTreeMap;

use circuitfabric_contracts::{DocumentKind, DocumentRecord, EvidencePackage, Project, ProjectId};
use circuitfabric_document::{DocumentError, DocumentService};
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
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
}

#[derive(Default)]
pub struct ProjectWorkspace {
    projects: BTreeMap<ProjectId, Project>,
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

    fn require_project(&self, project_id: &str) -> Result<(), ProjectError> {
        self.projects
            .contains_key(project_id)
            .then_some(())
            .ok_or_else(|| ProjectError::NotFound(project_id.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::DocumentKind;

    use super::*;

    fn project(id: &str) -> Project {
        Project { id: id.to_owned(), name: format!("Project {id}"), description: None }
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

        assert_eq!(error, ProjectError::NotFound("missing".to_owned()));
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

        assert_eq!(error, ProjectError::AlreadyExists("alpha".to_owned()));
    }
}
