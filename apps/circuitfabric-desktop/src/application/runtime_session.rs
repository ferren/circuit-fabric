//! Persist observed runtime counters and immutable event text independently of GPUI.
use circuitfabric_codex_runtime::execution::TaskTokenUsage;
use circuitfabric_project::{
    ProjectStorage, SessionActor, SessionEvent, SessionEventKind, SessionSeed, SessionStatus,
    SessionUsage,
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

pub fn start(
    storage: &ProjectStorage,
    provider_id: &str,
    runtime_id: &str,
) -> Result<String, String> {
    let session_id = format!(
        "session_{}_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(),
        NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
    );
    storage
        .start_session(SessionSeed {
            session_id: session_id.clone(),
            runtime_profile_id: provider_id.into(),
            backend_id: Some(runtime_id.into()),
        })
        .map_err(|error| error.to_string())?;
    Ok(session_id)
}

/// Retains already observed usage even when the task fails or is cancelled.
pub fn finish(
    storage: &ProjectStorage,
    session_id: &str,
    prompt: &str,
    result: Result<&str, &str>,
    reported: Option<TaskTokenUsage>,
) -> Result<(), String> {
    let timestamp_unix_seconds =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let status = if result.is_ok() { SessionStatus::Completed } else { SessionStatus::Failed };
    let usage = reported.map(|value| SessionUsage {
        input_tokens: value.input_tokens,
        output_tokens: value.output_tokens,
    });
    let kinds = [
        SessionEventKind::Turn { actor: SessionActor::User, message: excerpt(prompt) },
        match result {
            Ok(output) => {
                SessionEventKind::Turn { actor: SessionActor::Agent, message: excerpt(output) }
            }
            Err(error) => {
                SessionEventKind::Note { text: format!("任务失败：{}", excerpt(error)) }
            }
        },
        usage.map_or_else(
            || SessionEventKind::Note {
                text: "Token usage unavailable: runtime did not report counters".into(),
            },
            |usage| SessionEventKind::UsageRecorded { usage },
        ),
        SessionEventKind::StatusChanged { from: SessionStatus::Running, to: status },
    ];
    let mut errors = Vec::new();
    for kind in kinds {
        if let Err(error) =
            storage.append_session_event(session_id, &SessionEvent { timestamp_unix_seconds, kind })
        {
            errors.push(error.to_string());
        }
    }
    if let Err(error) =
        storage.complete_session(session_id, usage.unwrap_or_default(), Vec::new(), status)
    {
        errors.push(error.to_string());
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

fn excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let mut value: String = chars.by_ref().take(2000).collect();
    if chars.next().is_some() {
        value.push('…');
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::usage_audit::UsageRecord;
    #[test]
    fn successful_failed_zero_and_unavailable_runs_round_trip_with_identity() {
        let root = std::env::temp_dir().join(format!(
            "cf-observed-session-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = ProjectStorage::create(
            &root,
            circuitfabric_contracts::Project {
                id: "p".into(),
                name: "p".into(),
                description: None,
            },
        )
        .unwrap();
        for (result, reported) in [
            (Ok("done"), Some(TaskTokenUsage::default())),
            (Err("cancelled"), Some(TaskTokenUsage { input_tokens: 23, output_tokens: 5 })),
            (Err("no response"), None),
        ] {
            let session_id = start(&storage, "provider", "codex-app-server").unwrap();
            finish(&storage, &session_id, "文档提取 / document=doc-1", result, reported).unwrap();
            let reopened = ProjectStorage::open(&root).unwrap();
            let replay = reopened.load_session(&session_id).unwrap();
            assert_eq!(
                replay.metadata.status,
                if result.is_ok() { SessionStatus::Completed } else { SessionStatus::Failed }
            );
            assert_eq!(
                replay.metadata.usage.input_tokens,
                reported.map_or(0, |value| value.input_tokens)
            );
            assert_eq!(
                replay.metadata.usage.output_tokens,
                reported.map_or(0, |value| value.output_tokens)
            );
            assert_eq!(replay.metadata.runtime_profile_id, "provider");
            assert_eq!(replay.metadata.backend_id.as_deref(), Some("codex-app-server"));
            assert_eq!(
                UsageRecord::from_session(&replay, "s.md").usage_reported,
                reported.is_some()
            );
            assert!(replay.body.contains("document=doc-1"));
        }
        assert_eq!(storage.list_sessions().unwrap().sessions.len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }
}
