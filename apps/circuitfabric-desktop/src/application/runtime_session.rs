//! Persist observed runtime counters and immutable event text independently of GPUI.
use circuitfabric_codex_runtime::execution::TaskTokenUsage;
use circuitfabric_project::{
    ProjectStorage, SessionActor, SessionCategory, SessionEvent, SessionEventKind, SessionSeed,
    SessionStatus, SessionUsage,
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
    start_for(storage, provider_id, runtime_id, SessionCategory::Runtime, None)
}

pub fn start_for(
    storage: &ProjectStorage,
    provider_id: &str,
    runtime_id: &str,
    category: SessionCategory,
    subject_id: Option<String>,
) -> Result<String, String> {
    let session_id = format!(
        "session_{}_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(),
        NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
    );
    storage
        .start_categorized_session(
            SessionSeed {
                session_id: session_id.clone(),
                runtime_profile_id: provider_id.into(),
                backend_id: Some(runtime_id.into()),
            },
            category,
            subject_id,
        )
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
    let previous =
        storage.load_session(session_id).map_err(|error| error.to_string())?.metadata.usage;
    let latest = usage.unwrap_or_default();
    let total = SessionUsage {
        input_tokens: previous.input_tokens.saturating_add(latest.input_tokens),
        output_tokens: previous.output_tokens.saturating_add(latest.output_tokens),
    };
    if let Err(error) = storage.complete_session(session_id, total, Vec::new(), status) {
        errors.push(error.to_string());
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

fn excerpt(text: &str) -> String {
    text.to_owned()
}

/// Final job state, independent of whether its last model turn succeeded.
pub fn settle(
    storage: &ProjectStorage,
    session_id: &str,
    result: Result<&str, &str>,
) -> Result<(), String> {
    let metadata = storage.load_session(session_id).map_err(|error| error.to_string())?.metadata;
    storage
        .append_session_event(
            session_id,
            &SessionEvent {
                timestamp_unix_seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                kind: SessionEventKind::Note {
                    text: match result {
                        Ok(text) => text.into(),
                        Err(error) => format!("提取任务暂停：{error}"),
                    },
                },
            },
        )
        .map_err(|error| error.to_string())?;
    storage
        .complete_session(
            session_id,
            metadata.usage,
            metadata.citations,
            if result.is_ok() { SessionStatus::Completed } else { SessionStatus::Failed },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
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
        let id = start_for(
            &storage,
            "provider",
            "codex",
            SessionCategory::Datasheet,
            Some("doc-1".into()),
        )
        .unwrap();
        let long_reply = "完整上下文".repeat(1000);
        finish(
            &storage,
            &id,
            "first",
            Ok(&long_reply),
            Some(TaskTokenUsage { input_tokens: 10, output_tokens: 2 }),
        )
        .unwrap();
        storage.resume_session(&id).unwrap();
        finish(
            &storage,
            &id,
            "continue",
            Err("quota"),
            Some(TaskTokenUsage { input_tokens: 4, output_tokens: 1 }),
        )
        .unwrap();
        let resumed = ProjectStorage::open(&root).unwrap().load_session(&id).unwrap();
        assert_eq!(resumed.metadata.usage.input_tokens, 14);
        assert_eq!(resumed.metadata.usage.output_tokens, 3);
        storage.resume_session(&id).unwrap();
        settle(&storage, &id, Err("Jev quota")).unwrap();
        let settled = storage.load_session(&id).unwrap();
        assert_eq!(settled.metadata.status, SessionStatus::Failed);
        assert_eq!(settled.metadata.usage.input_tokens, 14);
        assert!(settled.body.contains("Jev quota"));
        assert!(resumed.body.contains(&long_reply));
        assert_eq!(
            storage
                .list_sessions_for(SessionCategory::Datasheet, Some("doc-1"))
                .unwrap()
                .sessions
                .len(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
