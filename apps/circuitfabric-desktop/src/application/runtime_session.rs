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

pub fn append_note(storage: &ProjectStorage, id: &str, text: &str) -> Result<(), String> {
    append_observation(storage, id, SessionEventKind::Note { text: text.into() })
}

pub fn append_reasoning(storage: &ProjectStorage, id: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    append_observation(storage, id, SessionEventKind::ReasoningSummary { text: text.into() })
}

pub fn append_note_reply(storage: &ProjectStorage, id: &str, text: &str) -> Result<(), String> {
    append_observation(
        storage,
        id,
        SessionEventKind::Turn { actor: SessionActor::Agent, message: text.into() },
    )
}

fn append_observation(
    storage: &ProjectStorage,
    id: &str,
    kind: SessionEventKind,
) -> Result<(), String> {
    storage
        .append_session_event(
            id,
            &SessionEvent {
                timestamp_unix_seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                kind,
            },
        )
        .map_err(|e| e.to_string())
}

/// Resolve the original provider instead of the current default. The native runner
/// checks the saved model/endpoint/tool scope before sending the next turn.
pub fn followup_settings(
    storage: &ProjectStorage,
    session_id: &str,
    mut settings: circuitfabric_codex_runtime::RuntimeSettings,
) -> Result<circuitfabric_codex_runtime::RuntimeSettings, String> {
    let metadata = storage.load_session(session_id).map_err(|error| error.to_string())?.metadata;
    if !matches!(metadata.category, SessionCategory::Runtime | SessionCategory::Datasheet) {
        return Err("此类会话不支持在此入口续聊".into());
    }
    if !matches!(metadata.backend_id.as_deref(), Some("codex" | "codex-app-server")) {
        let runtime = match metadata.backend_id.as_deref() {
            Some("claude" | "claude-code") => "Claude Code",
            Some("kimi" | "kimi-code") => "Kimi Code",
            Some("dsh") => "DSH",
            _ => "该运行时",
        };
        return Err(format!("当前应用尚未接入 {runtime} 的原生会话恢复；此记录仅供查看。"));
    }
    let history =
        storage.session_runtime_directory(session_id).map_err(|error| error.to_string())?;
    if !history.join("circuitfabric-thread.json").is_file() {
        return Err("此记录缺少 Codex 原生会话历史，无法恢复完整上下文".into());
    }
    if !settings.providers.iter().any(|p| p.id == metadata.runtime_profile_id && p.enabled) {
        return Err(format!(
            "原会话的 Provider「{}」已删除或停用，请先恢复配置",
            metadata.runtime_profile_id
        ));
    }
    settings.adapters.codex_provider_id = metadata.runtime_profile_id;
    Ok(settings)
}

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
#[cfg(test)]
pub fn finish(
    storage: &ProjectStorage,
    session_id: &str,
    prompt: &str,
    result: Result<&str, &str>,
    reported: Option<TaskTokenUsage>,
) -> Result<(), String> {
    finish_turn(storage, session_id, Some(prompt), result, reported)
}

/// Persist the human message before launching a resumable follow-up.
pub fn begin_followup(
    storage: &ProjectStorage,
    session_id: &str,
    prompt: &str,
) -> Result<(), String> {
    storage.resume_session(session_id).map_err(|error| error.to_string())?;
    storage
        .append_session_event(
            session_id,
            &SessionEvent {
                timestamp_unix_seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                kind: SessionEventKind::Turn { actor: SessionActor::User, message: prompt.into() },
            },
        )
        .map_err(|error| error.to_string())
}

/// The prompt was already persisted by `begin_followup`.
pub fn finish_followup(
    storage: &ProjectStorage,
    session_id: &str,
    result: Result<&str, &str>,
    reported: Option<TaskTokenUsage>,
) -> Result<(), String> {
    finish_turn(storage, session_id, None, result, reported)
}

fn finish_turn(
    storage: &ProjectStorage,
    session_id: &str,
    prompt: Option<&str>,
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
    let mut kinds = Vec::new();
    if let Some(prompt) = prompt {
        kinds.push(SessionEventKind::Turn { actor: SessionActor::User, message: excerpt(prompt) });
    }
    kinds.extend([
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
    ]);
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
    fn followups_keep_original_provider_document_and_usage_and_require_native_history() {
        let root = std::env::temp_dir().join(format!(
            "cf-followup-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = ProjectStorage::create(
            &root,
            circuitfabric_contracts::Project {
                id: "original-project".into(),
                name: "original".into(),
                description: None,
            },
        )
        .unwrap();
        let mut settings = circuitfabric_codex_runtime::RuntimeSettings::default();
        let original = settings.providers[0].id.clone();
        let mut replacement = settings.providers[0].clone();
        replacement.id = "new-default".into();
        settings.default_provider_id = replacement.id.clone();
        settings.adapters.codex_provider_id = replacement.id.clone();
        settings.providers.push(replacement);
        for category in [SessionCategory::Datasheet, SessionCategory::Runtime] {
            let id = start_for(&storage, &original, "codex", category, Some("doc-original".into()))
                .unwrap();
            finish(
                &storage,
                &id,
                "first question",
                Ok("first reply"),
                Some(TaskTokenUsage { input_tokens: 10, output_tokens: 2 }),
            )
            .unwrap();
            assert!(
                followup_settings(&storage, &id, settings.clone())
                    .unwrap_err()
                    .contains("原生会话历史")
            );
            let history = storage.session_runtime_directory(&id).unwrap();
            std::fs::create_dir_all(&history).unwrap();
            std::fs::write(history.join("circuitfabric-thread.json"), "{}").unwrap();
            let resolved = followup_settings(&storage, &id, settings.clone()).unwrap();
            assert_eq!(resolved.adapters.codex_provider_id, original);
            let mut removed = settings.clone();
            removed.providers.retain(|p| p.id != original);
            assert!(
                followup_settings(&storage, &id, removed).unwrap_err().contains("已删除或停用")
            );
            begin_followup(&storage, &id, "human supplement\nsecond line").unwrap();
            let pending = ProjectStorage::open(&root).unwrap().load_session(&id).unwrap();
            assert!(pending.body.contains("human supplement"));
            assert_eq!(pending.metadata.status, SessionStatus::Running);
            finish_followup(
                &storage,
                &id,
                Ok("# second reply"),
                Some(TaskTokenUsage { input_tokens: 4, output_tokens: 1 }),
            )
            .unwrap();
            let resumed = ProjectStorage::open(&root).unwrap().load_session(&id).unwrap();
            assert_eq!(resumed.metadata.category, category);
            assert_eq!(resumed.metadata.subject_id.as_deref(), Some("doc-original"));
            assert_eq!(resumed.metadata.usage.input_tokens, 14);
            assert_eq!(resumed.metadata.usage.output_tokens, 3);
            assert_eq!(resumed.body.matches("human supplement").count(), 1);
            assert!(
                resumed.body.contains("first reply") && resumed.body.contains("# second reply")
            );
        }
        for (backend, label) in
            [("claude-code", "Claude Code"), ("kimi-code", "Kimi Code"), ("dsh", "DSH")]
        {
            let unsupported = start(&storage, &original, backend).unwrap();
            let error = followup_settings(&storage, &unsupported, settings.clone()).unwrap_err();
            assert!(error.contains(label) && error.contains("尚未接入"));
            assert!(!error.contains("Codex"), "the reason must describe this runtime");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

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
