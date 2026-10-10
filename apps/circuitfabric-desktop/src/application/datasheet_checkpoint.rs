//! Durable extraction state, separate from runtime conversation history.
use std::collections::BTreeMap;
/// Completed steps of an interrupted datasheet extraction. "Continue" reuses them while
/// the document content is unchanged: completed category replies and Jev batches are reused.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct DatasheetCheckpoint {
    pub(crate) content_hash: String,
    pub(crate) session_id: Option<String>,
    pub(crate) model_steps: Vec<(String, String)>,
    /// Invalid category output and diagnostics handed back to the extraction agent.
    pub(crate) model_feedback:
        BTreeMap<String, crate::application::datasheet_recovery::CategoryFeedback>,
    /// `(arguments, result)` per fully validated Jev batch, in call order.
    pub(crate) jev_results: Vec<(serde_json::Value, serde_json::Value)>,
    /// Completed items and pending recovery context inside an unfinished Jev batch.
    pub(crate) judge_recovery:
        BTreeMap<usize, crate::application::datasheet_recovery::JudgeRecovery>,
    /// Cached judgments belong to the backend/model configuration that produced them.
    pub(crate) judge_definition: Option<circuitfabric_codex_runtime::tools::McpServerDefinition>,
}

impl DatasheetCheckpoint {
    fn path(
        storage: &circuitfabric_project::ProjectStorage,
        document_id: &str,
    ) -> Result<std::path::PathBuf, String> {
        use std::fmt::Write as _;
        let mut name = String::new();
        for byte in document_id.bytes() {
            let _ = write!(name, "{byte:02x}");
        }
        if name.is_empty() || name.len() > 220 {
            return Err("文档标识不适用于提取断点".into());
        }
        storage
            .resolve_relative_path(format!("sessions/datasheet/checkpoints/{name}.json"))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn load(
        storage: &circuitfabric_project::ProjectStorage,
        document_id: &str,
    ) -> Result<Option<Self>, String> {
        let path = Self::path(storage, document_id)?;
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("提取断点读取失败：{error}")),
        };
        let saved: SavedCheckpoint =
            serde_json::from_slice(&raw).map_err(|error| format!("提取断点损坏：{error}"))?;
        if saved.schema_version != 1
            || saved.project_id != storage.manifest().project.id
            || saved.document_id != document_id
        {
            return Err("提取断点版本或项目/文档身份不匹配".into());
        }
        let request = storage
            .prepare_document_open(&storage.manifest().project.id, document_id)
            .map_err(|error| error.to_string())?;
        if saved.checkpoint.content_hash != request.content_hash {
            return Err("文档内容已变化，请重新提取；旧断点保留".into());
        }
        if let Some(session_id) = &saved.checkpoint.session_id {
            let session = storage
                .load_session(session_id)
                .map_err(|error| format!("提取会话记录不可恢复：{error}"))?;
            if session.metadata.category != circuitfabric_project::SessionCategory::Datasheet
                || session.metadata.subject_id.as_deref() != Some(document_id)
            {
                return Err("提取断点引用了其他类别或文档的会话".into());
            }
        }
        Ok(Some(saved.checkpoint))
    }

    pub(crate) fn save(
        &self,
        storage: &circuitfabric_project::ProjectStorage,
        document_id: &str,
    ) -> Result<(), String> {
        let path = Self::path(storage, document_id)?;
        std::fs::create_dir_all(path.parent().expect("checkpoint directory"))
            .map_err(|error| error.to_string())?;
        let saved = SavedCheckpoint {
            schema_version: 1,
            project_id: storage.manifest().project.id.clone(),
            document_id: document_id.into(),
            checkpoint: self.clone(),
        };
        let temporary = path.with_extension("tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(&saved).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        std::fs::rename(temporary, path).map_err(|error| format!("提取断点保存失败：{error}"))
    }

    pub(crate) fn clear(
        storage: &circuitfabric_project::ProjectStorage,
        document_id: &str,
    ) -> Result<(), String> {
        match std::fs::remove_file(Self::path(storage, document_id)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
    pub(crate) fn cached_model_reply(
        &mut self,
        step: usize,
        category: &str,
        prompt: &str,
    ) -> Option<String> {
        if let Some((cached_prompt, response)) = self.model_steps.get(step)
            && cached_prompt == prompt
        {
            match circuitfabric_document_opener::datasheet::validate_datasheet_category_reply(
                response, category,
            ) {
                Ok(()) => return Some(response.clone()),
                Err(error) => {
                    self.model_feedback.insert(
                        category.to_owned(),
                        crate::application::datasheet_recovery::CategoryFeedback {
                            prompt: prompt.to_owned(),
                            response: response.clone(),
                            error,
                        },
                    );
                }
            }
        }
        if self.model_steps.len() > step {
            self.model_steps.truncate(step);
            self.jev_results.clear();
            self.judge_recovery.clear();
        }
        None
    }

    pub(crate) fn cached_jev_result(
        &mut self,
        batch: usize,
        arguments: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        if let Some((cached_arguments, result)) = self.jev_results.get(batch)
            && cached_arguments == arguments
            && circuitfabric_document_opener::datasheet::validate_datasheet_judge_result(
                arguments, result,
            )
            .is_ok()
        {
            return Some(result.clone());
        }
        // Discard incomplete legacy checkpoints as well as changed batch inputs.
        self.jev_results.truncate(batch);
        None
    }

    pub(crate) fn cache_jev_result(
        &mut self,
        batch: usize,
        arguments: &serde_json::Value,
        result: &serde_json::Value,
    ) -> Result<(), String> {
        circuitfabric_document_opener::datasheet::validate_datasheet_judge_result(
            arguments, result,
        )?;
        if self.jev_results.len() == batch {
            self.jev_results.push((arguments.clone(), result.clone()));
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedCheckpoint {
    schema_version: u32,
    project_id: String,
    document_id: String,
    checkpoint: DatasheetCheckpoint,
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuitfabric_project::{DocumentCategory, ProjectStorage};
    #[test]
    fn restart_keeps_each_document_checkpoint_and_rejects_stale_or_corrupt_state() {
        let root = std::env::temp_dir().join(format!(
            "cf-checkpoint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
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
        let mut documents = Vec::new();
        for index in 0..2 {
            let path = root.join(format!("doc-{index}.pdf"));
            std::fs::write(&path, format!("%PDF fixture {index}")).unwrap();
            let document = storage
                .import_document(&path, DocumentCategory::Datasheet, "fixture")
                .unwrap()
                .document;
            storage
                .start_categorized_session(
                    circuitfabric_project::SessionSeed {
                        session_id: format!("session-{index}"),
                        runtime_profile_id: "provider".into(),
                        backend_id: Some("codex".into()),
                    },
                    circuitfabric_project::SessionCategory::Datasheet,
                    Some(document.id.clone()),
                )
                .unwrap();
            let mut checkpoint = DatasheetCheckpoint {
                content_hash: document.content_hash.clone(),
                session_id: Some(format!("session-{index}")),
                ..Default::default()
            };
            checkpoint.model_steps.push(("prompt".into(), format!("reply-{index}")));
            checkpoint.judge_recovery.insert(
                0,
                crate::application::datasheet_recovery::JudgeRecovery {
                    results: serde_json::Map::from_iter([(
                        "done".into(),
                        serde_json::json!({"verified":true}),
                    )]),
                    last_error: "quota exhausted".into(),
                    batch_size: Some(1),
                    ..Default::default()
                },
            );
            checkpoint.save(&storage, &document.id).unwrap();
            documents.push(document);
        }
        let reopened = ProjectStorage::open(&root).unwrap();
        for (index, document) in documents.iter().enumerate() {
            let recovered = DatasheetCheckpoint::load(&reopened, &document.id).unwrap().unwrap();
            assert_eq!(recovered.model_steps[0].1, format!("reply-{index}"));
            assert_eq!(recovered.judge_recovery[&0].last_error, "quota exhausted");
            assert_eq!(recovered.judge_recovery[&0].results.len(), 1);
        }
        let mut stale = DatasheetCheckpoint::load(&reopened, &documents[0].id).unwrap().unwrap();
        stale.content_hash = "sha256:stale".into();
        stale.save(&reopened, &documents[0].id).unwrap();
        assert!(
            DatasheetCheckpoint::load(&reopened, &documents[0].id)
                .unwrap_err()
                .contains("内容已变化")
        );
        std::fs::write(DatasheetCheckpoint::path(&reopened, &documents[1].id).unwrap(), "{")
            .unwrap();
        assert!(
            DatasheetCheckpoint::load(&reopened, &documents[1].id).unwrap_err().contains("损坏")
        );
        DatasheetCheckpoint::clear(&reopened, &documents[0].id).unwrap();
        assert!(DatasheetCheckpoint::load(&reopened, &documents[0].id).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
