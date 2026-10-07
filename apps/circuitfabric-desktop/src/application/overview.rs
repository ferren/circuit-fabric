//! Cached, read-only workspace projection. No file reads or domain writes occur here.
use super::project_data::ProjectWorkspaceData;
use circuitfabric_contracts::{FactStatus, LogicalCircuitSnapshot};
use circuitfabric_project::SessionStatus;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct ResourceRow {
    pub id: String,
    pub name: String,
    /// None means unread or stale; it must never become a confirmed zero.
    pub counts: Option<[usize; 3]>,
    pub loaded_at: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionRow {
    pub project_id: String,
    pub id: String,
    pub status: SessionStatus,
    pub started_at: u64,
    pub locator: String,
}

#[derive(Clone, Debug)]
pub(crate) enum TodoTarget {
    ChangeSet(String),
    Constraint { snapshot_hash: String, constraint_id: String },
}

#[derive(Clone, Debug)]
pub(crate) struct TodoRow {
    pub project_id: String,
    pub target: TodoTarget,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OverviewModel {
    pub resources: Vec<ResourceRow>,
    pub documents: usize,
    pub pending: usize,
    pub unknown_approval_states: usize,
    pub registry_unknown: bool,
    pub loaded_projects: usize,
    pub sessions: Vec<SessionRow>,
    pub todos: Vec<TodoRow>,
    /// passed, failed, inconclusive, not_run, from one unambiguous lineage tip per project.
    pub facts: [usize; 4],
    pub fact_projects: usize,
    pub ambiguous_projects: usize,
    pub daily_sessions: Vec<(u64, usize)>,
    pub unreliable_session_times: usize,
    pub diagnostics: Vec<String>,
}

impl OverviewModel {
    pub fn complete(&self) -> bool {
        !self.registry_unknown && self.loaded_projects == self.resources.len()
    }

    pub fn build(
        projects: &BTreeMap<String, String>,
        data: &BTreeMap<String, ProjectWorkspaceData>,
        errors: &BTreeMap<String, String>,
        now: u64,
    ) -> Self {
        let today = now / 86_400;
        let mut model = Self {
            daily_sessions: (today.saturating_sub(6)..=today).map(|day| (day, 0)).collect(),
            ..Self::default()
        };
        for (id, name) in projects {
            let cached = data.get(id);
            let error = errors
                .get(id)
                .cloned()
                .or_else(|| cached.is_none().then(|| "未加载 / not loaded".into()));
            let current = cached.filter(|_| error.is_none());
            let counts = current.map(|data| {
                [
                    data.documents.iter().filter(|doc| doc.authorized).count(),
                    data.session_listing.sessions.len(),
                    data.semantic_snapshots.len(),
                ]
            });
            model.resources.push(ResourceRow {
                id: id.clone(),
                name: name.clone(),
                counts,
                loaded_at: cached.map_or(0, |d| (d.loaded_at_unix_nanos / 1_000_000_000) as u64),
                error,
            });
            let Some(data) = current else {
                continue;
            };
            model.loaded_projects += 1;
            model.documents += counts.unwrap()[0];
            for document in &data.documents {
                if data.document_integrity.get(&document.id) == Some(&false) {
                    model.diagnostics.push(format!("{id} / {}：文档完整性检查失败，授权标记不代表证据可用 / document integrity failed; authorization does not imply usable evidence", document.id));
                }
            }
            for record in &data.change_sets {
                // The persisted schema has audit decisions, not an approval-status field.
                // No decision, pending, and awaiting_approval are the only pending states.
                let decision = record.decision().map(|entry| entry.decision.as_str());
                if matches!(decision, None | Some("pending" | "awaiting_approval")) {
                    model.pending += 1;
                    model.todos.push(TodoRow {
                        project_id: id.clone(),
                        target: TodoTarget::ChangeSet(record.id.clone()),
                        label: format!(
                            "{} · pending · logic/changesets/{}.json",
                            record.id, record.id
                        ),
                    });
                } else if !matches!(
                    decision,
                    Some(
                        "approved"
                            | "rejected"
                            | "cancelled"
                            | "canceled"
                            | "applied"
                            | "rolled_back"
                            | "superseded"
                    )
                ) {
                    model.unknown_approval_states += 1;
                    model.diagnostics.push(format!(
                        "{id} / {}：未知审批状态 / unknown decision {decision:?}",
                        record.id
                    ));
                }
            }
            for summary in &data.session_listing.sessions {
                let meta = &summary.metadata;
                if meta.project_id != *id {
                    model.diagnostics.push(format!(
                        "{id} / {}：会话项目身份不匹配 / project identity mismatch",
                        meta.session_id
                    ));
                    continue;
                }
                let timestamp = meta.started_at_unix_seconds;
                // Schema-v1 uses integer timestamps. Zero and future values are not reliable history.
                if timestamp == 0 || timestamp > now {
                    model.unreliable_session_times += 1;
                } else if let Some((_, count)) =
                    model.daily_sessions.iter_mut().find(|(day, _)| *day == timestamp / 86_400)
                {
                    *count += 1;
                }
                model.sessions.push(SessionRow {
                    project_id: id.clone(),
                    id: meta.session_id.clone(),
                    status: meta.status,
                    started_at: timestamp,
                    locator: format!("sessions/{}#front-matter", summary.file_name),
                });
            }
            for file in &data.session_listing.orphaned_temp_files {
                model
                    .diagnostics
                    .push(format!("{id}：未完成的会话写入 / interrupted session write: {file}"));
            }
            match current_snapshot(&data.semantic_snapshots) {
                Ok(Some(snapshot)) => {
                    let mut constraint_ids = BTreeSet::new();
                    if snapshot.constraints.iter().any(|fact| {
                        fact.snapshot_hash != snapshot.snapshot_hash
                            || !constraint_ids.insert(&fact.constraint_id)
                    }) {
                        model.ambiguous_projects += 1;
                        model.diagnostics.push(format!("{id}：验证事实的快照身份不一致或规则身份重复，未统计 / inconsistent fact snapshot or duplicate constraint identity, excluded"));
                        continue;
                    }
                    model.fact_projects += 1;
                    for fact in &snapshot.constraints {
                        let index = match fact.status {
                            FactStatus::Passed => 0,
                            FactStatus::Failed => 1,
                            FactStatus::Inconclusive => 2,
                            FactStatus::NotRun => 3,
                        };
                        model.facts[index] += 1;
                        if index != 0 {
                            model.todos.push(TodoRow {
                                project_id: id.clone(),
                                target: TodoTarget::Constraint {
                                    snapshot_hash: snapshot.snapshot_hash.clone(),
                                    constraint_id: fact.constraint_id.clone(),
                                },
                                label: format!(
                                    "{} · {} · {} · {}",
                                    fact.constraint_id,
                                    ["passed", "failed", "inconclusive", "not_run"][index],
                                    snapshot.snapshot_hash,
                                    fact.explanation
                                ),
                            });
                        }
                    }
                }
                Ok(None) => (),
                Err(reason) => {
                    model.ambiguous_projects += 1;
                    model.diagnostics.push(format!("{id}：{reason}"));
                }
            }
        }
        model.sessions.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.project_id.cmp(&b.project_id))
                .then_with(|| a.id.cmp(&b.id))
        });
        model
    }
}

/// Snapshot identifiers and filenames are not clocks. Only a unique lineage tip can
/// represent current facts; independent branches, conflicting duplicate hashes and cycles
/// remain unknown. Identical historical copies are deduplicated without rewriting files.
pub(crate) fn current_snapshot(
    snapshots: &[LogicalCircuitSnapshot],
) -> Result<Option<&LogicalCircuitSnapshot>, &'static str> {
    if snapshots.is_empty() {
        return Ok(None);
    }
    let mut unique = BTreeMap::new();
    for snapshot in snapshots {
        if snapshot.snapshot_hash.is_empty() {
            return Err("快照缺少身份 / snapshot identity missing");
        }
        if let Some(previous) = unique.insert(&snapshot.snapshot_hash, snapshot) {
            if previous != snapshot {
                return Err("相同快照身份内容冲突 / conflicting snapshot identity");
            }
        }
    }
    let parents: BTreeSet<_> =
        snapshots.iter().filter_map(|s| s.parent_snapshot_hash.as_ref()).collect();
    let tips: Vec<_> =
        unique.values().filter(|s| !parents.contains(&s.snapshot_hash)).copied().collect();
    if tips.len() != 1 {
        return Err(
            "当前快照不明确（分支或循环），未统计 / ambiguous current snapshot (branches or cycles), excluded",
        );
    }
    let mut visited = BTreeSet::new();
    let mut cursor = Some(tips[0]);
    while let Some(snapshot) = cursor {
        if !visited.insert(&snapshot.snapshot_hash) {
            return Err("快照谱系循环 / cyclic snapshot lineage");
        }
        cursor =
            snapshot.parent_snapshot_hash.as_ref().and_then(|parent| unique.get(parent).copied());
    }
    if visited.len() != unique.len() {
        return Err("存在独立快照分支 / disconnected snapshot lineage");
    }
    Ok(Some(tips[0]))
}

type CacheKey = (u64, Vec<(String, String, Option<u128>, Option<String>)>);
#[derive(Default)]
pub(crate) struct OverviewCache {
    key: Option<CacheKey>,
    model: OverviewModel,
}
impl OverviewCache {
    pub fn get(
        &mut self,
        projects: &BTreeMap<String, String>,
        data: &BTreeMap<String, ProjectWorkspaceData>,
        errors: &BTreeMap<String, String>,
        now: u64,
    ) -> &OverviewModel {
        let key = (
            now / 86_400,
            projects
                .iter()
                .map(|(id, name)| {
                    (
                        id.clone(),
                        name.clone(),
                        data.get(id).map(|d| d.loaded_at_unix_nanos),
                        errors.get(id).cloned(),
                    )
                })
                .collect(),
        );
        if self.key.as_ref() != Some(&key) {
            self.model = OverviewModel::build(projects, data, errors, now);
            self.key = Some(key);
        }
        &self.model
    }
}

#[cfg(test)]
mod tests;
