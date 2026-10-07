//! Project read-model loading. Disk facts are projected once, outside render methods.
use circuitfabric_contracts::{LogicalCircuitSnapshot, ProjectId};
use circuitfabric_project::{
    ProjectDocument, ProjectRegistry, ProjectStorage, ProjectWorkspace, SessionListing,
    StoredChangeSet,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// One project's cached view of its persisted workspace state.
#[derive(Clone, Default)]
pub(crate) struct ProjectWorkspaceData {
    pub(crate) loaded_at_unix_nanos: u128,
    pub(crate) documents: Vec<ProjectDocument>,
    /// Integrity verdicts (`read_verified_document_content`) computed when this listing
    /// was loaded — never per render. Re-hashing every managed copy on every frame was
    /// the root cause of the whole-window lag while the preview pane was open.
    pub(crate) document_integrity: std::collections::BTreeMap<String, bool>,
    pub(crate) session_listing: SessionListing,
    pub(crate) semantic_snapshots: Vec<LogicalCircuitSnapshot>,
    pub(crate) change_sets: Vec<StoredChangeSet>,
}

impl ProjectWorkspaceData {
    /// Build the whole replacement before publishing it. A corrupt late-stage file
    /// must never mix new documents/sessions with old ChangeSets in the UI cache.
    pub(crate) fn load(storage: &ProjectStorage) -> Result<Self, String> {
        let documents =
            storage.list_documents().map_err(|error| format!("文档索引未读取：{error}"))?;
        let document_integrity = compute_document_integrity(storage, &documents);
        let session_listing =
            storage.list_sessions().map_err(|error| format!("会话记录未读取：{error}"))?;
        let mut session_ids = std::collections::BTreeSet::new();
        for summary in &session_listing.sessions {
            if summary.metadata.project_id != storage.manifest().project.id
                || !session_ids.insert(&summary.metadata.session_id)
            {
                return Err(format!(
                    "会话来源身份不一致或重复：{} / {}",
                    summary.metadata.session_id, summary.file_name
                ));
            }
        }
        let semantic_snapshots =
            storage.list_logical_snapshots().map_err(|error| format!("语义快照未读取：{error}"))?;
        let change_sets = storage.list_change_sets().map_err(|error| error.to_string())?;
        Ok(Self {
            loaded_at_unix_nanos: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            documents,
            document_integrity,
            session_listing,
            semantic_snapshots,
            change_sets,
        })
    }
}

pub(crate) fn refresh_project_data(
    storages: &BTreeMap<ProjectId, ProjectStorage>,
    data: &mut BTreeMap<ProjectId, ProjectWorkspaceData>,
    project_id: &str,
) -> Result<(), String> {
    let storage = storages.get(project_id).ok_or_else(|| "项目根目录未打开".to_owned())?;
    let snapshot = ProjectWorkspaceData::load(storage)?;
    data.insert(project_id.to_owned(), snapshot);
    Ok(())
}

pub(crate) fn project_registry_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("projects.json")
}

/// Loads one opened storage into the in-memory workspace and caches its listings.
pub(crate) fn attach_project_storage(
    workspace: &mut ProjectWorkspace,
    storages: &mut BTreeMap<ProjectId, ProjectStorage>,
    data: &mut BTreeMap<ProjectId, ProjectWorkspaceData>,
    storage: ProjectStorage,
) -> Result<(), String> {
    let project_id = storage.manifest().project.id.clone();
    let configuration =
        storage.load_configuration().map_err(|error| format!("项目配置未恢复：{error}"))?;
    let snapshot = ProjectWorkspaceData::load(&storage)?;
    if workspace.project(&project_id).is_none() {
        workspace
            .create_project(storage.manifest().project.clone())
            .map_err(|error| error.to_string())?;
    }
    workspace.set_configuration(&project_id, configuration).map_err(|error| error.to_string())?;
    if let Err(error) = workspace.hydrate_project_documents(&project_id, &storage) {
        return Err(format!("文档证据未恢复：{error}"));
    }
    data.insert(project_id.clone(), snapshot);
    storages.insert(project_id, storage);
    Ok(())
}

/// One-shot integrity verification for a document listing.
///
/// The documents page used to re-read and re-hash every managed copy on every
/// render; with hover states on the cards that became a hash storm per frame.
/// Verification now happens when a listing is (re)loaded — attach, refresh,
/// import — and the render path reads these cached verdicts.
pub(crate) fn compute_document_integrity(
    storage: &ProjectStorage,
    documents: &[ProjectDocument],
) -> BTreeMap<String, bool> {
    documents
        .iter()
        .map(|document| {
            (document.id.clone(), storage.read_verified_document_content(document).is_ok())
        })
        .collect()
}

pub(crate) fn restore_project_workspace(
    registry_path: &Path,
) -> (
    ProjectRegistry,
    ProjectWorkspace,
    BTreeMap<ProjectId, ProjectStorage>,
    BTreeMap<ProjectId, ProjectWorkspaceData>,
    Vec<String>,
) {
    let registry = match ProjectRegistry::load_or_default(registry_path) {
        Ok(registry) => registry,
        Err(error) => {
            return (
                ProjectRegistry::default(),
                ProjectWorkspace::default(),
                BTreeMap::new(),
                BTreeMap::new(),
                vec![format!("项目注册表未加载：{error}")],
            );
        }
    };
    let mut workspace = ProjectWorkspace::default();
    let mut storages = BTreeMap::new();
    let mut data = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for entry in registry.entries() {
        match ProjectStorage::open(&entry.canonical_root_path) {
            Ok(storage) if storage.manifest().project.id == entry.project_id => {
                if let Err(error) =
                    attach_project_storage(&mut workspace, &mut storages, &mut data, storage)
                {
                    diagnostics.push(format!("未恢复项目 `{}`：{error}", entry.project_id));
                }
            }
            Ok(_) => diagnostics
                .push(format!("项目 `{}` 的根目录身份已变化，未自动打开。", entry.project_id)),
            Err(error) => {
                diagnostics.push(format!("项目 `{}` 无法打开：{error}", entry.project_id));
            }
        }
    }
    (registry, workspace, storages, data, diagnostics)
}

use super::usage_audit::{AuditRecord, UsageAuditModel, UsageRecord};
pub(crate) fn build_usage_audit_model(
    project_data: &BTreeMap<ProjectId, ProjectWorkspaceData>,
    project_storages: &BTreeMap<ProjectId, ProjectStorage>,
) -> UsageAuditModel {
    let mut model = UsageAuditModel::default();
    for (project_id, data) in project_data {
        for summary in &data.session_listing.sessions {
            let replay = project_storages
                .get(project_id)
                .ok_or_else(|| "project storage unavailable".to_owned())
                .and_then(|storage| {
                    storage
                        .load_session(&summary.metadata.session_id)
                        .map_err(|error| error.to_string())
                });
            match replay {
                Ok(replay) => {
                    model.usage.push(UsageRecord::from_session(&replay, &summary.file_name));
                    let mut audit = AuditRecord::from_session(&replay);
                    for record in &mut audit {
                        record.source.locator =
                            format!("sessions/{}#{}", summary.file_name, record.source.locator);
                    }
                    model.audit.extend(audit);
                }
                Err(error) => {
                    let mut usage = UsageRecord::from(&summary.metadata);
                    usage.source_locator =
                        format!("sessions/{}#front-matter:usage", summary.file_name);
                    model.usage.push(usage);
                    model.diagnostics.push(format!(
                        "项目 {project_id} / 会话 {} 审计未读取：{error}",
                        summary.metadata.session_id
                    ));
                }
            }
        }
        for record in &data.change_sets {
            model.audit.extend(AuditRecord::from_change_set(project_id, record));
        }
    }
    model
        .audit
        .sort_by(|left, right| right.timestamp_unix_seconds.cmp(&left.timestamp_unix_seconds));
    model
}
