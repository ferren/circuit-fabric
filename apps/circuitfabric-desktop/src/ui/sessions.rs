//! Sessions presentation and event handlers.
use super::*;

impl ControlPlaneView {
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_sessions_tab(
        &mut self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;

        if let Some(selection) = &self.session_replay
            && selection.project_id == project.id
        {
            let replay = &selection.replay;
            let metadata = &replay.metadata;
            let closer = entity.clone();
            let mut body = div().v_flex().gap_0p5();
            for line in replay.body.lines() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT_PRIMARY))
                        .whitespace_normal()
                        .child(line.to_owned()),
                );
            }
            return div()
                .v_flex()
                .gap_3()
                .size_full()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .whitespace_normal()
                                        .child(metadata.session_id.clone()),
                                )
                                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                            "{} · {} · {} → {}",
                                            metadata
                                                .backend_id
                                                .clone()
                                                .unwrap_or_else(|| "-".to_owned()),
                                            rfc3339(metadata.started_at_unix_seconds),
                                            metadata.status.as_str(),
                                            metadata
                                                .completed_at_unix_seconds
                                                .map(rfc3339)
                                                .unwrap_or_else(|| "—".to_owned()),
                                        ))),
                        )
                        .child(
                            action_button("close-session-replay")
                                .ghost()
                                .label(language.choose("返回列表", "Back to list"))
                                .on_click(move |_, _, cx| {
                                    closer.update(cx, ControlPlaneView::close_session_replay);
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(div().flex_1().min_w(px(0.)).whitespace_normal().child(format!(
                            "{} input / {} output tokens",
                            metadata.usage.input_tokens, metadata.usage.output_tokens
                        )))
                        .when(!metadata.citations.is_empty(), |this| {
                            this.child(
                                div()
                                    .min_w(px(0.))
                                    .whitespace_normal()
                                    .child(format!("引用 {}", metadata.citations.join("、"))),
                            )
                        }),
                )
                .child(
                    div()
                        .id("session-replay-body")
                        .v_flex()
                        .gap_0p5()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .max_h(px(480.))
                        .scroll_y()
                        .child(body),
                )
                .into_any_element();
        }

        let listing = self
            .project_data
            .get(&project.id)
            .map(|data| data.session_listing.clone())
            .unwrap_or_default();

        if listing.sessions.is_empty() {
            return div()
                .v_flex()
                .gap_2()
                .items_center()
                .justify_center()
                .h_full()
                .text_center()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("还没有会话", "No sessions yet")),
                )
                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                    language.choose(
                        "会话由受支持的 EDA bridge 启动后，会以 Markdown 审计记录的形式出现在这里。",
                        "Sessions started from a supported EDA bridge appear here as Markdown audit records.",
                    ),
                ))
                .when(!listing.orphaned_temp_files.is_empty(), |this| {
                    this.child(div().text_xs().text_color(rgb(0x00b4_5309)).child(
                        language.choose_owned(
                            format!(
                                "检测到 {} 个中断写入的临时文件，可在确认后手动删除。",
                                listing.orphaned_temp_files.len()
                            ),
                            format!(
                                "{} interrupted-write temporary files detected; review and remove them manually.",
                                listing.orphaned_temp_files.len()
                            ),
                        ),
                    ))
                })
                .into_any_element();
        }

        let mut rows = div().v_flex().gap_2();
        for summary in &listing.sessions {
            let metadata = &summary.metadata;
            let project_id = project.id.clone();
            let session_id = metadata.session_id.clone();
            let opener = entity.clone();
            let (status_bg, status_fg) = match metadata.status.as_str() {
                "completed" => (0x00dc_fce7, 0x0016_a34a),
                "failed" => (0x00fe_e2e2, 0x00b4_2323),
                _ => (0x00fe_f3c7, 0x00b4_5309),
            };
            rows = rows.child(
                div()
                    .id(format!("session-row-{}", metadata.session_id))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                    .on_click(move |_, _, cx| {
                        opener.update(cx, |view, cx| {
                            view.open_session_replay(project_id.clone(), session_id.clone(), cx);
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
                                    .child(metadata.session_id.clone()),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(status_bg))
                                    .text_color(rgb(status_fg))
                                    .child(metadata.status.as_str()),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(SURFACE_BG))
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(
                                        metadata
                                            .backend_id
                                            .clone()
                                            .unwrap_or_else(|| "-".to_owned()),
                                    ),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(rfc3339(metadata.started_at_unix_seconds)),
                            ),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                        "{} in / {} out tokens · {}",
                        metadata.usage.input_tokens,
                        metadata.usage.output_tokens,
                        summary.file_name
                    ))),
            );
        }
        if !listing.orphaned_temp_files.is_empty() {
            rows = rows.child(
                div().text_xs().text_color(rgb(0x00b4_5309)).child(language.choose_owned(
                    format!(
                        "检测到 {} 个中断写入的临时文件，可在确认后手动删除。",
                        listing.orphaned_temp_files.len()
                    ),
                    format!(
                        "{} interrupted-write temporary files detected; review and remove them manually.",
                        listing.orphaned_temp_files.len()
                    ),
                )),
            );
        }

        div().v_flex().gap_3().size_full().child(rows).into_any_element()
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

        let content = if let Some(project) = selected {
            let project_id = project.id.clone();
            let refresher = entity.clone();
            div()
                .v_flex()
                .gap_3()
                .size_full()
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
                .child(
                    div()
                        .id("sessions-page-body")
                        .flex_1()
                        .min_h(px(0.))
                        .scroll_y()
                        .child(self.render_sessions_tab(&project, cx)),
                )
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
        workspace_page("sessions-page")
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("会话与任务", "Sessions & tasks")),
                    )
                    .when(self.task_cancel.is_some(), |this| {
                        this.child(
                            action_button("sessions-cancel-task")
                                .label(language.choose("取消当前任务", "Cancel current task"))
                                .on_click(move |_, _, cx| {
                                    canceller.update(cx, |view, cx| {
                                        if let Some(cancel) = &view.task_cancel {
                                            cancel.cancel();
                                        }
                                        cx.notify();
                                    });
                                }),
                        )
                    }),
            )
            .child(filters)
            .child(div().flex_1().min_h(px(0.)).child(content))
    }
}
