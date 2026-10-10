//! Workspace overview presentation; aggregation lives in application/overview.
use super::*;
use crate::application::overview::{OverviewModel, TodoTarget};

impl ControlPlaneView {
    pub(super) fn refresh_overview(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = ProjectRegistry::load_or_default(&self.project_registry_path) {
            self.overview_restore_diagnostics =
                vec![format!("项目注册表未刷新 / registry refresh failed: {error}")];
            for project in self.workspace.projects() {
                self.project_read_errors.insert(project.id.clone(), error.to_string());
            }
            cx.notify();
            return;
        }
        let (registry, workspace, storages, mut data, diagnostics) =
            project_data::restore_project_workspace(&self.project_registry_path);
        let mut errors = BTreeMap::new();
        for entry in registry.entries() {
            if !data.contains_key(&entry.project_id) {
                errors.insert(
                    entry.project_id.clone(),
                    diagnostics
                        .iter()
                        .filter(|message| message.contains(&format!("`{}`", entry.project_id)))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; "),
                );
                if let Some(old) = self.project_data.get(&entry.project_id) {
                    data.insert(entry.project_id.clone(), old.clone());
                }
            }
        }
        self.workspace = workspace;
        self.project_registry = registry;
        self.project_storages = storages;
        self.project_data = data;
        self.project_read_errors = errors;
        self.overview_restore_diagnostics = diagnostics;
        if self
            .navigation
            .selected_project
            .as_ref()
            .is_some_and(|id| self.workspace.project(id).is_none())
        {
            self.navigation.selected_project = None;
        }
        self.usage_audit_cached = None;
        self.status = "工作区总览已刷新；来源失败详情保留在总览中。".into();
        cx.notify();
    }

    pub(super) fn overview_model(&mut self) -> OverviewModel {
        let mut projects: BTreeMap<_, _> = self
            .project_registry
            .entries()
            .map(|entry| (entry.project_id.clone(), entry.display_name.clone()))
            .collect();
        projects
            .extend(self.workspace.projects().into_iter().map(|p| (p.id.clone(), p.name.clone())));
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut model = self
            .overview_cached
            .get(&projects, &self.project_data, &self.project_read_errors, now)
            .clone();
        model.registry_unknown =
            self.overview_restore_diagnostics.iter().any(|message| message.contains("项目注册表"));
        model
    }

    pub(super) fn overview_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.maybe_probe_bridge_health(window, cx);
        let language = self.language;
        let model = self.overview_model();
        let entity = cx.entity();
        let compact = window.viewport_size().width < px(1150.);
        let value = |count: usize| {
            if !model.complete() && model.loaded_projects == 0 {
                language.choose("— 未读取", "— Unread").into()
            } else if model.complete() {
                count.to_string()
            } else {
                format!("{count} {}", language.choose("（已读部分）", "(loaded subset)"))
            }
        };
        let refresher = entity.clone();
        let mut actions = div().flex().flex_wrap().gap_2().child(
            action_button("refresh-overview")
                .label(language.choose("刷新工作区", "Refresh workspace"))
                .on_click(move |_, _, cx| {
                    refresher.update(cx, |view, cx| view.refresh_overview(cx))
                }),
        );
        for (id, label, screen) in [
            (
                "overview-projects",
                language.choose("创建 / 打开项目", "Create / open project"),
                ControlPlaneScreen::Projects,
            ),
            (
                "overview-documents",
                language.choose("导入 / 授权文档", "Import / authorize documents"),
                ControlPlaneScreen::Documents,
            ),
            (
                "overview-runtime",
                language.choose("配置运行时 / 启动会话", "Configure runtime / start session"),
                ControlPlaneScreen::AgentsAndMcp,
            ),
        ] {
            let target = entity.clone();
            actions =
                actions.child(action_button(id).ghost().label(label).on_click(move |_, _, cx| {
                    target.update(cx, |view, cx| {
                        view.navigation.screen = screen;
                        cx.notify();
                    });
                }));
        }
        for (id, label, screen) in [
            (
                "overview-changes",
                language.choose("查看变更", "View changes"),
                ControlPlaneScreen::ChangesAndApprovals,
            ),
            (
                "overview-validation",
                language.choose("查看验证", "View validation"),
                ControlPlaneScreen::Semantics,
            ),
        ] {
            let target = entity.clone();
            actions = actions.child(
                action_button(id)
                    .ghost()
                    .disabled(self.navigation.selected_project.is_none())
                    .label(label)
                    .on_click(move |_, _, cx| {
                        target.update(cx, |view, cx| {
                            view.navigation.screen = screen;
                            cx.notify();
                        })
                    }),
            );
        }
        let metric = |selector: &'static str, value: String, label: &'static str, color| {
            div()
                .debug_selector(move || selector.into())
                .flex_1()
                .min_w(px(150.))
                .v_flex()
                .gap_2()
                .p_4()
                .bg(rgb(CARD_BG))
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(status_dot(color))
                        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value)),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
        };
        let bridge_value = match &self.bridge_health {
            BridgeHealth::Unknown => language.choose("— 未知", "— Unknown").to_owned(),
            BridgeHealth::Listening { at } if at.elapsed() <= Duration::from_secs(15) => "1".into(),
            BridgeHealth::Listening { .. } => {
                language.choose("— 探测已过期", "— Probe expired").into()
            }
            BridgeHealth::Unreachable { .. }
                if self
                    .bridge_probed_at
                    .is_some_and(|at| at.elapsed() <= Duration::from_secs(15)) =>
            {
                "0".into()
            }
            BridgeHealth::Unreachable { .. } => {
                language.choose("— 探测已过期", "— Probe expired").into()
            }
        };
        let charts = div()
            .flex()
            .gap_4()
            .when(compact, |d| d.flex_col())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(super::overview_charts::fact_chart(&model, language)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(super::overview_charts::session_chart(&model, language)),
            );
        let mut diagnostics = div().v_flex().gap_1().text_xs().text_color(rgb(0x00b4_5309));
        for message in self.overview_restore_diagnostics.iter().chain(&model.diagnostics) {
            diagnostics = diagnostics.child(message.clone());
        }
        for row in &model.resources {
            if let Some(error) = &row.error {
                diagnostics = diagnostics.child(format!(
                    "{} / {} · {error} · {} {}",
                    row.name,
                    row.id,
                    language.choose("保留缓存时间（UTC）", "Retained cache at (UTC)"),
                    if row.loaded_at == 0 { "—".into() } else { rfc3339(row.loaded_at) }
                ));
            }
        }
        let current_task = self
            .task_identity
            .as_ref()
            .map(|(project, session, runtime)| {
                format!(
                    "{} · {runtime} · {} / {}",
                    if self
                        .task_cancel
                        .as_ref()
                        .is_some_and(|cancel| cancel.0.load(Ordering::SeqCst))
                    {
                        language.choose("正在取消", "Cancelling")
                    } else {
                        language.choose("运行中", "Running")
                    },
                    project.as_deref().unwrap_or("no project"),
                    session.as_deref().unwrap_or("not persisted")
                )
            })
            .unwrap_or_else(|| language.choose("无运行任务", "Idle").into());
        let health = panel(language.choose("运行时健康", "Runtime health"))
            .child(format!("Codex App Server · {}", self.codex_check_label(language)))
            .child(format!("EDA bridge · {}", self.bridge_health.label(language)))
            .child(language.choose("Codex 显示最近一次连接检查；bridge 来自受监管进程及最近 TCP 探测，TCP 可达不等于 EDA 插件就绪。", "Codex shows the latest connection check; the bridge row comes from the supervised process and latest TCP probe, and reachable TCP does not imply EDA readiness."))
            .child(format!("{} · {}", language.choose("当前任务", "Current task"),
                current_task));
        let mut sessions = panel(language.choose("最近持久会话", "Recent persisted sessions"));
        if model.sessions.is_empty() {
            sessions = sessions.child(language.choose(
                "暂无会话；配置运行时后启动任务。",
                "No sessions; configure a runtime and start a task.",
            ));
        }
        for (index, row) in model.sessions.iter().take(5).enumerate() {
            let target = entity.clone();
            let project = row.project_id.clone();
            let session = row.id.clone();
            sessions = sessions
                .child(div().text_xs().child(format!(
                    "{} / {} · {} · {}",
                    row.project_id,
                    row.id,
                    row.status.as_str(),
                    if row.started_at == 0 {
                        "unknown time".into()
                    } else {
                        rfc3339(row.started_at)
                    }
                )))
                .child(
                    action_button(format!("overview-session-{index}"))
                        .ghost()
                        .label(language.choose("查看会话", "Open session"))
                        .on_click(move |_, _, cx| {
                            target.update(cx, |view, cx| {
                                view.navigation.screen = ControlPlaneScreen::SessionsAndTasks;
                                view.session_project_filter = Some(project.clone());
                                view.open_session_replay(project.clone(), session.clone(), cx);
                            })
                        }),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(row.locator.clone()));
        }
        let mut todos = panel(language.choose("待办与验证事实", "To-do & verification facts"));
        if model.todos.is_empty() {
            todos = todos.child(language.choose(
                "已读部分没有待办；导入语义快照并运行验证。",
                "No to-dos in loaded sources; import a semantic snapshot and run validation.",
            ));
        }
        for (index, row) in model.todos.iter().take(20).enumerate() {
            let opener = entity.clone();
            let project = row.project_id.clone();
            let target = row.target.clone();
            todos = todos.child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(div().text_xs().child(format!("{} / {}", row.project_id, row.label)))
                    .child(
                        action_button(format!("overview-todo-{index}"))
                            .ghost()
                            .label(language.choose("查看原记录", "Open source"))
                            .on_click(move |_, window, cx| {
                                opener.update(cx, |view, cx| {
                                    view.select_project(project.clone(), cx);
                                    match &target {
                                        TodoTarget::ChangeSet(id) => {
                                            view.navigation.screen =
                                                ControlPlaneScreen::ChangesAndApprovals;
                                            view.selected_change_set =
                                                Some((project.clone(), id.clone()));
                                        }
                                        TodoTarget::Constraint { snapshot_hash, constraint_id } => {
                                            view.navigation.screen = ControlPlaneScreen::Semantics;
                                            view.selected_semantic_snapshot =
                                                Some((project.clone(), snapshot_hash.clone()));
                                            view.semantic_query.update(cx, |state, cx| {
                                                state.set_value(constraint_id.clone(), window, cx)
                                            });
                                        }
                                    }
                                    cx.notify();
                                })
                            }),
                    ),
            );
        }
        if model.todos.len() > 20 {
            todos = todos.child(format!(
                "20 / {} · {}",
                model.todos.len(),
                language.choose(
                    "其余请到变更或语义页查看",
                    "See changes or semantics for remaining records"
                )
            ));
        }
        page("overview-page")
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.choose("总览", "Overview")),
            )
            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(format!(
                "{} · {}/{} {}",
                language.choose(
                    "工作区全部登记项目；不随当前项目选择变化",
                    "All registered workspace projects; independent of selected project"
                ),
                model.loaded_projects,
                model.resources.len(),
                language.choose("项目已读取", "projects loaded")
            )))
            .child(actions)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .child(metric(
                        "overview-project-kpi",
                        if model.registry_unknown {
                            "—".into()
                        } else {
                            model.resources.len().to_string()
                        },
                        language.choose("登记项目", "Registered projects"),
                        ACCENT,
                    ))
                    .child(metric(
                        "overview-document-kpi",
                        value(model.documents),
                        language.choose("已授权文档", "Authorized documents"),
                        0x008b_5cf6,
                    ))
                    .child(metric(
                        "overview-bridge-kpi",
                        bridge_value,
                        language.choose(
                            "在线 bridge（配置端点）",
                            "Online bridge (configured endpoint)",
                        ),
                        self.bridge_health.dot(),
                    ))
                    .child(metric(
                        "overview-pending-kpi",
                        if model.unknown_approval_states > 0 {
                            format!(
                                "{} · {} {}",
                                value(model.pending),
                                model.unknown_approval_states,
                                language.choose("状态未知", "unknown states")
                            )
                        } else {
                            value(model.pending)
                        },
                        language.choose("待审批 ChangeSet", "Pending ChangeSets"),
                        0x00b4_5309,
                    )),
            )
            .child(diagnostics)
            .child(charts)
            .child(super::overview_charts::resource_chart(&model, language))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .when(compact, |d| d.flex_col())
                    .child(health)
                    .child(sessions)
                    .child(todos),
            )
    }
}

fn panel(title: &'static str) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .v_flex()
        .gap_2()
        .p_4()
        .bg(rgb(CARD_BG))
        .rounded_xl()
        .border_1()
        .border_color(rgb(BORDER))
        .text_sm()
        .whitespace_normal()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
}
