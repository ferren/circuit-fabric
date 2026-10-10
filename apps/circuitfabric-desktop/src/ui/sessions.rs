//! Sessions presentation and event handlers.
use super::*;

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

impl ControlPlaneView {
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
                        row.metadata.category == category
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
                        .id(format!("feature-session-list-{}", category.as_str()))
                        .flex_1()
                        .scroll_y()
                        .child(rows),
                ),
            )
        }
    }

    /// One bounded, window-level modal for every session entry point.
    pub(super) fn render_session_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.session_replay.as_ref().expect("session modal requires a replay");
        let metadata = &selection.replay.metadata;
        let language = self.language;
        let closer = cx.entity().clone();
        let backdrop_closer = closer.clone();
        let project_name = self
            .workspace
            .project(&selection.project_id)
            .map_or(selection.project_id.as_str(), |project| project.name.as_str());
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
                    .max_w(px(1000.))
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
                    .child(
                        div()
                            .debug_selector(|| "session-modal-header".into())
                            .flex_none()
                            .v_flex()
                            .gap_2()
                            .p_4()
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
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(language.choose("会话详情", "Session details")),
                                    )
                                    .child(
                                        action_button("close-session-replay")
                                            .ghost()
                                            .label(language.choose("关闭", "Close"))
                                            .on_click(move |_, _, cx| {
                                                closer.update(
                                                    cx,
                                                    ControlPlaneView::close_session_replay,
                                                )
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
                                    .gap_2()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(session_status_label(metadata.status, language))
                                    .child(
                                        metadata.backend_id.clone().unwrap_or_else(|| "—".into()),
                                    )
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
                            ),
                    )
                    .child(
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
                            ),
                    ),
            )
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

        let mut categories = div().flex().flex_wrap().gap_2();
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
            categories = categories.child(
                action_button(format!("session-category-{}", category.as_str()))
                    .label(label)
                    .when(self.session_category_filter == category, |button| button.primary())
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
