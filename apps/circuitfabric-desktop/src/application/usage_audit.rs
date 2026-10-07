//! Read-only projections for the desktop usage and audit screens.
//!
//! Session files remain the source of truth.  This module deliberately builds two separate
//! in-memory projections from them: token usage can be aggregated, while audit entries retain
//! the source context needed to inspect an immutable event.

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use circuitfabric_project::{SessionMetadata, SessionReplay, StoredChangeSet, rfc3339};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UsagePeriod {
    #[default]
    All,
    Last7Days,
    Last30Days,
}

impl UsagePeriod {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All time",
            Self::Last7Days => "Last 7 days",
            Self::Last30Days => "Last 30 days",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::Last7Days,
            Self::Last7Days => Self::Last30Days,
            Self::Last30Days => Self::All,
        }
    }

    #[must_use]
    pub fn includes(self, timestamp: u64, now: u64) -> bool {
        let seconds = match self {
            Self::All => return true,
            Self::Last7Days => 7 * 24 * 60 * 60,
            Self::Last30Days => 30 * 24 * 60 * 60,
        };
        timestamp <= now && now.saturating_sub(timestamp) <= seconds
    }
}

/// A usage row is purpose-built for aggregation and never carries audit text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageRecord {
    pub project_id: String,
    pub provider_id: String,
    pub runtime_id: String,
    pub session_id: String,
    pub timestamp_unix_seconds: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub usage_reported: bool,
    pub source_locator: String,
}

impl From<&SessionMetadata> for UsageRecord {
    fn from(metadata: &SessionMetadata) -> Self {
        Self {
            project_id: metadata.project_id.clone(),
            // A session's runtime profile is the selected provider profile.  The backend is
            // intentionally kept separately, so a provider and runtime cannot be conflated.
            provider_id: metadata.runtime_profile_id.clone(),
            runtime_id: metadata.backend_id.clone().unwrap_or_else(|| "unknown".to_owned()),
            session_id: metadata.session_id.clone(),
            timestamp_unix_seconds: metadata
                .completed_at_unix_seconds
                .unwrap_or(metadata.started_at_unix_seconds),
            input_tokens: metadata.usage.input_tokens,
            output_tokens: metadata.usage.output_tokens,
            usage_reported: metadata.usage.input_tokens > 0 || metadata.usage.output_tokens > 0,
            source_locator: String::new(),
        }
    }
}

impl UsageRecord {
    #[must_use]
    pub fn from_session(replay: &SessionReplay, file_name: &str) -> Self {
        let mut record = Self::from(&replay.metadata);
        // The explicit event distinguishes a reported zero from unavailable counters in
        // schema-v1 files, whose metadata has no availability flag.
        record.usage_reported |= replay.body.lines().any(|line| {
            session_event_header(line)
                .is_some_and(|(_, header)| header == "用量" || header == "Usage")
        });
        record.source_locator = format!("sessions/{file_name}#front-matter:usage");
        record
    }

    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditKind {
    Session,
    Approval,
    Materialization,
    Rollback,
    Verification,
    Other,
}

impl AuditKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Approval => "Approval",
            Self::Materialization => "Materialization",
            Self::Rollback => "Rollback",
            Self::Verification => "Verification",
            Self::Other => "Other",
        }
    }

    #[must_use]
    fn from_event_text(text: &str) -> Self {
        let text = text.to_lowercase();
        if text.contains("approval") || text.contains("审批") {
            Self::Approval
        } else if text.contains("materializ") || text.contains("物化") {
            Self::Materialization
        } else if text.contains("rollback") || text.contains("回滚") {
            Self::Rollback
        } else if text.contains("verif") || text.contains("validat") || text.contains("验证") {
            Self::Verification
        } else if text.contains("session")
            || text.contains("会话")
            || text.contains("turn")
            || text.contains("轮次")
            || matches!(text.as_str(), "用户" | "智能体" | "状态" | "用量")
        {
            Self::Session
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuditKindFilter {
    #[default]
    All,
    Sessions,
    Approvals,
    Materializations,
    Rollbacks,
    Verifications,
    Other,
}

impl AuditKindFilter {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All events",
            Self::Sessions => "Sessions",
            Self::Approvals => "Approvals",
            Self::Materializations => "Materializations",
            Self::Rollbacks => "Rollbacks",
            Self::Verifications => "Verifications",
            Self::Other => "Other",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::Sessions,
            Self::Sessions => Self::Approvals,
            Self::Approvals => Self::Materializations,
            Self::Materializations => Self::Rollbacks,
            Self::Rollbacks => Self::Verifications,
            Self::Verifications => Self::Other,
            Self::Other => Self::All,
        }
    }

    #[must_use]
    pub const fn includes(self, kind: AuditKind) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, kind),
                (Self::Sessions, AuditKind::Session)
                    | (Self::Approvals, AuditKind::Approval)
                    | (Self::Materializations, AuditKind::Materialization)
                    | (Self::Rollbacks, AuditKind::Rollback)
                    | (Self::Verifications, AuditKind::Verification)
                    | (Self::Other, AuditKind::Other)
            )
    }
}

/// Source information is duplicated onto each audit row so filtering/exporting never drops it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditSourceContext {
    pub project_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub runtime_id: String,
    pub source_type: String,
    pub source_id: String,
    pub locator: String,
}

/// A rendered event is immutable: it contains no edit state or mutation operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord {
    pub timestamp_unix_seconds: u64,
    pub kind: AuditKind,
    pub summary: String,
    pub source: AuditSourceContext,
    /// Historical stage snapshots have no event time; never invent a timestamp for them.
    pub timestamp_known: bool,
}

impl AuditRecord {
    #[must_use]
    pub fn from_session(replay: &SessionReplay) -> Vec<Self> {
        let metadata = &replay.metadata;
        let source = AuditSourceContext {
            project_id: metadata.project_id.clone(),
            session_id: metadata.session_id.clone(),
            provider_id: metadata.runtime_profile_id.clone(),
            runtime_id: metadata.backend_id.clone().unwrap_or_else(|| "unknown".to_owned()),
            source_type: "session".into(),
            source_id: metadata.session_id.clone(),
            locator: String::new(),
        };
        let mut records = vec![Self {
            timestamp_unix_seconds: metadata.started_at_unix_seconds,
            timestamp_known: true,
            kind: AuditKind::Session,
            summary: format!("Session {} started", metadata.session_id),
            source: AuditSourceContext {
                locator: "front-matter:startedAt".into(),
                ..source.clone()
            },
        }];
        for (line_index, raw) in replay.body.lines().enumerate() {
            let line = raw.trim();
            // Only storage-generated event headers are audit events. A list in a message
            // or the word "approval" inside an agent answer cannot create an approval event.
            let Some((timestamp, header)) = session_event_header(line) else {
                continue;
            };
            records.push(Self {
                timestamp_unix_seconds: timestamp,
                timestamp_known: true,
                kind: AuditKind::from_event_text(header),
                summary: line.trim_start_matches('-').trim().to_owned(),
                source: AuditSourceContext {
                    locator: format!("body-line:{}", line_index + 1),
                    ..source.clone()
                },
            });
        }
        if let Some(completed) = metadata.completed_at_unix_seconds {
            records.push(Self {
                timestamp_unix_seconds: completed,
                timestamp_known: true,
                kind: AuditKind::Session,
                summary: format!("Session {} {}", metadata.session_id, metadata.status.as_str()),
                source: AuditSourceContext { locator: "front-matter:completedAt".into(), ..source },
            });
        }
        records
    }

    /// Projects persisted decisions and execution stages without fabricating missing
    /// session/provider identities or treating a rollback handle as an executed rollback.
    #[must_use]
    pub fn from_change_set(project_id: &str, record: &StoredChangeSet) -> Vec<Self> {
        let source = AuditSourceContext {
            project_id: project_id.into(),
            session_id: "not-recorded".into(),
            provider_id: "not-recorded".into(),
            runtime_id: "not-recorded".into(),
            source_type: "changeset".into(),
            source_id: record.id.clone(),
            locator: format!("logic/changesets/{}.json", record.id),
        };
        let mut records = Vec::new();
        for (index, entry) in record.audit.iter().enumerate() {
            let kind = match entry.decision.as_str() {
                "approved" | "rejected" => AuditKind::Approval,
                other => AuditKind::from_event_text(other),
            };
            records.push(Self {
                timestamp_unix_seconds: entry.timestamp_unix_seconds,
                timestamp_known: true,
                kind,
                summary: format!("{} · actor={} · {} · observed={} · rollback={} · base={} · target={} · plan={}",
                    entry.decision, entry.actor, entry.reason,
                    entry.observed_snapshot_hash.as_deref().unwrap_or("not-recorded"),
                    entry.rollback_handle.as_deref().unwrap_or("not-recorded"),
                    record.base_snapshot_hash, record.target_snapshot_hash, record.plan_hash),
                source: AuditSourceContext { locator: format!("{}#/audit/{index}", source.locator), ..source.clone() },
            });
        }
        for (name, kind, stage) in [
            ("write", AuditKind::Materialization, &record.execution.write),
            ("readback", AuditKind::Materialization, &record.execution.readback),
            ("verification", AuditKind::Verification, &record.execution.verification),
        ] {
            if stage.status == circuitfabric_project::ChangeSetStageStatus::NotRun {
                continue;
            }
            records.push(Self {
                timestamp_unix_seconds: 0,
                timestamp_known: false,
                kind,
                summary: format!("Recorded stage snapshot (event time unavailable): {name} {:?} · {} · base={} · target={} · plan={}",
                    stage.status, stage.detail, record.base_snapshot_hash, record.target_snapshot_hash, record.plan_hash),
                source: AuditSourceContext { locator: format!("{}#/execution/{name}", source.locator), ..source.clone() },
            });
        }
        records
    }
}

fn session_event_header(line: &str) -> Option<(u64, &str)> {
    let (value, rest) = line.trim().strip_prefix("- **")?.split_once("** · ")?;
    let (header, _) = rest.split_once(" · ")?;
    Some((parse_rfc3339(value)?, header.trim()))
}

fn parse_rfc3339(value: &str) -> Option<u64> {
    // Session storage uses second-resolution UTC. Validate by round-tripping its renderer.
    let start = value.get(..10)?;
    let year = start.get(..4)?.parse::<u64>().ok()?;
    if !(1970..=9999).contains(&year)
        || value.len() != 20
        || value.as_bytes()[10] != b'T'
        || value.as_bytes()[19] != b'Z'
        || value.as_bytes()[13] != b':'
        || value.as_bytes()[16] != b':'
    {
        return None;
    }
    let jan_1 = (year - 1970) * 365 * 86_400 + leap_years_before(year) * 86_400;
    let month = value.get(5..7)?.parse::<usize>().ok()?;
    let day = value.get(8..10)?.parse::<u64>().ok()?;
    let hour = value.get(11..13)?.parse::<u64>().ok()?;
    let minute = value.get(14..16)?.parse::<u64>().ok()?;
    let second = value.get(17..19)?.parse::<u64>().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let days_before_month = [0_u64, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][month - 1]
        + u64::from(month > 2 && is_leap(year));
    let candidate =
        jan_1 + (days_before_month + day - 1) * 86_400 + hour * 3600 + minute * 60 + second;
    (rfc3339(candidate) == value).then_some(candidate)
}

const fn is_leap(year: u64) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

const fn leap_years_before(year: u64) -> u64 {
    (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400
}

#[derive(Clone, Debug, Default)]
pub struct UsageAuditModel {
    pub usage: Vec<UsageRecord>,
    pub audit: Vec<AuditRecord>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Copy)]
pub enum UsageDimension {
    Project,
    Provider,
    Runtime,
}

pub const AUDIT_PAGE_SIZE: usize = 50;

#[must_use]
pub fn audit_page_range(total: usize, requested_page: usize) -> std::ops::Range<usize> {
    let page = requested_page.min(total.saturating_sub(1) / AUDIT_PAGE_SIZE);
    let start = page * AUDIT_PAGE_SIZE;
    start..start.saturating_add(AUDIT_PAGE_SIZE).min(total)
}

fn csv_cell(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

impl UsageAuditModel {
    #[must_use]
    pub fn now_unix_seconds() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }

    #[must_use]
    pub fn filtered_usage(&self, period: UsagePeriod, query: &str, now: u64) -> Vec<&UsageRecord> {
        let query = query.trim().to_lowercase();
        self.usage
            .iter()
            .filter(|record| {
                period.includes(record.timestamp_unix_seconds, now)
                    && (query.is_empty()
                        || format!(
                            "{} {} {} {} {}",
                            record.project_id,
                            record.provider_id,
                            record.runtime_id,
                            record.session_id,
                            record.source_locator
                        )
                        .to_lowercase()
                        .contains(&query))
            })
            .collect()
    }

    #[must_use]
    pub fn filtered_audit(
        &self,
        period: UsagePeriod,
        kind: AuditKindFilter,
        query: &str,
        now: u64,
    ) -> Vec<&AuditRecord> {
        let query = query.trim().to_lowercase();
        self.audit
            .iter()
            .filter(|record| {
                (record.timestamp_known && period.includes(record.timestamp_unix_seconds, now)
                    || !record.timestamp_known && period == UsagePeriod::All)
                    && kind.includes(record.kind)
                    && (query.is_empty()
                        || format!(
                            "{} {} {} {} {} {} {} {}",
                            record.summary,
                            record.source.project_id,
                            record.source.provider_id,
                            record.source.runtime_id,
                            record.source.session_id,
                            record.source.source_type,
                            record.source.source_id,
                            record.source.locator
                        )
                        .to_lowercase()
                        .contains(&query))
            })
            .collect()
    }

    #[must_use]
    pub fn aggregate_usage(
        records: &[&UsageRecord],
    ) -> BTreeMap<(String, String, String), (u64, u64, u64)> {
        let mut grouped = BTreeMap::new();
        for record in records {
            let totals = grouped
                .entry((
                    record.project_id.clone(),
                    record.provider_id.clone(),
                    record.runtime_id.clone(),
                ))
                .or_insert((0_u64, 0_u64, 0_u64));
            totals.0 = totals.0.saturating_add(record.input_tokens);
            totals.1 = totals.1.saturating_add(record.output_tokens);
            totals.2 = totals.2.saturating_add(record.total_tokens());
        }
        grouped
    }

    #[must_use]
    pub fn daily_input_output(records: &[&UsageRecord]) -> BTreeMap<String, (u64, u64)> {
        let mut daily = BTreeMap::new();
        for record in records {
            let day = rfc3339(record.timestamp_unix_seconds)[..10].to_owned();
            let totals = daily.entry(day).or_insert((0_u64, 0_u64));
            totals.0 = totals.0.saturating_add(record.input_tokens);
            totals.1 = totals.1.saturating_add(record.output_tokens);
        }
        daily
    }

    #[must_use]
    pub fn distribution(
        records: &[&UsageRecord],
        dimension: UsageDimension,
    ) -> BTreeMap<String, u64> {
        let mut grouped = BTreeMap::new();
        for record in records {
            let key = match dimension {
                UsageDimension::Project => &record.project_id,
                UsageDimension::Provider => &record.provider_id,
                UsageDimension::Runtime => &record.runtime_id,
            };
            let total = grouped.entry(key.clone()).or_insert(0_u64);
            *total = total.saturating_add(record.total_tokens());
        }
        grouped
    }

    #[must_use]
    pub fn usage_csv(records: &[&UsageRecord]) -> String {
        let mut csv = String::from(
            "timestamp,project,provider,runtime,session,input_tokens,output_tokens,total_tokens,usage_reported,source_type,source_id,locator\n",
        );
        for record in records {
            csv.push_str(
                &[
                    rfc3339(record.timestamp_unix_seconds),
                    record.project_id.clone(),
                    record.provider_id.clone(),
                    record.runtime_id.clone(),
                    record.session_id.clone(),
                    record.input_tokens.to_string(),
                    record.output_tokens.to_string(),
                    record.total_tokens().to_string(),
                    record.usage_reported.to_string(),
                    "session".into(),
                    record.session_id.clone(),
                    record.source_locator.clone(),
                ]
                .iter()
                .map(|value| csv_cell(value))
                .collect::<Vec<_>>()
                .join(","),
            );
            csv.push('\n');
        }
        csv
    }

    /// Sessions can exist without recorded token usage, so distributions can be non-empty
    /// while every total is zero;
    /// the normalization scale must stay positive to avoid division by zero.
    #[must_use]
    pub fn chart_scale(daily: &BTreeMap<String, u64>) -> u64 {
        daily.values().copied().filter(|total| *total > 0).max().unwrap_or(1)
    }

    #[must_use]
    pub fn audit_csv(records: &[&AuditRecord]) -> String {
        let mut csv = String::from(
            "timestamp,kind,project,provider,runtime,session,summary,source_type,source_id,locator,timestamp_known\n",
        );
        for record in records {
            let row = [
                if record.timestamp_known {
                    rfc3339(record.timestamp_unix_seconds)
                } else {
                    String::new()
                },
                record.kind.label().to_owned(),
                record.source.project_id.clone(),
                record.source.provider_id.clone(),
                record.source.runtime_id.clone(),
                record.source.session_id.clone(),
                record.summary.clone(),
                record.source.source_type.clone(),
                record.source.source_id.clone(),
                record.source.locator.clone(),
                record.timestamp_known.to_string(),
            ];
            csv.push_str(
                &row.into_iter().map(|value| csv_cell(&value)).collect::<Vec<_>>().join(","),
            );
            csv.push('\n');
        }
        csv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(timestamp: u64, project: &str, input: u64, output: u64) -> UsageRecord {
        UsageRecord {
            project_id: project.into(),
            provider_id: "provider".into(),
            runtime_id: "codex".into(),
            session_id: format!("s-{timestamp}"),
            timestamp_unix_seconds: timestamp,
            input_tokens: input,
            output_tokens: output,
            usage_reported: true,
            source_locator: format!("sessions/s-{timestamp}.md#front-matter:usage"),
        }
    }

    fn replay(body: &str) -> SessionReplay {
        SessionReplay {
            metadata: SessionMetadata {
                session_id: "session".into(),
                project_id: "project".into(),
                runtime_profile_id: "provider".into(),
                backend_id: Some("codex".into()),
                started_at_unix_seconds: 0,
                completed_at_unix_seconds: Some(1),
                status: circuitfabric_project::SessionStatus::Completed,
                usage: circuitfabric_project::SessionUsage::default(),
                citations: Vec::new(),
            },
            body: body.into(),
        }
    }

    fn change_set() -> StoredChangeSet {
        StoredChangeSet {
            schema_version: 1,
            id: "cs".into(),
            base_snapshot_hash: "base".into(),
            target_snapshot_hash: "target".into(),
            plan_hash: "plan".into(),
            ir_diff: Vec::new(),
            evidence: Vec::new(),
            observed_snapshot_hash: Some("base".into()),
            rollback_handle: Some("rollback://available".into()),
            execution: circuitfabric_project::ChangeSetExecutionReport {
                write: circuitfabric_project::ChangeSetStage {
                    status: circuitfabric_project::ChangeSetStageStatus::Passed,
                    detail: "written".into(),
                },
                verification: circuitfabric_project::ChangeSetStage {
                    status: circuitfabric_project::ChangeSetStageStatus::Failed,
                    detail: "mismatch".into(),
                },
                ..Default::default()
            },
            audit: vec![circuitfabric_project::ChangeSetAuditEntry {
                timestamp_unix_seconds: 10,
                actor: "operator".into(),
                decision: "approved".into(),
                reason: "reviewed".into(),
                observed_snapshot_hash: Some("base".into()),
                rollback_handle: Some("handle".into()),
            }],
        }
    }

    #[test]
    fn every_chart_and_usage_export_use_the_same_period_and_query() {
        let now = 40 * 86_400;
        let model = UsageAuditModel {
            usage: vec![
                usage(34 * 86_400, "selected", 10, 4),
                usage(34 * 86_400, "other", 90, 90),
                usage(20 * 86_400, "selected", 70, 80),
                usage(now + 1, "selected", 100, 100),
            ],
            ..Default::default()
        };
        let filtered = model.filtered_usage(UsagePeriod::Last7Days, " SELECTED ", now);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            UsageAuditModel::daily_input_output(&filtered).values().copied().collect::<Vec<_>>(),
            [(10, 4)]
        );
        for dimension in
            [UsageDimension::Project, UsageDimension::Provider, UsageDimension::Runtime]
        {
            assert_eq!(
                UsageAuditModel::distribution(&filtered, dimension).values().sum::<u64>(),
                14
            );
        }
        assert_eq!(
            UsageAuditModel::aggregate_usage(&filtered).values().copied().collect::<Vec<_>>(),
            [(10, 4, 14)]
        );
        let csv = UsageAuditModel::usage_csv(&filtered);
        assert_eq!(csv.lines().count(), 2);
        assert!(csv.contains(&filtered[0].source_locator));
        assert!(!csv.contains("other"));
        assert!(UsagePeriod::Last7Days.includes(now - 7 * 86_400, now));
        assert!(!UsagePeriod::Last7Days.includes(now - 7 * 86_400 - 1, now));
    }

    #[test]
    fn empty_zero_and_extreme_values_preserve_totals_and_exports() {
        let empty = UsageAuditModel::default();
        let rows = empty.filtered_usage(UsagePeriod::All, "", 0);
        assert!(UsageAuditModel::daily_input_output(&rows).is_empty());
        assert!(UsageAuditModel::distribution(&rows, UsageDimension::Project).is_empty());
        assert_eq!(UsageAuditModel::usage_csv(&rows).lines().count(), 1);
        assert_eq!(UsageAuditModel::audit_csv(&[]).lines().count(), 1);
        let zero = usage(0, "zero", 0, 0);
        let rows = [&zero];
        assert_eq!(UsageAuditModel::daily_input_output(&rows)["1970-01-01"], (0, 0));
        assert_eq!(UsageAuditModel::distribution(&rows, UsageDimension::Project)["zero"], 0);
        let extreme = usage(0, "max", u64::MAX, 1);
        assert_eq!(extreme.total_tokens(), u64::MAX);
        assert_eq!(
            UsageAuditModel::aggregate_usage(&[&extreme, &extreme]).values().next().unwrap().2,
            u64::MAX
        );
    }

    #[test]
    fn reported_zero_differs_from_unavailable_and_messages_cannot_create_events() {
        let absent = replay(
            "- **1970-01-01T00:00:00Z** · 智能体 · approval rollback verification\n- approval requested\n",
        );
        assert!(!UsageRecord::from_session(&absent, "s.md").usage_reported);
        assert!(
            AuditRecord::from_session(&absent)
                .iter()
                .all(|record| record.kind == AuditKind::Session)
        );
        let zero = replay("- **1970-01-01T00:00:00Z** · 用量 · 输入 0 tokens · 输出 0 tokens\n");
        assert!(UsageRecord::from_session(&zero, "s.md").usage_reported);
        assert_eq!(AuditRecord::from_session(&zero).len(), 3);
        assert!(session_event_header("- **1970-01-01X00:00:00Z** · 审批 · yes").is_none());
        assert!(parse_rfc3339("2026-02-30T00:00:00Z").is_none());
    }

    #[test]
    fn changesets_preserve_context_and_never_invent_rollback_or_event_times() {
        let mut cs = change_set();
        let model =
            UsageAuditModel { audit: AuditRecord::from_change_set("p", &cs), ..Default::default() };
        assert_eq!(model.audit.len(), 3);
        assert_eq!(
            model.filtered_audit(UsagePeriod::All, AuditKindFilter::Rollbacks, "", 10).len(),
            0
        );
        let approval = model.filtered_audit(
            UsagePeriod::Last7Days,
            AuditKindFilter::Approvals,
            "plan=plan",
            10,
        );
        assert_eq!(approval.len(), 1);
        assert!(approval[0].source.locator.ends_with("#/audit/0"));
        let stage =
            model.filtered_audit(UsagePeriod::All, AuditKindFilter::Verifications, "cs", 10);
        assert_eq!(stage.len(), 1);
        assert!(!stage[0].timestamp_known);
        assert_eq!(stage[0].source.session_id, "not-recorded");
        assert!(UsageAuditModel::audit_csv(&stage).contains("\"\",\"Verification\""));
        assert!(
            model
                .filtered_audit(UsagePeriod::Last30Days, AuditKindFilter::Verifications, "", 10)
                .is_empty()
        );
        cs.audit[0].decision = "rollback_executed".into();
        assert_eq!(AuditRecord::from_change_set("p", &cs)[0].kind, AuditKind::Rollback);
    }

    #[test]
    fn pagination_covers_all_records_and_export_is_not_limited_to_a_page() {
        let row = AuditRecord::from_session(&replay(""))[0].clone();
        let model = UsageAuditModel { audit: vec![row; 237], ..Default::default() };
        let rows = model.filtered_audit(UsagePeriod::All, AuditKindFilter::All, "", 1);
        let mut visited = Vec::new();
        for page in 0..rows.len().div_ceil(AUDIT_PAGE_SIZE) {
            visited.extend(audit_page_range(rows.len(), page));
        }
        assert_eq!(visited, (0..237).collect::<Vec<_>>());
        assert_eq!(audit_page_range(0, usize::MAX), 0..0);
        assert_eq!(audit_page_range(3, usize::MAX), 0..3);
        assert_eq!(audit_page_range(237, usize::MAX), 200..237);
        assert_eq!(UsageAuditModel::audit_csv(&rows).lines().count(), 238);
    }

    #[test]
    fn csv_preserves_quotes_newlines_and_source_identifiers() {
        let mut row = AuditRecord::from_session(&replay(""))[0].clone();
        row.summary = "审核, \"通过\"\n第二行".into();
        row.source.locator = "sessions/test.md#body-line:99".into();
        let csv = UsageAuditModel::audit_csv(&[&row]);
        assert!(csv.contains("\"审核, \"\"通过\"\"\n第二行\""));
        assert!(csv.contains("sessions/test.md#body-line:99"));
        assert!(csv.contains("\"session\",\"session\""));
    }

    #[test]
    fn aggregation_keeps_provider_and_runtime_as_separate_dimensions() {
        let model = UsageAuditModel {
            usage: vec![
                UsageRecord {
                    project_id: "p".into(),
                    provider_id: "provider-a".into(),
                    runtime_id: "runtime-x".into(),
                    session_id: "one".into(),
                    timestamp_unix_seconds: 100,
                    input_tokens: 2,
                    output_tokens: 3,
                    usage_reported: true,
                    source_locator: "sessions/one.md#front-matter:usage".into(),
                },
                UsageRecord {
                    project_id: "p".into(),
                    provider_id: "provider-a".into(),
                    runtime_id: "runtime-y".into(),
                    session_id: "two".into(),
                    timestamp_unix_seconds: 100,
                    input_tokens: 5,
                    output_tokens: 7,
                    usage_reported: true,
                    source_locator: "sessions/two.md#front-matter:usage".into(),
                },
            ],
            audit: Vec::new(),
            diagnostics: Vec::new(),
        };
        let records = model.filtered_usage(UsagePeriod::All, "", 200);
        let grouped = UsageAuditModel::aggregate_usage(&records);
        assert_eq!(grouped.len(), 2);
        assert_eq!(
            grouped.get(&("p".into(), "provider-a".into(), "runtime-x".into())),
            Some(&(2, 3, 5))
        );
    }

    #[test]
    fn chart_scale_stays_positive_when_no_tokens_were_recorded() {
        let model = UsageAuditModel {
            usage: vec![UsageRecord {
                project_id: "p".into(),
                provider_id: "provider".into(),
                runtime_id: "runtime".into(),
                session_id: "session".into(),
                timestamp_unix_seconds: 100,
                input_tokens: 0,
                output_tokens: 0,
                usage_reported: true,
                source_locator: "sessions/session.md#front-matter:usage".into(),
            }],
            audit: Vec::new(),
            diagnostics: Vec::new(),
        };
        let records = model.filtered_usage(UsagePeriod::All, "", 200);
        let daily = UsageAuditModel::distribution(&records, UsageDimension::Project);
        // The zero-token session still occupies a day; scaling by that zero total would
        // panic the usage view's bar-width division.
        assert!(!daily.is_empty());
        assert_eq!(UsageAuditModel::chart_scale(&daily), 1);

        let mut mixed = daily;
        mixed.insert("2026-09-22".into(), 500);
        assert_eq!(UsageAuditModel::chart_scale(&mixed), 500);
    }

    #[test]
    fn export_retains_every_source_column() {
        let record = AuditRecord {
            timestamp_unix_seconds: 0,
            timestamp_known: true,
            kind: AuditKind::Approval,
            summary: "accepted".into(),
            source: AuditSourceContext {
                project_id: "p".into(),
                session_id: "s".into(),
                provider_id: "provider".into(),
                runtime_id: "runtime".into(),
                source_type: "session".into(),
                source_id: "s".into(),
                locator: "sessions/s.md#body-line:1".into(),
            },
        };
        let csv = UsageAuditModel::audit_csv(&[&record]);
        assert!(csv.contains("project,provider,runtime,session"));
        assert!(csv.contains("\"p\",\"provider\",\"runtime\",\"s\""));
    }

    #[test]
    fn audit_filter_never_drops_its_source_context() {
        let replay = SessionReplay {
            metadata: SessionMetadata {
                session_id: "session-7".into(),
                project_id: "project-7".into(),
                runtime_profile_id: "provider-7".into(),
                backend_id: Some("runtime-7".into()),
                started_at_unix_seconds: 0,
                completed_at_unix_seconds: Some(0),
                status: circuitfabric_project::SessionStatus::Completed,
                usage: circuitfabric_project::SessionUsage::default(),
                citations: Vec::new(),
            },
            body: "- **1970-01-01T00:00:00Z** · Approval · accepted\n".into(),
        };
        let model = UsageAuditModel {
            audit: AuditRecord::from_session(&replay),
            ..UsageAuditModel::default()
        };
        let records =
            model.filtered_audit(UsagePeriod::All, AuditKindFilter::Approvals, "project-7", 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source.session_id, "session-7");
        assert_eq!(records[0].source.provider_id, "provider-7");
        assert_eq!(records[0].source.runtime_id, "runtime-7");
    }
}
