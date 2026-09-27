//! Managed document directory service.
//!
//! The `documents/` tree is jointly owned by the filesystem and `document-index.json`. Every
//! mutating operation below keeps both sides in lockstep: it validates the request against the
//! index, moves filesystem entries first, then persists the index atomically and rolls the
//! filesystem back when that commit fails. Path safety reuses
//! [`crate::ProjectStorage::resolve_relative_path`], so traversal components, absolute paths,
//! symlink escapes, and anything outside the project root are rejected before a byte moves.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::documents::DocumentIndex;
use crate::{
    DocumentCategory, DocumentDirectory, ProjectDocument, ProjectStorageError, ScanStatus,
};

/// One directory in the listed tree, with its derived location and direct document count.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDirectoryNode {
    pub directory: DocumentDirectory,
    /// Path relative to the project root with `/` separators, e.g. `documents/datasheets/ti`.
    pub path: String,
    /// Documents whose `directory_id` is exactly this directory.
    pub document_count: usize,
}

/// The full directory tree of a project's managed `documents/` root.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDirectoryTree {
    /// Directories ordered by their derived path.
    pub directories: Vec<DocumentDirectoryNode>,
    /// All indexed documents, whatever directory they live in.
    pub documents: Vec<ProjectDocument>,
}

/// Read-only verification that the filesystem and `document-index.json` still agree.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentConsistencyReport {
    /// Directory records whose derived path is missing (or unsafe) on disk.
    pub missing_directories: Vec<String>,
    /// Documents whose managed copy is missing (or unsafe) on disk.
    pub missing_documents: Vec<String>,
    /// Documents whose managed copy does not live in their recorded directory.
    pub membership_mismatches: Vec<String>,
}

impl DocumentConsistencyReport {
    /// Whether the persisted tree and the filesystem agree.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.missing_directories.is_empty()
            && self.missing_documents.is_empty()
            && self.membership_mismatches.is_empty()
    }
}

/// Where one import places its managed copy.
pub(crate) struct DocumentImportTarget {
    pub directory_id: Option<String>,
    pub relative_directory: PathBuf,
}

/// Resolves the import destination: an explicit directory record, or the category's built-in
/// directory when none is given.
pub(crate) fn resolve_import_target(
    index: &DocumentIndex,
    category: DocumentCategory,
    directory_id: Option<&str>,
) -> Result<DocumentImportTarget, ProjectStorageError> {
    let paths = directory_paths(&index.directories)?;
    match directory_id {
        None => {
            let relative = paths
                .by_id
                .get(category.system_directory_id())
                .cloned()
                .unwrap_or_else(|| Path::new("documents").join(category.directory()));
            Ok(DocumentImportTarget {
                directory_id: Some(category.system_directory_id().to_owned()),
                relative_directory: relative,
            })
        }
        Some(id) => {
            let relative = paths.by_id.get(id).cloned().ok_or_else(|| {
                ProjectStorageError::DocumentDirectoryNotFound { id: id.to_owned() }
            })?;
            Ok(DocumentImportTarget {
                directory_id: Some(id.to_owned()),
                relative_directory: relative,
            })
        }
    }
}

/// Derived path lookup for every directory record, guarded against parent cycles.
pub(crate) struct DirectoryPaths {
    /// Directory ID to its path relative to the project root, e.g. `documents/datasheets/ti`.
    pub by_id: BTreeMap<String, PathBuf>,
    /// Reverse lookup for membership recovery.
    pub ids_by_path: BTreeMap<PathBuf, String>,
}

/// Computes every directory's derived path; fails on unknown parents or parent cycles.
pub(crate) fn directory_paths(
    directories: &[DocumentDirectory],
) -> Result<DirectoryPaths, ProjectStorageError> {
    let mut by_id = BTreeMap::new();
    let mut ids_by_path = BTreeMap::new();
    for directory in directories {
        let path = directory_path(directories, &directory.id)?;
        by_id.insert(directory.id.clone(), path.clone());
        ids_by_path.insert(path, directory.id.clone());
    }
    Ok(DirectoryPaths { by_id, ids_by_path })
}

/// Path of one directory relative to the project root, built from its name chain.
pub(crate) fn directory_path(
    directories: &[DocumentDirectory],
    directory_id: &str,
) -> Result<PathBuf, ProjectStorageError> {
    let chain = directory_chain(directories, directory_id)?;
    let mut path = PathBuf::from("documents");
    for directory in chain {
        path.push(&directory.name);
    }
    Ok(path)
}

/// Walks from a directory up to the `documents/` root, returning the chain root-first.
fn directory_chain<'a>(
    directories: &'a [DocumentDirectory],
    directory_id: &str,
) -> Result<Vec<&'a DocumentDirectory>, ProjectStorageError> {
    let mut chain = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = directory_id;
    loop {
        if !visited.insert(current) {
            return Err(ProjectStorageError::DocumentDirectoryCycle {
                id: directory_id.to_owned(),
            });
        }
        let directory =
            directories.iter().find(|directory| directory.id == current).ok_or_else(|| {
                ProjectStorageError::DocumentDirectoryNotFound { id: current.to_owned() }
            })?;
        chain.push(directory);
        match directory.parent_id.as_deref() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    chain.reverse();
    Ok(chain)
}

/// A directory plus all of its descendants.
fn descendant_directory_ids(
    directories: &[DocumentDirectory],
    directory_id: &str,
) -> Result<BTreeSet<String>, ProjectStorageError> {
    // Every record must be reachable without cycles before we walk children.
    directory_paths(directories)?;
    let mut subtree = BTreeSet::from([directory_id.to_owned()]);
    let mut grew = true;
    while grew {
        grew = false;
        for directory in directories {
            if let Some(parent) = directory.parent_id.as_deref()
                && subtree.contains(parent)
                && subtree.insert(directory.id.clone())
            {
                grew = true;
            }
        }
    }
    Ok(subtree)
}

fn find_directory<'a>(
    directories: &'a [DocumentDirectory],
    directory_id: &str,
) -> Result<&'a DocumentDirectory, ProjectStorageError> {
    directories.iter().find(|directory| directory.id == directory_id).ok_or_else(|| {
        ProjectStorageError::DocumentDirectoryNotFound { id: directory_id.to_owned() }
    })
}

/// Next free `dir-<n>` identifier; system records use non-numeric suffixes and never collide.
fn next_directory_id(directories: &[DocumentDirectory]) -> String {
    let next = directories
        .iter()
        .filter_map(|directory| directory.id.strip_prefix("dir-"))
        .filter_map(|suffix| suffix.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("dir-{next}")
}

/// Whether two names would collide on a case-insensitive filesystem.
fn names_collide(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validates one directory path segment.
///
/// Trims surrounding whitespace and rejects empty, over-long (more than 64 characters),
/// traversal, separator-bearing, control-character, Windows-reserved, and dot-anchored names.
fn validate_directory_name(name: &str) -> Result<String, ProjectStorageError> {
    let trimmed = name.trim();
    let invalid = |reason: &'static str| ProjectStorageError::InvalidDocumentDirectoryName {
        name: name.to_owned(),
        reason,
    };
    if trimmed.is_empty() {
        return Err(invalid("the name cannot be empty"));
    }
    if trimmed.chars().count() > 64 {
        return Err(invalid("the name is longer than 64 characters"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(invalid("the name cannot be a dot segment"));
    }
    if trimmed.chars().any(|character| {
        character.is_control()
            || matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    }) {
        return Err(invalid("the name contains an unsupported character"));
    }
    if trimmed.starts_with('.') || trimmed.ends_with('.') {
        return Err(invalid("the name cannot start or end with a dot"));
    }
    if WINDOWS_RESERVED_NAMES.contains(&trimmed.to_ascii_uppercase().as_str()) {
        return Err(invalid("the name is reserved by Windows"));
    }
    Ok(trimmed.to_owned())
}

/// Validates a display file name for a managed document (never used as a path).
fn validate_document_file_name(name: &str) -> Result<String, ProjectStorageError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ProjectStorageError::InvalidDocumentFileName { name: name.to_owned() });
    }
    if trimmed.chars().count() > 255
        || trimmed == "."
        || trimmed == ".."
        || trimmed.chars().any(|character| {
            character.is_control()
                || matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
    {
        return Err(ProjectStorageError::InvalidDocumentFileName { name: name.to_owned() });
    }
    Ok(trimmed.to_owned())
}

fn portable(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Recomputes every document's managed-copy path from its recorded directory membership.
fn recompute_document_paths(index: &mut DocumentIndex) -> Result<(), ProjectStorageError> {
    let paths = directory_paths(&index.directories)?;
    for document in &mut index.documents {
        let Some(directory_id) = document.directory_id.as_ref() else { continue };
        let Some(directory) = paths.by_id.get(directory_id) else { continue };
        let Some(file_name) = document.relative_path.file_name() else { continue };
        document.relative_path = directory.join(file_name);
    }
    Ok(())
}

/// Refuses a directory move that would strand a managed copy outside its recorded directory.
fn reject_misplaced_copies(
    index: &DocumentIndex,
    subtree_ids: &BTreeSet<String>,
    subtree_path: &Path,
) -> Result<(), ProjectStorageError> {
    for document in &index.documents {
        let directory_in_subtree =
            document.directory_id.as_ref().is_some_and(|id| subtree_ids.contains(id));
        let path_in_subtree = document.relative_path.starts_with(subtree_path);
        if directory_in_subtree != path_in_subtree {
            return Err(ProjectStorageError::ManagedCopyMisplaced {
                document_id: document.id.clone(),
            });
        }
    }
    Ok(())
}

impl crate::ProjectStorage {
    /// Lists the managed directory tree together with all indexed documents.
    ///
    /// Directory membership of every document is part of the same index, so the tree and the
    /// documents always come from one consistent snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the index cannot be read or its directory records are cyclic.
    pub fn list_document_directory_tree(
        &self,
    ) -> Result<DocumentDirectoryTree, ProjectStorageError> {
        let index = self.load_document_index()?;
        let paths = directory_paths(&index.directories)?;
        let mut nodes: Vec<DocumentDirectoryNode> = index
            .directories
            .iter()
            .map(|directory| {
                let path = paths
                    .by_id
                    .get(&directory.id)
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from("documents"));
                let document_count = index
                    .documents
                    .iter()
                    .filter(|document| {
                        document.directory_id.as_deref() == Some(directory.id.as_str())
                    })
                    .count();
                DocumentDirectoryNode {
                    directory: directory.clone(),
                    path: portable(&path),
                    document_count,
                }
            })
            .collect();
        nodes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(DocumentDirectoryTree { directories: nodes, documents: index.documents })
    }

    /// Creates a subdirectory under `documents/`, or under `parent_id` when given.
    ///
    /// The filesystem entry is created first and removed again when the index commit fails, so
    /// the two never disagree in the persisted state.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe or colliding names, unknown parents, paths that escape the
    /// project root, or when the index cannot be read or written.
    pub fn create_document_directory(
        &self,
        parent_id: Option<&str>,
        name: &str,
    ) -> Result<DocumentDirectory, ProjectStorageError> {
        let mut index = self.load_document_index()?;
        let name = validate_directory_name(name)?;
        if let Some(parent) = parent_id {
            find_directory(&index.directories, parent)?;
        }
        let parent_path = match parent_id {
            Some(parent) => directory_path(&index.directories, parent)?,
            None => PathBuf::from("documents"),
        };
        let collides = index.directories.iter().any(|directory| {
            directory.parent_id.as_deref() == parent_id && names_collide(&directory.name, &name)
        });
        if collides {
            return Err(ProjectStorageError::DocumentDirectoryNameConflict {
                path: parent_path.join(&name),
            });
        }
        let relative = parent_path.join(&name);
        let absolute = self.resolve_relative_path(&relative)?;
        if absolute.exists() {
            // The name is free in the index but taken on disk (out-of-band creation).
            return Err(ProjectStorageError::DocumentDirectoryNameConflict { path: relative });
        }
        fs::create_dir(&absolute).map_err(|source| ProjectStorageError::Io {
            action: "create document directory",
            path: absolute.clone(),
            source,
        })?;
        let directory = DocumentDirectory {
            id: next_directory_id(&index.directories),
            name,
            parent_id: parent_id.map(ToOwned::to_owned),
            system: false,
            created_at_unix_seconds: now_unix_seconds(),
        };
        index.directories.push(directory.clone());
        if let Err(error) = self.save_document_index(&index) {
            // Best-effort rollback of the directory entry just created.
            let _ = fs::remove_dir(&absolute);
            return Err(error);
        }
        Ok(directory)
    }

    /// Renames a directory, moving its filesystem entry and updating every descendant document
    /// path in one atomic index commit.
    ///
    /// # Errors
    ///
    /// Returns an error for system directories, unsafe or colliding names, misplaced managed
    /// copies, or when the filesystem move or index commit fails (the move is rolled back).
    pub fn rename_document_directory(
        &self,
        directory_id: &str,
        new_name: &str,
    ) -> Result<DocumentDirectory, ProjectStorageError> {
        let mut index = self.load_document_index()?;
        let directory = find_directory(&index.directories, directory_id)?.clone();
        if directory.system {
            return Err(ProjectStorageError::SystemDirectoryImmutable { id: directory.id.clone() });
        }
        let new_name = validate_directory_name(new_name)?;
        let old_relative = directory_path(&index.directories, directory_id)?;
        let Some(parent_path) = old_relative.parent().map(Path::to_owned) else {
            return Err(ProjectStorageError::DocumentDirectoryCycle {
                id: directory_id.to_owned(),
            });
        };
        let collides = index.directories.iter().any(|sibling| {
            sibling.id != directory_id
                && sibling.parent_id == directory.parent_id
                && names_collide(&sibling.name, &new_name)
        });
        if collides {
            return Err(ProjectStorageError::DocumentDirectoryNameConflict {
                path: parent_path.join(&new_name),
            });
        }
        let subtree_ids = descendant_directory_ids(&index.directories, directory_id)?;
        reject_misplaced_copies(&index, &subtree_ids, &old_relative)?;
        let new_relative = parent_path.join(&new_name);
        let old_absolute = self.resolve_relative_path(&old_relative)?;
        let new_absolute = self.resolve_relative_path(&new_relative)?;
        if new_absolute.exists() {
            return Err(ProjectStorageError::DocumentDirectoryNameConflict { path: new_relative });
        }
        fs::rename(&old_absolute, &new_absolute).map_err(|source| ProjectStorageError::Io {
            action: "rename document directory",
            path: old_absolute.clone(),
            source,
        })?;
        for record in &mut index.directories {
            if record.id == directory_id {
                record.name.clone_from(&new_name);
            }
        }
        recompute_document_paths(&mut index)?;
        if let Err(error) = self.save_document_index(&index) {
            // Roll the filesystem entry back to match the still-authoritative old index.
            let _ = fs::rename(&new_absolute, &old_absolute);
            return Err(error);
        }
        Ok(DocumentDirectory { name: new_name, ..directory })
    }

    /// Moves a directory (with everything inside it) under a new parent directory.
    ///
    /// `new_parent_id: None` anchors it directly under `documents/`. Moving a directory into
    /// itself or one of its descendants is rejected, as is moving a system directory.
    ///
    /// # Errors
    ///
    /// Returns an error for system directories, cycles, unknown parents, colliding names,
    /// misplaced managed copies, or when the filesystem move or index commit fails.
    pub fn move_document_directory(
        &self,
        directory_id: &str,
        new_parent_id: Option<&str>,
    ) -> Result<DocumentDirectory, ProjectStorageError> {
        let mut index = self.load_document_index()?;
        let directory = find_directory(&index.directories, directory_id)?.clone();
        if directory.system {
            return Err(ProjectStorageError::SystemDirectoryImmutable { id: directory.id.clone() });
        }
        if new_parent_id == Some(directory_id) {
            return Err(ProjectStorageError::InvalidDirectoryMove {
                id: directory_id.to_owned(),
                reason: "a directory cannot be moved into itself",
            });
        }
        if let Some(parent) = new_parent_id {
            find_directory(&index.directories, parent)?;
        }
        let subtree_ids = descendant_directory_ids(&index.directories, directory_id)?;
        if let Some(parent) = new_parent_id
            && subtree_ids.contains(parent)
        {
            return Err(ProjectStorageError::InvalidDirectoryMove {
                id: directory_id.to_owned(),
                reason: "a directory cannot be moved into its own descendant",
            });
        }
        if directory.parent_id.as_deref() == new_parent_id {
            // Already in place; persisting anything would be a no-op.
            return Ok(directory);
        }
        let old_relative = directory_path(&index.directories, directory_id)?;
        let parent_path = match new_parent_id {
            Some(parent) => directory_path(&index.directories, parent)?,
            None => PathBuf::from("documents"),
        };
        reject_misplaced_copies(&index, &subtree_ids, &old_relative)?;
        let new_relative = parent_path.join(&directory.name);
        let collides = index.directories.iter().any(|sibling| {
            sibling.id != directory_id
                && sibling.parent_id.as_deref() == new_parent_id
                && names_collide(&sibling.name, &directory.name)
        });
        if collides || new_relative == old_relative {
            return Err(ProjectStorageError::DocumentDirectoryNameConflict { path: new_relative });
        }
        let old_absolute = self.resolve_relative_path(&old_relative)?;
        let new_absolute = self.resolve_relative_path(&new_relative)?;
        if new_absolute.exists() {
            return Err(ProjectStorageError::DocumentDirectoryNameConflict { path: new_relative });
        }
        fs::rename(&old_absolute, &new_absolute).map_err(|source| ProjectStorageError::Io {
            action: "move document directory",
            path: old_absolute.clone(),
            source,
        })?;
        for record in &mut index.directories {
            if record.id == directory_id {
                record.parent_id = new_parent_id.map(ToOwned::to_owned);
            }
        }
        recompute_document_paths(&mut index)?;
        if let Err(error) = self.save_document_index(&index) {
            let _ = fs::rename(&new_absolute, &old_absolute);
            return Err(error);
        }
        Ok(DocumentDirectory { parent_id: new_parent_id.map(ToOwned::to_owned), ..directory })
    }

    /// Moves one document's managed copy into another directory of the same project.
    ///
    /// `target_directory_id: None` moves it directly under `documents/`. When other records
    /// share the managed copy, the bytes are copied instead of renamed so the remaining
    /// records keep their copy in place.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown documents or directories, escaping paths, conflicting
    /// copies at the destination, or when the filesystem move or index commit fails.
    pub fn move_document(
        &self,
        document_id: &str,
        target_directory_id: Option<&str>,
    ) -> Result<ProjectDocument, ProjectStorageError> {
        let mut index = self.load_document_index()?;
        let position =
            index.documents.iter().position(|document| document.id == document_id).ok_or_else(
                || ProjectStorageError::DocumentNotFound { id: document_id.to_owned() },
            )?;
        if let Some(target) = target_directory_id {
            find_directory(&index.directories, target)?;
        }
        let target_path = match target_directory_id {
            Some(target) => directory_path(&index.directories, target)?,
            None => PathBuf::from("documents"),
        };
        let document = index.documents[position].clone();
        let Some(file_name) = document.relative_path.file_name().map(PathBuf::from) else {
            return Err(ProjectStorageError::DocumentNotFound { id: document_id.to_owned() });
        };
        let new_relative = target_path.join(&file_name);
        if new_relative == document.relative_path {
            index.documents[position].directory_id = target_directory_id.map(ToOwned::to_owned);
            self.save_document_index(&index)?;
            return Ok(index.documents[position].clone());
        }
        let shared = index
            .documents
            .iter()
            .any(|other| other.id != document.id && other.relative_path == document.relative_path);
        let old_absolute = self.resolve_relative_path(&document.relative_path)?;
        let new_absolute = self.resolve_relative_path(&new_relative)?;
        let mut rollback: Option<Box<dyn FnOnce() + Send + Sync>> = None;
        if new_absolute.exists() {
            // Same content-addressed name already lives there; adopt it after verifying.
            self.verify_managed_copy(&new_relative, &document.content_hash)?;
        } else if shared {
            fs::copy(&old_absolute, &new_absolute).map_err(|source| ProjectStorageError::Io {
                action: "copy shared managed document copy",
                path: old_absolute.clone(),
                source,
            })?;
            let copied = new_absolute.clone();
            rollback = Some(Box::new(move || {
                let _ = fs::remove_file(&copied);
            }));
        } else {
            fs::rename(&old_absolute, &new_absolute).map_err(|source| ProjectStorageError::Io {
                action: "move managed document copy",
                path: old_absolute.clone(),
                source,
            })?;
            let (from, to) = (new_absolute.clone(), old_absolute.clone());
            rollback = Some(Box::new(move || {
                let _ = fs::rename(&from, &to);
            }));
        }
        index.documents[position].directory_id = target_directory_id.map(ToOwned::to_owned);
        index.documents[position].relative_path = new_relative;
        if let Err(error) = self.save_document_index(&index) {
            if let Some(rollback) = rollback {
                rollback();
            }
            return Err(error);
        }
        Ok(index.documents[position].clone())
    }

    /// Renames a document's display name (its original file name).
    ///
    /// Managed copies stay content-addressed, so nothing moves on disk; only the provenance
    /// record changes.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown documents or unsafe file names.
    pub fn rename_document(
        &self,
        document_id: &str,
        new_file_name: &str,
    ) -> Result<ProjectDocument, ProjectStorageError> {
        let mut index = self.load_document_index()?;
        let new_file_name = validate_document_file_name(new_file_name)?;
        let document =
            index.documents.iter_mut().find(|document| document.id == document_id).ok_or_else(
                || ProjectStorageError::DocumentNotFound { id: document_id.to_owned() },
            )?;
        document.original_file_name = new_file_name;
        let updated = document.clone();
        self.save_document_index(&index)?;
        Ok(updated)
    }

    /// Grants or revokes a document's authorization.
    ///
    /// Revoked documents stay indexed for audit but can no longer be opened or used as
    /// evidence sources.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown documents or when the index cannot be written.
    pub fn set_document_authorized(
        &self,
        document_id: &str,
        authorized: bool,
    ) -> Result<ProjectDocument, ProjectStorageError> {
        self.update_document(document_id, move |document| document.authorized = authorized)
    }

    /// Records the content-scan outcome for a document.
    ///
    /// Only [`ScanStatus::Scanned`] documents may be opened or indexed as evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown documents or when the index cannot be written.
    pub fn set_document_scan_status(
        &self,
        document_id: &str,
        status: ScanStatus,
    ) -> Result<ProjectDocument, ProjectStorageError> {
        self.update_document(document_id, move |document| document.scan_status = status)
    }

    fn update_document(
        &self,
        document_id: &str,
        apply: impl FnOnce(&mut ProjectDocument),
    ) -> Result<ProjectDocument, ProjectStorageError> {
        let mut index = self.load_document_index()?;
        let document =
            index.documents.iter_mut().find(|document| document.id == document_id).ok_or_else(
                || ProjectStorageError::DocumentNotFound { id: document_id.to_owned() },
            )?;
        apply(document);
        let updated = document.clone();
        self.save_document_index(&index)?;
        Ok(updated)
    }

    /// Verifies that the persisted directory tree, document membership, and managed copies on
    /// disk all agree. Read-only: nothing is repaired.
    ///
    /// # Errors
    ///
    /// Returns an error when the index cannot be read or contains cyclic directory records.
    pub fn check_document_consistency(
        &self,
    ) -> Result<DocumentConsistencyReport, ProjectStorageError> {
        let index = self.load_document_index()?;
        let paths = directory_paths(&index.directories)?;
        let mut report = DocumentConsistencyReport::default();
        for directory in &index.directories {
            let relative = &paths.by_id[&directory.id];
            match self.resolve_relative_path(relative) {
                Ok(absolute) if absolute.is_dir() => {}
                Ok(_) | Err(_) => report.missing_directories.push(directory.id.clone()),
            }
        }
        for document in &index.documents {
            match self.resolve_relative_path(&document.relative_path) {
                Ok(absolute) if absolute.is_file() => {}
                Ok(_) | Err(_) => {
                    report.missing_documents.push(document.id.clone());
                    continue;
                }
            }
            let expected_parent = document
                .directory_id
                .as_ref()
                .map_or_else(|| PathBuf::from("documents"), |id| paths.by_id[id].clone());
            let actual_parent = document
                .relative_path
                .parent()
                .map_or_else(|| PathBuf::from("documents"), ToOwned::to_owned);
            if expected_parent != actual_parent {
                report.membership_mismatches.push(document.id.clone());
            }
        }
        Ok(report)
    }
}

fn now_unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use circuitfabric_contracts::Project;

    use super::*;
    use crate::{DATASHEETS_DIRECTORY_ID, DOCUMENT_INDEX_SCHEMA_VERSION, ProjectStorage};

    fn project(id: &str) -> Project {
        Project { id: id.to_owned(), name: format!("Project {id}"), description: None }
    }

    fn test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("circuitfabric-project-directories-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn write_source(root: &Path, name: &str, contents: &str) -> PathBuf {
        let path = root.join(name);
        fs::write(&path, contents).expect("write source file");
        path
    }

    fn import_markdown(
        storage: &ProjectStorage,
        root: &Path,
        name: &str,
        contents: &str,
        directory_id: Option<&str>,
    ) -> ProjectDocument {
        let source = write_source(root, name, contents);
        storage
            .import_document_into(
                &source,
                DocumentCategory::Datasheet,
                directory_id,
                source.display().to_string(),
            )
            .expect("import document")
            .document
    }

    fn paths_of(tree: &DocumentDirectoryTree) -> Vec<String> {
        tree.directories.iter().map(|node| node.path.clone()).collect()
    }

    /// Replaces a directory with a link pointing outside the project root; junctions need no
    /// privilege on Windows. Returns `false` when links cannot be created in this environment.
    fn replace_with_outside_link(link: &Path, target: &Path) -> bool {
        fs::remove_dir(link).expect("remove directory before linking");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    &link.display().to_string(),
                    &target.display().to_string(),
                ])
                .output()
                .expect("run mklink");
            output.status.success()
        }
    }

    #[test]
    fn nested_directories_survive_a_restart_and_stay_consistent() {
        let root = test_root("nested-restart");
        let storage = ProjectStorage::create(&root, project("nested")).expect("create project");

        let vendor = storage
            .create_document_directory(None, "vendors")
            .expect("create root-level directory");
        let ti = storage
            .create_document_directory(Some(&vendor.id), "ti")
            .expect("create nested directory");
        let deep =
            storage.create_document_directory(Some(&ti.id), "regulators").expect("deep directory");
        let document = import_markdown(&storage, &root, "lm317.md", "# LM317\n", Some(&deep.id));

        let tree = storage.list_document_directory_tree().expect("list tree");
        assert_eq!(
            paths_of(&tree),
            [
                "documents/datasheets",
                "documents/reference-designs",
                "documents/vendors",
                "documents/vendors/ti",
                "documents/vendors/ti/regulators",
            ]
        );
        let ti_path = format!("documents/{}/ti", vendor.name);
        assert!(
            tree.directories.iter().any(|node| node.path == ti_path && node.document_count == 0)
        );
        assert_eq!(document.directory_id.as_deref(), Some(deep.id.as_str()));
        assert!(
            document
                .relative_path
                .starts_with(format!("documents/{}/{}/regulators", vendor.name, ti.name))
        );
        assert!(
            fs::read(storage.root().join(&document.relative_path)).is_ok(),
            "the managed copy lives inside the nested directory"
        );
        assert!(storage.check_document_consistency().expect("consistency").is_consistent());

        // Restart: reopen the same root and verify the tree and index agree.
        let reopened = ProjectStorage::open(&root).expect("reopen project");
        let restarted_tree = reopened.list_document_directory_tree().expect("list tree");
        assert_eq!(paths_of(&restarted_tree), paths_of(&tree));
        assert_eq!(restarted_tree.documents, tree.documents);
        assert!(reopened.check_document_consistency().expect("consistency").is_consistent());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn renaming_a_directory_moves_copies_and_updates_membership() {
        let root = test_root("rename-directory");
        let storage = ProjectStorage::create(&root, project("rename")).expect("create project");
        let vendor = storage.create_document_directory(None, "vendors").expect("directory");
        let document = import_markdown(&storage, &root, "notes.md", "body", Some(&vendor.id));
        let old_path = storage.root().join(&document.relative_path);

        let renamed =
            storage.rename_document_directory(&vendor.id, "suppliers").expect("rename directory");

        assert_eq!(renamed.name, "suppliers");
        assert!(!old_path.exists(), "the old filesystem location is vacated");
        let tree = storage.list_document_directory_tree().expect("list tree");
        assert!(tree.directories.iter().any(|node| node.path == "documents/suppliers"));
        let moved = tree.documents.iter().find(|doc| doc.id == document.id).expect("document");
        assert!(moved.relative_path.starts_with("documents/suppliers"));
        assert!(storage.check_document_consistency().expect("consistency").is_consistent());
        storage.read_verified_document_content(moved).expect("verified copy follows the move");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn moving_a_directory_reparents_it_and_rejects_cycles_and_system_directories() {
        let root = test_root("move-directory");
        let storage = ProjectStorage::create(&root, project("move")).expect("create project");
        let first = storage.create_document_directory(None, "alpha").expect("first");
        let second = storage.create_document_directory(None, "beta").expect("second");
        let child = storage.create_document_directory(Some(&first.id), "child").expect("child");
        let document = import_markdown(&storage, &root, "doc.md", "body", Some(&child.id));

        let moved = storage
            .move_document_directory(&first.id, Some(&second.id))
            .expect("move under another directory");

        assert_eq!(moved.parent_id.as_deref(), Some(second.id.as_str()));
        let tree = storage.list_document_directory_tree().expect("list tree");
        assert!(tree.directories.iter().any(|node| node.path == "documents/beta/alpha/child"));
        assert!(
            tree.documents
                .iter()
                .any(|doc| doc.relative_path.starts_with("documents/beta/alpha/child"))
        );
        assert!(storage.check_document_consistency().expect("consistency").is_consistent());

        assert!(matches!(
            storage.move_document_directory(&second.id, Some(&moved.id)),
            Err(ProjectStorageError::InvalidDirectoryMove { .. })
        ));
        assert!(matches!(
            storage.move_document_directory(&first.id, Some(&first.id)),
            Err(ProjectStorageError::InvalidDirectoryMove { .. })
        ));
        assert!(matches!(
            storage.rename_document_directory(DATASHEETS_DIRECTORY_ID, "renamed"),
            Err(ProjectStorageError::SystemDirectoryImmutable { .. })
        ));
        assert!(matches!(
            storage.move_document_directory(DATASHEETS_DIRECTORY_ID, Some(&second.id)),
            Err(ProjectStorageError::SystemDirectoryImmutable { .. })
        ));
        let _ = document;
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn documents_move_between_directories_without_breaking_shares() {
        let root = test_root("move-document");
        let storage = ProjectStorage::create(&root, project("move-doc")).expect("create project");
        let inbox = storage.create_document_directory(None, "inbox").expect("inbox");
        let archive = storage.create_document_directory(None, "archive").expect("archive");
        // Two records with identical content in the same directory share one managed copy.
        let first = import_markdown(&storage, &root, "a.md", "same bytes", Some(&inbox.id));
        let second = import_markdown(&storage, &root, "b.md", "same bytes", Some(&inbox.id));
        assert_eq!(first.relative_path, second.relative_path, "one copy is shared");

        let moved = storage.move_document(&first.id, Some(&archive.id)).expect("move first record");

        assert_eq!(moved.directory_id.as_deref(), Some(archive.id.as_str()));
        assert!(moved.relative_path.starts_with("documents/archive"));
        assert!(storage.root().join(&second.relative_path).is_file(), "the shared copy stays");
        assert!(storage.root().join(&moved.relative_path).is_file(), "the moved copy exists");
        storage.read_verified_document_content(&moved).expect("moved copy verifies");
        let untouched = storage
            .list_documents()
            .expect("list")
            .into_iter()
            .find(|document| document.id == second.id)
            .expect("second record");
        assert!(untouched.relative_path.starts_with("documents/inbox"));
        assert!(storage.check_document_consistency().expect("consistency").is_consistent());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn renaming_a_document_changes_only_its_display_name() {
        let root = test_root("rename-document");
        let storage = ProjectStorage::create(&root, project("rename-doc")).expect("create project");
        let document = import_markdown(&storage, &root, "draft.md", "body", None);
        let blob = document.relative_path.clone();

        let renamed = storage.rename_document(&document.id, "final-report.md").expect("rename");

        assert_eq!(renamed.original_file_name, "final-report.md");
        assert_eq!(renamed.relative_path, blob, "managed copies stay content-addressed");
        assert!(matches!(
            storage.rename_document(&document.id, "../escape.md"),
            Err(ProjectStorageError::InvalidDocumentFileName { .. })
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unsafe_and_colliding_directory_names_are_rejected() {
        let root = test_root("name-safety");
        let storage = ProjectStorage::create(&root, project("names")).expect("create project");

        for name in ["", "  ", "..", ".", "a/b", "a\\b", "trailing.", "CON", "a*b", "nul\x00"] {
            assert!(
                matches!(
                    storage.create_document_directory(None, name),
                    Err(ProjectStorageError::InvalidDocumentDirectoryName { .. })
                ),
                "`{name}` must be rejected as a directory name"
            );
        }
        let long: String = std::iter::repeat_n('x', 65).collect();
        assert!(matches!(
            storage.create_document_directory(None, &long),
            Err(ProjectStorageError::InvalidDocumentDirectoryName { .. })
        ));

        let created = storage.create_document_directory(None, "Vendors").expect("create");
        assert!(
            matches!(
                storage.create_document_directory(None, "vendors"),
                Err(ProjectStorageError::DocumentDirectoryNameConflict { .. })
            ),
            "names collide case-insensitively on Windows filesystems"
        );
        assert!(matches!(
            storage.create_document_directory(Some("dir-missing"), "child"),
            Err(ProjectStorageError::DocumentDirectoryNotFound { .. })
        ));
        let _ = created;
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_link_that_escapes_the_project_root_cannot_gather_children() {
        let root = test_root("link-escape");
        let outside = test_root("link-escape-outside");
        let storage = ProjectStorage::create(&root, project("links")).expect("create project");
        let directory = storage.create_document_directory(None, "linked").expect("directory");
        let link = storage.root().join("documents").join("linked");

        if !replace_with_outside_link(&link, &outside) {
            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&outside);
            return; // environment cannot create links; the guard is exercised on such hosts only
        }

        let error = storage
            .create_document_directory(Some(&directory.id), "child")
            .expect_err("a directory that resolves outside the root rejects children");
        assert!(
            matches!(error, ProjectStorageError::PathEscapesRoot { .. }),
            "expected a root escape, got {error:?}"
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn a_legacy_version_one_index_migrates_on_load_and_persists_version_two() {
        let root = test_root("v1-migration");
        let storage = ProjectStorage::create(&root, project("legacy")).expect("create project");
        let document = import_markdown(&storage, &root, "old.md", "legacy content", None);

        // Rewrite the index into the legacy version-1 shape: no directories, no membership,
        // no scan status, schemaVersion 1.
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "documents": [{
                "id": document.id,
                "category": "datasheet",
                "originalFileName": "old.md",
                "relativePath": document.relative_path.to_string_lossy().replace('\\', "/"),
                "contentHash": document.content_hash,
                "byteSize": document.byte_size,
                "documentKind": "markdown",
                "sourceLocator": document.source_locator,
                "authorized": true,
                "importedAtUnixSeconds": 1,
            }],
        });
        fs::write(storage.document_index_path(), legacy.to_string()).expect("write v1 index");

        let reopened = ProjectStorage::open(&root).expect("reopen legacy project");
        let tree = reopened.list_document_directory_tree().expect("list tree");
        assert!(
            tree.directories
                .iter()
                .any(|node| node.directory.id == DATASHEETS_DIRECTORY_ID && node.directory.system)
        );
        let migrated = tree.documents.iter().find(|doc| doc.id == document.id).expect("document");
        assert_eq!(migrated.directory_id.as_deref(), Some(DATASHEETS_DIRECTORY_ID));
        assert_eq!(migrated.scan_status, ScanStatus::Scanned);
        assert!(reopened.check_document_consistency().expect("consistency").is_consistent());

        // The next write persists the migrated shape.
        reopened.create_document_directory(None, "post-migration").expect("write through");
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(reopened.document_index_path()).expect("read index"))
                .expect("parse index");
        assert_eq!(raw["schemaVersion"], DOCUMENT_INDEX_SCHEMA_VERSION);
        assert!(raw["directories"].as_array().is_some_and(|directories| !directories.is_empty()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_schema_versions_are_still_rejected() {
        let root = test_root("schema-unknown");
        let storage = ProjectStorage::create(&root, project("schema")).expect("create project");
        fs::write(storage.document_index_path(), r#"{"schemaVersion": 99, "documents": []}"#)
            .expect("write future index");

        assert!(matches!(
            storage.list_documents(),
            Err(ProjectStorageError::UnsupportedDocumentIndexSchema { found: 99, .. })
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_operation_stays_inside_one_project_root() {
        let alpha_root = test_root("isolate-alpha");
        let beta_root = test_root("isolate-beta");
        let alpha = ProjectStorage::create(&alpha_root, project("alpha")).expect("alpha");
        let beta = ProjectStorage::create(&beta_root, project("beta")).expect("beta");
        let directory = alpha.create_document_directory(None, "alpha-only").expect("directory");
        let document = import_markdown(&alpha, &alpha_root, "a.md", "alpha bytes", None);

        // Identifiers from one project never resolve against another project's storage.
        assert!(matches!(
            beta.create_document_directory(Some(&directory.id), "child"),
            Err(ProjectStorageError::DocumentDirectoryNotFound { .. })
        ));
        assert!(matches!(
            beta.move_document(&document.id, Some(&directory.id)),
            Err(ProjectStorageError::DocumentNotFound { .. })
        ));
        assert!(beta.list_document_directory_tree().expect("beta tree").documents.is_empty());
        assert!(
            alpha
                .list_document_directory_tree()
                .expect("alpha tree")
                .documents
                .iter()
                .any(|doc| doc.id == document.id)
        );
        let _ = fs::remove_dir_all(&alpha_root);
        let _ = fs::remove_dir_all(&beta_root);
    }
}
