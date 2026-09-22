//! Read-only projections for the desktop usage and audit screens.
//!
//! Session files remain the source of truth.  This module deliberately builds two separate
//! in-memory projections from them: token usage can be aggregated, while audit entries retain
//! the source context needed to inspect an immutable event.

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use circuitfabric_project::{SessionMetadata, SessionReplay, rfc3339};

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
        }
    }
}

impl UsageRecord {
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
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
            Self::Verifications => Self::All,
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
}

/// A rendered event is immutable: it contains no edit state or mutation operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord {
    pub timestamp_unix_seconds: u64,
    pub kind: AuditKind,
    pub summary: String,
    pub source: AuditSourceContext,
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
        };
        let mut records = Vec::new();
        for raw in replay.body.lines() {
            let line = raw.trim();
            if !line.starts_with('-') {
                continue;
            }
            let timestamp =
                timestamp_from_markdown_line(line).unwrap_or(metadata.started_at_unix_seconds);
            records.push(Self {
                timestamp_unix_seconds: timestamp,
                kind: AuditKind::from_event_text(line),
                summary: line.trim_start_matches('-').trim().to_owned(),
                source: source.clone(),
            });
        }
        if records.is_empty() {
            records.push(Self {
                timestamp_unix_seconds: metadata.started_at_unix_seconds,
                kind: AuditKind::Session,
                summary: format!("Session {} started", metadata.session_id),
                source,
            });
        }
        records
    }
}

fn timestamp_from_markdown_line(line: &str) -> Option<u64> {
    let value = line.split("**").nth(1)?;
    parse_rfc3339(value)
}

fn parse_rfc3339(value: &str) -> Option<u64> {
    // Convert the timestamp through the same stable calendar rendering used by session storage.
    // Audit files contain only second-resolution UTC timestamps, so a small bounded search keeps
    // this module dependency-free while accepting exactly that wire format.
    let start = value.get(..10)?;
    let year = start.get(..4)?.parse::<u64>().ok()?;
    if !(1970..=9999).contains(&year) || value.len() != 20 {
        return None;
    }
    // The session timestamp is also present in a sortable ISO string.  Search only within the
    // corresponding calendar year; at most 366 * 24 * 60 probes in the uncommon parsed-event path.
    let jan_1 = (year - 1970) * 365 * 86_400 + leap_years_before(year) * 86_400;
    let limit = jan_1 + if is_leap(year) { 366 } else { 365 } * 86_400;
    let mut candidate = jan_1;
    while candidate < limit {
        if rfc3339(candidate).get(..10) == Some(start) {
            let time = value.get(11..19)?;
            let hour = time.get(..2)?.parse::<u64>().ok()?;
            let minute = time.get(3..5)?.parse::<u64>().ok()?;
            let second = time.get(6..)?.parse::<u64>().ok()?;
            return (hour < 24 && minute < 60 && second < 60)
                .then_some(candidate + hour * 3600 + minute * 60 + second);
        }
        candidate += 86_400;
    }
    None
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
                            "{} {} {} {}",
                            record.project_id,
                            record.provider_id,
                            record.runtime_id,
                            record.session_id
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
                period.includes(record.timestamp_unix_seconds, now)
                    && kind.includes(record.kind)
                    && (query.is_empty()
                        || format!(
                            "{} {} {} {} {}",
                            record.summary,
                            record.source.project_id,
                            record.source.provider_id,
                            record.source.runtime_id,
                            record.source.session_id
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
                .or_insert((0, 0, 0));
            totals.0 += record.input_tokens;
            totals.1 += record.output_tokens;
            totals.2 += record.total_tokens();
        }
        grouped
    }

    #[must_use]
    pub fn daily_totals(records: &[&UsageRecord]) -> BTreeMap<String, u64> {
        let mut totals = BTreeMap::new();
        for record in records {
            let day = rfc3339(record.timestamp_unix_seconds)[..10].to_owned();
            *totals.entry(day).or_insert(0) += record.total_tokens();
        }
        totals
    }

    /// The trend chart scales bars against the busiest day.  Sessions can exist without
    /// recorded token usage, so `daily_totals` can be non-empty while every total is zero;
    /// the scale must stay positive or the bar-width division divides by zero.
    #[must_use]
    pub fn chart_scale(daily: &BTreeMap<String, u64>) -> u64 {
        daily.values().copied().filter(|total| *total > 0).max().unwrap_or(1)
    }

    #[must_use]
    pub fn audit_csv(records: &[&AuditRecord]) -> String {
        let mut csv = String::from("timestamp,kind,project,provider,runtime,session,summary\n");
        for record in records {
            let row = [
                rfc3339(record.timestamp_unix_seconds),
                record.kind.label().to_owned(),
                record.source.project_id.clone(),
                record.source.provider_id.clone(),
                record.source.runtime_id.clone(),
                record.source.session_id.clone(),
                record.summary.clone(),
            ];
            csv.push_str(
                &row.into_iter()
                    .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            csv.push('\n');
        }
        csv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                },
                UsageRecord {
                    project_id: "p".into(),
                    provider_id: "provider-a".into(),
                    runtime_id: "runtime-y".into(),
                    session_id: "two".into(),
                    timestamp_unix_seconds: 100,
                    input_tokens: 5,
                    output_tokens: 7,
                },
            ],
            audit: Vec::new(),
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
            }],
            audit: Vec::new(),
        };
        let records = model.filtered_usage(UsagePeriod::All, "", 200);
        let daily = UsageAuditModel::daily_totals(&records);
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
            kind: AuditKind::Approval,
            summary: "accepted".into(),
            source: AuditSourceContext {
                project_id: "p".into(),
                session_id: "s".into(),
                provider_id: "provider".into(),
                runtime_id: "runtime".into(),
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
        let model =
            UsageAuditModel { usage: Vec::new(), audit: AuditRecord::from_session(&replay) };
        let records =
            model.filtered_audit(UsagePeriod::All, AuditKindFilter::Approvals, "project-7", 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source.session_id, "session-7");
        assert_eq!(records[0].source.provider_id, "provider-7");
        assert_eq!(records[0].source.runtime_id, "runtime-7");
    }
}
