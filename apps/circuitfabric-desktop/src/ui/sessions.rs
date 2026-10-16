//! Sessions presentation and event handlers.
use super::*;

pub(super) fn datasheet_display_rows(data: Option<&DatasheetExtraction>) -> Vec<(String, String)> {
    let Some(data) = data else {
        return vec![];
    };
    let mut rows = vec![];
    rows.push((format!("引脚 · {} 行", data.pins.len()), String::new()));
    for (i, pin) in data.pins.iter().enumerate() {
        rows.push((
            format!("第 {} 行 · {} · {}", i + 1, pin.number, pin.name),
            format!(
                "{}\n{}\n原文：{}",
                pin.kind.as_str(),
                pin.description,
                pin.evidence.as_deref().unwrap_or("—")
            ),
        ));
    }
    for (title, parameters) in [
        ("绝对最大额定值", &data.absolute_maximum_ratings),
        ("电特性", &data.electrical_characteristics),
        ("工作条件", &data.operating_conditions),
    ] {
        rows.push((format!("{title} · {} 行", parameters.len()), String::new()));
        for (i, parameter) in parameters.iter().enumerate() {
            rows.push((
                format!(
                    "第 {} 行 · {} {}",
                    i + 1,
                    parameter.parameter,
                    parameter.symbol.as_deref().unwrap_or("")
                ),
                format!(
                    "Min：{}   Typ：{}   Max：{}   {}\n条件：{}\n原文：{}",
                    parameter.min.as_deref().unwrap_or("—"),
                    parameter.typ.as_deref().unwrap_or("—"),
                    parameter.max.as_deref().unwrap_or("—"),
                    parameter.unit.as_deref().unwrap_or(""),
                    parameter.conditions.as_deref().unwrap_or("—"),
                    parameter.evidence.as_deref().unwrap_or("—")
                ),
            ));
        }
    }
    rows
}

fn session_status_label(
    status: circuitfabric_project::SessionStatus,
    language: UiLanguage,
) -> &'static str {
    use circuitfabric_project::SessionStatus;
    match status {
        SessionStatus::Completed => language.choose("已完成", "Completed"),
        SessionStatus::Failed => language.choose("失败", "Failed"),
        SessionStatus::Running => language.choose("进行中", "Running"),
    }
}

/// One chat row in the conversation tab. User turns sit on the right, agent turns on
/// the left; non-message audit events collapse into a compact centered annotation.
fn conversation_row_element(
    row: &super::session_markdown::ConversationRow,
    ix: usize,
    language: UiLanguage,
) -> AnyElement {
    use super::session_markdown::ConversationRow;
    match row {
        ConversationRow::Reasoning { timestamp, text } => div()
            .px_4()
            .pb_3()
            .w_full()
            .child(
                div()
                    .v_flex()
                    .gap_1p5()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .border_l_2()
                    .bg(rgb(CARD_BG))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0x000e_7490))
                                    .child(language.choose("思考摘要", "Reasoning")),
                            )
                            .child(div().text_color(rgb(TEXT_MUTED)).child(timestamp.clone())),
                    )
                    .child(
                        div()
                            .debug_selector(|| "session-reasoning-panel".into())
                            .h(px(112.))
                            .overflow_hidden()
                            .child(
                                gpui_component::text::TextView::markdown(
                                    ("session-reasoning-text", ix),
                                    super::session_markdown::bound_plain_paragraphs(text),
                                )
                                .h_full()
                                .w_full()
                                .scrollable(true)
                                .selectable(true),
                            ),
                    ),
            )
            .into_any_element(),
        ConversationRow::Turn { actor, timestamp, first, text } => {
            let is_user = matches!(actor, circuitfabric_project::SessionActor::User);
            let actor_label = if is_user {
                language.choose("用户", "User")
            } else {
                language.choose("智能体", "Agent")
            };
            let mut column = div().v_flex().gap_1().min_w(px(0.)).max_w(gpui::relative(0.78));
            if *first {
                column = column.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_1()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .when(is_user, Styled::flex_row_reverse)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(if is_user { 0x000e_7490 } else { TEXT_SECONDARY }))
                                .child(actor_label),
                        )
                        .child(timestamp.clone()),
                );
            }
            let bubble = div()
                .debug_selector(move || format!("session-chat-bubble-{ix}"))
                .rounded_xl()
                .py_2p5()
                .px_3p5()
                .child(
                    gpui_component::text::TextView::markdown(
                        ("session-chat-chunk", ix),
                        text.clone(),
                    )
                    .selectable(true),
                );
            // Chat "tail": the corner pointing at its sender stays tight.
            let bubble = if is_user {
                bubble.rounded_br_sm().bg(rgb(0x00e0_f2fe))
            } else {
                bubble
                    .rounded_bl_sm()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .shadow_sm()
            };
            let column = column.child(bubble);
            div()
                .w_full()
                .flex()
                .px_4()
                .pb_2()
                .when(*first, |row| row.pt_2())
                .when(is_user, Styled::justify_end)
                .child(column)
                .into_any_element()
        }
        ConversationRow::System { timestamp, label, detail } => div()
            .w_full()
            .flex()
            .justify_center()
            .px_4()
            .pb_2()
            .child(
                div()
                    .debug_selector(move || format!("session-chat-system-{ix}"))
                    .max_w_full()
                    .truncate()
                    .px_2p5()
                    .py_0p5()
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(format!("{timestamp} · {label} · {detail}")),
            )
            .into_any_element(),
    }
}

impl ControlPlaneView {
    fn update_conversation_rows(
        selection: &mut SessionReplaySelection,
        rows: std::rc::Rc<Vec<super::session_markdown::ConversationRow>>,
    ) {
        let common =
            selection.conversation.iter().zip(rows.iter()).take_while(|(a, b)| a == b).count();
        if rows.len() == selection.conversation.len() {
            selection.conversation_list.remeasure_items(common..rows.len());
        } else {
            selection
                .conversation_list
                .splice(common..selection.conversation.len(), rows.len() - common);
        }
        selection.conversation = rows;
    }

    fn sync_session_stream(&mut self, cx: &mut Context<Self>) {
        let Some(run) = &mut self.session_followup else {
            return;
        };
        let Some(selection) = &mut self.session_replay else {
            return;
        };
        if run.project_id != selection.project_id
            || run.session_id != selection.replay.metadata.session_id
        {
            return;
        }
        let Ok(stream) = run.stream.lock() else {
            return;
        };
        if run.displayed_revision == stream.revision {
            return;
        }
        run.displayed_revision = stream.revision;
        let mut rows = selection.conversation_base.as_ref().clone();
        rows.extend(super::session_markdown::live_rows(&stream.answer, &stream.reasoning));
        if !stream.phase.is_empty() {
            rows.push(super::session_markdown::ConversationRow::System {
                timestamp: String::new(),
                label: "数据".into(),
                detail: stream.phase.clone(),
            });
        }
        Self::update_conversation_rows(selection, std::rc::Rc::new(rows));
        cx.notify();
    }
    /// Refresh only the record that is still selected. Completion must not open a
    /// dismissed modal or replace another project's conversation or draft.
    fn refresh_followup_replay(&mut self, project: &ProjectId, id: &str, cx: &mut Context<Self>) {
        if !self
            .session_replay
            .as_ref()
            .is_some_and(|s| &s.project_id == project && s.replay.metadata.session_id == id)
        {
            return;
        }
        let prompt = self.session_replay.as_mut().unwrap().followup_prompt.take();
        let list = self.session_replay.as_ref().unwrap().conversation_list.clone();
        let at_end = self.session_replay.as_ref().unwrap().conversation_at_end.clone();
        let rows = self.session_replay.as_ref().unwrap().conversation.clone();
        let tab = self.session_modal_tab;
        let open = self.session_modal_open;
        self.open_session_replay(project.clone(), id.into(), cx);
        self.session_modal_tab = tab;
        self.session_modal_open = open;
        if let Some(selection) = &mut self.session_replay {
            selection.followup_prompt = prompt;
            let next = selection.conversation.clone();
            selection.conversation_list = list;
            selection.conversation_at_end = at_end;
            selection.conversation = rows;
            Self::update_conversation_rows(selection, next);
        }
    }

    pub(super) fn send_session_followup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_session_followup_request(None, window, cx);
    }

    fn send_session_followup_request(
        &mut self,
        generated_prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = &self.session_replay else {
            return;
        };
        let project = selection.project_id.clone();
        let id = selection.replay.metadata.session_id.clone();
        let category = selection.replay.metadata.category;
        let document_id = selection
            .replay
            .metadata
            .subject_id
            .clone()
            .filter(|_| category == circuitfabric_project::SessionCategory::Datasheet);
        let prompt_entity = selection.followup_prompt.clone();
        let generated = generated_prompt.is_some();
        let prompt = generated_prompt.unwrap_or_else(|| {
            prompt_entity.as_ref().map(|p| p.read(cx).value().to_string()).unwrap_or_default()
        });
        let fail = |view: &mut Self, message: String, cx: &mut Context<Self>| {
            view.session_followup_feedback = Some((project.clone(), id.clone(), message));
            cx.notify();
        };
        if self.session_followup.is_some()
            || self.quick_task(RuntimeAdapter::CodexAppServer).cancel.is_some()
            || self.datasheet_extracting
        {
            fail(self, "请等待当前任务结束或取消后再继续对话".into(), cx);
            return;
        }
        if prompt.trim().is_empty() {
            fail(self, "请先输入补充信息或处理要求".into(), cx);
            return;
        }
        let Some(storage) = self.project_storages.get(&project).cloned() else {
            fail(self, "项目根目录未打开，请重新打开项目".into(), cx);
            return;
        };
        let settings = match RuntimeSettings::load_or_default(&self.settings_path)
            .map_err(|e| e.to_string())
            .and_then(|settings| {
                crate::application::runtime_session::followup_settings(&storage, &id, settings)
            }) {
            Ok(settings) => settings,
            Err(error) => {
                fail(self, error, cx);
                return;
            }
        };
        let history = match storage.session_runtime_directory(&id) {
            Ok(history) => history,
            Err(error) => {
                fail(self, error.to_string(), cx);
                return;
            }
        };
        // Extraction invokes Jev through the application controller, not the
        // Codex thread. Keep the same model scope when continuing its history.
        let grants = if category == circuitfabric_project::SessionCategory::Datasheet {
            ToolAuthorizationSettings::default()
        } else {
            self.effective_grants_for(&project)
        };
        if let Err(error) =
            crate::application::runtime_session::begin_followup(&storage, &id, &prompt)
        {
            fail(self, format!("补充消息未保存：{error}"), cx);
            return;
        }
        let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
        let cancel = circuitfabric_codex_runtime::execution::Cancellation::default();
        let stream = std::sync::Arc::new(std::sync::Mutex::new(SessionFollowupStream::default()));
        let previous_outcome = self
            .session_followup_feedback
            .as_ref()
            .filter(|(p, s, _)| p == &project && s == &id)
            .map(|(_, _, text)| text.clone());
        self.session_followup = Some(SessionFollowupRun {
            project_id: project.clone(),
            session_id: id.clone(),
            cancel: cancel.clone(),
            stream: stream.clone(),
            displayed_revision: u64::MAX,
        });
        self.session_followup_feedback =
            Some((project.clone(), id.clone(), "正在恢复原会话并等待模型回复…".into()));
        if !generated && let Some(input) = prompt_entity {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.refresh_followup_replay(&project, &id, cx);
        if let Some(selection) = &self.session_replay {
            selection.conversation_list.set_follow_mode(gpui::FollowMode::Tail);
            selection.conversation_list.scroll_to_end();
        }
        self.sync_session_stream(cx);
        let completed_project = project.clone();
        let completed_id = id.clone();
        let completed_prompt = prompt.clone();
        let completed_document = document_id.clone();
        let judge_grants = self.effective_grants_for(&project);
        let repaint_stream = stream.clone();
        let summary_language = self.language.choose("简体中文", "English");
        let work = cx.background_spawn(async move {
            let document = document_id.as_deref().map(|doc| {
                crate::application::datasheet_dialogue::DialogueDocument::prepare(
                    &storage, &project, doc,
                )
            });
            let mut model_prompt = document.as_ref().and_then(|d| d.as_ref().ok()).map_or_else(
                || prompt.clone(),
                |doc| doc.prompt_in_language(&prompt, summary_language),
            );
            if document.as_ref().is_some_and(|d| d.is_ok())
                && let Some(outcome) = previous_outcome
            {
                model_prompt.push_str(&format!("\nApplication status: {outcome}\n"));
            }
            let mut usage = None;
            let result = circuitfabric_codex_runtime::execution::resume_session_streaming(
                &settings,
                &grants,
                &model_prompt,
                None,
                storage.root(),
                &history,
                secrets.as_ref(),
                &cancel,
                &mut |kind, delta| {
                    if let Ok(mut stream) = stream.lock() {
                        match kind {
                            circuitfabric_codex_runtime::TurnDelta::Answer => {
                                stream.answer.push_str(delta)
                            }
                            circuitfabric_codex_runtime::TurnDelta::Reasoning => {
                                stream.reasoning.push_str(delta)
                            }
                        }
                        stream.revision = stream.revision.wrapping_add(1);
                    }
                },
                &mut |value| usage = Some(value),
            )
            .map_err(|error| error.to_string());
            let amendment = result.as_ref().ok().and_then(|reply| {
                if !reply.contains(crate::application::datasheet_dialogue::EDIT_FENCE) {
                    return None;
                }
                if let Ok(mut stream) = stream.lock() {
                    stream.phase = "正在校验并保存数据修改…".into();
                    stream.revision = stream.revision.wrapping_add(1);
                }
                Some((|| -> Result<Option<String>, String> {
                    if cancel.0.load(std::sync::atomic::Ordering::SeqCst) {
                        return Err("请求已取消，数据修改未保存".into());
                    }
                    let doc = document
                        .as_ref()
                        .ok_or("此会话不能修改结构化数据")?
                        .as_ref()
                        .map_err(|e| e.clone())?;
                    doc.apply(&storage, reply, |arguments| {
                        settings
                            .catalog
                            .request_with_cancellation(
                                circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID,
                                &judge_grants,
                                Some(("evaluate", arguments)),
                                secrets.as_ref(),
                                &cancel,
                            )
                            .map_err(|e| e.to_string())
                    })
                })())
            });
            if result.is_err()
                && let Ok(stream) = stream.lock()
                && !stream.answer.is_empty()
            {
                let _ = crate::application::runtime_session::append_note_reply(
                    &storage,
                    &id,
                    &stream.answer,
                );
            }
            let reasoning_persistence =
                stream.lock().map_err(|e| e.to_string()).and_then(|stream| {
                    crate::application::runtime_session::append_reasoning(
                        &storage,
                        &id,
                        &stream.reasoning,
                    )
                });
            let persistence = crate::application::runtime_session::finish_followup(
                &storage,
                &id,
                result.as_deref().map_err(String::as_str),
                usage,
            )
            .and(reasoning_persistence);
            if let Some(report) = &amendment {
                let text = match report {
                    Ok(Some(message)) => message.clone(),
                    Ok(None) => String::new(),
                    Err(e) => format!("数据未更新：{e}"),
                };
                let _ = crate::application::runtime_session::append_note(&storage, &id, &text);
            }
            (result, persistence, amendment)
        });
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                let active = cx
                    .update(|_, cx| {
                        view.update(cx, |view, cx| {
                            let active = view.session_followup.as_ref().is_some_and(|run| {
                                std::sync::Arc::ptr_eq(&run.stream, &repaint_stream)
                            });
                            if active {
                                view.sync_session_stream(cx);
                            }
                            active
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |view, cx| {
            let (result, persistence, amendment) = work.await;
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.session_followup = None;
                    let failed = result.is_err();
                    let mut feedback = match result {
                        Ok(_) => "回复已保存到原会话，可继续补充信息".into(),
                        Err(error) => {
                            format!("本次对话未完成：{error}。补充消息已保存，可修复后重试。")
                        }
                    };
                    if let Some(report) = amendment {
                        match report {
                            Ok(Some(message)) => {
                                feedback = message;
                                if let Some(doc) = &completed_document {
                                    if !feedback.starts_with("文档概述")
                                        && view.datasheet_checkpoint.as_ref().is_some_and(
                                            |(p, d, _)| p == &completed_project && d == doc,
                                        )
                                    {
                                        view.datasheet_checkpoint = None;
                                    }
                                    if let Some(preview) = &mut view.document_preview
                                        && preview.project_id == completed_project
                                        && &preview.document_id == doc
                                    {
                                        preview.extraction = view
                                            .project_storages
                                            .get(&completed_project)
                                            .and_then(|s| {
                                                s.load_datasheet_extraction(doc)
                                                    .ok()
                                                    .flatten()
                                                    .map(std::sync::Arc::new)
                                            });
                                    }
                                }
                            }
                            Err(error) => feedback = format!("回复已保存；数据未更新：{error}"),
                            Ok(None) => {}
                        }
                    }
                    if let Err(error) = persistence {
                        feedback.push_str(&format!("；会话记录未完整写入：{error}"));
                    }
                    view.status = feedback.clone();
                    view.session_followup_feedback =
                        Some((completed_project.clone(), completed_id.clone(), feedback));
                    if let Err(error) = view.refresh_project_data(&completed_project) {
                        view.status = format!("会话列表未刷新：{error}");
                    }
                    view.refresh_followup_replay(&completed_project, &completed_id, cx);
                    if failed
                        && !generated
                        && let Some(selection) = view.session_replay.as_ref().filter(|s| {
                            s.project_id == completed_project
                                && s.replay.metadata.session_id == completed_id
                        })
                        && let Some(input) = &selection.followup_prompt
                        && input.read(cx).value().trim().is_empty()
                    {
                        // Permit resending an input that failed before it reached the
                        // runtime, while preserving any new draft typed during the turn.
                        input.update(cx, |input, cx| input.set_value(completed_prompt, window, cx));
                    }
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn render_session_followup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.session_replay.as_ref().unwrap();
        let language = self.language;
        let panel = div()
            .id("session-followup-panel")
            .debug_selector(|| "session-followup-panel".into())
            .flex_none()
            .v_flex()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG));
        if let Some(reason) = &selection.followup_unavailable {
            return panel.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(reason.clone()));
        }
        let sender = cx.entity().clone();
        let stopper = sender.clone();
        let busy = self.session_followup.is_some()
            || self.quick_task(RuntimeAdapter::CodexAppServer).cancel.is_some()
            || self.datasheet_extracting;
        let this_running = self.session_followup.as_ref().is_some_and(|run| {
            run.project_id == selection.project_id
                && run.session_id == selection.replay.metadata.session_id
        });
        let empty =
            selection.followup_prompt.as_ref().is_none_or(|p| p.read(cx).value().trim().is_empty());
        let feedback = self
            .session_followup_feedback
            .as_ref()
            .filter(|(project, id, _)| {
                project == &selection.project_id && id == &selection.replay.metadata.session_id
            })
            .map(|(_, _, text)| text.clone());
        let is_datasheet =
            selection.replay.metadata.category == circuitfabric_project::SessionCategory::Datasheet;
        let hint = if is_datasheet {
            language.choose(
                "沿用此会话上下文；可要求增删或修改右侧数据，新增和修改通过原文及 Jev 校验后保存。",
                "Continue with this session's context; ask to add, remove or fix rows on the right — changes are saved after source and Jev validation.",
            )
        } else {
            language.choose(
                "沿用此会话上下文；补充消息和回复保存到原记录。",
                "Continue with this session's context; messages and replies stay in this record.",
            )
        };
        panel
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(hint))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .id("session-followup-input")
                            .debug_selector(|| "session-followup-input".into())
                            .flex_1()
                            .min_w(px(0.))
                            .child(gpui_component::input::Textarea::new(
                                selection.followup_prompt.as_ref().unwrap(),
                            )),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                action_button("send-session-followup")
                                    .label(language.choose("发送", "Send"))
                                    .primary()
                                    .rounded_lg()
                                    .disabled(busy || empty)
                                    .on_click(move |_, window, cx| {
                                        sender.update(cx, |view, cx| {
                                            view.send_session_followup(window, cx)
                                        });
                                    }),
                            )
                            .when(this_running, |row| {
                                row.child(
                                    action_button("cancel-session-followup")
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, _, cx| {
                                            stopper.update(cx, |view, cx| {
                                                if let Some(run) = &view.session_followup {
                                                    run.cancel.cancel();
                                                }
                                                cx.notify();
                                            });
                                        }),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_1().min_w(px(0.)).text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "Enter 发送 · Shift+Enter 换行",
                                "Enter to send · Shift+Enter for a new line",
                            ),
                        ),
                    )
                    .when(busy && !this_running, |row| {
                        row.child(
                            div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose(
                                    "当前有任务运行，请稍后继续",
                                    "Another task is running",
                                ),
                            ),
                        )
                    }),
            )
            .when_some(feedback, |panel, feedback| {
                let color = if feedback.contains("未完成")
                    || feedback.contains("未保存")
                    || feedback.contains("未更新")
                {
                    0x00b4_2323
                } else if feedback.starts_with("正在") {
                    0x00b4_5309
                } else {
                    TEXT_MUTED
                };
                panel.child(
                    div()
                        .id("session-followup-feedback")
                        .max_h(px(64.))
                        .overflow_y_scroll()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(rgb(color))
                        .child(feedback),
                )
            })
    }

    pub(super) fn render_sessions_tab(
        &mut self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.render_category_sessions_tab(project, circuitfabric_project::SessionCategory::Eda, cx)
    }

    fn render_category_sessions_tab(
        &mut self,
        project: &Project,
        category: circuitfabric_project::SessionCategory,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let mut listing = self
            .project_data
            .get(&project.id)
            .map(|data| data.session_listing.clone())
            .unwrap_or_default();
        listing.sessions.retain(|row| row.metadata.category == category);
        listing.orphaned_temp_files.retain(|name| {
            if category == circuitfabric_project::SessionCategory::Legacy {
                !name.contains('/')
            } else {
                name.starts_with(&format!("{}/", category.as_str()))
            }
        });
        let mut rows = div().v_flex().gap_2();
        if listing.sessions.is_empty() {
            rows = rows.child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose("暂无这类会话", "No sessions in this category")),
            );
        }
        for summary in &listing.sessions {
            rows = rows.child(self.render_session_row(&project.id, summary, &entity));
        }
        if !listing.orphaned_temp_files.is_empty() {
            rows = rows.child(div().text_xs().text_color(rgb(0x00b4_5309)).child(
                language.choose_owned(
                    format!("检测到 {} 个中断写入的临时文件，可在确认后手动删除。", listing.orphaned_temp_files.len()),
                    format!("{} interrupted-write temporary files detected; review and remove them manually.", listing.orphaned_temp_files.len()),
                ),
            ));
        }
        rows
    }

    /// The same compact two-line row is used by global and feature-local histories.
    fn render_session_row(
        &self,
        project_id: &str,
        summary: &circuitfabric_project::SessionSummary,
        entity: &Entity<Self>,
    ) -> impl IntoElement {
        let metadata = &summary.metadata;
        let project_id = project_id.to_owned();
        let session_id = metadata.session_id.clone();
        let selector = format!("session-row-{session_id}");
        let opener = entity.clone();
        let (status_bg, status_fg) = match metadata.status.as_str() {
            "completed" => (0x00dc_fce7, 0x0016_a34a),
            "failed" => (0x00fe_e2e2, 0x00b4_2323),
            _ => (0x00fe_f3c7, 0x00b4_5309),
        };
        div()
            .id(format!("session-row-{session_id}"))
            .debug_selector(move || selector)
            .v_flex()
            .gap_1()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .cursor_pointer()
            .hover(|this| this.border_color(rgb(ACCENT_SOFT)).bg(rgb(SURFACE_BG)))
            .on_click(move |_, _, cx| {
                opener.update(cx, |view, cx| {
                    view.open_session_replay(project_id.clone(), session_id.clone(), cx)
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(rfc3339(metadata.started_at_unix_seconds)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .text_xs()
                            .bg(rgb(status_bg))
                            .text_color(rgb(status_fg))
                            .child(session_status_label(metadata.status, self.language)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!(
                                "{} · {}",
                                metadata.backend_id.as_deref().unwrap_or("—"),
                                metadata.session_id
                            )),
                    )
                    .child(div().flex_none().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                        "{} / {} tokens",
                        metadata.usage.input_tokens, metadata.usage.output_tokens
                    ))),
            )
    }

    pub(super) fn render_feature_session_history(
        &self,
        project_id: &str,
        category: circuitfabric_project::SessionCategory,
        subject_id: Option<&str>,
        adapter: Option<RuntimeAdapter>,
        entity: &Entity<Self>,
    ) -> Div {
        let sessions = self
            .project_data
            .get(project_id)
            .map(|data| {
                data.session_listing
                    .sessions
                    .iter()
                    .filter(|row| {
                        // Runtime histories also include pre-category records:
                        // those files have no `category` frontmatter and load
                        // as `Legacy`, but they are the same runtimes' quick
                        // sessions. Their backend ids still decide which
                        // runtime page shows them, so legacy records never
                        // mix into other runtimes' pages either.
                        let category_matches = row.metadata.category == category
                            || (category == circuitfabric_project::SessionCategory::Runtime
                                && row.metadata.category
                                    == circuitfabric_project::SessionCategory::Legacy);
                        category_matches
                            && adapter.is_none_or(|adapter| {
                                adapter.matches_backend(row.metadata.backend_id.as_deref())
                            })
                            && subject_id
                                .is_none_or(|id| row.metadata.subject_id.as_deref() == Some(id))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut rows = div().v_flex().gap_2();
        for summary in &sessions {
            rows = rows.child(self.render_session_row(project_id, summary, entity));
        }
        let panel = div().v_flex().gap_2().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.language.choose("会话历史", "Session history")),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    self.language.choose_owned(
                        format!("{} 条", sessions.len()),
                        format!("{} sessions", sessions.len()),
                    ),
                )),
        );
        if sessions.is_empty() {
            panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(self.language.choose("暂无这类会话", "No sessions in this category")),
            )
        } else {
            // The surrounding document pane keeps its own independent scroll position.
            panel.child(
                div().v_flex().max_h(px(280.)).child(
                    div()
                        .id(format!(
                            "feature-session-list-{}-{}",
                            category.as_str(),
                            adapter.map_or("all", RuntimeAdapter::backend_id),
                        ))
                        .flex_1()
                        .scroll_y()
                        .child(rows),
                ),
            )
        }
    }

    /// One bounded, window-level modal for every session entry point.
    pub(super) fn render_session_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sync_session_stream(cx);
        if self.session_replay.as_ref().is_some_and(|s| s.followup_prompt.is_none()) {
            let prompt = cx.new(|cx| {
                gpui_component::input::TextareaState::new(window, cx)
                    .auto_grow(2, 6)
                    .submit_on_enter(true)
                    .placeholder("补充信息，或要求模型进一步处理…")
            });
            cx.subscribe_in(&prompt, window, |view, emitter, event, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { shift: false, .. }
                    if view.session_modal_open
                        && view.session_modal_tab == SessionModalTab::Conversation
                        && view
                            .session_replay
                            .as_ref()
                            .and_then(|s| s.followup_prompt.as_ref())
                            .is_some_and(|p| p.entity_id() == emitter.entity_id()) =>
                {
                    view.send_session_followup(window, cx);
                }
                _ => {}
            })
            .detach();
            self.session_replay.as_mut().unwrap().followup_prompt = Some(prompt);
        }
        let selection = self.session_replay.as_ref().expect("session modal requires a replay");
        let metadata = &selection.replay.metadata;
        let language = self.language;
        let closer = cx.entity().clone();
        let backdrop_closer = closer.clone();
        let project_name = self
            .workspace
            .project(&selection.project_id)
            .map_or(selection.project_id.as_str(), |project| project.name.as_str());
        let active_tab = self.session_modal_tab;
        let mut tabs = TabBar::new("session-modal-tabs")
            .underline()
            .flex_none()
            .px_4()
            .border_b_1()
            .border_color(rgb(BORDER));
        for (tab, label, selector) in [
            (
                SessionModalTab::Conversation,
                language.choose("对话", "Conversation"),
                "session-tab-conversation",
            ),
            (SessionModalTab::RawData, language.choose("原始数据", "Raw data"), "session-tab-raw"),
        ] {
            let chooser = cx.entity().clone();
            tabs = tabs.child(
                Tab::new()
                    .debug_selector(move || selector.to_owned())
                    .label(label)
                    .selected(tab == active_tab)
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            view.session_modal_tab = tab;
                            cx.notify();
                        });
                    }),
            );
        }
        let body = match active_tab {
            SessionModalTab::Conversation => {
                let mut content =
                    div().flex().flex_1().min_h(px(0.)).min_w(px(0.)).overflow_hidden().child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .min_h(px(0.))
                            .child(self.render_session_conversation(cx))
                            .child(self.render_session_followup(cx)),
                    );
                if metadata.category == circuitfabric_project::SessionCategory::Datasheet {
                    content = content.child(self.render_session_datasheet(cx));
                }
                content.into_any_element()
            }
            SessionModalTab::RawData => self.render_session_raw_body().into_any_element(),
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("session-modal-backdrop")
                    .debug_selector(|| "session-modal-backdrop".into())
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x0000_0f17_2ab3))
                    .occlude()
                    .on_click(move |_, _, cx| {
                        backdrop_closer.update(cx, ControlPlaneView::close_session_replay)
                    }),
            )
            .child(
                div()
                    .id("session-detail-modal")
                    .debug_selector(|| "session-detail-modal".into())
                    .relative()
                    .occlude()
                    .w_full()
                    .max_w(px(
                        if metadata.category == circuitfabric_project::SessionCategory::Datasheet {
                            1600.
                        } else {
                            1100.
                        },
                    ))
                    .h_full()
                    .max_h(px(860.))
                    .v_flex()
                    .min_h(px(0.))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(CARD_BG))
                    .shadow_lg()
                    .overflow_hidden()
                    .child(self.session_modal_header(metadata, project_name, closer))
                    .child(tabs)
                    .child(body),
            )
    }

    fn render_session_datasheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.session_replay.as_ref().unwrap();
        let rows = selection.datasheet_rows.clone();
        let list = selection.datasheet_list.clone();
        let language = self.language;
        let entity = cx.entity().clone();
        let disabled = self.session_followup.is_some()
            || self.quick_task(RuntimeAdapter::CodexAppServer).cancel.is_some()
            || self.datasheet_extracting
            || selection.followup_unavailable.is_some()
            || selection.datasheet.is_none();
        div()
            .id("session-datasheet-panel")
            .debug_selector(|| "session-datasheet-panel".into())
            .w(gpui::relative(0.38))
            .min_w(px(240.))
            .max_w(px(560.))
            .h_full()
            .min_h(px(0.))
            .v_flex()
            .border_l_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(
                div()
                    .p_3()
                    .v_flex()
                    .gap_2()
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language.choose("当前文档数据", "Document data")),
                            )
                            .child(
                                action_button("session-generate-overview")
                                    .ghost()
                                    .label(language.choose("生成概述", "Generate overview"))
                                    .disabled(disabled)
                                    .on_click(move |_, window, cx| {
                                        entity.update(cx, |view, cx| {
                                            view.send_session_followup_request(
                                                Some(language.choose(
                                                    "请根据当前文档生成或更新文档概述，说明这是什么芯片或器件、用途和主要功能。只更新概述，保留所有数据表。",
                                                    "Generate or update the document overview: describe the chip or device, its purpose and main functions. Update only the overview; preserve all data tables."
                                                ).into()),
                                                window,
                                                cx,
                                            );
                                        })
                                    }),
                            ),
                    )
                    .when_some(selection.datasheet.as_ref(), |heading, data| {
                        let mut overview_markdown = data.overview.description.trim().to_owned();
                        if !data.overview.features.is_empty() {
                            if !overview_markdown.is_empty() {
                                overview_markdown.push_str("\n\n");
                            }
                            overview_markdown.push_str(
                                &data.overview
                                    .features
                                    .iter()
                                    .map(|feature| format!("- {feature}"))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            );
                        }
                        heading
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .truncate()
                                    .child(data.overview.title.clone()),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .p_3()
                                    .rounded_lg()
                                    .bg(rgb(SURFACE_BG))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .child(language.choose("文档概述", "Document overview")),
                                    )
                                    .when(!overview_markdown.is_empty(), |card| {
                                        card.child(
                                            div()
                                                .id("session-document-overview")
                                                .debug_selector(|| {
                                                    "session-document-overview".into()
                                                })
                                                .max_h(px(180.))
                                                .scroll_y()
                                                .text_sm()
                                                .child(
                                                    gpui_component::text::TextView::markdown(
                                                        "session-overview-text",
                                                        super::session_markdown::bound_plain_paragraphs(
                                                            &overview_markdown,
                                                        ),
                                                    )
                                                    .selectable(true),
                                                ),
                                        )
                                    }),
                            )
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose(
                                    "可通过对话生成或更新概述；概述不经过 Jev 校验",
                                    "Ask in chat to generate or update the overview; no Jev validation",
                                ),
                            ))
                    }),
            )
            .child(if rows.is_empty() {
                div()
                    .flex_1()
                    .p_3()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose(
                        "暂无已保存的数据，可通过对话补充",
                        "No saved data yet; add rows through this conversation",
                    ))
                    .into_any_element()
            } else {
                let rendered_rows = rows.clone();
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(
                        gpui::list(list.clone(), move |ix, _, _| {
                            let (title, detail) = &rendered_rows[ix];
                            if detail.is_empty() {
                                // Section heading between row groups.
                                div()
                                    .pt_3()
                                    .pb_1()
                                    .px_3()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .child(title.clone()),
                                    )
                                    .into_any_element()
                            } else {
                                div()
                                    .p_3()
                                    .v_flex()
                                    .gap_1()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(title.clone()),
                                    )
                                    .child(div().text_xs().whitespace_normal().child(
                                        gpui_base::SelectableText::new(
                                            ("session-data-row", ix),
                                            detail.clone(),
                                        ),
                                    ))
                                    .into_any_element()
                            }
                        })
                        .size_full(),
                    )
                    .child(
                        div().absolute().inset_0().child(
                            gpui_base::Scrollbar::vertical(&list)
                                .id("session-data-scrollbar")
                                .viewport_from_layout(),
                        ),
                    )
                    .into_any_element()
            })
    }

    /// Title, identity and lifecycle line; stays visible while the body switches tabs.
    fn session_modal_header(
        &self,
        metadata: &circuitfabric_project::SessionMetadata,
        project_name: &str,
        closer: Entity<Self>,
    ) -> impl IntoElement {
        let language = self.language;
        let (status_bg, status_fg) = match metadata.status.as_str() {
            "completed" => (0x00dc_fce7, 0x0016_a34a),
            "failed" => (0x00fe_e2e2, 0x00b4_2323),
            _ => (0x00fe_f3c7, 0x00b4_5309),
        };
        div()
            .debug_selector(|| "session-modal-header".into())
            .flex_none()
            .v_flex()
            .gap_1p5()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language.choose("会话详情", "Session details")),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_full()
                                    .text_xs()
                                    .bg(rgb(status_bg))
                                    .text_color(rgb(status_fg))
                                    .child(session_status_label(metadata.status, language)),
                            ),
                    )
                    .child(
                        action_button("close-session-replay")
                            .ghost()
                            .label(language.choose("关闭", "Close"))
                            .on_click(move |_, _, cx| {
                                closer.update(cx, ControlPlaneView::close_session_replay)
                            }),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT_SECONDARY))
                    .truncate()
                    .child(format!("{project_name} · {}", metadata.session_id)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x_2()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(metadata.backend_id.clone().unwrap_or_else(|| "—".into()))
                    .child(format!(
                        "{} → {}",
                        rfc3339(metadata.started_at_unix_seconds),
                        metadata
                            .completed_at_unix_seconds
                            .map(rfc3339)
                            .unwrap_or_else(|| "—".into())
                    ))
                    .child(format!(
                        "{} / {} tokens",
                        metadata.usage.input_tokens, metadata.usage.output_tokens
                    )),
            )
    }

    /// The raw tab: the complete audit record with parsed Markdown and full-text copy.
    fn render_session_raw_body(&self) -> impl IntoElement {
        let selection = self.session_replay.as_ref().expect("raw body requires a replay");
        div()
            .id("session-replay-body")
            .debug_selector(|| "session-replay-body".into())
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .overflow_hidden()
            .child(
                gpui_component::text::TextView::new(&selection.markdown)
                    .w_full()
                    .h_full()
                    .p_4()
                    .scrollable(true)
                    .selectable(true),
            )
    }

    /// The conversation tab: chat bubbles over a virtualized list, so dragging the
    /// scrollbar only ever lays out the handful of rows on screen.
    fn render_session_conversation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.session_replay.as_ref().expect("conversation requires a replay");
        let language = self.language;
        let rows = selection.conversation.clone();
        let list_state = selection.conversation_list.clone();
        let away_from_end = !list_state.is_following_tail() && !selection.conversation_at_end.get();
        let tail_list = list_state.clone();
        let entity = cx.entity().clone();
        let measured_list = list_state.clone();
        let measured_end = selection.conversation_at_end.clone();
        let repaint = entity.clone();
        let body = if rows.is_empty() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(
                    language.choose("此会话还没有对话内容", "No conversation in this session yet"),
                ))
                .into_any_element()
        } else {
            let render_rows = rows.clone();
            div()
                .size_full()
                .relative()
                .child(
                    gpui::list(list_state.clone(), move |ix, _window, _cx| {
                        conversation_row_element(&render_rows[ix], ix, language)
                    })
                    .size_full()
                    .pt_2(),
                )
                .child(
                    div().absolute().inset_0().child(
                        gpui_base::Scrollbar::vertical(&list_state)
                            .id("session-conversation-scrollbar")
                            .viewport_from_layout(),
                    ),
                )
                .into_any_element()
        };
        div()
            .id("session-conversation-body")
            .debug_selector(|| "session-conversation-body".into())
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .relative()
            .overflow_hidden()
            .bg(rgb(SURFACE_BG))
            .child(body)
            // Inspect the last row after layout: summing cached heights before
            // layout is unreliable when streaming invalidates virtualized rows.
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |_, _, _, cx| {
                        let count = measured_list.item_count();
                        let at_end = count == 0
                            || measured_list.is_following_tail()
                            || measured_list.bounds_for_item(count - 1).is_some_and(|bounds| {
                                bounds.bottom() <= measured_list.viewport_bounds().bottom() + px(2.)
                            });
                        if measured_end.replace(at_end) != at_end {
                            cx.defer(move |cx| repaint.update(cx, |_, cx| cx.notify()));
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(away_from_end, |body| {
                body.child(
                    div().absolute().top_2().left_0().right_0().flex().justify_center().child(
                        action_button("session-jump-to-latest")
                            .debug_selector(|| "session-jump-to-latest".into())
                            .icon(gpui_component::IconName::ArrowDown)
                            .tooltip(language.choose("跳到最后一条", "Jump to latest"))
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(CARD_BG))
                            .shadow_md()
                            .on_click(move |_, _, cx| {
                                tail_list.set_follow_mode(gpui::FollowMode::Tail);
                                tail_list.scroll_to_end();
                                entity.update(cx, |_, cx| cx.notify());
                            }),
                    ),
                )
            })
    }

    /// The sidebar route uses the same persisted replay as the project detail tab.
    /// Filtering and opening a record only read project storage; neither starts a session.
    pub(super) fn render_sessions_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let entity = cx.entity().clone();
        let projects = self.workspace.projects().into_iter().cloned().collect::<Vec<_>>();
        let selected = self
            .session_project_filter
            .as_ref()
            .and_then(|id| projects.iter().find(|project| &project.id == id))
            .or_else(|| {
                self.navigation
                    .selected_project
                    .as_ref()
                    .and_then(|id| projects.iter().find(|project| &project.id == id))
            })
            .or_else(|| projects.first())
            .cloned();

        let mut filters = div().flex().flex_wrap().gap_2();
        for project in &projects {
            let project_id = project.id.clone();
            let active = selected.as_ref().is_some_and(|current| current.id == project_id);
            let chooser = entity.clone();
            filters = filters.child(
                action_button(format!("session-project-{project_id}"))
                    .label(project.name.clone())
                    .when(active, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            view.session_project_filter = Some(project_id.clone());
                            view.session_replay = None;
                            cx.notify();
                        });
                    }),
            );
        }

        let mut categories = TabBar::new("session-category-tabs").underline().flex_none();
        for (category, label) in [
            (circuitfabric_project::SessionCategory::Eda, language.choose("EDA", "EDA")),
            (
                circuitfabric_project::SessionCategory::Datasheet,
                language.choose("结构化提取", "Extraction"),
            ),
            (
                circuitfabric_project::SessionCategory::Runtime,
                language.choose("运行时任务", "Runtime tasks"),
            ),
            (
                circuitfabric_project::SessionCategory::Legacy,
                language.choose("旧版审计", "Legacy audit"),
            ),
        ] {
            let chooser = entity.clone();
            let selector = format!("session-category-{}", category.as_str());
            categories = categories.child(
                Tab::new()
                    .debug_selector(move || selector.clone())
                    .label(label)
                    .selected(self.session_category_filter == category)
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            view.session_category_filter = category;
                            view.session_replay = None;
                            cx.notify();
                        });
                    }),
            );
        }

        let content = if let Some(project) = selected {
            let project_id = project.id.clone();
            let refresher = entity.clone();
            div()
                .v_flex()
                .gap_3()
                .size_full()
                .child(categories)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                            language.choose_owned(
                                format!("当前项目：{} · {}", project.name, project.id),
                                format!("Current project: {} · {}", project.name, project.id),
                            ),
                        ))
                        .child(
                            action_button("refresh-session-list")
                                .ghost()
                                .label(language.choose("刷新", "Refresh"))
                                .on_click(move |_, _, cx| {
                                    refresher.update(cx, |view, cx| {
                                        if let Err(error) = view.refresh_project_data(&project_id) {
                                            view.status = format!("会话列表未刷新：{error}");
                                        }
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(div().id("sessions-page-body").flex_1().min_h(px(0.)).scroll_y().child(
                    self.render_category_sessions_tab(&project, self.session_category_filter, cx),
                ))
                .into_any_element()
        } else {
            let opener = entity.clone();
            div()
                .v_flex()
                .gap_3()
                .items_center()
                .justify_center()
                .h_full()
                .child(language.choose("还没有项目", "No projects yet"))
                .child(
                    action_button("sessions-open-projects")
                        .primary()
                        .label(language.choose("打开项目", "Open projects"))
                        .on_click(move |_, _, cx| {
                            opener.update(cx, |view, cx| {
                                view.navigation.screen = ControlPlaneScreen::Projects;
                                cx.notify();
                            });
                        }),
                )
                .into_any_element()
        };

        let canceller = entity;
        let active_quick_tasks =
            self.quick_tasks.values().filter(|task| task.cancel.is_some()).count();
        let any_quick_task_running = self.any_quick_task_running();
        workspace_page("sessions-page")
            .child(div().flex().items_center().justify_between().when(
                any_quick_task_running,
                |this| {
                    this.child(
                        action_button("sessions-cancel-task")
                            .label(if active_quick_tasks > 1 {
                                language.choose_owned(
                                    format!("取消全部 {active_quick_tasks} 个验证任务"),
                                    format!("Cancel all {active_quick_tasks} verification tasks"),
                                )
                            } else {
                                language.choose("取消当前任务", "Cancel current task").to_owned()
                            })
                            .on_click(move |_, _, cx| {
                                canceller.update(cx, |view, cx| {
                                    view.cancel_all_quick_tasks();
                                    cx.notify();
                                });
                            }),
                    )
                },
            ))
            .child(filters)
            .child(div().flex_1().min_h(px(0.)).child(content))
    }
}
