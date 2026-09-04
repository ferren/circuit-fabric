//! Immutable-per-session Markdown audit records under `sessions/`.
//!
//! Every agent/EDA session is persisted as one Markdown file named
//! `<YYYY-MM-DDTHH-mm-ssZ>--<session-id>.md` with a YAML front matter block (schema version,
//! identity, timing, status, usage, citations) followed by a readable event log. Session
//! creation and metadata transitions write a temporary file and rename it into place, so a
//! crash never leaves a half-written official record — only detectable temporary files.

use std::{
    ffi::OsStr,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use circuitfabric_contracts::{ProjectId, SessionId};
use serde::{Deserialize, Serialize};

use crate::ProjectStorageError;

/// Schema version of the session Markdown front matter.
pub const SESSION_MARKDOWN_SCHEMA_VERSION: u32 = 1;

const SESSIONS_DIRECTORY: &str = "sessions";
const SUMMARY_HEADING: &str = "## 会话摘要";
const TURNS_HEADING: &str = "## 轮次与工具调用";

/// Lifecycle state persisted in the session front matter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
}

impl SessionStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Token usage recorded for one session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// The YAML front matter of one session record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionMetadata {
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub runtime_profile_id: String,
    pub backend_id: Option<String>,
    pub started_at_unix_seconds: u64,
    pub completed_at_unix_seconds: Option<u64>,
    pub status: SessionStatus,
    pub usage: SessionUsage,
    pub citations: Vec<String>,
}

/// Identity required to start a session record; project identity comes from the root manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSeed {
    pub session_id: SessionId,
    pub runtime_profile_id: String,
    pub backend_id: Option<String>,
}

/// One row of the session listing, parsed from a session file's front matter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub metadata: SessionMetadata,
    pub file_name: String,
    pub byte_size: u64,
}

/// Result of listing a project's session records.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionListing {
    /// Newest session first.
    pub sessions: Vec<SessionSummary>,
    /// Temporary files left behind by interrupted writes; reported, never deleted silently.
    pub orphaned_temp_files: Vec<String>,
}

/// A full session record for read-only replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionReplay {
    pub metadata: SessionMetadata,
    pub body: String,
}

/// The actor a turn is attributed to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionActor {
    User,
    Agent,
}

impl SessionActor {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "用户",
            Self::Agent => "智能体",
        }
    }
}

/// One append-only audit event rendered into the session Markdown body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionEvent {
    pub timestamp_unix_seconds: u64,
    pub kind: SessionEventKind,
}

/// The auditable event shapes a session record can accumulate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEventKind {
    Turn { actor: SessionActor, message: String },
    ToolCall { summary: String },
    Approval { decision: String },
    EvidenceCited { document_id: String, content_hash: String, locator: String },
    UsageRecorded { usage: SessionUsage },
    StatusChanged { from: SessionStatus, to: SessionStatus },
}

impl crate::ProjectStorage {
    /// Creates a new session record with `running` status and the contract's body skeleton.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe session ID, an existing file for this session, or a
    /// write failure.
    pub fn start_session(&self, seed: SessionSeed) -> Result<SessionMetadata, ProjectStorageError> {
        validate_session_id(&seed.session_id)?;
        let started_at = now_unix_seconds();
        let metadata = SessionMetadata {
            session_id: seed.session_id,
            project_id: self.manifest().project.id.clone(),
            runtime_profile_id: seed.runtime_profile_id,
            backend_id: seed.backend_id,
            started_at_unix_seconds: started_at,
            completed_at_unix_seconds: None,
            status: SessionStatus::Running,
            usage: SessionUsage::default(),
            citations: Vec::new(),
        };
        let path = self.session_file_path(&metadata)?;
        if path.exists() {
            return Err(ProjectStorageError::SessionAlreadyExists { path });
        }
        let contents = format!(
            "{}\n\n{}\n\n{}\n",
            render_front_matter(&metadata),
            SUMMARY_HEADING,
            TURNS_HEADING
        );
        write_text_atomically(&path, &contents)?;
        Ok(metadata)
    }

    /// Appends one audit event to a session's turn log.
    ///
    /// Appends are single flushed writes at end of file, so a crash can only ever truncate the
    /// last event line, never the front matter or earlier events.
    ///
    /// # Errors
    ///
    /// Returns an error when the session record is missing or cannot be appended to.
    pub fn append_session_event(
        &self,
        session_id: &str,
        event: &SessionEvent,
    ) -> Result<(), ProjectStorageError> {
        let path = self.find_session_file(session_id)?;
        let mut file = fs::OpenOptions::new().append(true).open(&path).map_err(|source| {
            ProjectStorageError::Io { action: "append session event", path: path.clone(), source }
        })?;
        let line = render_event(event);
        file.write_all(format!("\n{line}").as_bytes()).and_then(|()| file.flush()).map_err(
            |source| ProjectStorageError::Io { action: "append session event", path, source },
        )
    }

    /// Finalizes a session: rewrites only its front matter with completion, usage, citations.
    ///
    /// # Errors
    ///
    /// Returns an error when the session record is missing, unreadable, or cannot be replaced.
    pub fn complete_session(
        &self,
        session_id: &str,
        final_usage: SessionUsage,
        citations: Vec<String>,
        status: SessionStatus,
    ) -> Result<SessionMetadata, ProjectStorageError> {
        if status == SessionStatus::Running {
            return Err(ProjectStorageError::InvalidCompletionStatus);
        }
        let path = self.find_session_file(session_id)?;
        let raw = fs::read_to_string(&path).map_err(|source| ProjectStorageError::Io {
            action: "read session record",
            path: path.clone(),
            source,
        })?;
        let (mut metadata, body) = parse_session_markdown(&raw, &path)?;
        metadata.completed_at_unix_seconds = Some(now_unix_seconds());
        metadata.status = status;
        metadata.usage = final_usage;
        metadata.citations = citations;
        let contents = format!("{}\n{}", render_front_matter(&metadata), body);
        write_text_atomically(&path, &contents)?;
        Ok(metadata)
    }

    /// Lists session records, newest first, plus any detectable interrupted-write leftovers.
    ///
    /// # Errors
    ///
    /// Returns an error when the sessions directory cannot be read.
    pub fn list_sessions(&self) -> Result<SessionListing, ProjectStorageError> {
        let directory = self.resolve_relative_path(SESSIONS_DIRECTORY)?;
        let mut sessions = Vec::new();
        let mut orphaned_temp_files = Vec::new();
        let entries = fs::read_dir(&directory).map_err(|source| ProjectStorageError::Io {
            action: "list sessions",
            path: directory.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| ProjectStorageError::Io {
                action: "list sessions",
                path: directory.clone(),
                source,
            })?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.')
                && Path::new(&file_name).extension() == Some(OsStr::new("tmp"))
            {
                orphaned_temp_files.push(file_name);
                continue;
            }
            if Path::new(&file_name).extension() != Some(OsStr::new("md")) {
                continue;
            }
            let path = entry.path();
            let byte_size = entry.metadata().map_err(|source| ProjectStorageError::Io {
                action: "inspect session record",
                path: path.clone(),
                source,
            })?;
            let raw = fs::read_to_string(&path).map_err(|source| ProjectStorageError::Io {
                action: "read session record",
                path: path.clone(),
                source,
            })?;
            let (metadata, _) = parse_session_markdown(&raw, &path)?;
            sessions.push(SessionSummary { metadata, file_name, byte_size: byte_size.len() });
        }
        sessions.sort_by(|left, right| right.file_name.cmp(&left.file_name));
        orphaned_temp_files.sort();
        Ok(SessionListing { sessions, orphaned_temp_files })
    }

    /// Loads one session record for read-only replay.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing or its record cannot be parsed.
    pub fn load_session(&self, session_id: &str) -> Result<SessionReplay, ProjectStorageError> {
        let path = self.find_session_file(session_id)?;
        let raw = fs::read_to_string(&path).map_err(|source| ProjectStorageError::Io {
            action: "read session record",
            path: path.clone(),
            source,
        })?;
        let (metadata, body) = parse_session_markdown(&raw, &path)?;
        Ok(SessionReplay { metadata, body })
    }

    fn session_file_path(
        &self,
        metadata: &SessionMetadata,
    ) -> Result<PathBuf, ProjectStorageError> {
        let file_name = format!(
            "{}--{}.md",
            file_name_timestamp(metadata.started_at_unix_seconds),
            metadata.session_id
        );
        self.resolve_relative_path(Path::new(SESSIONS_DIRECTORY).join(file_name))
    }

    fn find_session_file(&self, session_id: &str) -> Result<PathBuf, ProjectStorageError> {
        validate_session_id(session_id)?;
        let directory = self.resolve_relative_path(SESSIONS_DIRECTORY)?;
        let suffix = format!("--{session_id}.md");
        let entries = fs::read_dir(&directory).map_err(|source| ProjectStorageError::Io {
            action: "locate session record",
            path: directory.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| ProjectStorageError::Io {
                action: "locate session record",
                path: directory.clone(),
                source,
            })?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.ends_with(&suffix) {
                return Ok(entry.path());
            }
        }
        Err(ProjectStorageError::SessionNotFound { session_id: session_id.to_owned() })
    }
}

fn validate_session_id(session_id: &str) -> Result<(), ProjectStorageError> {
    let safe = !session_id.is_empty()
        && session_id.len() <= 128
        && !session_id.starts_with('.')
        && session_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        });
    if safe {
        Ok(())
    } else {
        Err(ProjectStorageError::InvalidSessionId { session_id: session_id.to_owned() })
    }
}

fn render_front_matter(metadata: &SessionMetadata) -> String {
    use std::fmt::Write as _;

    let mut lines = String::from("---\n");
    let _ = writeln!(lines, "schemaVersion: {SESSION_MARKDOWN_SCHEMA_VERSION}");
    let _ = writeln!(lines, "sessionId: {}", metadata.session_id);
    let _ = writeln!(lines, "projectId: {}", metadata.project_id);
    let _ = writeln!(lines, "runtimeProfileId: {}", metadata.runtime_profile_id);
    match &metadata.backend_id {
        Some(backend_id) => {
            let _ = writeln!(lines, "backendId: {backend_id}");
        }
        None => {
            lines.push_str("backendId: null\n");
        }
    }
    let _ = writeln!(lines, "startedAt: {}", rfc3339(metadata.started_at_unix_seconds));
    match metadata.completed_at_unix_seconds {
        Some(seconds) => {
            let _ = writeln!(lines, "completedAt: {}", rfc3339(seconds));
        }
        None => {
            lines.push_str("completedAt: null\n");
        }
    }
    let _ = writeln!(lines, "status: {}", metadata.status.as_str());
    let _ = writeln!(
        lines,
        "usage: {{ inputTokens: {}, outputTokens: {} }}",
        metadata.usage.input_tokens, metadata.usage.output_tokens
    );
    let _ = writeln!(
        lines,
        "citations: [{}]",
        metadata
            .citations
            .iter()
            .map(|citation| format!("\"{}\"", citation.replace('"', "'")))
            .collect::<Vec<_>>()
            .join(", ")
    );
    lines.push_str("---");
    lines
}

fn parse_session_markdown(
    raw: &str,
    path: &Path,
) -> Result<(SessionMetadata, String), ProjectStorageError> {
    let parse_error =
        |reason: String| ProjectStorageError::ParseSession { path: path.to_owned(), reason };
    let mut lines = raw.lines();
    if lines.next() != Some("---") {
        return Err(parse_error("front matter must open with `---`".to_owned()));
    }
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut body_start = None;
    for (index, line) in lines.enumerate() {
        if line == "---" {
            body_start = Some(index + 2);
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(parse_error(format!("expected `key: value`, found `{line}`")));
        };
        if key.contains(' ') {
            return Err(parse_error(format!("unexpected front matter line `{line}`")));
        }
        fields.push((key.trim().to_owned(), value.trim().to_owned()));
    }
    let Some(body_start) = body_start else {
        return Err(parse_error("front matter must close with `---`".to_owned()));
    };
    let body = raw.lines().skip(body_start).collect::<Vec<_>>().join("\n");

    let field = |name: &str| -> Result<&str, ProjectStorageError> {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| parse_error(format!("missing front matter field `{name}`")))
    };
    if field("schemaVersion")?.parse::<u32>() != Ok(SESSION_MARKDOWN_SCHEMA_VERSION) {
        return Err(parse_error(format!(
            "unsupported session schema version {}",
            field("schemaVersion").unwrap_or_default()
        )));
    }
    let status = match field("status")? {
        "running" => SessionStatus::Running,
        "completed" => SessionStatus::Completed,
        "failed" => SessionStatus::Failed,
        other => return Err(parse_error(format!("unknown session status `{other}`"))),
    };
    let usage = parse_usage(field("usage")?, &parse_error)?;
    let citations = parse_citations(field("citations")?, &parse_error)?;
    let completed_at = parse_optional_timestamp(field("completedAt")?, &parse_error)?;
    let backend_id = parse_optional_text(field("backendId")?);
    Ok((
        SessionMetadata {
            session_id: field("sessionId")?.to_owned(),
            project_id: field("projectId")?.to_owned(),
            runtime_profile_id: field("runtimeProfileId")?.to_owned(),
            backend_id,
            started_at_unix_seconds: parse_rfc3339(field("startedAt")?).ok_or_else(|| {
                parse_error("startedAt must be an RFC 3339 UTC timestamp".to_owned())
            })?,
            completed_at_unix_seconds: completed_at,
            status,
            usage,
            citations,
        },
        body,
    ))
}

fn parse_optional_timestamp(
    value: &str,
    parse_error: &dyn Fn(String) -> ProjectStorageError,
) -> Result<Option<u64>, ProjectStorageError> {
    if value == "null" {
        return Ok(None);
    }
    parse_rfc3339(value)
        .ok_or_else(|| parse_error(format!("`{value}` is not an RFC 3339 UTC timestamp")))
        .map(Some)
}

fn parse_optional_text(value: &str) -> Option<String> {
    match value {
        "null" => None,
        text => Some(text.trim_matches('"').to_owned()),
    }
}

fn parse_usage(
    value: &str,
    parse_error: &dyn Fn(String) -> ProjectStorageError,
) -> Result<SessionUsage, ProjectStorageError> {
    let invalid = || parse_error(format!("`{value}` is not a usage map"));
    let inner =
        value.strip_prefix('{').and_then(|inner| inner.strip_suffix('}')).ok_or_else(invalid)?;
    let mut usage = SessionUsage::default();
    for pair in inner.split(',') {
        let (key, token) = pair.split_once(':').ok_or_else(invalid)?;
        let token = token.trim().parse::<u64>().map_err(|_| invalid())?;
        match key.trim() {
            "inputTokens" => usage.input_tokens = token,
            "outputTokens" => usage.output_tokens = token,
            _ => return Err(invalid()),
        }
    }
    Ok(usage)
}

fn parse_citations(
    value: &str,
    parse_error: &dyn Fn(String) -> ProjectStorageError,
) -> Result<Vec<String>, ProjectStorageError> {
    if value == "[]" {
        return Ok(Vec::new());
    }
    let inner = value
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .ok_or_else(|| parse_error(format!("`{value}` is not a citation list")))?;
    Ok(inner
        .split(',')
        .map(|entry| entry.trim().trim_matches('"').to_owned())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>())
}

fn render_event(event: &SessionEvent) -> String {
    let timestamp = rfc3339(event.timestamp_unix_seconds);
    match &event.kind {
        SessionEventKind::Turn { actor, message } => {
            format!("- **{timestamp}** · {} · {}", actor.as_str(), single_line(message))
        }
        SessionEventKind::ToolCall { summary } => {
            format!("- **{timestamp}** · 工具调用 · {}", single_line(summary))
        }
        SessionEventKind::Approval { decision } => {
            format!("- **{timestamp}** · 审批 · {}", single_line(decision))
        }
        SessionEventKind::EvidenceCited { document_id, content_hash, locator } => format!(
            "- **{timestamp}** · 证据引用 · document={} hash={} locator={}",
            single_line(document_id),
            single_line(content_hash),
            single_line(locator)
        ),
        SessionEventKind::UsageRecorded { usage } => format!(
            "- **{timestamp}** · 用量 · 输入 {} tokens · 输出 {} tokens",
            usage.input_tokens, usage.output_tokens
        ),
        SessionEventKind::StatusChanged { from, to } => {
            format!("- **{timestamp}** · 状态 · {} → {}", from.as_str(), to.as_str())
        }
    }
}

fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

fn write_text_atomically(path: &Path, contents: &str) -> Result<(), ProjectStorageError> {
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("session");
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    fs::write(&temporary, contents).map_err(|source| ProjectStorageError::Io {
        action: "write session record",
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| ProjectStorageError::Io {
        action: "commit session record",
        path: path.to_owned(),
        source,
    })
}

/// Formats unix seconds as an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
#[must_use]
pub fn rfc3339(unix_seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_unix(unix_seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn file_name_timestamp(unix_seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_unix(unix_seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}Z")
}

fn parse_rfc3339(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u64> {
        value.get(range)?.chars().try_fold(0_u64, |accumulator, character| {
            character.to_digit(10).map(|digit| accumulator * 10 + u64::from(digit))
        })
    };
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    if year < 1970
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    days.checked_mul(86_400)?.checked_add(
        hour.checked_mul(3_600)?.checked_add(minute.checked_mul(60)?.checked_add(second)?)?,
    )
}

/// Splits unix seconds into UTC calendar and clock fields.
fn civil_from_unix(unix_seconds: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = unix_seconds / 86_400;
    let seconds_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, seconds_of_day / 3_600, (seconds_of_day % 3_600) / 60, seconds_of_day % 60)
}

/// Howard Hinnant's `civil_from_days`, restricted to non-negative unix days: days since
/// 1970-01-01 to (year, month, day).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shift = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_shift + 2) / 5 + 1;
    let month = if month_shift < 10 { month_shift + 3 } else { month_shift - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Inverse of [`civil_from_days`]; `None` for dates before 1970-01-01.
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    let years_before = year - u64::from(month <= 2);
    let era = years_before / 400;
    let year_of_era = years_before % 400;
    let month_shift = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shift + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era).checked_sub(719_468)
}

fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
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
            .join(format!("circuitfabric-project-sessions-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn seed(session_id: &str) -> SessionSeed {
        SessionSeed {
            session_id: session_id.to_owned(),
            runtime_profile_id: "default".to_owned(),
            backend_id: Some("jlcircuit".to_owned()),
        }
    }

    #[test]
    fn a_session_round_trips_through_markdown_and_survives_reopening() {
        let root = test_root("roundtrip");
        let storage = ProjectStorage::create(&root, project("power-supply")).expect("project");
        let metadata = storage.start_session(seed("session_first")).expect("start session");

        assert_eq!(metadata.status, SessionStatus::Running);
        assert_eq!(metadata.project_id, "power-supply");
        storage
            .append_session_event(
                "session_first",
                &SessionEvent {
                    timestamp_unix_seconds: metadata.started_at_unix_seconds + 5,
                    kind: SessionEventKind::Turn {
                        actor: SessionActor::User,
                        message: "Add an input capacitor\nand keep it stable".to_owned(),
                    },
                },
            )
            .expect("append turn");
        storage
            .append_session_event(
                "session_first",
                &SessionEvent {
                    timestamp_unix_seconds: metadata.started_at_unix_seconds + 9,
                    kind: SessionEventKind::EvidenceCited {
                        document_id: "doc-abc".to_owned(),
                        content_hash: "sha256:00".to_owned(),
                        locator: "documents/datasheets/x.md#line=3".to_owned(),
                    },
                },
            )
            .expect("append citation");
        let final_metadata = storage
            .complete_session(
                "session_first",
                SessionUsage { input_tokens: 1_200, output_tokens: 340 },
                vec!["doc-abc".to_owned()],
                SessionStatus::Completed,
            )
            .expect("complete session");

        let reopened = ProjectStorage::open(&root).expect("reopen project");
        let listing = reopened.list_sessions().expect("list sessions");
        assert_eq!(listing.sessions.len(), 1);
        assert_eq!(listing.orphaned_temp_files, Vec::<String>::new());
        assert_eq!(listing.sessions[0].metadata, final_metadata);
        assert_eq!(listing.sessions[0].metadata.status, SessionStatus::Completed);
        assert_eq!(listing.sessions[0].metadata.usage.input_tokens, 1_200);
        // The file name is a compatibility contract: `<YYYY-MM-DDTHH-mm-ssZ>--<session-id>.md`.
        assert_eq!(
            listing.sessions[0].file_name,
            format!(
                "{}--session_first.md",
                file_name_timestamp(final_metadata.started_at_unix_seconds)
            )
        );

        let replay = reopened.load_session("session_first").expect("load replay");
        assert_eq!(replay.metadata.session_id, "session_first");
        assert!(replay.body.contains("## 会话摘要"));
        assert!(replay.body.contains("## 轮次与工具调用"));
        assert!(replay.body.contains("用户 · Add an input capacitor and keep it stable"));
        assert!(replay.body.contains("document=doc-abc hash=sha256:00"));
        let raw = fs::read_to_string(
            reopened.root().join(SESSIONS_DIRECTORY).join(listing.sessions[0].file_name.clone()),
        )
        .expect("read session file");
        assert!(raw.contains("usage: { inputTokens: 1200, outputTokens: 340 }"));
        assert!(raw.contains("citations: [\"doc-abc\"]"));
        assert!(raw.contains("completedAt: "));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unsafe_session_ids_and_duplicates_are_rejected() {
        let root = test_root("unsafe");
        let storage = ProjectStorage::create(&root, project("unsafe")).expect("project");

        for invalid in ["", "../escape", "session/one", ".hidden", "session one"] {
            assert!(
                matches!(
                    storage.start_session(SessionSeed {
                        session_id: invalid.to_owned(),
                        runtime_profile_id: "default".to_owned(),
                        backend_id: None,
                    }),
                    Err(ProjectStorageError::InvalidSessionId { .. })
                ),
                "`{invalid}` must be rejected"
            );
        }
        storage.start_session(seed("session_dup")).expect("start once");
        let error = storage.start_session(seed("session_dup")).expect_err("duplicate session");
        assert!(matches!(error, ProjectStorageError::SessionAlreadyExists { .. }));
        assert!(matches!(
            storage.load_session("missing"),
            Err(ProjectStorageError::SessionNotFound { .. })
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphaned_temp_files_are_detected_without_touching_records() {
        let root = test_root("orphans");
        let storage = ProjectStorage::create(&root, project("orphans")).expect("project");
        storage.start_session(seed("session_ok")).expect("start session");
        let sessions = storage.root().join("sessions");
        fs::write(sessions.join(".2026-09-04T00-00-00Z--session_crash.md.4242.tmp"), "partial")
            .expect("write leftover temp");

        let listing = storage.list_sessions().expect("list sessions");

        assert_eq!(listing.sessions.len(), 1);
        assert_eq!(
            listing.orphaned_temp_files,
            [".2026-09-04T00-00-00Z--session_crash.md.4242.tmp".to_owned()]
        );
        assert!(sessions.join(".2026-09-04T00-00-00Z--session_crash.md.4242.tmp").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn completing_a_session_requires_a_terminal_status() {
        let root = test_root("complete");
        let storage = ProjectStorage::create(&root, project("complete")).expect("project");
        storage.start_session(seed("session_x")).expect("start session");

        let error = storage
            .complete_session(
                "session_x",
                SessionUsage::default(),
                Vec::new(),
                SessionStatus::Running,
            )
            .expect_err("running is not a completion status");

        assert!(matches!(error, ProjectStorageError::InvalidCompletionStatus));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn timestamps_round_trip_and_span_leap_days() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        // 2024-02-29 is a leap day: 1709164800 = 2024-02-29T00:00:00Z.
        assert_eq!(rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
        for seconds in [0_u64, 951_782_400, 1_709_164_800, 4_102_444_800] {
            assert_eq!(parse_rfc3339(&rfc3339(seconds)), Some(seconds), "round trip for {seconds}");
        }
        assert_eq!(parse_rfc3339("not-a-timestamp"), None);
        assert_eq!(file_name_timestamp(1_709_164_800), "2024-02-29T00-00-00Z");
    }
}
