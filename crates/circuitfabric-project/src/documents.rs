//! Authorized document import and the persistent document index.
//!
//! Imports copy user-selected files into the managed `documents/` tree as content-addressed
//! copies. Provenance (original file name and source locator) lives in
//! `.circuitfabric/document-index.json`; identical content is stored once while each index
//! record keeps its own authorization and source. Directory structure (see [`DocumentDirectory`])
//! is persisted in the same index so the tree survives restarts unchanged.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use circuitfabric_contracts::DocumentKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ProjectStorageError;

/// Schema version of `document-index.json`.
///
/// Version 2 added `directories` and per-document directory membership plus scan status; the
/// loader accepts legacy version-1 indexes and migrates them in memory.
pub const DOCUMENT_INDEX_SCHEMA_VERSION: u32 = 2;

/// Stable ID of the built-in `documents/datasheets` directory record.
pub const DATASHEETS_DIRECTORY_ID: &str = "dir-datasheets";
/// Stable ID of the built-in `documents/reference-designs` directory record.
pub const REFERENCE_DESIGNS_DIRECTORY_ID: &str = "dir-reference-designs";

/// The managed document categories, each mapped to a stable directory under `documents/`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentCategory {
    Datasheet,
    ReferenceDesign,
}

impl DocumentCategory {
    #[must_use]
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Datasheet => "datasheets",
            Self::ReferenceDesign => "reference-designs",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Datasheet => "datasheet",
            Self::ReferenceDesign => "reference-design",
        }
    }

    #[must_use]
    pub const fn system_directory_id(self) -> &'static str {
        match self {
            Self::Datasheet => DATASHEETS_DIRECTORY_ID,
            Self::ReferenceDesign => REFERENCE_DESIGNS_DIRECTORY_ID,
        }
    }
}

/// Content-scan lifecycle of a managed document.
///
/// Importing hashes the bytes, so newly imported documents start as [`ScanStatus::Scanned`].
/// A future malware scanner may instead admit documents as [`ScanStatus::Pending`] until it
/// clears them; only [`ScanStatus::Scanned`] documents may be opened or indexed as evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScanStatus {
    Pending,
    Scanned,
    Rejected,
}

/// Legacy records predate scan status; import-time hashing has always been the admission scan.
fn scanned_by_default() -> ScanStatus {
    ScanStatus::Scanned
}

/// One directory in the managed `documents/` tree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDirectory {
    pub id: String,
    /// Single path segment; never a nested or traversal path.
    pub name: String,
    /// Parent directory record; `None` anchors the directory directly under `documents/`.
    pub parent_id: Option<String>,
    /// Built-in category roots are always present and cannot be renamed or moved.
    #[serde(default)]
    pub system: bool,
    pub created_at_unix_seconds: u64,
}

/// One authorized document record persisted in the project document index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    pub id: String,
    pub category: DocumentCategory,
    pub original_file_name: String,
    /// Managed-copy path relative to the project root, serialized with `/` separators so the
    /// index stays portable across operating systems.
    #[serde(with = "relative_path_serde")]
    pub relative_path: PathBuf,
    pub content_hash: String,
    pub byte_size: u64,
    pub document_kind: DocumentKind,
    pub source_locator: String,
    pub authorized: bool,
    pub imported_at_unix_seconds: u64,
    /// Owning directory record; `None` means the document sits directly under `documents/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory_id: Option<String>,
    #[serde(default = "scanned_by_default")]
    pub scan_status: ScanStatus,
}

/// Serializes relative paths with `/` separators and accepts either separator on read.
mod relative_path_serde {
    use std::path::{Path, PathBuf};

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string_lossy().replace('\\', "/"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(PathBuf::from(raw.replace('\\', "/")))
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocumentIndex {
    pub schema_version: u32,
    #[serde(default)]
    pub directories: Vec<DocumentDirectory>,
    pub documents: Vec<ProjectDocument>,
}

/// The built-in, immutable directory records every project index carries.
pub(crate) fn system_directories() -> Vec<DocumentDirectory> {
    vec![
        DocumentDirectory {
            id: DATASHEETS_DIRECTORY_ID.to_owned(),
            name: "datasheets".to_owned(),
            parent_id: None,
            system: true,
            created_at_unix_seconds: 0,
        },
        DocumentDirectory {
            id: REFERENCE_DESIGNS_DIRECTORY_ID.to_owned(),
            name: "reference-designs".to_owned(),
            parent_id: None,
            system: true,
            created_at_unix_seconds: 0,
        },
    ]
}

/// Classifies a document kind from its original file name.
///
/// Unknown extensions fall back to [`DocumentKind::Text`]; callers that need the bytes to be
/// UTF-8 must still verify before extracting evidence text.
#[must_use]
pub fn classify_document_kind(file_name: &str) -> DocumentKind {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => DocumentKind::Pdf,
        "doc" | "docx" | "rtf" | "odt" => DocumentKind::Word,
        "md" | "markdown" => DocumentKind::Markdown,
        "xls" | "xlsx" => DocumentKind::Excel,
        "csv" => DocumentKind::Bom,
        "net" | "cir" | "spice" => DocumentKind::Netlist,
        _ => DocumentKind::Text,
    }
}

/// Whether a kind's managed copy can be read as UTF-8 text for evidence retrieval.
///
/// PDFs are indexed through [`crate::extract_pdf_pages`] instead; word processing and
/// spreadsheet formats have no evidence extractor yet.
#[must_use]
pub const fn is_text_extractable(kind: &DocumentKind) -> bool {
    matches!(
        kind,
        DocumentKind::Markdown | DocumentKind::Bom | DocumentKind::Netlist | DocumentKind::Text
    )
}

/// Whether a kind can become full-text evidence at all: UTF-8 text directly, PDFs after
/// their text has been extracted.
#[must_use]
pub const fn is_evidence_indexable(kind: &DocumentKind) -> bool {
    is_text_extractable(kind) || matches!(kind, DocumentKind::Pdf)
}

/// The outcome of one document import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentImport {
    /// The index record of the imported document.
    pub document: ProjectDocument,
    /// `false` when the very same source file was already indexed; nothing was written.
    pub created: bool,
}

impl crate::ProjectStorage {
    /// Imports a user-selected file as an authorized, managed copy in this project root.
    ///
    /// Equivalent to [`Self::import_document_into`] with `directory_id: None`, which places
    /// the copy in the category's built-in directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the source cannot be read, the index is missing or unreadable, or
    /// the managed copy cannot be written.
    pub fn import_document(
        &self,
        source: impl AsRef<Path>,
        category: DocumentCategory,
        source_locator: impl Into<String>,
    ) -> Result<DocumentImport, ProjectStorageError> {
        self.import_document_into(source, category, None, source_locator)
    }

    /// Imports a user-selected file into a chosen directory of the managed `documents/` tree.
    ///
    /// The bytes are hashed with SHA-256 and stored with a content-addressed file name.
    /// `directory_id: None` targets the category's built-in directory; `Some(id)` must name an
    /// existing directory record (see [`crate::ProjectStorage::create_document_directory`]).
    /// Re-importing the same source file is idempotent: the existing record is returned with
    /// `created: false` and no new record appears. Distinct sources with identical content
    /// share a managed copy while it lives in the same directory, and each keeps its own
    /// source locator and authorization.
    ///
    /// # Errors
    ///
    /// Returns an error when the source cannot be read, the index is missing or unreadable,
    /// `directory_id` does not name a directory, or the managed copy cannot be written.
    pub fn import_document_into(
        &self,
        source: impl AsRef<Path>,
        category: DocumentCategory,
        directory_id: Option<&str>,
        source_locator: impl Into<String>,
    ) -> Result<DocumentImport, ProjectStorageError> {
        let source = source.as_ref();
        let source_locator = source_locator.into();
        let original_file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| "unnamed-document".to_owned(), ToOwned::to_owned);
        let mut bytes = Vec::new();
        fs::File::open(source).and_then(|mut file| file.read_to_end(&mut bytes)).map_err(
            |error| ProjectStorageError::DocumentSourceNotFound {
                path: dunce::simplified(source).to_owned(),
                source: error,
            },
        )?;
        // Only the hex digest goes into the file name: the `sha256:` prefix contains a colon,
        // which is reserved on Windows and would resolve as an NTFS alternate data stream.
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let content_hash = format!("sha256:{digest}");

        let mut index = self.load_document_index()?;
        let target = crate::directories::resolve_import_target(&index, category, directory_id)?;
        if let Some(existing) = index.documents.iter().find(|document| {
            document.content_hash == content_hash
                && (document.source_locator == source_locator
                    || sources_match(&document.source_locator, source))
        }) {
            self.verify_managed_copy(&existing.relative_path, &content_hash)?;
            return Ok(DocumentImport { document: existing.clone(), created: false });
        }
        // Reuse an existing copy only when it already lives in the target directory; a copy in
        // another directory stays where it is so each directory remains self-contained.
        let existing_copy = index.documents.iter().find(|document| {
            document.content_hash == content_hash && document.directory_id == target.directory_id
        });
        let relative_path = if let Some(existing) = existing_copy {
            self.verify_managed_copy(&existing.relative_path, &content_hash)?;
            existing.relative_path.clone()
        } else {
            let path = target
                .relative_directory
                .join(format!("{digest}.{}", managed_extension(&original_file_name)));
            let absolute = self.resolve_relative_path(&path)?;
            if absolute.exists() {
                self.verify_managed_copy(&path, &content_hash)?;
            } else {
                if let Some(parent) = absolute.parent() {
                    fs::create_dir_all(parent).map_err(|source| ProjectStorageError::Io {
                        action: "create document directory",
                        path: parent.to_owned(),
                        source,
                    })?;
                }
                fs::write(&absolute, &bytes).map_err(|source| ProjectStorageError::Io {
                    action: "write managed document copy",
                    path: absolute.clone(),
                    source,
                })?;
            }
            path
        };

        let ordinal =
            index.documents.iter().filter(|document| document.content_hash == content_hash).count()
                + 1;
        let document_kind = classify_document_kind(&original_file_name);
        let document = ProjectDocument {
            id: format!("doc-{}-{ordinal}", &content_hash["sha256:".len().."sha256:".len() + 12]),
            category,
            original_file_name,
            relative_path,
            content_hash,
            byte_size: bytes.len() as u64,
            document_kind,
            source_locator,
            authorized: true,
            imported_at_unix_seconds: now_unix_seconds(),
            directory_id: target.directory_id,
            scan_status: ScanStatus::Scanned,
        };
        index.documents.push(document.clone());
        self.save_document_index(&index)?;
        Ok(DocumentImport { document, created: true })
    }

    /// Reads the persisted document index.
    ///
    /// # Errors
    ///
    /// Returns an error when the index is missing, unreadable, or uses an unsupported schema.
    pub fn list_documents(&self) -> Result<Vec<ProjectDocument>, ProjectStorageError> {
        Ok(self.load_document_index()?.documents)
    }

    /// Reads the managed copy of an indexed document after validating its containment.
    ///
    /// # Errors
    ///
    /// Returns an error when the relative path is unsafe or the copy cannot be read.
    pub fn read_document_content(
        &self,
        document: &ProjectDocument,
    ) -> Result<Vec<u8>, ProjectStorageError> {
        let absolute = self.resolve_relative_path(&document.relative_path)?;
        fs::read(&absolute).map_err(|source| ProjectStorageError::Io {
            action: "read managed document copy",
            path: absolute,
            source,
        })
    }

    /// Reads a managed document only after confirming that it still matches the content hash
    /// recorded at authorization time.
    ///
    /// This is the integrity-scan gate for evidence indexing.  Callers must use this method,
    /// rather than [`Self::read_document_content`], before deriving searchable evidence: a
    /// modified, missing, or otherwise invalid managed copy must never become an evidence
    /// source.
    ///
    /// # Errors
    ///
    /// Returns the underlying read error or [`ProjectStorageError::ManagedCopyConflict`] when
    /// the managed bytes no longer match the authorized content hash.
    pub fn read_verified_document_content(
        &self,
        document: &ProjectDocument,
    ) -> Result<Vec<u8>, ProjectStorageError> {
        let bytes = self.read_document_content(document)?;
        let actual_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
        if actual_hash != document.content_hash {
            return Err(ProjectStorageError::ManagedCopyConflict {
                path: self.resolve_relative_path(&document.relative_path)?,
            });
        }
        Ok(bytes)
    }

    pub(crate) fn load_document_index(&self) -> Result<DocumentIndex, ProjectStorageError> {
        let path = self.document_index_path();
        let raw = fs::read_to_string(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ProjectStorageError::MissingDocumentIndex { path: path.clone() }
            } else {
                ProjectStorageError::Io {
                    action: "read document index",
                    path: path.clone(),
                    source,
                }
            }
        })?;
        let mut index = serde_json::from_str::<DocumentIndex>(&raw).map_err(|source| {
            ProjectStorageError::ParseDocumentIndex { path: path.clone(), source }
        })?;
        match index.schema_version {
            DOCUMENT_INDEX_SCHEMA_VERSION => normalize_index(&mut index)?,
            // Version 1 predates directories and scan status; migrate in memory so reads of a
            // legacy project work unchanged and the next write persists version 2.
            1 => {
                index.schema_version = DOCUMENT_INDEX_SCHEMA_VERSION;
                normalize_index(&mut index)?;
            }
            found => {
                return Err(ProjectStorageError::UnsupportedDocumentIndexSchema {
                    path,
                    found,
                    expected: DOCUMENT_INDEX_SCHEMA_VERSION,
                });
            }
        }
        Ok(index)
    }

    pub(crate) fn save_document_index(
        &self,
        index: &DocumentIndex,
    ) -> Result<(), ProjectStorageError> {
        crate::ProjectStorage::write_json_atomically(&self.document_index_path(), index)
    }

    /// Confirms an existing managed copy really holds the expected content before reuse.
    pub(crate) fn verify_managed_copy(
        &self,
        relative_path: &Path,
        expected_hash: &str,
    ) -> Result<(), ProjectStorageError> {
        let absolute = self.resolve_relative_path(relative_path)?;
        let mut bytes = Vec::new();
        fs::File::open(&absolute).and_then(|mut file| file.read_to_end(&mut bytes)).map_err(
            |source| ProjectStorageError::Io {
                action: "verify managed document copy",
                path: absolute.clone(),
                source,
            },
        )?;
        let actual_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
        if actual_hash != expected_hash {
            return Err(ProjectStorageError::ManagedCopyConflict { path: absolute });
        }
        Ok(())
    }
}

/// Whether a stored source locator refers to the same origin file being imported now.
///
/// Paths are compared exactly first, then via canonicalization so case or separator differences
/// for one and the same file still match.
fn sources_match(locator: &str, source: &Path) -> bool {
    let locator_path = Path::new(locator);
    if locator_path == source {
        return true;
    }
    matches!(
        (dunce::canonicalize(locator_path), dunce::canonicalize(source)),
        (Ok(locator_canonical), Ok(source_canonical)) if locator_canonical == source_canonical
    )
}

fn managed_extension(original_file_name: &str) -> String {
    let extension = Path::new(original_file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase();
    if extension.is_empty() { "bin".to_owned() } else { extension }
}

fn now_unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Normalizes a loaded index in memory: guarantees the built-in system directories, and fills
/// directory membership for legacy records whose relative path already implies it.
///
/// Fails only when the directory records form a parent cycle.
fn normalize_index(index: &mut DocumentIndex) -> Result<(), ProjectStorageError> {
    for directory in system_directories() {
        if !index.directories.iter().any(|existing| existing.id == directory.id) {
            index.directories.push(directory);
        }
    }
    let paths = crate::directories::directory_paths(&index.directories)?;
    for document in &mut index.documents {
        if document.directory_id.is_some() {
            continue;
        }
        let parent = document
            .relative_path
            .parent()
            .map_or_else(|| PathBuf::from("documents"), ToOwned::to_owned);
        document.directory_id = paths.ids_by_path.get(&parent).cloned();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use circuitfabric_contracts::Project;

    use super::*;
    use crate::ProjectStorage;

    fn project(id: &str) -> Project {
        Project { id: id.to_owned(), name: format!("Project {id}"), description: None }
    }

    fn test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("circuitfabric-project-documents-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn write_source(root: &Path, name: &str, contents: &str) -> PathBuf {
        let path = root.join(name);
        fs::write(&path, contents).expect("write source file");
        path
    }

    #[test]
    fn import_copies_content_addressed_bytes_and_indexes_provenance() {
        let root = test_root("import");
        let storage = ProjectStorage::create(&root, project("import")).expect("create project");
        let source = write_source(&root, "lm317.md", "# LM317\nUse a 1uF capacitor.\n");

        let imported = storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("import document");
        assert!(imported.created, "a first import creates its record");
        let document = imported.document;

        assert_eq!(document.category, DocumentCategory::Datasheet);
        assert_eq!(document.original_file_name, "lm317.md");
        assert_eq!(document.document_kind, DocumentKind::Markdown);
        assert!(document.authorized);
        assert!(document.content_hash.starts_with("sha256:"));
        assert!(document.relative_path.starts_with("documents/datasheets/"));
        let listed = storage.list_documents().expect("list documents");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], document);
        let bytes = storage.read_document_content(&document).expect("read copy");
        assert_eq!(String::from_utf8(bytes).unwrap(), "# LM317\nUse a 1uF capacitor.\n");
        let index: serde_json::Value =
            serde_json::from_slice(&fs::read(storage.document_index_path()).expect("read index"))
                .expect("parse index");
        assert_eq!(index["schemaVersion"], DOCUMENT_INDEX_SCHEMA_VERSION);
        let relative = index["documents"][0]["relativePath"].as_str().expect("relative path");
        assert!(
            relative.starts_with("documents/datasheets/"),
            "the index must use portable `/` separators: {relative}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn re_importing_the_same_file_adds_no_record() {
        let root = test_root("dedupe-same-file");
        let storage =
            ProjectStorage::create(&root, project("dedupe-same-file")).expect("create project");
        let source = write_source(&root, "a.md", "same bytes");

        let first = storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("first import");
        let again = storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("re-import is accepted");

        assert!(first.created);
        assert!(!again.created, "the same source file is not re-recorded");
        assert_eq!(again.document, first.document);
        assert_eq!(storage.list_documents().expect("list").len(), 1);
        let datasheets = storage.root().join("documents/datasheets");
        assert_eq!(fs::read_dir(datasheets).expect("list datasheets").count(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn identical_content_from_another_source_keeps_a_distinct_record() {
        let root = test_root("dedupe-other-source");
        let storage =
            ProjectStorage::create(&root, project("dedupe-other-source")).expect("create project");
        let first_source = write_source(&root, "a.md", "same bytes");
        let second_source = write_source(&root, "b.md", "same bytes");

        let first = storage
            .import_document(
                &first_source,
                DocumentCategory::Datasheet,
                first_source.display().to_string(),
            )
            .expect("first import");
        let second = storage
            .import_document(
                &second_source,
                DocumentCategory::ReferenceDesign,
                second_source.display().to_string(),
            )
            .expect("second import");

        assert!(first.created);
        assert!(second.created, "a different source is its own document record");
        // Each category directory stays self-contained: identical content imported into two
        // categories keeps one copy per directory instead of cross-linking them.
        assert_ne!(first.document.relative_path, second.document.relative_path);
        assert!(first.document.relative_path.starts_with("documents/datasheets"));
        assert!(second.document.relative_path.starts_with("documents/reference-designs"));
        assert_ne!(first.document.id, second.document.id);
        assert_eq!(second.document.category, DocumentCategory::ReferenceDesign);
        assert_eq!(second.document.original_file_name, "b.md");
        assert_eq!(storage.list_documents().expect("list").len(), 2);
        let datasheets = storage.root().join("documents/datasheets");
        assert_eq!(fs::read_dir(datasheets).expect("list datasheets").count(), 1);
        let reference_designs = storage.root().join("documents/reference-designs");
        assert_eq!(fs::read_dir(reference_designs).expect("list reference designs").count(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn import_rejects_a_missing_source_and_a_conflicting_managed_copy() {
        let root = test_root("import-errors");
        let storage = ProjectStorage::create(&root, project("import-errors")).expect("project");

        let error = storage
            .import_document(root.join("missing.md"), DocumentCategory::Datasheet, "source")
            .expect_err("missing source is rejected");
        assert!(matches!(error, ProjectStorageError::DocumentSourceNotFound { .. }));

        let source = write_source(&root, "c.md", "content");
        let document = storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("import")
            .document;
        let absolute = storage.root().join(&document.relative_path);
        fs::write(&absolute, "tampered").expect("tamper with the managed copy");
        let error = storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect_err("conflicting managed copy is rejected");
        assert!(matches!(error, ProjectStorageError::ManagedCopyConflict { .. }));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn classification_covers_supported_kinds() {
        assert_eq!(classify_document_kind("mcu.pdf"), DocumentKind::Pdf);
        assert_eq!(classify_document_kind("notes.docx"), DocumentKind::Word);
        assert_eq!(classify_document_kind("notes.md"), DocumentKind::Markdown);
        assert_eq!(classify_document_kind("bom.xlsx"), DocumentKind::Excel);
        assert_eq!(classify_document_kind("bom.csv"), DocumentKind::Bom);
        assert_eq!(classify_document_kind("board.net"), DocumentKind::Netlist);
        assert_eq!(classify_document_kind("readme.txt"), DocumentKind::Text);
        assert!(!is_text_extractable(&DocumentKind::Pdf));
        assert!(!is_text_extractable(&DocumentKind::Excel));
        assert!(is_text_extractable(&DocumentKind::Markdown));
    }
}
