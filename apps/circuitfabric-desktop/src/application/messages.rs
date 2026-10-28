//! Desktop message-center read model.
//!
//! Messages are a presentation-layer aggregation over persisted records:
//! they point at `ChangeSets`, observations, and the bridge, and never become
//! a source of truth for approval or verification. A stage message states
//! one stage's own status (`write` passed says nothing about `readback`).
use circuitfabric_project::{ChangeSetStageStatus, ProjectStorage, StoredChangeSet};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Persisted messages kept, oldest evicted first.
pub(crate) const MAX_MESSAGES: usize = 200;
/// Observation files parsed per scan, bounding UI-thread work after a burst.
pub(crate) const MAX_OBSERVATION_FILES_PER_SCAN: usize = 8;
/// Minimum spacing between disk scans; the render loop calls `scan` freely.
pub(crate) const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MessageSource {
    ChangesAndApprovals,
    CircuitSemantics,
    EdaServices,
    SessionsAndTasks,
}

impl MessageSource {
    pub(crate) const ALL: [Self; 4] = [
        Self::ChangesAndApprovals,
        Self::CircuitSemantics,
        Self::EdaServices,
        Self::SessionsAndTasks,
    ];

    /// Bilingual group label: (Chinese, English).
    pub(crate) const fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::ChangesAndApprovals => ("变更与审批", "Changes & approvals"),
            Self::CircuitSemantics => ("电路语义识别", "Circuit semantics"),
            Self::EdaServices => ("EDA 服务", "EDA services"),
            Self::SessionsAndTasks => ("会话与任务", "Sessions & tasks"),
        }
    }
}

/// Where a message deep-links to; mirrors the overview todo panel's
/// `navigation.screen + selected_*` preselection pattern.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum MessageTarget {
    ChangeSet { project_id: String, change_set_id: String },
    Observation { project_id: String, snapshot_hash: String },
    EdaServices,
    Sessions { project_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct DesktopMessage {
    /// Stable dedupe key; persisted so a restart (or a cleared list with a
    /// retained scan cursor) never replays an event.
    pub(crate) id: String,
    pub(crate) source: MessageSource,
    pub(crate) title_zh: String,
    pub(crate) title_en: String,
    pub(crate) body_zh: String,
    pub(crate) body_en: String,
    pub(crate) at_unix_seconds: u64,
    pub(crate) read: bool,
    pub(crate) target: MessageTarget,
}

/// Live state-derived entry (pending approvals, session-directory cleanup):
/// recomputed on every scan, never persisted. `dismissed` ids persist so a
/// read entry stays read while it exists; if it disappears and later returns
/// (for example a new orphan file), it counts as unread again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DerivedMessage {
    pub(crate) id: String,
    pub(crate) source: MessageSource,
    pub(crate) title_zh: String,
    pub(crate) title_en: String,
    pub(crate) body_zh: String,
    pub(crate) body_en: String,
    pub(crate) at_unix_seconds: u64,
    pub(crate) target: MessageTarget,
}

impl DerivedMessage {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

/// One materialization stage observation, used only for edge detection.
/// Never persisted: a restart re-baselines silently instead of replaying
/// transitions that happened while the app was closed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct StageSnapshot {
    write: ChangeSetStageStatus,
    readback: ChangeSetStageStatus,
    verification: ChangeSetStageStatus,
}

impl StageSnapshot {
    fn of(record: &StoredChangeSet) -> Self {
        Self {
            write: record.execution.write.status.clone(),
            readback: record.execution.readback.status.clone(),
            verification: record.execution.verification.status.clone(),
        }
    }
}

/// A bridge lifecycle/health event the UI layer reports at the moment it
/// observes the transition. Sequential ids come from the persisted counter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BridgeEvent {
    Running { pid: u32 },
    Stopped,
    Failed { reason: String },
    Reachable,
    Unreachable { reason: String },
}

impl BridgeEvent {
    const fn kind(&self) -> &'static str {
        match self {
            Self::Running { .. } => "running",
            Self::Stopped => "stopped",
            Self::Failed { .. } => "failed",
            Self::Reachable => "reachable",
            Self::Unreachable { .. } => "unreachable",
        }
    }

    fn copy(&self) -> (String, String, String, String) {
        // (title zh, title en, body zh, body en)
        match self {
            Self::Running { pid } => (
                "bridge 已启动".into(),
                "Bridge started".into(),
                format!("受监管的 bridge 子进程正在运行（PID {pid}）；协议就绪以连接测试为准。"),
                format!(
                    "The supervised bridge child is running (PID {pid}); protocol readiness is only claimed by a connection test."
                ),
            ),
            Self::Stopped => (
                "bridge 已停止".into(),
                "Bridge stopped".into(),
                "bridge 子进程已停止，EDA 扩展将无法连接。".into(),
                "The bridge child has stopped; the EDA extension can no longer connect.".into(),
            ),
            Self::Failed { reason } => (
                "bridge 启动失败".into(),
                "Bridge failed".into(),
                format!("bridge 进程失败：{reason}"),
                format!("The bridge process failed: {reason}"),
            ),
            Self::Reachable => (
                "bridge 端口可达".into(),
                "Bridge port reachable".into(),
                "TCP 探测连上了配置的 bridge 端口；TCP 可达不等于 EDA 插件就绪。".into(),
                "A TCP probe connected to the configured bridge port; reachable TCP does not imply EDA readiness.".into(),
            ),
            Self::Unreachable { reason } => (
                "bridge 端口不可达".into(),
                "Bridge port unreachable".into(),
                format!("TCP 探测失败：{reason}"),
                format!("The TCP probe failed: {reason}"),
            ),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedStore {
    /// Newest first.
    messages: Vec<DesktopMessage>,
    /// Derived ids the user has already read, so live entries stay read
    /// across restarts and until they disappear.
    dismissed: BTreeSet<String>,
    /// Audit entries already turned into messages, keyed
    /// `"{project}\u{1}{change_set}"` (JSON maps need string keys).
    seen_audit_len: BTreeMap<String, usize>,
    /// Observation files already parsed, per project (relative names).
    ingested_observations: BTreeMap<String, BTreeSet<String>>,
    bridge_seq: u64,
}

/// Join a project and change set into one string key. `\u{1}` cannot occur
/// in either id, so the split is unambiguous.
fn audit_key(project_id: &str, change_set_id: &str) -> String {
    format!("{project_id}\u{1}{change_set_id}")
}

/// The message-center read model: an append-only event log plus derived
/// live entries, both produced by scanning persisted project state.
#[derive(Default)]
pub(crate) struct MessageCenter {
    messages: Vec<DesktopMessage>,
    derived: Vec<DerivedMessage>,
    dismissed: BTreeSet<String>,
    seen_stages: BTreeMap<(String, String), StageSnapshot>,
    seen_audit_len: BTreeMap<String, usize>,
    ingested_observations: BTreeMap<String, BTreeSet<String>>,
    bridge_seq: u64,
    last_scan: Option<Instant>,
}

fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn stage_label_zh(status: &ChangeSetStageStatus) -> &'static str {
    match status {
        ChangeSetStageStatus::NotRun => "未运行",
        ChangeSetStageStatus::Passed => "通过",
        ChangeSetStageStatus::Failed => "失败",
        ChangeSetStageStatus::Inconclusive => "无法判定",
    }
}

fn stage_label_en(status: &ChangeSetStageStatus) -> &'static str {
    match status {
        ChangeSetStageStatus::NotRun => "not run",
        ChangeSetStageStatus::Passed => "passed",
        ChangeSetStageStatus::Failed => "failed",
        ChangeSetStageStatus::Inconclusive => "inconclusive",
    }
}

const PENDING_DECISIONS: [&str; 3] = ["", "pending", "awaiting_approval"];

/// Audit decisions that end a pending state or mark rollback-class events.
const DECISION_LABELS: [(&str, &str, &str); 10] = [
    ("approved", "已批准", "approved"),
    ("rejected", "已拒绝", "rejected"),
    ("cancelled", "已取消", "cancelled"),
    ("canceled", "已取消", "cancelled"),
    ("rollback_executed", "已执行回滚", "rollback executed"),
    ("rollback_error", "回滚执行失败", "rollback error"),
    ("compensation", "已执行补偿", "compensation executed"),
    ("applied", "已应用", "applied"),
    ("rolled_back", "已回滚", "rolled back"),
    ("superseded", "已取代", "superseded"),
];

/// Bridge-side progress markers that appear as audit entries but are not
/// operator decisions. They are ingested silently: stage transitions are
/// already messaged from the three-stage snapshot diff, and echoing each
/// bridge log line would flood the center.
const AUDIT_EVENT_LABELS: [&str; 14] = [
    "write",
    "readback",
    "verification",
    "preview",
    "readback_request",
    "verification_request",
    "apply_request",
    "apply_requested",
    "rollback_request",
    "rollback_requested",
    "rollback_reconciled",
    "rollback_executor_status",
    "write_completed",
    "step_written",
];

impl MessageCenter {
    /// Append an event message; a repeated id is a no-op (returns `false`).
    pub(crate) fn push(&mut self, message: DesktopMessage) -> bool {
        if self.messages.iter().any(|existing| existing.id == message.id) {
            return false;
        }
        self.messages.insert(0, message);
        if self.messages.len() > MAX_MESSAGES {
            self.messages.truncate(MAX_MESSAGES);
        }
        true
    }

    /// Record a bridge transition observed by the UI layer.
    pub(crate) fn push_bridge_event(&mut self, event: &BridgeEvent, at_unix_seconds: u64) -> bool {
        let id = format!("eda:bridge:{}:{}", self.bridge_seq, event.kind());
        self.bridge_seq += 1;
        let (title_zh, title_en, body_zh, body_en) = event.copy();
        self.push(DesktopMessage {
            id,
            source: MessageSource::EdaServices,
            title_zh,
            title_en,
            body_zh,
            body_en,
            at_unix_seconds,
            read: false,
            target: MessageTarget::EdaServices,
        })
    }

    fn stage_message(
        project_id: &str,
        record: &StoredChangeSet,
        stage: &str,
        old: &ChangeSetStageStatus,
        new: &ChangeSetStageStatus,
        detail: &str,
        at: u64,
    ) -> DesktopMessage {
        // Single-stage fact only: a passed write never claims readback or
        // verification succeeded.
        let title = format!("物化阶段变化：{stage} {} → {}", stage_label_zh(old), stage_label_zh(new));
        let title_en =
            format!("Materialization stage: {stage} {} → {}", stage_label_en(old), stage_label_en(new));
        let body = format!(
            "ChangeSet `{}` 的 {stage} 阶段从「{}」变为「{}」。{}仅此阶段的状态，不代表整体验证通过；三态详情见变更页。",
            record.id,
            stage_label_zh(old),
            stage_label_zh(new),
            if detail.trim().is_empty() { String::new() } else { format!("详情：{detail}。") }
        );
        let body_en = format!(
            "ChangeSet `{}` stage {stage} moved from “{}” to “{}”. {}This is that one stage's own status, not overall verification; see the changes page for all three stages.",
            record.id,
            stage_label_en(old),
            stage_label_en(new),
            if detail.trim().is_empty() { String::new() } else { format!("Detail: {detail}. ") }
        );
        DesktopMessage {
            id: format!("changes:{project_id}:{}:stage:{stage}:{old:?}:{new:?}", record.id),
            source: MessageSource::ChangesAndApprovals,
            title_zh: title,
            title_en,
            body_zh: body,
            body_en,
            at_unix_seconds: at,
            read: false,
            target: MessageTarget::ChangeSet {
                project_id: project_id.to_owned(),
                change_set_id: record.id.clone(),
            },
        }
    }

    /// An audit entry whose decision is not a known decision or progress
    /// marker: reported honestly as an appended audit event, never phrased
    /// as an approval outcome.
    fn audit_event_message(
        project_id: &str,
        record: &StoredChangeSet,
        index: usize,
        at: u64,
    ) -> DesktopMessage {
        let entry = &record.audit[index];
        let at = if entry.timestamp_unix_seconds > 0 { entry.timestamp_unix_seconds } else { at };
        DesktopMessage {
            id: format!("changes:{project_id}:{}:audit:{index}", record.id),
            source: MessageSource::ChangesAndApprovals,
            title_zh: format!("ChangeSet 审计追加：{}", entry.decision),
            title_en: format!("ChangeSet audit entry appended: {}", entry.decision),
            body_zh: format!(
                "审计记录 #{index}（{}）由 {} 追加；这是审计流事件，不是审批决定；理由：{}。验证状态以物化三态为准。",
                entry.decision, entry.actor, entry.reason
            ),
            body_en: format!(
                "Audit entry #{index} ({}) was appended by {}; this is an audit-stream event, not an approval decision; reason: {}. Verification state stays with the three materialization stages.",
                entry.decision, entry.actor, entry.reason
            ),
            at_unix_seconds: at,
            read: false,
            target: MessageTarget::ChangeSet {
                project_id: project_id.to_owned(),
                change_set_id: record.id.clone(),
            },
        }
    }

    fn audit_message(
        project_id: &str,
        record: &StoredChangeSet,
        index: usize,
        at: u64,
    ) -> DesktopMessage {
        let entry = &record.audit[index];
        let known = DECISION_LABELS.iter().find(|(decision, _, _)| *decision == entry.decision);
        let (zh, en) = known.map_or((entry.decision.as_str(), entry.decision.as_str()), |(_, zh, en)| (*zh, *en));
        let observed = entry
            .observed_snapshot_hash
            .as_deref()
            .map_or_else(|| "无".to_owned(), short_hash_for_message);
        let at = if entry.timestamp_unix_seconds > 0 { entry.timestamp_unix_seconds } else { at };
        DesktopMessage {
            id: format!("changes:{project_id}:{}:audit:{index}", record.id),
            source: MessageSource::ChangesAndApprovals,
            title_zh: format!("ChangeSet 决定已落库：{zh}"),
            title_en: format!("ChangeSet decision recorded: {en}"),
            body_zh: format!(
                "审计记录 #{index}：{zh}，由 {} 落库；理由：{}；观测基线：{observed}。决定引用审计事实，验证状态以物化三态为准。",
                entry.actor, entry.reason
            ),
            body_en: format!(
                "Audit entry #{index}: {en}, recorded by {}; reason: {}; observed baseline: {observed}. The decision cites the audit record; verification state stays with the three materialization stages.",
                entry.actor, entry.reason
            ),
            at_unix_seconds: at,
            read: false,
            target: MessageTarget::ChangeSet {
                project_id: project_id.to_owned(),
                change_set_id: record.id.clone(),
            },
        }
    }

    /// Fold one project's `ChangeSets` into messages and derived entries.
    ///
    /// First sight of a change set baselines its stages silently (no replay
    /// of history that happened before this app saw it); later stage moves
    /// and new audit entries produce messages. A change set with no decision
    /// (or a pending one) contributes a derived "awaiting approval" entry.
    fn scan_change_sets(&mut self, project_id: &str, records: &[StoredChangeSet], at: u64) {
        let mut live: BTreeSet<String> = BTreeSet::new();
        for record in records {
            live.insert(record.id.clone());
            let stage_key = (project_id.to_owned(), record.id.clone());
            let audit_key = audit_key(project_id, &record.id);

            let seen_len = self.seen_audit_len.get(&audit_key).copied().unwrap_or(0);
            if record.audit.len() > seen_len {
                for index in seen_len..record.audit.len() {
                    let decision = record.audit[index].decision.as_str();
                    if DECISION_LABELS.iter().any(|(label, _, _)| *label == decision) {
                        self.push(Self::audit_message(project_id, record, index, at));
                    } else if !AUDIT_EVENT_LABELS.contains(&decision) {
                        self.push(Self::audit_event_message(project_id, record, index, at));
                    }
                }
                self.seen_audit_len.insert(audit_key, record.audit.len());
            }

            let snapshot = StageSnapshot::of(record);
            let previous = self.seen_stages.get(&stage_key).cloned();
            if let Some(previous) = previous {
                if previous.write != snapshot.write {
                    self.push(Self::stage_message(
                        project_id,
                        record,
                        "write",
                        &previous.write,
                        &snapshot.write,
                        &record.execution.write.detail,
                        at,
                    ));
                }
                if previous.readback != snapshot.readback {
                    self.push(Self::stage_message(
                        project_id,
                        record,
                        "readback",
                        &previous.readback,
                        &snapshot.readback,
                        &record.execution.readback.detail,
                        at,
                    ));
                }
                if previous.verification != snapshot.verification {
                    self.push(Self::stage_message(
                        project_id,
                        record,
                        "verification",
                        &previous.verification,
                        &snapshot.verification,
                        &record.execution.verification.detail,
                        at,
                    ));
                }
            }
            self.seen_stages.insert(stage_key, snapshot);

            let decision = record.decision().map(|entry| entry.decision.as_str());
            if decision.is_none_or(|decision| PENDING_DECISIONS.contains(&decision)) {
                self.derived.push(DerivedMessage {
                    id: format!("pending:{project_id}:{}", record.id),
                    source: MessageSource::ChangesAndApprovals,
                    title_zh: "ChangeSet 等待审批决定".into(),
                    title_en: "ChangeSet awaiting an approval decision".into(),
                    body_zh: format!(
                        "ChangeSet `{}` 尚无审批决定；批准前置条件（观测基线匹配、预览校验通过）在变更页审批抽屉中核验。",
                        record.id
                    ),
                    body_en: format!(
                        "ChangeSet `{}` has no approval decision yet; approval preconditions (observed-baseline match, preview validation) are checked in the changes page drawer.",
                        record.id
                    ),
                    at_unix_seconds: at,
                    target: MessageTarget::ChangeSet {
                        project_id: project_id.to_owned(),
                        change_set_id: record.id.clone(),
                    },
                });
            }
        }
        // Deleted change sets stop being tracked; their cursors are dropped.
        self.seen_stages.retain(|(project, id), _| project != project_id || live.contains(id));
        self.seen_audit_len.retain(|key, _| match key.split_once('\u{1}') {
            Some((project, id)) => project != project_id || live.contains(id),
            None => true,
        });
    }

    /// One noteworthy persisted observation record becomes one message.
    /// Only `inspect` records are interpreted; DRC reports and visual
    /// captures are different schemas and are ingested silently until they
    /// get their own message copy.
    fn observation_message(
        project_id: &str,
        file_name: &str,
        record: &serde_json::Value,
        at: u64,
    ) -> Option<DesktopMessage> {
        if record.get("kind").and_then(|value| value.as_str()) != Some("inspect") {
            return None;
        }
        let diagnostics = record.get("diagnostics").and_then(|value| value.as_array());
        let inconclusive = record.get("inconclusive").and_then(|value| value.as_array());
        let validation = record.get("validation").and_then(|value| value.as_array());
        let mut failed = 0usize;
        let mut undetermined = 0usize;
        if let Some(facts) = validation {
            for fact in facts {
                match fact.get("status").and_then(|status| status.as_str()) {
                    Some("failed") => failed += 1,
                    Some("inconclusive" | "not_run") => undetermined += 1,
                    _ => (),
                }
            }
        }
        let diagnostic_count = diagnostics.map_or(0, Vec::len);
        if diagnostic_count == 0 && failed == 0 && undetermined == 0 && inconclusive.is_none_or(Vec::is_empty) {
            return None;
        }
        let hash = record
            .pointer("/snapshot/snapshot_hash")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned();
        let mut summary_zh = String::new();
        let mut summary_en = String::new();
        if let Some(entries) = diagnostics {
            for entry in entries.iter().take(3) {
                let code = entry.get("code").and_then(|value| value.as_str()).unwrap_or("unknown");
                let count = entry.get("count").and_then(serde_json::Value::as_u64).unwrap_or(1);
                let message = entry.get("message").and_then(|value| value.as_str()).unwrap_or("");
                summary_zh.push_str(&format!("\n· {code} ×{count}：{message}"));
                summary_en.push_str(&format!("\n· {code} ×{count}: {message}"));
            }
            if entries.len() > 3 {
                summary_zh.push_str(&format!("\n· 其余 {} 条见观测记录", entries.len() - 3));
                summary_en.push_str(&format!("\n· {} more in the observation record", entries.len() - 3));
            }
        }
        if let Some(faces) = inconclusive
            && !faces.is_empty()
        {
            let names: Vec<&str> = faces.iter().filter_map(|face| face.as_str()).collect();
            summary_zh.push_str(&format!("\n· 不可判定的观测面：{}", names.join("、")));
            summary_en.push_str(&format!("\n· Inconclusive faces: {}", names.join(", ")));
        }
        if failed > 0 || undetermined > 0 {
            summary_zh.push_str(&format!("\n· 验证事实：{failed} 失败 / {undetermined} 无法判定"));
            summary_en.push_str(&format!("\n· Validation facts: {failed} failed / {undetermined} undetermined"));
        }
        Some(DesktopMessage {
            id: format!("observation:{project_id}:{file_name}"),
            source: MessageSource::CircuitSemantics,
            title_zh: format!("电路观测诊断：{}", short_hash_for_message(&hash)),
            title_en: format!("Circuit observation diagnostics: {}", short_hash_for_message(&hash)),
            body_zh: format!(
                "新观测记录带有采集问题或验证告警：{summary_zh}\n这是观测层的解释，不是设计验证结论；详情见电路语义页。"
            ),
            body_en: format!(
                "A new observation record carries collection problems or validation warnings: {summary_en}\nThis is the observation layer's own explanation, not a design-verification verdict; see the semantics page."
            ),
            at_unix_seconds: at,
            read: false,
            target: MessageTarget::Observation { project_id: project_id.to_owned(), snapshot_hash: hash },
        })
    }

    /// Parse observation files this center has not ingested yet. Clean
    /// records are ingested silently; only noteworthy ones message.
    fn scan_observations(&mut self, project_id: &str, storage_root: &Path, at: u64) {
        let root = jlcircuit_eda_bridge::observation::observations_dir(storage_root);
        let history = root.join("inspect-history");
        let mut candidates = Vec::new();
        for dir in [&root, &history] {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
                // Content-addressed copies live flat; history entries are namespaced.
                let relative =
                    if dir == &history { format!("inspect-history/{name}") } else { name.to_owned() };
                candidates.push(relative);
            }
        }
        candidates.sort();
        let known = self.ingested_observations.get(project_id).cloned().unwrap_or_default();
        let fresh: Vec<String> = candidates
            .into_iter()
            .filter(|relative| !known.contains(relative))
            .take(MAX_OBSERVATION_FILES_PER_SCAN)
            .collect();
        for relative in fresh {
            self.ingested_observations
                .entry(project_id.to_owned())
                .or_default()
                .insert(relative.clone());
            let path = root.join(&relative);
            let Ok(raw) = std::fs::read_to_string(&path) else { continue };
            let Ok(record) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
            if let Some(message) = Self::observation_message(project_id, &relative, &record, at) {
                self.push(message);
            }
        }
    }

    /// Derived session-side entries from the cached workspace listing.
    fn derive_session_entries(&mut self, project_id: &str, orphaned_temp_files: &[String], at: u64) {
        if orphaned_temp_files.is_empty() {
            return;
        }
        self.derived.push(DerivedMessage {
            id: format!("sessions:{project_id}:orphans:{}", orphaned_temp_files.len()),
            source: MessageSource::SessionsAndTasks,
            title_zh: format!("会话目录有 {} 个中断写入的临时文件", orphaned_temp_files.len()),
            title_en: format!("{} interrupted-write temp files in the sessions directory", orphaned_temp_files.len()),
            body_zh: format!(
                "检测到中断写入的临时文件：{}。请在会话页确认后手动删除；它们不会被自动清理。",
                orphaned_temp_files.join("、")
            ),
            body_en: format!(
                "Interrupted-write temporary files detected: {}. Review and remove them on the sessions page; they are never cleaned automatically.",
                orphaned_temp_files.join(", ")
            ),
            at_unix_seconds: at,
            target: MessageTarget::Sessions { project_id: project_id.to_owned() },
        });
    }

    /// Scan every opened project's persisted state. Rate-limited internally
    /// so the render loop can call it on every frame. Returns `true` when
    /// anything persisted changed (caller should save).
    pub(crate) fn scan(
        &mut self,
        storages: &BTreeMap<String, ProjectStorage>,
        cached_data: &BTreeMap<String, crate::application::project_data::ProjectWorkspaceData>,
    ) -> bool {
        if self.last_scan.is_some_and(|at| at.elapsed() < SCAN_INTERVAL) {
            return false;
        }
        self.last_scan = Some(Instant::now());
        self.scan_now(storages, cached_data)
    }

    /// The unthrottled scan body; also the test entry point.
    pub(crate) fn scan_now(
        &mut self,
        storages: &BTreeMap<String, ProjectStorage>,
        cached_data: &BTreeMap<String, crate::application::project_data::ProjectWorkspaceData>,
    ) -> bool {
        let at = now_unix_seconds();
        let before = self.signature();
        self.derived.clear();
        for (project_id, storage) in storages {
            let Ok(records) = storage.list_change_sets() else { continue };
            self.scan_change_sets(project_id, &records, at);
            self.scan_observations(project_id, storage.root(), at);
            if let Some(data) = cached_data.get(project_id) {
                let orphans = data.session_listing.orphaned_temp_files.clone();
                self.derive_session_entries(project_id, &orphans, at);
            }
        }
        // Dismissed ids only describe live derived entries; once an entry is
        // gone its id is dropped, so a later re-appearance reads as unread.
        let live: BTreeSet<&str> = self.derived.iter().map(DerivedMessage::id).collect();
        self.dismissed.retain(|id| live.contains(id.as_str()));
        self.signature() != before
    }

    /// Cheap mutation signature; covers everything `scan_now` can persist.
    fn signature(&self) -> (usize, usize, usize, usize, u64) {
        (
            self.messages.len(),
            self.seen_audit_len.values().sum::<usize>(),
            self.seen_audit_len.len(),
            self.ingested_observations.values().map(BTreeSet::len).sum::<usize>(),
            self.bridge_seq,
        )
    }

    /// Unread rows: persisted events plus undismissed derived entries.
    pub(crate) fn unread_count(&self) -> usize {
        self.messages.iter().filter(|message| !message.read).count()
            + self.derived.iter().filter(|entry| !self.dismissed.contains(entry.id())).count()
    }

    /// Mark one row read: persisted events flip their own flag; derived
    /// entries are covered by the dismissed set (both by id).
    pub(crate) fn mark_read(&mut self, id: &str) {
        if let Some(message) = self.messages.iter_mut().find(|message| message.id == id) {
            message.read = true;
        }
        self.dismissed.insert(id.to_owned());
    }

    pub(crate) fn mark_all_read(&mut self) {
        for message in &mut self.messages {
            message.read = true;
        }
        for entry in &self.derived {
            self.dismissed.insert(entry.id.clone());
        }
    }

    /// Empty the event log. Scan cursors and dismissed ids stay, so cleared
    /// history is never replayed and live entries keep their read state.
    pub(crate) fn clear(&mut self) {
        self.messages.clear();
    }

    pub(crate) fn messages(&self) -> &[DesktopMessage] {
        &self.messages
    }

    pub(crate) fn derived(&self) -> &[DerivedMessage] {
        &self.derived
    }

    pub(crate) fn is_dismissed(&self, id: &str) -> bool {
        self.dismissed.contains(id)
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), String> {
        let store = PersistedStore {
            messages: self.messages.clone(),
            dismissed: self.dismissed.clone(),
            seen_audit_len: self.seen_audit_len.clone(),
            ingested_observations: self.ingested_observations.clone(),
            bridge_seq: self.bridge_seq,
        };
        let bytes = serde_json::to_vec_pretty(&store).map_err(|error| error.to_string())?;
        let staging = path.with_extension("tmp");
        std::fs::write(&staging, bytes).map_err(|error| error.to_string())?;
        std::fs::rename(&staging, path).map_err(|error| error.to_string())?;
        Ok(())
    }

    /// A corrupt store falls back to empty without overwriting the file —
    /// the same recovery posture as the plugin-governance store.
    pub(crate) fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<PersistedStore>(&raw) {
            Ok(store) => Self {
                messages: store.messages,
                derived: Vec::new(),
                dismissed: store.dismissed,
                seen_stages: BTreeMap::new(),
                seen_audit_len: store.seen_audit_len,
                ingested_observations: store.ingested_observations,
                bridge_seq: store.bridge_seq,
                last_scan: None,
            },
            Err(error) => {
                eprintln!("Message store was not loaded: {error}");
                Self::default()
            }
        }
    }
}

fn short_hash_for_message(hash: &str) -> String {
    let trimmed = hash.strip_prefix("sha256:").unwrap_or(hash);
    if trimmed.len() <= 12 {
        trimmed.to_owned()
    } else {
        format!("{}…", &trimmed[..12])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuitfabric_project::{
        ChangeSetAuditEntry, ChangeSetStage, SessionListing, SessionSummary,
    };

    struct Fixture {
        _dir: tempdir::TempDirGuard,
        storage: ProjectStorage,
    }

    fn fixture(project_id: &str) -> Fixture {
        let (dir, storage) = tempdir::with_project(project_id);
        Fixture { _dir: dir, storage }
    }

    fn scan(center: &mut MessageCenter, fixture: &Fixture) -> bool {
        let storages = BTreeMap::from([("p".to_owned(), fixture.storage.clone())]);
        center.scan_now(&storages, &BTreeMap::new())
    }

    fn change_set(id: &str) -> StoredChangeSet {
        StoredChangeSet {
            schema_version: 1,
            id: id.into(),
            base_snapshot_hash: "sha256:base".into(),
            target_snapshot_hash: "sha256:target".into(),
            plan_hash: "sha256:plan".into(),
            ir_diff: vec![],
            evidence: vec![],
            preview_validation: Default::default(),
            execution: Default::default(),
            observed_snapshot_hash: None,
            rollback_handle: None,
            audit: vec![],
        }
    }

    fn audit_entry(decision: &str, at: u64) -> ChangeSetAuditEntry {
        ChangeSetAuditEntry {
            timestamp_unix_seconds: at,
            actor: "desktop operator".into(),
            decision: decision.into(),
            reason: "测试".into(),
            observed_snapshot_hash: Some("sha256:base".into()),
            rollback_handle: None,
        }
    }

    #[test]
    fn push_dedupes_by_id_and_caps_the_log() {
        let mut center = MessageCenter::default();
        let message = |id: &str| DesktopMessage {
            id: id.into(),
            source: MessageSource::EdaServices,
            title_zh: String::new(),
            title_en: String::new(),
            body_zh: String::new(),
            body_en: String::new(),
            at_unix_seconds: 1,
            read: false,
            target: MessageTarget::EdaServices,
        };
        assert!(center.push(message("a")));
        assert!(!center.push(message("a")));
        for index in 0..MAX_MESSAGES {
            center.push(message(&format!("extra-{index}")));
        }
        assert_eq!(center.messages().len(), MAX_MESSAGES);
        assert!(center.messages().iter().all(|entry| entry.id != "a"), "oldest evicted first");
    }

    #[test]
    fn pending_change_set_derives_a_live_entry_and_a_decision_replaces_it() {
        let fixture = fixture("p");
        let mut record = change_set("cs-1");
        fixture.storage.save_change_set(&record).unwrap();
        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);
        assert!(
            center.derived().iter().any(|entry| entry.id == "pending:p:cs-1"),
            "pending decision derives a live entry"
        );

        record.audit.push(audit_entry("approved", 42));
        fixture.storage.save_change_set(&record).unwrap();
        scan(&mut center, &fixture);
        assert!(
            !center.derived().iter().any(|entry| entry.id == "pending:p:cs-1"),
            "a decision removes the pending entry"
        );
        assert!(
            center.messages().iter().any(|message| message.id == "changes:p:cs-1:audit:0"),
            "the decision itself is an event message"
        );

        // The same audit entry must not message twice.
        let before = center.messages().len();
        scan(&mut center, &fixture);
        assert_eq!(center.messages().len(), before);
    }

    #[test]
    fn first_scan_baselines_stages_and_only_transitions_message() {
        let fixture = fixture("p");
        let mut record = change_set("cs-1");
        record.execution.write =
            ChangeSetStage { status: ChangeSetStageStatus::Passed, detail: String::new() };
        fixture.storage.save_change_set(&record).unwrap();
        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);
        assert!(center.messages().is_empty(), "pre-existing stages are baselined, not replayed");

        record.execution.readback =
            ChangeSetStage { status: ChangeSetStageStatus::Passed, detail: "回读一致".into() };
        fixture.storage.save_change_set(&record).unwrap();
        scan(&mut center, &fixture);
        let message = center
            .messages()
            .iter()
            .find(|message| message.id.contains("stage:readback"))
            .expect("readback transition messages");
        assert!(message.body_en.contains("not overall verification"));
        assert!(
            !center.messages().iter().any(|message| message.id.contains("stage:write")),
            "an unchanged stage stays silent"
        );
    }

    #[test]
    fn stage_message_never_claims_verification_from_a_write_pass() {
        let fixture = fixture("p");
        let mut record = change_set("cs-1");
        fixture.storage.save_change_set(&record).unwrap();
        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);

        record.execution.write =
            ChangeSetStage { status: ChangeSetStageStatus::Passed, detail: "写入完成".into() };
        fixture.storage.save_change_set(&record).unwrap();
        scan(&mut center, &fixture);
        let message = center
            .messages()
            .iter()
            .find(|message| message.id.contains("stage:write"))
            .expect("write transition messages");
        assert!(message.title_zh.contains("write"));
        assert!(!message.title_zh.contains("验证"));
        assert!(
            message.body_zh.contains("不代表整体验证通过"),
            "a write pass must carry the not-overall-verification disclaimer"
        );
    }

    #[test]
    fn bridge_progress_markers_are_skipped_and_unknown_labels_stay_honest() {
        let fixture = fixture("p");
        let mut record = change_set("cs-1");
        record.audit = vec![
            audit_entry("readback", 1),
            audit_entry("verification", 2),
            audit_entry("rollback_requested", 3),
            audit_entry("approved", 4),
            audit_entry("brand_new_label", 5),
        ];
        fixture.storage.save_change_set(&record).unwrap();
        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);

        let titles: Vec<&str> =
            center.messages().iter().map(|message| message.title_zh.as_str()).collect();
        assert_eq!(titles.len(), 2, "progress markers do not message: {titles:?}");
        assert!(
            titles.contains(&"ChangeSet 决定已落库：已批准"),
            "the real decision messages: {titles:?}"
        );
        let unknown = center
            .messages()
            .iter()
            .find(|message| message.title_zh.contains("brand_new_label"))
            .expect("an unknown label still surfaces as an audit event");
        assert!(unknown.title_zh.contains("审计追加"), "not phrased as a decision");
        assert!(unknown.body_zh.contains("不是审批决定"));
    }

    #[test]
    fn drc_and_visual_records_are_ingested_without_inspect_copy() {
        let fixture = fixture("p");
        let dir = jlcircuit_eda_bridge::observation::observations_dir(fixture.storage.root());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("drc-1793013581.json"),
            serde_json::json!({
                "kind": "drc",
                "inconclusive": ["drc"],
                "errors": [],
                "status": "inconclusive",
            })
            .to_string(),
        )
        .unwrap();
        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);
        assert!(center.messages().is_empty(), "non-inspect records stay silent");
    }

    #[test]
    fn observation_diagnostics_message_once_per_file_and_clean_records_stay_silent() {
        let fixture = fixture("p");
        let dir = jlcircuit_eda_bridge::observation::observations_dir(fixture.storage.root());
        std::fs::create_dir_all(dir.join("inspect-history")).unwrap();
        let record = serde_json::json!({
            "kind": "inspect",
            "snapshot": {"snapshot_hash": "sha256:0123456789abcdef"},
            "diagnostics": [
                {"face": "components", "code": "missing_designator", "count": 2, "message": "2 个元件缺少位号"}
            ],
            "inconclusive": ["nets"],
            "validation": [{"status": "failed"}],
        });
        std::fs::write(
            dir.join("inspect-history").join("0001-test.json"),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
        let clean = serde_json::json!({
            "kind": "inspect",
            "snapshot": {"snapshot_hash": "sha256:clean"},
            "diagnostics": [],
            "inconclusive": [],
            "validation": [{"status": "passed"}],
        });
        std::fs::write(
            dir.join("inspect-history").join("0002-test.json"),
            serde_json::to_string(&clean).unwrap(),
        )
        .unwrap();

        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);
        let message = center
            .messages()
            .iter()
            .find(|message| message.id == "observation:p:inspect-history/0001-test.json")
            .expect("diagnostic observation messages");
        assert_eq!(message.source, MessageSource::CircuitSemantics);
        assert!(message.body_zh.contains("missing_designator"));
        assert!(matches!(message.target, MessageTarget::Observation { .. }));
        assert!(
            !center.messages().iter().any(|message| message.id.contains("0002")),
            "a clean record is ingested silently"
        );

        let before = center.messages().len();
        scan(&mut center, &fixture);
        assert_eq!(center.messages().len(), before, "no duplicate on the next scan");
    }

    #[test]
    fn bridge_events_each_get_a_fresh_message() {
        let mut center = MessageCenter::default();
        assert!(center.push_bridge_event(&BridgeEvent::Running { pid: 7 }, 1));
        assert!(center.push_bridge_event(&BridgeEvent::Stopped, 2));
        assert!(center.push_bridge_event(&BridgeEvent::Running { pid: 8 }, 3));
        let ids: Vec<&str> = center.messages().iter().map(|message| message.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert!(center.messages().iter().all(|message| message.source == MessageSource::EdaServices));
    }

    #[test]
    fn read_state_and_clear_behaviour() {
        let mut center = MessageCenter::default();
        center.push_bridge_event(&BridgeEvent::Running { pid: 7 }, 1);
        center.mark_read("eda:bridge:0:running");
        assert_eq!(center.unread_count(), 0);
        center.mark_all_read();
        center.clear();
        assert!(center.messages().is_empty());
        // The seq cursor survives a clear, so the next event cannot reuse an id.
        center.push_bridge_event(&BridgeEvent::Running { pid: 9 }, 2);
        assert_eq!(center.messages().len(), 1);
        assert_ne!(center.messages()[0].id, "eda:bridge:0:running");
    }

    #[test]
    fn persistence_round_trips_and_prevents_replay_after_clear() {
        let fixture = fixture("p");
        let mut record = change_set("cs-1");
        record.audit.push(audit_entry("rejected", 5));
        fixture.storage.save_change_set(&record).unwrap();

        let path = fixture.storage.root().join("messages.json");
        let mut center = MessageCenter::default();
        scan(&mut center, &fixture);
        center.mark_all_read();
        center.clear();
        center.save(&path).unwrap();

        let mut restored = MessageCenter::load(&path);
        scan(&mut restored, &fixture);
        assert!(
            restored.messages().is_empty(),
            "a cleared list must not replay ingested audit history"
        );
        assert!(
            !restored.derived().iter().any(|entry| entry.id == "pending:p:cs-1"),
            "the rejected change set is not pending"
        );
    }

    #[test]
    fn corrupt_store_falls_back_to_empty_without_overwriting() {
        let fixture = fixture("p");
        let path = fixture.storage.root().join("messages.json");
        std::fs::write(&path, b"not json").unwrap();
        let center = MessageCenter::load(&path);
        assert!(center.messages().is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");
    }

    #[test]
    fn orphaned_session_files_derive_a_sessions_entry_from_cached_data() {
        use circuitfabric_project::{SessionMetadata, SessionStatus};
        let fixture = fixture("p");
        let mut data = crate::application::project_data::ProjectWorkspaceData::default();
        data.session_listing = SessionListing {
            sessions: vec![SessionSummary {
                metadata: SessionMetadata {
                    session_id: "s1".into(),
                    project_id: "p".into(),
                    category: circuitfabric_project::SessionCategory::Eda,
                    backend_id: Some("codex".into()),
                    runtime_profile_id: "profile".into(),
                    subject_id: None,
                    started_at_unix_seconds: 1,
                    completed_at_unix_seconds: None,
                    status: SessionStatus::Completed,
                    usage: Default::default(),
                    citations: vec![],
                },
                file_name: "s1.md".into(),
                byte_size: 10,
            }],
            orphaned_temp_files: vec!["eda/.s1.tmp".into()],
        };
        let mut center = MessageCenter::default();
        let storages = BTreeMap::from([("p".to_owned(), fixture.storage.clone())]);
        let cached = BTreeMap::from([("p".to_owned(), data)]);
        center.scan_now(&storages, &cached);
        let entry = center
            .derived()
            .iter()
            .find(|entry| entry.id.starts_with("sessions:p:orphans"))
            .expect("orphan temp files derive an entry");
        assert_eq!(entry.source, MessageSource::SessionsAndTasks);
        assert!(matches!(entry.target, MessageTarget::Sessions { .. }));
    }
}

// Small test-only helpers so the test module stays free of fixture sprawl.
#[cfg(test)]
mod tempdir {
    use circuitfabric_contracts::Project;
    use circuitfabric_project::ProjectStorage;
    use std::path::PathBuf;

    pub(super) struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub(super) fn with_project(project_id: &str) -> (TempDirGuard, ProjectStorage) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "cf-messages-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        let storage = ProjectStorage::create(
            &root,
            Project { id: project_id.into(), name: "测试".into(), description: None },
        )
        .unwrap();
        (TempDirGuard(dir), storage)
    }
}
