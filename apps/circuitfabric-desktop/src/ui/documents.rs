//! Documents presentation and event handlers.
use super::*;

/// Upper bound for one datasheet extraction model call. Healthy category calls finish in
/// seconds to a few minutes; a model still reasoning after this is stuck and is retried.
const DATASHEET_STEP_MAX_DURATION: std::time::Duration = std::time::Duration::from_secs(8 * 60);

impl ControlPlaneView {
    /// Opens the docked preview for one document.
    ///
    /// The gate (`prepare_document_open`) and the opener registry run synchronously; the
    /// pane shows the read-only view, or the explicit refusal/unsupported reason. The
    /// first open widens the window to the right by the pane width, mirroring how
    /// document tools extend their window for a preview; closing restores the size.
    pub(super) fn open_document_preview(
        &mut self,
        project_id: ProjectId,
        document: &ProjectDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        gpui_base::TextSelection::clear(window, cx);
        let storage = self.project_storages.get(&project_id).cloned();
        let document_id = document.id.clone();
        if self.document_preview.is_none() && !window.is_fullscreen() && !window.is_maximized() {
            let previous = window.bounds().size;
            self.pre_preview_window_size = Some(previous);
            window.resize(Size {
                width: previous.width + px(self.document_preview_width),
                height: previous.height,
            });
        }
        self.release_preview_images(window);
        self.preview_show_data = false;
        self.preview_focus = None;
        self.reset_pdf_view();
        self.preview_focus_generation += 1;
        self.preview_row_anchor = None;
        self.preview_scroll_pending.set(false);
        self.preview_scroll.set_offset(gpui::Point::default());
        self.datasheet_feedback = None;
        self.datasheet_rows_visible = 40;
        self.document_preview = Some(DocumentPreviewSelection {
            project_id: project_id.clone(),
            file_name: document.original_file_name.clone(),
            document_id: document_id.clone(),
            state: DocumentPreviewState::Loading,
            extraction: None,
        });
        let completed_project = project_id.clone();
        let completed_document = document_id.clone();
        let work = cx.background_spawn(async move {
            let Some(storage) = storage else {
                return (
                    DocumentPreviewState::Refused { denial: DocumentOpenDenial::NotFound },
                    None,
                    Ok(None),
                );
            };
            let state = match storage.prepare_document_open(&project_id, &document_id) {
                Ok(request) => {
                    match DocumentOpenerRegistry::with_builtin_openers().open(&request) {
                        circuitfabric_plugin_api::DocumentOpenerOutcome::Loaded { mut view } => {
                            let raster =
                                Self::raster_preview(&mut view, request.managed_copy.data());
                            DocumentPreviewState::Loaded { view: std::sync::Arc::new(view), raster }
                        }
                        circuitfabric_plugin_api::DocumentOpenerOutcome::Unsupported { reason }
                        | circuitfabric_plugin_api::DocumentOpenerOutcome::Failed { reason } => {
                            DocumentPreviewState::Unavailable { reason }
                        }
                    }
                }
                Err(denial) => DocumentPreviewState::Refused { denial },
            };
            let extraction = if matches!(state, DocumentPreviewState::Loaded { .. }) {
                storage
                    .load_datasheet_extraction(&document_id)
                    .ok()
                    .flatten()
                    .map(std::sync::Arc::new)
            } else {
                None
            };
            let checkpoint = DatasheetCheckpoint::load(&storage, &document_id);
            (state, extraction, checkpoint)
        });
        cx.spawn_in(window, async move |view, cx| {
            let (state, extraction, checkpoint) = work.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    if let Some(selection) = view.document_preview.as_mut()
                        && selection.project_id == completed_project
                        && selection.document_id == completed_document
                    {
                        selection.state = state;
                        if selection.extraction.is_none() {
                            selection.extraction = extraction;
                        }
                        if !view.datasheet_extracting {
                            match checkpoint {
                                Ok(Some(checkpoint)) => {
                                    view.datasheet_checkpoint = Some((
                                        completed_project.clone(),
                                        completed_document.clone(),
                                        checkpoint,
                                    ));
                                    view.datasheet_feedback =
                                        Some("已恢复上次未完成的提取，可点击“继续”。".into());
                                }
                                Ok(None) => {
                                    view.datasheet_checkpoint = None;
                                }
                                Err(error) => {
                                    view.datasheet_feedback = Some(error);
                                    view.datasheet_checkpoint = None;
                                }
                            }
                        }
                        cx.notify();
                    }
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Extracts the structured datasheet projection for the previewed document through
    /// the open gate (verified bytes only), persists it, and switches to the data tab.
    ///
    /// With `resume`, the checkpoint left by a stopped or failed run of the same
    /// document is reused; otherwise any such checkpoint is discarded.
    #[allow(clippy::too_many_lines)]
    pub(super) fn extract_datasheet_for_preview(
        &mut self,
        clear_existing: bool,
        resume: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.datasheet_extracting {
            return;
        }
        let Some(preview) = self.document_preview.as_ref() else {
            return;
        };
        let project_id = preview.project_id.clone();
        let document_id = preview.document_id.clone();
        let previous_extraction = preview.extraction.clone();
        let completed_project = project_id.clone();
        let completed_document = document_id.clone();
        let Some(storage) = self.project_storages.get(&project_id).cloned() else {
            return;
        };
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        let catalog = self.catalog.clone();
        let grants = self.effective_grants_for(&project_id);
        if !grants.authorized_mcp_server_ids.iter().any(|id| id == BUNDLED_JEV_SERVER_ID) {
            let reason = if self
                .tool_authorizations
                .authorized_mcp_server_ids
                .iter()
                .any(|id| id == BUNDLED_JEV_SERVER_ID)
            {
                "当前项目尚未授权 Jev；请到 Jev 页面授权当前项目后再提取。"
            } else {
                "Jev 尚未获得全局授权；请到 Jev 页面先授权全局及当前项目。"
            };
            self.datasheet_feedback = Some(reason.to_owned());
            self.status = reason.to_owned();
            cx.notify();
            return;
        }
        let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
        self.datasheet_feedback = None;
        self.datasheet_extracting = true;
        let stream = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        self.datasheet_stream = Some((project_id.clone(), document_id.clone(), stream.clone()));
        self.datasheet_stream_scroll = gpui::ScrollHandle::new();
        self.datasheet_stream_modal_scroll = gpui::ScrollHandle::new();
        self.datasheet_extract_started = Some(Instant::now());
        let cancel = circuitfabric_codex_runtime::execution::Cancellation::default();
        self.datasheet_cancel = Some(cancel.clone());
        let resume_from = if resume {
            match DatasheetCheckpoint::load(&storage, &document_id) {
                Ok(checkpoint) => checkpoint,
                Err(error) => {
                    self.datasheet_extracting = false;
                    self.datasheet_feedback = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let checkpoint =
            std::sync::Arc::new(std::sync::Mutex::new(resume_from.unwrap_or_default()));
        let final_checkpoint = checkpoint.clone();
        let run_cancel = cancel.clone();
        let stopped = move || cancel.0.load(std::sync::atomic::Ordering::SeqCst);
        let log = move |text: &str| {
            if let Ok(mut buffer) = stream.lock() {
                buffer.push_str(text);
            }
        };
        if clear_existing {
            if let Some(selection) = self.document_preview.as_mut() {
                selection.extraction = None;
            }
            self.datasheet_rows_visible = 40;
            self.status = "正在清空旧数据并重新提取…".to_owned();
        } else {
            self.status = "正在后台提取候选数据；随后调用 Jev evaluate…".to_owned();
        }
        let work = cx.background_spawn(async move {
            let mut cleared = false;
            let mut not_applicable = false;
            let mut jev_evaluate_calls = 0_usize;
            let mut jev_evaluate_responses = 0_usize;
            let mut result = (|| -> Result<DatasheetExtraction, String> {
                let request = storage
                    .prepare_document_open(&project_id, &document_id)
                    .map_err(|error| error.to_string())?;
                // Refuse documents with no datasheet sections (application notes, SDK
                // manuals) before any model call, session, or clearing of old results.
                log("▶ 正在读取 PDF 文本…\n");
                let pages = circuitfabric_document_opener::extract_pdf_text_pages(request.managed_copy.data())?;
                if !circuitfabric_document_opener::datasheet_sections_found(&pages) {
                    not_applicable = true;
                    return Err("这份文档看起来不是芯片数据手册：没有找到引脚表，也没有绝对最大额定值、电气特性或工作条件章节，结构化提取不适用".to_owned());
                }
                let tools = catalog
                    .request_with_cancellation(BUNDLED_JEV_SERVER_ID, &grants, None, secrets.as_ref(), &run_cancel)
                    .map_err(|error| error.to_string())?;
                if !tools["tools"]
                    .as_array()
                    .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "evaluate"))
                {
                    return Err("Jev MCP 未提供 evaluate 工具".to_owned());
                }
                settings.validate().map_err(|error| error.to_string())?;
                if circuitfabric_codex_runtime::execution::selected_provider(
                    &settings,
                    circuitfabric_codex_runtime::execution::AgentKind::Codex,
                )
                .is_none()
                {
                    return Err("请先配置 Codex Provider".to_owned());
                }
                if clear_existing {
                    storage
                        .clear_datasheet_extraction(&document_id)
                        .map_err(|error| error.to_string())?;
                    cleared = true;
                }
                {
                    let mut checkpoint =
                        checkpoint.lock().map_err(|error| error.to_string())?;
                    if checkpoint.content_hash != request.content_hash {
                        *checkpoint = DatasheetCheckpoint {
                            content_hash: request.content_hash.clone(),
                            ..DatasheetCheckpoint::default()
                        };
                    } else if !checkpoint.model_steps.is_empty() {
                        log("▶ 找到上次提取检查点，正在核对选页与提示词…\n");
                    }
                    let definition = catalog.mcp_servers.iter()
                        .find(|server| server.id == BUNDLED_JEV_SERVER_ID).cloned();
                    if checkpoint.judge_definition != definition {
                        checkpoint.jev_results.clear();
                        checkpoint.judge_recovery.clear();
                        checkpoint.judge_definition = definition;
                        log("▶ 判断后端配置已变化，将重新复核候选数据…\n");
                    }
                    if checkpoint.session_id.is_none() {
                        let provider = circuitfabric_codex_runtime::execution::selected_provider(&settings, circuitfabric_codex_runtime::execution::AgentKind::Codex)
                            .ok_or("Codex Provider 不可用")?;
                        checkpoint.session_id = Some(crate::application::runtime_session::start_for(&storage, &provider.id, "codex",
                            circuitfabric_project::SessionCategory::Datasheet, Some(document_id.clone()))?);
                    }
                    checkpoint.save(&storage, &document_id)?;
                }
                let mut jev_batch = 0_usize;
                let mut model_step = 0_usize;
                use crate::application::datasheet_recovery::{recover_category, recover_judgments_persisted};
                // Model turns are sequential. Share the observed runner between extraction
                // and tool-recovery callbacks without holding the checkpoint lock in flight.
                let model_runner = std::cell::RefCell::new(|label: &str, prompt: &str| {
                    use circuitfabric_codex_runtime::{TurnDelta, execution::run_session_observed_within};
                    if stopped() {
                        return Err("已停止".to_owned());
                    }
                    log(&format!("▶ {label}：已发送提示（{} 字符），等待智能体响应…\n", prompt.chars().count()));
                    // A provider stream that goes silent mid-turn, or a model that keeps
                    // reasoning past the step limit, usually behaves on a fresh attempt; every
                    // step is checkpointed, so one automatic retry is cheap.
                    let mut attempt = 0;
                    loop {
                        attempt += 1;
                        let session_id = checkpoint.lock().map_err(|error| error.to_string())?.session_id.clone().ok_or("缺少提取会话")?;
                        storage.resume_session(&session_id).map_err(|error| error.to_string())?;
                        let history = storage.session_runtime_directory(&session_id).map_err(|error| error.to_string())?;
                        let mut usage = None;
                        let mut current_stream = None;
                        let outcome = run_session_observed_within(
                            &settings, &ToolAuthorizationSettings::default(), prompt,
                            None, storage.root(), &history, secrets.as_ref(), &run_cancel,
                            &mut |kind, delta| {
                                if current_stream != Some(kind) {
                                    current_stream = Some(kind);
                                    log(match kind { TurnDelta::Reasoning => "\n[思考]\n", TurnDelta::Answer => "\n[输出]\n" });
                                }
                                log(delta);
                            },
                            &mut |reported| usage = Some(reported),
                            DATASHEET_STEP_MAX_DURATION,
                        );
                        let stalled = outcome.as_ref().is_err_and(|error| error.is_stalled() || error.is_too_long());
                        let response = outcome.map_err(|error| error.to_string());
                        crate::application::runtime_session::finish(&storage, &session_id,
                            &format!("文档提取 document={document_id} hash={} stage={label} attempt={attempt}\n{prompt}", request.content_hash),
                            response.as_deref().map_err(String::as_str), usage)
                            .map_err(|error| format!("文档提取会话用量未保存：{error}"))?;
                        storage.resume_session(&session_id).map_err(|error| error.to_string())?;
                        if stalled && attempt == 1 && !stopped() {
                            log(&format!(
                                "\n▶ {}\n▶ 自动重试 {label}（第 2 次尝试）…\n",
                                response.as_ref().err().map_or("", String::as_str)
                            ));
                            continue;
                        }
                        log("\n▶ 智能体调用结束，正在检查结果…\n");
                        break response;
                    }
                });
                let mut extraction = extract_datasheet_by_category_with_pages(
                    &request,
                    &pages,
                    |category, prompt, selected_pages| {
                        let step = model_step;
                        model_step += 1;
                        let category_label = match category {
                            "pins" => "引脚",
                            "absoluteMaximumRatings" => "绝对最大额定值",
                            "electricalCharacteristics" => "电气特性",
                            "operatingConditions" => "工作条件",
                            _ => "数据",
                        };
                        log(&format!(
                            "▶ {category_label}（第 {} 次模型调用）已选页：{}\n",
                            step + 1,
                            selected_pages
                                .iter()
                                .map(usize::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                        ));
                        if stopped() {
                            return Err("已停止".to_owned());
                        }
                        let cached = checkpoint.lock().ok()
                            .and_then(|mut checkpoint| checkpoint.cached_model_reply(step, category, prompt));
                        if let Some(response) = cached {
                            log("▶ 复用上次完整的模型响应，跳过模型调用\n");
                            return Ok(response);
                        }
                        let mut feedback = checkpoint.lock().map_err(|error| error.to_string())?
                            .model_feedback.get(category).cloned();
                        let response = recover_category(category, prompt, &mut feedback,
                            |next_prompt| (model_runner.borrow_mut())(category_label, next_prompt),
                            &stopped, &log);
                        let mut checkpoint = checkpoint.lock().map_err(|error| error.to_string())?;
                        if let Some(feedback) = feedback {
                            checkpoint.model_feedback.insert(category.to_owned(), feedback);
                        } else {
                            checkpoint.model_feedback.remove(category);
                        }
                        if let Ok(response) = &response
                            && checkpoint.model_steps.len() == step {
                            checkpoint.model_steps.push((prompt.to_owned(), response.clone()));
                        }
                        checkpoint.save(&storage, &document_id)?;
                        response
                    },
                    |arguments| {
                        let batch = jev_batch;
                        jev_batch += 1;
                        let cached = checkpoint.lock().ok()
                            .and_then(|mut checkpoint| checkpoint.cached_jev_result(batch, arguments));
                        if let Some(result) = cached {
                            log(&format!("▶ 复用第 {} 批 Jev 结果\n", batch + 1));
                            return Ok(result);
                        }
                        if stopped() {
                            return Err("已停止".to_owned());
                        }
                        let mut recovery = checkpoint.lock().map_err(|error| error.to_string())?
                            .judge_recovery.get(&batch).cloned().unwrap_or_default();
                        let response = recover_judgments_persisted(arguments, &mut recovery,
                            |next_arguments| {
                                jev_evaluate_calls += 1;
                                log(&format!("▶ Jev evaluate 第 {jev_evaluate_calls} 次…\n"));
                                let response = catalog.request_with_cancellation(
                                    BUNDLED_JEV_SERVER_ID, &grants, Some(("evaluate", next_arguments)),
                                    secrets.as_ref(), &run_cancel,
                                ).map_err(|error| error.to_string());
                                if response.as_ref().is_ok_and(|value| value["isError"] != true) {
                                    jev_evaluate_responses += 1;
                                }
                                response
                            },
                            |prompt| (model_runner.borrow_mut())("Jev 恢复决策", prompt),
                            &stopped, &log, |recovery| {
                                let mut checkpoint = checkpoint.lock().map_err(|error| error.to_string())?;
                                checkpoint.judge_recovery.insert(batch, recovery.clone());
                                checkpoint.save(&storage, &document_id)
                            });
                        let mut checkpoint = checkpoint.lock().map_err(|error| error.to_string())?;
                        checkpoint.judge_recovery.insert(batch, recovery);
                        if let Ok(value) = &response {
                            checkpoint.cache_jev_result(batch, arguments, value)?;
                            checkpoint.judge_recovery.remove(&batch);
                        }
                        checkpoint.save(&storage, &document_id)?;
                        response
                    },
                )?;
                extraction.notes.push(format!("Jev evaluate MCP calls: {jev_evaluate_calls}"));
                if let Some(judge) = catalog.mcp_servers.iter()
                    .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
                    .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
                {
                    extraction.notes.push(format!(
                        "LLM judgment backend: {} ({:?}); probabilities are uncalibrated estimates.",
                        judge.model, judge.answer_mode,
                    ));
                }
                let current = storage
                    .prepare_document_open(&project_id, &document_id)
                    .map_err(|error| error.to_string())?;
                if current.content_hash != extraction.content_hash {
                    return Err("文档在提取过程中发生变化".to_owned());
                }
                storage
                    .save_datasheet_extraction(&extraction)
                    .map_err(|error| error.to_string())?;
                Ok(extraction)
            })();
            let saved = (|| -> Result<(), String> {
                let mut checkpoint = checkpoint.lock().map_err(|error| error.to_string())?;
                // Every proposed row failed the evidence check: the cached replies are
                // complete but useless, and replaying them on "continue" would fail the same
                // way forever. Keep only the session so the next attempt asks the model again.
                if result.as_ref().is_err_and(|error| {
                    error.starts_with(circuitfabric_document_opener::NO_EVIDENCE_ROWS)
                }) {
                    checkpoint.model_steps.clear();
                    checkpoint.model_feedback.clear();
                    checkpoint.jev_results.clear();
                    checkpoint.judge_recovery.clear();
                }
                if let Some(session_id) = &checkpoint.session_id {
                    crate::application::runtime_session::settle(&storage, session_id,
                        result.as_ref().map(|_| "提取任务完成，结构化结果已保存").map_err(String::as_str))?;
                    if result.is_err() && !not_applicable { checkpoint.save(&storage, &document_id)?; }
                }
                // Nothing to resume for a document that is not a datasheet.
                if result.is_ok() || not_applicable { DatasheetCheckpoint::clear(&storage, &document_id)?; }
                Ok(())
            })();
            if let Err(error) = saved {
                result = Err(format!("{}；任务状态未保存：{error}", result.as_ref().err().map_or("提取结果已保存", String::as_str)));
            }
            (result, cleared, not_applicable, jev_evaluate_calls, jev_evaluate_responses)
        });
        // Repaint while the log grows; stops once the extraction settles.
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(200)).await;
                let extracting = cx
                    .update(|_, cx| {
                        view.update(cx, |view, cx| {
                            cx.notify();
                            view.datasheet_extracting
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !extracting {
                    break;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |view, cx| {
            let (result, cleared, not_applicable, jev_evaluate_calls, jev_evaluate_responses) =
                work.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.datasheet_extracting = false;
                    let refresh_error = view.refresh_project_data(&completed_project).err();
                    let stopped = view.datasheet_cancel.take().is_some_and(|cancel| {
                        cancel.0.load(std::sync::atomic::Ordering::SeqCst)
                    });
                    let showing_document = view.document_preview.as_ref().is_some_and(|selection| {
                        selection.project_id == completed_project
                            && selection.document_id == completed_document
                    });
                    match result {
                        Ok(extraction) => {
                            view.datasheet_checkpoint = None;
                            if showing_document {
                                view.datasheet_feedback = Some(format!(
                                    "Jev evaluate 已调用 {jev_evaluate_calls} 次；候选行判断结果见下方解析诊断。"
                                ));
                            }
                            let document = view.project_data.get(&completed_project).and_then(
                                |data| {
                                    data.documents
                                        .iter()
                                        .find(|document| document.id == completed_document)
                                        .cloned()
                                },
                            );
                            let evidence_rows = document.map_or(0, |document| {
                                view.workspace
                                    .register_datasheet_evidence(
                                        &completed_project,
                                        &document,
                                        &extraction,
                                    )
                                    .unwrap_or(0)
                            });
                            view.status = format!(
                                "已提取并验证：{} 个引脚、{} 条参数；{evidence_rows} 行已登记为证据。",
                                extraction.pins.len(),
                                extraction.absolute_maximum_ratings.len()
                                    + extraction.electrical_characteristics.len()
                                    + extraction.operating_conditions.len()
                            );
                            if let Some(selection) = view.document_preview.as_mut()
                                && selection.project_id == completed_project
                                && selection.document_id == completed_document
                            {
                                selection.extraction = Some(std::sync::Arc::new(extraction));
                                view.preview_show_data = true;
                            }
                        }
                        Err(error) if not_applicable => {
                            // Checked before anything was cleared; old results stay as they are.
                            if clear_existing
                                && let Some(selection) = view.document_preview.as_mut()
                                && selection.project_id == completed_project
                                && selection.document_id == completed_document
                            {
                                selection.extraction = previous_extraction;
                            }
                            view.datasheet_checkpoint = None;
                            view.status = format!("未提取：{error}。");
                            if showing_document {
                                view.datasheet_feedback = Some(view.status.clone());
                            }
                        }
                        Err(error) => {
                            if clear_existing && !cleared {
                                if let Some(selection) = view.document_preview.as_mut()
                                    && selection.project_id == completed_project
                                    && selection.document_id == completed_document
                                {
                                    selection.extraction = previous_extraction;
                                }
                            }
                            if cleared {
                                view.workspace.clear_datasheet_evidence(
                                    &completed_project,
                                    &completed_document,
                                );
                            }
                            let checkpoint = final_checkpoint
                                .lock()
                                .map(|checkpoint| checkpoint.clone())
                                .unwrap_or_default();
                            view.datasheet_checkpoint = Some((
                                completed_project.clone(),
                                completed_document.clone(),
                                checkpoint,
                            ));
                            view.status = if stopped {
                                format!(
                                    "已停止提取{}；点击“继续”可复用已完成的步骤。",
                                    if cleared { "（旧数据已清空）" } else { "" }
                                )
                            } else if cleared {
                                format!("提取已暂停（旧数据已清空）；已保留恢复上下文，可点击“继续”：{error}（Jev evaluate 发起 {jev_evaluate_calls} 次，收到 {jev_evaluate_responses} 次响应）")
                            } else {
                                format!("提取已暂停（旧结果保留）；已保留恢复上下文，可点击“继续”：{error}（Jev evaluate 发起 {jev_evaluate_calls} 次，收到 {jev_evaluate_responses} 次响应）")
                            };
                            if showing_document {
                                view.datasheet_feedback = Some(view.status.clone());
                            }
                        }
                    }
                    if let Some(error) = refresh_error {
                        view.status.push_str(&format!("；文档提取后的会话与用量未刷新：{error}"));
                    }
                    view.refresh_evidence_search(cx);
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Requests a stop: the model call is cancelled at once (its process is killed); a
    /// Jev call already in flight finishes first. Completed steps stay resumable.
    pub(super) fn stop_datasheet_extraction(&mut self, cx: &mut Context<Self>) {
        let Some(cancel) = &self.datasheet_cancel else {
            return;
        };
        cancel.cancel();
        if let Some((_, _, stream)) = &self.datasheet_stream
            && let Ok(mut buffer) = stream.lock()
        {
            buffer.push_str("\n▶ 已请求停止…\n");
        }
        "正在停止提取…".clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn close_document_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        gpui_base::TextSelection::clear(window, cx);
        self.release_preview_images(window);
        self.document_preview = None;
        self.stop_pdf_interaction();
        self.preview_focus = None;
        self.preview_focus_generation += 1;
        self.preview_row_anchor = None;
        #[cfg(windows)]
        crate::pdf_cursors::clear();
        if let Some(previous) = self.pre_preview_window_size.take()
            && !window.is_fullscreen()
            && !window.is_maximized()
        {
            window.resize(previous);
        }
        cx.notify();
    }
}
