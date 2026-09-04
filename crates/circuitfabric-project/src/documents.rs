//! Authorized document import and the persistent document index.
//!
//! Imports copy user-selected files into the managed `documents/` tree as content-addressed
//! copies. Provenance (original file name and source locator) lives in
//! `.circuitfabric/document-index.json`; identical content is stored once while each index
//! record keeps its own authorization and source.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use circuitfabric_contracts::DocumentKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ProjectStorageError;

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
    pub documents: Vec<ProjectDocument>,
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
        "csv" | "xls" | "xlsx" => DocumentKind::Bom,
        "net" | "cir" | "spice" => DocumentKind::Netlist,
        _ => DocumentKind::Text,
    }
}

/// Whether a kind's managed copy can be read as UTF-8 text for evidence retrieval.
///
/// Word processing formats and PDFs need an extractor that is not part of this layer yet.
#[must_use]
pub const fn is_text_extractable(kind: &DocumentKind) -> bool {
    matches!(
        kind,
        DocumentKind::Markdown | DocumentKind::Bom | DocumentKind::Netlist | DocumentKind::Text
    )
}

impl crate::ProjectStorage {
    /// Imports a user-selected file as an authorized, managed copy in this project root.
    ///
    /// The bytes are hashed with SHA-256 and stored under `documents/<category>/` with a
    /// content-addressed file name. Identical content is stored once; every import still gets
    /// its own index record with its own source locator and authorization.
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
    ) -> Result<ProjectDocument, ProjectStorageError> {
        let source = source.as_ref();
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
        let existing_copy = index
            .documents
            .iter()
            .find(|document| document.content_hash == content_hash)
            .map(|document| document.relative_path.clone());
        let relative_path = if let Some(path) = existing_copy {
            self.verify_managed_copy(&path, &content_hash)?;
            path
        } else {
            let path = Path::new("documents")
                .join(category.directory())
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
            source_locator: source_locator.into(),
            authorized: true,
            imported_at_unix_seconds: now_unix_seconds(),
        };
        index.documents.push(document.clone());
        self.save_document_index(&index)?;
        Ok(document)
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
        let index = serde_json::from_str::<DocumentIndex>(&raw).map_err(|source| {
            ProjectStorageError::ParseDocumentIndex { path: path.clone(), source }
        })?;
        if index.schema_version != crate::PROJECT_STORAGE_SCHEMA_VERSION {
            return Err(ProjectStorageError::UnsupportedDocumentIndexSchema {
                path,
                found: index.schema_version,
                expected: crate::PROJECT_STORAGE_SCHEMA_VERSION,
            });
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
    fn verify_managed_copy(
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

#[cfg(test)]
mod tests {
    use std::fs;

    use circuitfabric_contracts::Project;

    use super::*;
    use crate::{PROJECT_STORAGE_SCHEMA_VERSION, ProjectStorage};

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

        let document = storage
            .import_document(&source, DocumentCategory::Datasheet, source.display().to_string())
            .expect("import document");

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
        assert_eq!(index["schemaVersion"], PROJECT_STORAGE_SCHEMA_VERSION);
        let relative = index["documents"][0]["relativePath"].as_str().expect("relative path");
        assert!(
            relative.starts_with("documents/datasheets/"),
            "the index must use portable `/` separators: {relative}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn identical_content_is_deduplicated_but_keeps_distinct_records() {
        let root = test_root("dedupe");
        let storage = ProjectStorage::create(&root, project("dedupe")).expect("create project");
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

        assert_eq!(first.relative_path, second.relative_path);
        assert_ne!(first.id, second.id);
        assert_eq!(second.category, DocumentCategory::ReferenceDesign);
        assert_eq!(second.original_file_name, "b.md");
        assert_eq!(storage.list_documents().expect("list").len(), 2);
        let datasheets = storage.root().join("documents/datasheets");
        assert_eq!(fs::read_dir(datasheets).expect("list datasheets").count(), 1);
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
            .expect("import");
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
        assert_eq!(classify_document_kind("bom.csv"), DocumentKind::Bom);
        assert_eq!(classify_document_kind("board.net"), DocumentKind::Netlist);
        assert_eq!(classify_document_kind("readme.txt"), DocumentKind::Text);
        assert!(!is_text_extractable(&DocumentKind::Pdf));
        assert!(is_text_extractable(&DocumentKind::Markdown));
    }
}
