//! Filesystem-backed project workspace storage.
//!
//! The storage layer owns the on-disk project boundary. Callers supply only project roots and
//! validated relative paths; it never accepts an arbitrary destination for managed project data.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use circuitfabric_contracts::{Project, ProjectId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ProjectConfiguration;

/// Version of the persisted workspace manifest and its companion metadata files.
pub const PROJECT_STORAGE_SCHEMA_VERSION: u32 = 1;
/// Version of the application-level project registry document.
pub const PROJECT_REGISTRY_SCHEMA_VERSION: u32 = 1;

const CIRCUITFABRIC_DIRECTORY: &str = ".circuitfabric";
const MANIFEST_FILE: &str = "project.json";
const CONFIGURATION_FILE: &str = "project-config.json";
const DOCUMENT_INDEX_FILE: &str = "document-index.json";

const MANAGED_ROOT_ENTRIES: [&str; 5] =
    [CIRCUITFABRIC_DIRECTORY, "sessions", "documents", "logic", "schematics"];

const REQUIRED_DIRECTORIES: [&str; 10] = [
    CIRCUITFABRIC_DIRECTORY,
    "sessions",
    "documents",
    "documents/datasheets",
    "documents/reference-designs",
    "logic",
    "logic/snapshots",
    "logic/changesets",
    "logic/indexes",
    "schematics",
];

const REQUIRED_FILES: [&str; 3] = [
    ".circuitfabric/project.json",
    ".circuitfabric/project-config.json",
    ".circuitfabric/document-index.json",
];

/// Metadata that identifies one project root.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub project: Project,
    pub created_at_unix_seconds: u64,
}

impl ProjectManifest {
    fn new(project: Project) -> Self {
        Self {
            schema_version: PROJECT_STORAGE_SCHEMA_VERSION,
            project,
            created_at_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentIndex {
    schema_version: u32,
    documents: Vec<DocumentIndexEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectConfigurationDocument {
    schema_version: u32,
    configuration: ProjectConfiguration,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentIndexEntry {
    id: String,
    relative_path: PathBuf,
    content_hash: String,
    source_locator: String,
}

/// A non-mutating report about required paths within an opened project root.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectLayoutDiagnostics {
    pub missing_entries: Vec<PathBuf>,
    pub unsafe_entries: Vec<PathBuf>,
}

impl ProjectLayoutDiagnostics {
    #[must_use]
    pub const fn is_healthy(&self) -> bool {
        self.missing_entries.is_empty() && self.unsafe_entries.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum ProjectStorageError {
    #[error("project root `{path}` does not exist")]
    RootNotFound { path: PathBuf },
    #[error("project root `{path}` is not a directory")]
    RootNotDirectory { path: PathBuf },
    #[error("managed project entry `{path}` already exists")]
    LayoutConflict { path: PathBuf },
    #[error("project manifest is missing at `{path}`")]
    MissingManifest { path: PathBuf },
    #[error("project manifest at `{path}` could not be parsed: {source}")]
    ParseManifest { path: PathBuf, source: serde_json::Error },
    #[error("project manifest schema version {found} is unsupported (expected {expected})")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("project manifest contains an invalid project: {reason}")]
    InvalidProject { reason: &'static str },
    #[error("relative project path `{path}` is unsafe")]
    UnsafeRelativePath { path: PathBuf },
    #[error("project path `{path}` resolves outside project root `{root}`")]
    PathEscapesRoot { path: PathBuf, root: PathBuf },
    #[error("EDA backend ID `{backend_id}` is invalid")]
    InvalidBackendId { backend_id: String },
    #[error("project ID `{project_id}` is already registered at `{root}`")]
    ProjectIdAlreadyRegistered { project_id: ProjectId, root: PathBuf },
    #[error("project root `{root}` is already registered for project `{project_id}`")]
    RootAlreadyRegistered { root: PathBuf, project_id: ProjectId },
    #[error("project `{project_id}` is not registered")]
    ProjectNotRegistered { project_id: ProjectId },
    #[error("cannot read project registry `{path}`: {source}")]
    ReadRegistry { path: PathBuf, source: io::Error },
    #[error("cannot parse project registry `{path}`: {source}")]
    ParseRegistry { path: PathBuf, source: serde_json::Error },
    #[error(
        "project registry `{path}` uses unsupported schema version {found}; expected {expected}"
    )]
    UnsupportedRegistrySchema { path: PathBuf, found: u32, expected: u32 },
    #[error("failed to {action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to serialize `{path}`: {source}")]
    Serialize { path: PathBuf, source: serde_json::Error },
}

/// The only filesystem entry point for a project root.
#[derive(Clone, Debug)]
pub struct ProjectStorage {
    root: PathBuf,
    manifest: ProjectManifest,
}

impl ProjectStorage {
    /// Initializes a project in an existing user-selected directory.
    ///
    /// The root itself is never created or emptied. To avoid merging with user-managed content,
    /// initialization rejects a root that already has any CircuitFabric-managed top-level entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the root is not an existing directory, a managed entry already
    /// exists, the project data is invalid, or a managed directory/metadata file cannot be made.
    pub fn create(root: impl AsRef<Path>, project: Project) -> Result<Self, ProjectStorageError> {
        validate_project(&project)?;
        let root = canonical_existing_directory(root.as_ref())?;
        for entry in MANAGED_ROOT_ENTRIES {
            let path = root.join(entry);
            if path.exists() {
                return Err(ProjectStorageError::LayoutConflict { path });
            }
        }

        let storage = Self { root, manifest: ProjectManifest::new(project) };
        for directory in REQUIRED_DIRECTORIES {
            let path = storage.root.join(directory);
            fs::create_dir_all(&path).map_err(|source| ProjectStorageError::Io {
                action: "create project directory",
                path,
                source,
            })?;
        }
        Self::write_json_atomically(
            &storage.configuration_path(),
            &ProjectConfigurationDocument {
                schema_version: PROJECT_STORAGE_SCHEMA_VERSION,
                configuration: ProjectConfiguration::default(),
            },
        )?;
        Self::write_json_atomically(
            &storage.document_index_path(),
            &DocumentIndex {
                schema_version: PROJECT_STORAGE_SCHEMA_VERSION,
                ..DocumentIndex::default()
            },
        )?;
        Self::write_json_atomically(&storage.manifest_path(), &storage.manifest)?;
        Ok(storage)
    }

    /// Opens an initialized project without modifying its root.
    ///
    /// Use [`Self::diagnose_layout`] after opening to inspect incomplete or unsafe layouts.
    ///
    /// # Errors
    ///
    /// Returns an error when the root or manifest cannot be read, parsed, or validated.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ProjectStorageError> {
        let root = canonical_existing_directory(root.as_ref())?;
        let path = root.join(CIRCUITFABRIC_DIRECTORY).join(MANIFEST_FILE);
        let raw = fs::read_to_string(&path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                ProjectStorageError::MissingManifest { path: path.clone() }
            } else {
                ProjectStorageError::Io {
                    action: "read project manifest",
                    path: path.clone(),
                    source,
                }
            }
        })?;
        let manifest = serde_json::from_str::<ProjectManifest>(&raw)
            .map_err(|source| ProjectStorageError::ParseManifest { path: path.clone(), source })?;
        validate_manifest(&manifest)?;
        Ok(Self { root, manifest })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join(CIRCUITFABRIC_DIRECTORY).join(MANIFEST_FILE)
    }

    #[must_use]
    pub fn configuration_path(&self) -> PathBuf {
        self.root.join(CIRCUITFABRIC_DIRECTORY).join(CONFIGURATION_FILE)
    }

    #[must_use]
    pub fn document_index_path(&self) -> PathBuf {
        self.root.join(CIRCUITFABRIC_DIRECTORY).join(DOCUMENT_INDEX_FILE)
    }

    /// Diagnoses the required layout without creating, deleting, or modifying any path.
    #[must_use]
    pub fn diagnose_layout(&self) -> ProjectLayoutDiagnostics {
        let mut diagnostics = ProjectLayoutDiagnostics::default();
        for entry in REQUIRED_DIRECTORIES.into_iter().chain(REQUIRED_FILES) {
            let relative = PathBuf::from(entry);
            let path = self.root.join(&relative);
            if !path.exists() {
                diagnostics.missing_entries.push(relative);
            } else if self.resolve_relative_path(&relative).is_err() {
                diagnostics.unsafe_entries.push(relative);
            }
        }
        diagnostics
    }

    /// Resolves a path only when it is relative, contains no traversal components, and remains
    /// within this project's canonical root after following any existing symlinks.
    ///
    /// # Errors
    ///
    /// Returns an error for absolute, empty, traversal, or root-escaping paths.
    pub fn resolve_relative_path(
        &self,
        relative_path: impl AsRef<Path>,
    ) -> Result<PathBuf, ProjectStorageError> {
        let relative_path = relative_path.as_ref();
        validate_relative_path(relative_path)?;
        let candidate = self.root.join(relative_path);
        let existing_ancestor = nearest_existing_ancestor(&candidate, &self.root);
        let canonical_ancestor =
            fs::canonicalize(&existing_ancestor).map_err(|source| ProjectStorageError::Io {
                action: "resolve project path",
                path: existing_ancestor.clone(),
                source,
            })?;
        if !canonical_ancestor.starts_with(&self.root) {
            return Err(ProjectStorageError::PathEscapesRoot {
                path: candidate,
                root: self.root.clone(),
            });
        }
        if candidate.exists() {
            let canonical_candidate =
                fs::canonicalize(&candidate).map_err(|source| ProjectStorageError::Io {
                    action: "resolve project path",
                    path: candidate.clone(),
                    source,
                })?;
            if !canonical_candidate.starts_with(&self.root) {
                return Err(ProjectStorageError::PathEscapesRoot {
                    path: candidate,
                    root: self.root.clone(),
                });
            }
        }
        Ok(candidate)
    }

    /// Returns the reserved directory for a registered-style EDA backend identifier.
    ///
    /// It returns a safe path but intentionally does not create it: a later EDA import/export
    /// operation must explicitly decide when to materialize a backend-specific directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend ID is unsafe or the resulting path leaves the root.
    pub fn eda_directory(&self, backend_id: &str) -> Result<PathBuf, ProjectStorageError> {
        if backend_id.is_empty()
            || !backend_id.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '-' | '_')
            })
        {
            return Err(ProjectStorageError::InvalidBackendId {
                backend_id: backend_id.to_owned(),
            });
        }
        self.resolve_relative_path(Path::new("schematics").join(backend_id))
    }

    fn write_json_atomically<T: Serialize>(
        path: &Path,
        value: &T,
    ) -> Result<(), ProjectStorageError> {
        let encoded = serde_json::to_vec_pretty(value)
            .map_err(|source| ProjectStorageError::Serialize { path: path.to_owned(), source })?;
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("metadata");
        let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
        fs::write(&temporary, encoded).map_err(|source| ProjectStorageError::Io {
            action: "write project metadata",
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| ProjectStorageError::Io {
            action: "commit project metadata",
            path: path.to_owned(),
            source,
        })?;
        Ok(())
    }
}

/// A non-secret application-level record for a registered project root.
///
/// The project manifest inside the root remains authoritative. These records only let the
/// desktop restore a list of known roots after a restart.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRegistryEntry {
    pub project_id: ProjectId,
    pub canonical_root_path: PathBuf,
    pub display_name: String,
    pub last_opened_unix_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRegistryDocument {
    schema_version: u32,
    projects: Vec<ProjectRegistryEntry>,
}

/// Persistable application-level index of project roots.
///
/// It enforces one canonical root and one root binding per `ProjectId`, while keeping runtime
/// connection settings and secrets out of this project-scoped data model.
#[derive(Debug, Default)]
pub struct ProjectRegistry {
    roots_by_project_id: BTreeMap<ProjectId, PathBuf>,
    project_ids_by_root: BTreeMap<PathBuf, ProjectId>,
    entries_by_project_id: BTreeMap<ProjectId, ProjectRegistryEntry>,
}

impl ProjectRegistry {
    /// Loads a project registry document.
    ///
    /// # Errors
    ///
    /// Returns an error if the registry cannot be read, parsed, or has an unsupported schema.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectStorageError> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|source| ProjectStorageError::ReadRegistry {
            path: path.to_owned(),
            source,
        })?;
        let document = serde_json::from_str::<ProjectRegistryDocument>(&raw).map_err(|source| {
            ProjectStorageError::ParseRegistry { path: path.to_owned(), source }
        })?;
        if document.schema_version != PROJECT_REGISTRY_SCHEMA_VERSION {
            return Err(ProjectStorageError::UnsupportedRegistrySchema {
                path: path.to_owned(),
                found: document.schema_version,
                expected: PROJECT_REGISTRY_SCHEMA_VERSION,
            });
        }

        let mut registry = Self::default();
        for entry in document.projects {
            registry.register_entry(entry)?;
        }
        Ok(registry)
    }

    /// Loads an existing registry or starts a new empty one if its file is absent.
    ///
    /// # Errors
    ///
    /// Returns errors other than a missing registry file.
    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, ProjectStorageError> {
        match Self::load(path.as_ref()) {
            Ok(registry) => Ok(registry),
            Err(ProjectStorageError::ReadRegistry { source, .. })
                if source.kind() == io::ErrorKind::NotFound =>
            {
                Ok(Self::default())
            }
            Err(error) => Err(error),
        }
    }

    /// Saves the registry atomically at an application-level path.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry directory or file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ProjectStorageError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ProjectStorageError::Io {
                action: "create project registry directory",
                path: parent.to_owned(),
                source,
            })?;
        }
        let document = ProjectRegistryDocument {
            schema_version: PROJECT_REGISTRY_SCHEMA_VERSION,
            projects: self.entries().cloned().collect(),
        };
        ProjectStorage::write_json_atomically(path, &document)
    }

    /// Opens a root and records its canonical project identity in this registry.
    ///
    /// # Errors
    ///
    /// Returns errors from [`ProjectStorage::open`] or when the ID/root is already bound.
    pub fn open_and_register(
        &mut self,
        root: impl AsRef<Path>,
    ) -> Result<ProjectStorage, ProjectStorageError> {
        let storage = ProjectStorage::open(root)?;
        self.register(&storage)?;
        Ok(storage)
    }

    /// Registers an opened storage root after enforcing one-to-one project/root ownership.
    ///
    /// # Errors
    ///
    /// Returns an error if the project ID or canonical root is already registered.
    pub fn register(&mut self, storage: &ProjectStorage) -> Result<(), ProjectStorageError> {
        self.register_entry(ProjectRegistryEntry {
            project_id: storage.manifest().project.id.clone(),
            canonical_root_path: storage.root().to_owned(),
            display_name: storage.manifest().project.name.clone(),
            last_opened_unix_seconds: now_unix_seconds(),
        })
    }

    #[must_use]
    pub fn root_for(&self, project_id: &str) -> Option<&Path> {
        self.roots_by_project_id.get(project_id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn entry_for(&self, project_id: &str) -> Option<&ProjectRegistryEntry> {
        self.entries_by_project_id.get(project_id)
    }

    pub fn entries(&self) -> impl Iterator<Item = &ProjectRegistryEntry> {
        self.entries_by_project_id.values()
    }

    /// Records a successful project open without changing its root binding.
    ///
    /// # Errors
    ///
    /// Returns an error if the project ID is absent from the registry.
    pub fn mark_opened(&mut self, project_id: &str) -> Result<(), ProjectStorageError> {
        let Some(entry) = self.entries_by_project_id.get_mut(project_id) else {
            return Err(ProjectStorageError::ProjectNotRegistered {
                project_id: project_id.to_owned(),
            });
        };
        entry.last_opened_unix_seconds = now_unix_seconds();
        Ok(())
    }

    fn register_entry(&mut self, entry: ProjectRegistryEntry) -> Result<(), ProjectStorageError> {
        let project_id = entry.project_id.clone();
        if let Some(root) = self.roots_by_project_id.get(&project_id) {
            return Err(ProjectStorageError::ProjectIdAlreadyRegistered {
                project_id,
                root: root.clone(),
            });
        }
        if let Some(project_id) = self.project_ids_by_root.get(&entry.canonical_root_path) {
            return Err(ProjectStorageError::RootAlreadyRegistered {
                root: entry.canonical_root_path.clone(),
                project_id: project_id.clone(),
            });
        }
        self.roots_by_project_id.insert(project_id.clone(), entry.canonical_root_path.clone());
        self.project_ids_by_root.insert(entry.canonical_root_path.clone(), project_id.clone());
        self.entries_by_project_id.insert(project_id, entry);
        Ok(())
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn canonical_existing_directory(root: &Path) -> Result<PathBuf, ProjectStorageError> {
    let metadata = fs::metadata(root).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ProjectStorageError::RootNotFound { path: root.to_owned() }
        } else {
            ProjectStorageError::Io {
                action: "inspect project root",
                path: root.to_owned(),
                source,
            }
        }
    })?;
    if !metadata.is_dir() {
        return Err(ProjectStorageError::RootNotDirectory { path: root.to_owned() });
    }
    fs::canonicalize(root).map_err(|source| ProjectStorageError::Io {
        action: "canonicalize project root",
        path: root.to_owned(),
        source,
    })
}

fn validate_project(project: &Project) -> Result<(), ProjectStorageError> {
    if project.id.trim().is_empty() {
        return Err(ProjectStorageError::InvalidProject { reason: "project ID cannot be empty" });
    }
    if project.name.trim().is_empty() {
        return Err(ProjectStorageError::InvalidProject { reason: "project name cannot be empty" });
    }
    Ok(())
}

fn validate_manifest(manifest: &ProjectManifest) -> Result<(), ProjectStorageError> {
    if manifest.schema_version != PROJECT_STORAGE_SCHEMA_VERSION {
        return Err(ProjectStorageError::UnsupportedSchema {
            found: manifest.schema_version,
            expected: PROJECT_STORAGE_SCHEMA_VERSION,
        });
    }
    validate_project(&manifest.project)
}

fn validate_relative_path(path: &Path) -> Result<(), ProjectStorageError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ProjectStorageError::UnsafeRelativePath { path: path.to_owned() });
    }
    Ok(())
}

fn nearest_existing_ancestor<'a>(candidate: &'a Path, root: &'a Path) -> PathBuf {
    let mut ancestor = candidate.to_owned();
    while !ancestor.exists() && ancestor != root {
        if !ancestor.pop() {
            return root.to_owned();
        }
    }
    ancestor
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static TEST_DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    fn project(id: &str) -> Project {
        Project {
            id: id.to_owned(),
            name: format!("Project {id}"),
            description: Some("Persistent workspace test fixture".to_owned()),
        }
    }

    fn test_root(label: &str) -> PathBuf {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "circuitfabric-project-storage-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create test project root");
        root
    }

    fn remove_test_root(root: &Path) {
        fs::remove_dir_all(root).expect("remove test project root");
    }

    #[test]
    fn create_materializes_the_full_layout_and_round_trips_the_manifest() {
        let root = test_root("create");
        let storage =
            ProjectStorage::create(&root, project("power-supply")).expect("create project");

        assert_eq!(storage.manifest().schema_version, PROJECT_STORAGE_SCHEMA_VERSION);
        assert_eq!(storage.manifest().project.id, "power-supply");
        assert!(storage.diagnose_layout().is_healthy());
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(storage.manifest_path()).expect("read project manifest"),
        )
        .expect("parse project manifest");
        let configuration: serde_json::Value = serde_json::from_slice(
            &fs::read(storage.configuration_path()).expect("read project configuration"),
        )
        .expect("parse project configuration");
        assert_eq!(manifest["schemaVersion"], PROJECT_STORAGE_SCHEMA_VERSION);
        assert_eq!(configuration["schemaVersion"], PROJECT_STORAGE_SCHEMA_VERSION);
        for entry in REQUIRED_DIRECTORIES.into_iter().chain(REQUIRED_FILES) {
            assert!(storage.root().join(entry).exists(), "{entry} must exist");
        }

        let reopened = ProjectStorage::open(&root).expect("open created project");
        assert_eq!(reopened.manifest(), storage.manifest());
        remove_test_root(&root);
    }

    #[test]
    fn initialization_refuses_to_merge_with_a_managed_directory() {
        let root = test_root("collision");
        let existing = root.join("documents");
        fs::create_dir(&existing).expect("create conflicting directory");

        let error =
            ProjectStorage::create(&root, project("collision")).expect_err("layout collision");

        assert!(
            matches!(error, ProjectStorageError::LayoutConflict { path } if path.ends_with("documents"))
        );
        assert!(!root.join(CIRCUITFABRIC_DIRECTORY).exists());
        remove_test_root(&root);
    }

    #[test]
    fn diagnostics_are_read_only_for_incomplete_layouts() {
        let root = test_root("diagnostics");
        let storage =
            ProjectStorage::create(&root, project("diagnostics")).expect("create project");
        let missing = storage.root().join("logic/snapshots");
        fs::remove_dir(&missing).expect("remove fixture directory");

        let diagnostics = storage.diagnose_layout();

        assert_eq!(diagnostics.missing_entries, [PathBuf::from("logic/snapshots")]);
        assert!(!missing.exists(), "diagnostics must not repair the layout");
        remove_test_root(&root);
    }

    #[test]
    fn path_resolution_rejects_traversal_and_invalid_backend_ids() {
        let root = test_root("path-safety");
        let storage = ProjectStorage::create(&root, project("safe")).expect("create project");

        assert!(matches!(
            storage.resolve_relative_path("../outside"),
            Err(ProjectStorageError::UnsafeRelativePath { .. })
        ));
        assert!(matches!(
            storage.resolve_relative_path(Path::new("C:\\outside")),
            Err(ProjectStorageError::UnsafeRelativePath { .. })
        ));
        assert!(matches!(
            storage.eda_directory("../jlcircuit"),
            Err(ProjectStorageError::InvalidBackendId { .. })
        ));
        assert_eq!(
            storage.eda_directory("jlcircuit").expect("safe backend directory"),
            storage.root().join("schematics/jlcircuit")
        );
        remove_test_root(&root);
    }

    #[test]
    fn registry_enforces_unique_project_ids_and_roots() {
        let first_root = test_root("registry-first");
        let second_root = test_root("registry-second");
        let first =
            ProjectStorage::create(&first_root, project("shared-id")).expect("first project");
        let second =
            ProjectStorage::create(&second_root, project("shared-id")).expect("second project");
        let mut registry = ProjectRegistry::default();

        registry.register(&first).expect("register first project");
        let duplicate_id = registry.register(&second).expect_err("duplicate project ID");
        let mut rebound_root = first.clone();
        rebound_root.manifest.project.id = "different-id".to_owned();
        let duplicate_root = registry.register(&rebound_root).expect_err("duplicate root");

        assert!(matches!(duplicate_id, ProjectStorageError::ProjectIdAlreadyRegistered { .. }));
        assert!(matches!(duplicate_root, ProjectStorageError::RootAlreadyRegistered { .. }));
        assert_eq!(registry.root_for("shared-id"), Some(first.root()));
        remove_test_root(&first_root);
        remove_test_root(&second_root);
    }

    #[test]
    fn registry_persists_canonical_roots_for_restart_recovery() {
        let project_root = test_root("registry-persist-project");
        let registry_root = test_root("registry-persist-app");
        let registry_path = registry_root.join("projects.json");
        let storage =
            ProjectStorage::create(&project_root, project("persistent-project")).expect("project");
        let mut registry = ProjectRegistry::default();

        registry.register(&storage).expect("register project");
        registry.save(&registry_path).expect("save registry");
        let reopened = ProjectRegistry::load(&registry_path).expect("reload registry");

        let entry = reopened.entry_for("persistent-project").expect("registered entry");
        assert_eq!(entry.canonical_root_path, storage.root());
        assert_eq!(entry.display_name, "Project persistent-project");
        assert_eq!(
            reopened.root_for("persistent-project"),
            Some(storage.root()),
            "registry preserves the canonical root after restart"
        );
        remove_test_root(&project_root);
        remove_test_root(&registry_root);
    }
}
