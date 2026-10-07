//! Projects presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn refresh_project_data(&mut self, project_id: &str) -> Result<(), String> {
        let result = project_data::refresh_project_data(
            &self.project_storages,
            &mut self.project_data,
            project_id,
        );
        match &result {
            Ok(()) => {
                self.project_read_errors.remove(project_id);
            }
            Err(error) => {
                self.project_read_errors.insert(project_id.to_owned(), error.clone());
            }
        }
        self.usage_audit_cached = None;
        result
    }

    pub(super) fn open_project_form(&mut self, cx: &mut Context<Self>) {
        self.project_form_open = true;
        "选择一个已有文件夹，再确认创建受管理的项目目录。".clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn choose_project_root(&mut self, window: &Window, cx: &mut Context<Self>) {
        let dialog = rfd::AsyncFileDialog::new().set_title("选择项目根文件夹").set_parent(window);
        cx.spawn_in(window, async move |view, cx| {
            let Some(file_handle) = dialog.pick_folder().await else {
                return;
            };
            let root = file_handle.path().to_path_buf();
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.new_project_root.update(cx, |state, cx| {
                        state.set_value(root.display().to_string(), window, cx);
                    });
                    view.status =
                        format!("已选择项目根文件夹：{}。创建前不会修改该文件夹。", root.display());
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn persist_project_registry(&self) -> Result<(), String> {
        self.project_registry.save(&self.project_registry_path).map_err(|error| error.to_string())
    }

    pub(super) fn select_project(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        if self.project_data.contains_key(&project_id)
            && self.project_storages.contains_key(&project_id)
        {
            self.overview_restore_diagnostics
                .retain(|message| !message.contains(&format!("`{project_id}`")));
        }
        self.invalidate_tool_runs();
        if let Some(cancel) = &self.task_cancel {
            cancel.cancel();
        }
        self.task_result.clear();
        if let Err(error) = self.project_registry.mark_opened(&project_id) {
            self.status = format!("项目已打开，但未能记录最近活动：{error}");
        } else if let Err(error) = self.persist_project_registry() {
            self.status = format!("项目已打开，但未能保存项目注册表：{error}");
        }
        self.navigation.select_project(project_id);
        self.session_project_filter = None;
        self.project_tab = ProjectDetailTab::Overview;
        self.session_replay = None;
        self.selected_semantic_snapshot = None;
        self.selected_change_set = None;
        self.approval_drawer_open = false;
        cx.notify();
    }

    /// Mirrors the selected project into the native window caption, e.g.
    /// "CircuitFabric — Signal-chain prototype". Runs every frame but only
    /// reaches the platform when the desired title actually changed, so
    /// every path that sets `selected_project` is covered without each
    /// one needing window access.
    pub(super) fn sync_window_title(&mut self, window: &mut Window) {
        let project_name = self
            .navigation
            .selected_project
            .as_deref()
            .and_then(|id| self.workspace.project(id))
            .map(|project| project.name.clone());
        let desired = app_window_title(project_name.as_deref());
        if desired != self.window_title {
            window.set_window_title(desired.as_str());
            self.window_title = desired;
        }
    }

    pub(super) fn import_project_document(
        &mut self,
        category: DocumentCategory,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self.navigation.selected_project.clone() else {
            return;
        };
        if !self.project_storages.contains_key(&project_id) {
            self.status = "未导入：项目根目录未打开。".to_owned();
            cx.notify();
            return;
        }
        let dialog_title = match category {
            DocumentCategory::Datasheet => "导入 Datasheet",
            DocumentCategory::ReferenceDesign => "导入参考设计",
        };
        let dialog = rfd::AsyncFileDialog::new().set_title(dialog_title).set_parent(window);
        cx.spawn_in(window, async move |view, cx| {
            let Some(file_handle) = dialog.pick_file().await else {
                return;
            };
            let source = file_handle.path().to_path_buf();
            cx.update(|_window, cx| {
                view.update(cx, |view, cx| {
                    let Some(storage) = view.project_storages.get(&project_id).cloned() else {
                        view.status = "未导入：项目根目录未打开。".to_owned();
                        cx.notify();
                        return;
                    };
                    match view.workspace.import_project_document(
                        &project_id,
                        &storage,
                        &source,
                        category,
                    ) {
                        Ok(imported) => {
                            let document = &imported.document;
                            if !imported.created {
                                view.status = format!(
                                    "`{}` 已导入过（{}，类别 {}），未新增记录。",
                                    document.original_file_name,
                                    document.id,
                                    document.category.label(),
                                );
                            } else {
                                let searchable = match document.document_kind {
                                    DocumentKind::Pdf => "，正在后台提取全文以供证据检索",
                                    kind if is_evidence_indexable(&kind) => "，文本可证据检索",
                                    _ => "，暂不参与文本检索",
                                };
                                if let Err(error) = view.refresh_project_data(&project_id) {
                                    view.status = format!("文档已导入，但列表未刷新：{error}");
                                } else {
                                    view.status = format!(
                                        "已导入 `{}`（{}，{}…）{searchable}。",
                                        document.original_file_name,
                                        document.id,
                                        &document.content_hash[..23],
                                    );
                                }
                                view.schedule_pdf_indexing(&project_id, cx);
                                view.refresh_evidence_search(cx);
                            }
                        }
                        Err(error) => view.status = format!("未导入文档：{error}"),
                    }
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn open_session_replay(
        &mut self,
        project_id: ProjectId,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(storage) = self.project_storages.get(&project_id) else {
            self.status = "未打开会话：项目根目录未打开。".to_owned();
            cx.notify();
            return;
        };
        match storage.load_session(&session_id) {
            Ok(replay) => {
                self.session_replay = Some(SessionReplaySelection { project_id, replay });
            }
            Err(error) => self.status = format!("未打开会话：{error}"),
        }
        cx.notify();
    }

    pub(super) fn close_session_replay(&mut self, cx: &mut Context<Self>) {
        self.session_replay = None;
        cx.notify();
    }

    pub(super) fn open_existing_project(&mut self, window: &Window, cx: &mut Context<Self>) {
        let dialog =
            rfd::AsyncFileDialog::new().set_title("打开已有 CircuitFabric 项目").set_parent(window);
        cx.spawn_in(window, async move |view, cx| {
            let Some(file_handle) = dialog.pick_folder().await else {
                return;
            };
            let root = file_handle.path().to_path_buf();
            cx.update(|_window, cx| {
                view.update(cx, |view, cx| {
                    let storage = match ProjectStorage::open(&root) {
                        Ok(storage) => storage,
                        Err(error) => {
                            view.status = format!("未打开项目：{error}");
                            cx.notify();
                            return;
                        }
                    };
                    let diagnostics = storage.diagnose_layout();
                    if !diagnostics.is_healthy() {
                        view.status = format!(
                            "项目未注册：目录布局不完整或不安全（缺失：{}；不安全：{}）。",
                            diagnostics
                                .missing_entries
                                .iter()
                                .map(|entry| entry.display().to_string())
                                .collect::<Vec<_>>()
                                .join("、"),
                            diagnostics
                                .unsafe_entries
                                .iter()
                                .map(|entry| entry.display().to_string())
                                .collect::<Vec<_>>()
                                .join("、"),
                        );
                        cx.notify();
                        return;
                    }

                    let project = storage.manifest().project.clone();
                    let opened_root = storage.root().display().to_string();
                    if let Some(registered_root) = view.project_registry.root_for(&project.id) {
                        if registered_root != storage.root() {
                            view.status = format!(
                                "未打开项目：项目 ID `{}` 已绑定到 {}。",
                                project.id,
                                registered_root.display()
                            );
                            cx.notify();
                            return;
                        }
                    } else if let Err(error) = view.project_registry.register(&storage) {
                        view.status = format!("未注册已有项目：{error}");
                        cx.notify();
                        return;
                    }
                    if !view.project_storages.contains_key(&project.id) {
                        if let Err(error) = project_data::attach_project_storage(
                            &mut view.workspace,
                            &mut view.project_storages,
                            &mut view.project_data,
                            storage,
                        ) {
                            view.status = format!("未打开项目：{error}");
                            cx.notify();
                            return;
                        }
                        view.project_read_errors.remove(&project.id);
                    }
                    if let Err(error) = view.project_registry.mark_opened(&project.id) {
                        view.status = format!("项目已打开，但未能记录最近活动：{error}");
                    } else if let Err(error) = view.persist_project_registry() {
                        view.status = format!("项目已打开，但未能保存项目注册表：{error}");
                    } else {
                        view.status = format!("已打开项目 `{}`：{opened_root}。", project.id);
                    }
                    view.schedule_pdf_indexing(&project.id, cx);
                    view.navigation.selected_project = Some(project.id);
                    view.project_tab = ProjectDetailTab::Overview;
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn create_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.new_project_id.read(cx).value().trim().to_owned();
        let name = self.new_project_name.read(cx).value().trim().to_owned();
        let description = self.new_project_description.read(cx).value().trim().to_owned();
        let root = self.new_project_root.read(cx).value().trim().to_owned();
        if root.is_empty() {
            "未创建项目：请先选择项目根文件夹。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if self.workspace.project(&id).is_some() {
            self.status = format!("未创建项目：项目 ID `{id}` 已被使用。");
            cx.notify();
            return;
        }
        let project = Project {
            id: id.clone(),
            name,
            description: (!description.is_empty()).then_some(description),
        };

        match ProjectStorage::create(&root, project.clone()) {
            Ok(storage) => {
                let created_root = storage.root().display().to_string();
                match self.project_registry.register(&storage) {
                    Ok(()) => {
                        let attached = project_data::attach_project_storage(
                            &mut self.workspace,
                            &mut self.project_storages,
                            &mut self.project_data,
                            storage,
                        );
                        match attached {
                            Ok(()) => {
                                let persistence_error = self.persist_project_registry().err();
                                self.navigation.selected_project = Some(id.clone());
                                self.project_tab = ProjectDetailTab::Overview;
                                self.project_form_open = false;
                                self.new_project_id
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                self.new_project_name
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                self.new_project_description
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                self.new_project_root
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                self.status = match persistence_error {
                                    Some(error) => format!(
                                        "项目文件已创建于 {created_root}，但项目注册表未保存：{error}。"
                                    ),
                                    None => format!("已创建项目 `{id}`：{created_root}。"),
                                };
                            }
                            Err(error) => {
                                self.status = format!(
                                    "项目文件已创建于 {created_root}，但未能加入当前工作区：{error}。"
                                );
                            }
                        }
                    }
                    Err(error) => {
                        self.status = format!(
                            "项目文件已创建于 {created_root}，但未能注册：{error}。文件未被删除。"
                        );
                    }
                }
            }
            Err(error) => self.status = format!("未创建项目：{error}"),
        }
        cx.notify();
    }

    pub(super) fn project_id_feedback(&self, cx: &Context<Self>) -> (&'static str, u32) {
        let id = self.new_project_id.read(cx).value();
        if id.trim().is_empty() {
            ("请输入唯一项目 ID。", TEXT_MUTED)
        } else if self.workspace.project(id.trim()).is_some() {
            ("此项目 ID 已被使用。", 0x00dc_2626)
        } else {
            ("此项目 ID 可用。", 0x0016_a34a)
        }
    }

    pub(super) fn project_matches(&self, project: &Project, query: &str) -> bool {
        let query_matches = query.is_empty()
            || project.id.to_lowercase().contains(query)
            || project.name.to_lowercase().contains(query)
            || project
                .description
                .as_deref()
                .is_some_and(|description| description.to_lowercase().contains(query));
        let filter_matches = match self.project_filter {
            ProjectFilter::All => true,
            ProjectFilter::NeedsConfiguration => {
                self.workspace.configuration(&project.id).is_none()
            }
        };
        query_matches && filter_matches
    }
    pub(super) fn render_project_form(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let (id_feedback, feedback_color) = self.project_id_feedback(cx);
        let field = |label: &'static str, id: &'static str, state: Entity<InputState>| {
            div()
                .v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                .child(
                    div()
                        .h(px(36.))
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            InputBase::new(id).flex_1().h_full().flex().items_center().child(state),
                        ),
                )
                .into_any_element()
        };
        let creator = entity.clone();
        let closer = entity.clone();
        let root_chooser = entity.clone();
        let selected_root = self.new_project_root.read(cx).value().trim().to_owned();
        let creation_preview = if selected_root.is_empty() {
            language
                .choose(
                    "请选择一个已有文件夹；在确认创建前，不会写入任何文件。",
                    "Choose an existing folder; no files are written until confirmation.",
                )
                .to_owned()
        } else {
            format!(
                "{}\n  .circuitfabric/、sessions/、documents/、logic/、schematics/",
                language.choose("将在以下根目录创建：", "Will create under:"),
            ) + &format!("\n  {selected_root}")
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
                    .id("project-form-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x000f_172a_b3))
                    .occlude()
                    .on_click(move |_, _, cx| {
                        closer.update(cx, |view, cx| {
                            view.project_form_open = false;
                            cx.notify();
                        });
                    }),
            )
            .child(
                div()
                    .relative()
                    .occlude()
                    .w(px(560.))
                    .v_flex()
                    .gap_4()
                    .p_5()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(SURFACE_BG))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(language.choose("新建项目", "New project")),
                            )
                            .child(
                                action_button("close-project-form")
                                    .ghost()
                                    .label(language.choose("取消", "Cancel"))
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, |view, cx| {
                                            view.project_form_open = false;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(field("Project ID", "new-project-id", self.new_project_id.clone()))
                    .child(div().text_xs().text_color(rgb(feedback_color)).child(id_feedback))
                    .child(field("Name", "new-project-name", self.new_project_name.clone()))
                    .child(field(
                        "Description",
                        "new-project-description",
                        self.new_project_description.clone(),
                    ))
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(language.choose("项目根文件夹", "Project root folder")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h(px(36.))
                                            .px_2()
                                            .flex_1()
                                            .flex()
                                            .items_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(BORDER))
                                            .bg(rgb(CARD_BG))
                                            .child(
                                                InputBase::new("new-project-root")
                                                    .flex_1()
                                                    .h_full()
                                                    .flex()
                                                    .items_center()
                                                    .child(self.new_project_root.clone()),
                                            ),
                                    )
                                    .child(
                                        action_button("choose-project-root")
                                            .label(language.choose("选择文件夹", "Choose folder"))
                                            .on_click(move |_, window, cx| {
                                                root_chooser.update(cx, |view, cx| {
                                                    view.choose_project_root(window, cx);
                                                });
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .whitespace_normal()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(creation_preview),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose(
                                    "项目级设置不会修改全局运行时设置。",
                                    "Project settings never modify global runtime settings.",
                                ),
                            ))
                            .child(
                                action_button("create-project")
                                    .primary()
                                    .label(language.choose("确认并创建", "Confirm and create"))
                                    .on_click(move |_, window, cx| {
                                        creator.update(cx, |view, cx| {
                                            view.create_project(window, cx);
                                        });
                                    }),
                            ),
                    ),
            )
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_projects_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let project_form =
            self.project_form_open.then(|| self.render_project_form(cx).into_any_element());
        let query = self.project_search.read(cx).value().trim().to_lowercase();
        let projects = self
            .workspace
            .projects()
            .into_iter()
            .filter(|project| self.project_matches(project, &query))
            .cloned()
            .collect::<Vec<_>>();
        let selected_project =
            self.navigation.selected_project().and_then(|id| self.workspace.project(id)).cloned();
        let mut cards = div().v_flex().gap_2();
        if projects.is_empty() {
            cards = cards.child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_5()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("没有匹配的项目", "No matching projects")),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                        "清除搜索或筛选，或创建第一个项目。",
                        "Clear the search or filter, or create the first project.",
                    ))),
            );
        }
        for project in projects {
            let project_id = project.id.clone();
            let is_selected = self.navigation.selected_project() == Some(project.id.as_str());
            let selector = entity.clone();
            let description = project.description.unwrap_or_else(|| {
                language.choose("尚未添加项目描述", "No project description yet").to_owned()
            });
            let root = self.project_registry.root_for(&project.id).map_or_else(
                || language.choose("根目录未注册", "Root not registered").to_owned(),
                |path| path.display().to_string(),
            );
            let (document_count, session_count, last_activity) =
                self.project_data.get(&project.id).map_or((0, 0, None), |data| {
                    let last_activity = data
                        .session_listing
                        .sessions
                        .first()
                        .map(|summary| rfc3339(summary.metadata.started_at_unix_seconds));
                    (data.documents.len(), data.session_listing.sessions.len(), last_activity)
                });
            let document_label = language.choose_owned(
                format!("{document_count} 份文档"),
                format!("{document_count} documents"),
            );
            let session_label = language.choose_owned(
                format!("{session_count} 个会话"),
                format!("{session_count} sessions"),
            );
            let activity_label = last_activity
                .unwrap_or_else(|| language.choose("尚无活动", "No activity yet").to_owned());
            cards = cards.child(
                div()
                    .id(format!("project-card-{}", project.id))
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if is_selected { ACCENT } else { BORDER }))
                    .bg(rgb(if is_selected { 0x00f0_f9ff } else { CARD_BG }))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                    .on_click(move |_, _, cx| {
                        selector.update(cx, |view, cx| {
                            view.select_project(project_id.clone(), cx);
                        });
                    })
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
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(project.name),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .truncate()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(SURFACE_BG))
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(project.id),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(description))
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(root))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(document_label)
                            .child(session_label)
                            .child(activity_label),
                    ),
            );
        }

        let detail = if let Some(project) = selected_project {
            self.render_project_detail(project, cx).into_any_element()
        } else {
            let opener = entity.clone();
            div()
                .flex_1()
                .v_flex()
                .items_center()
                .justify_center()
                .gap_3()
                .p_8()
                .bg(rgb(CARD_BG))
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("选择一个项目", "Select a project")),
                )
                .child(
                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                        "项目详情、文档、会话和项目级配置会显示在这里。",
                        "Project details, documents, sessions, and project-scoped configuration appear here.",
                    )),
                )
                .child(
                    action_button("open-project-form-empty")
                        .primary()
                        .label(language.choose("新建项目", "New project"))
                        .on_click(move |_, _, cx| {
                            opener.update(cx, ControlPlaneView::open_project_form);
                        }),
                )
                .into_any_element()
        };

        let open_form = entity.clone();
        let open_existing = entity.clone();
        let all_filter = entity.clone();
        let setup_filter = entity.clone();
        workspace_page("projects-page").relative()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language.choose("项目工作区", "Project workspaces")),
                            )
                            .child(
                                div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                    language.choose(
                                        "将项目资料、会话和授权配置限定在同一个设计工作区。",
                                        "Keep design evidence, sessions, and authorized configuration in one workspace.",
                                    ),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                action_button("open-existing-project")
                                    .label(language.choose("打开已有项目", "Open existing"))
                                    .on_click(move |_, window, cx| {
                                        open_existing.update(cx, |view, cx| {
                                            view.open_existing_project(window, cx);
                                        });
                                    }),
                            )
                            .child(
                                action_button("open-project-form")
                                    .primary()
                                    .label(language.choose("新建项目", "New project"))
                                    .on_click(move |_, _, cx| {
                                        open_form.update(cx, ControlPlaneView::open_project_form);
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .id("projects-list-scroll")
                            .min_h(px(0.))
                            .scroll_y()
                            .w(px(330.))
                            .flex_none()
                            .v_flex()
                            .gap_3()
                            .child(
                                div()
                                    .id("project-search")
                                    .w_full()
                                    .child(Input::new(&self.project_search)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        action_button("project-filter-all")
                                            .label(language.choose("全部", "All"))
                                            .when(self.project_filter == ProjectFilter::All, |button| {
                                                button.primary()
                                            })
                                            .on_click(move |_, _, cx| {
                                                all_filter.update(cx, |view, cx| {
                                                    view.project_filter = ProjectFilter::All;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(
                                        action_button("project-filter-needs-configuration")
                                            .label(language.choose("待配置", "Needs setup"))
                                            .when(
                                                self.project_filter == ProjectFilter::NeedsConfiguration,
                                                |button| button.primary(),
                                            )
                                            .on_click(move |_, _, cx| {
                                                setup_filter.update(cx, |view, cx| {
                                                    view.project_filter = ProjectFilter::NeedsConfiguration;
                                                    cx.notify();
                                                });
                                            }),
                                    ),
                            )
                            .child(cards),
                    )
                    .child(detail),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
            .when_some(project_form, ParentElement::child)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_project_detail(
        &mut self,
        project: Project,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let selected_tab = self.project_tab;
        let configuration = self.workspace.configuration(&project.id).cloned();
        let project_root = self.project_registry.root_for(&project.id).map_or_else(
            || language.choose("根目录未注册", "Root not registered").to_owned(),
            |path| path.display().to_string(),
        );
        let (document_count, session_count) = self
            .project_data
            .get(&project.id)
            .map_or((0, 0), |data| (data.documents.len(), data.session_listing.sessions.len()));
        let session_tokens = self
            .project_data
            .get(&project.id)
            .map(|data| {
                data.session_listing.sessions.iter().fold(0_u64, |total, summary| {
                    total
                        + summary.metadata.usage.input_tokens
                        + summary.metadata.usage.output_tokens
                })
            })
            .unwrap_or(0);
        let mut tabs = div().flex().gap_1().flex_wrap();
        for tab in ProjectDetailTab::ALL {
            let chooser = entity.clone();
            let active = tab == selected_tab;
            tabs = tabs.child(
                action_button(format!("project-tab-{}", tab.label(UiLanguage::English)))
                    .label(tab.label(language))
                    .when(active, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            view.project_tab = tab;
                            cx.notify();
                        });
                    }),
            );
        }

        let content = match selected_tab {
            ProjectDetailTab::Overview => div()
                .v_flex()
                .gap_3()
                .child(
                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                        project.description.clone().unwrap_or_else(|| {
                            language
                                .choose("尚未添加描述。", "No description has been added.")
                                .to_owned()
                        }),
                    ),
                )
                .child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(language.choose("项目根目录", "Project root")),
                        )
                        .child(div().text_sm().text_color(rgb(TEXT_PRIMARY)).child(project_root)),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_3()
                        .child(Self::project_metric(
                            document_count.to_string(),
                            language.choose("授权文档", "Authorized documents"),
                        ))
                        .child(Self::project_metric(
                            session_count.to_string(),
                            language.choose("会话记录", "Session records"),
                        ))
                        .child(Self::project_metric(
                            session_tokens.to_string(),
                            language.choose("累计 tokens", "Total tokens"),
                        )),
                )
                .into_any_element(),
            ProjectDetailTab::Documents => {
                self.render_documents_tab(&project, cx).into_any_element()
            }
            ProjectDetailTab::Sessions => {
                self.render_sessions_tab(&project, cx).into_any_element()
            }
            ProjectDetailTab::AgentConfiguration => {
                let scope_summary = if let Some(configuration) = configuration {
                    format!(
                        "{} skills · {} MCP servers",
                        configuration.enabled_skill_ids.len(),
                        configuration.enabled_mcp_server_ids.len()
                    )
                } else {
                    language
                        .choose("尚无项目级覆盖项", "No project-scoped overrides")
                        .to_owned()
                };
                div()
                    .v_flex()
                    .gap_4()
                    .child(self.render_project_agents_section(&project, cx))
                    .child(
                        div().v_flex().gap_3().child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("项目级配置", "Project-scoped configuration")),
                        )
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(scope_summary))
                        .child(
                            div()
                                .p_3()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .bg(rgb(SURFACE_BG))
                                .text_sm()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(language.choose(
                                    "技能许可、MCP 许可和项目说明会保存在此项目作用域内。Codex 命令、Provider 和 bridge 地址是全局运行时设置，只能在“智能体与工具”中修改，不会被此项目覆盖。",
                                    "Skill permissions, MCP permissions, and project instructions belong to this project. The Codex command, providers, and bridge address are global runtime settings; they can only be changed in Agents & tools and are never overridden here.",
                                )),
                        ),
                    )
                    .into_any_element()
            }
            ProjectDetailTab::Usage => project_empty_state(
                language.choose("尚无用量记录", "No usage recorded"),
                language.choose(
                    "用量会按项目、Provider 和运行时聚合，且不会混入其他项目。",
                    "Usage will be grouped by project, provider, and runtime without mixing other projects.",
                ),
            )
            .into_any_element(),
        };

        detail_pane("project-detail")
            .gap_4()
            .p_5()
            .rounded_xl()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(project.name),
                            )
                            .child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(project.id)),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(0x00dc_fce7))
                            .text_xs()
                            .text_color(rgb(0x0016_a34a))
                            .child(language.choose("项目作用域", "Project scope")),
                    ),
            )
            .child(tabs)
            .child(div().p_4().rounded_lg().bg(rgb(SURFACE_BG)).child(content))
    }

    pub(super) fn project_metric(value: String, label: &'static str) -> impl IntoElement {
        div()
            .flex_1()
            .min_w(px(0.))
            .v_flex()
            .gap_1()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value))
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_documents_tab(
        &mut self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let (documents, document_integrity) = self
            .project_data
            .get(&project.id)
            .map(|data| (data.documents.clone(), data.document_integrity.clone()))
            .unwrap_or_default();
        let evidence_ready = documents
            .iter()
            .filter(|document| {
                self.workspace.is_document_evidence_available(&project.id, &document.id)
            })
            .count();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.choose("授权文档", "Authorized documents")),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child({
                        let importer = entity.clone();
                        action_button("import-datasheet")
                            .label(language.choose("导入 Datasheet", "Import datasheet"))
                            .on_click(move |_, window, cx| {
                                importer.update(cx, |view, cx| {
                                    view.import_project_document(
                                        DocumentCategory::Datasheet,
                                        window,
                                        cx,
                                    );
                                });
                            })
                    })
                    .child({
                        let importer = entity.clone();
                        action_button("import-reference-design")
                            .primary()
                            .label(language.choose("导入参考设计", "Import reference design"))
                            .on_click(move |_, window, cx| {
                                importer.update(cx, |view, cx| {
                                    view.import_project_document(
                                        DocumentCategory::ReferenceDesign,
                                        window,
                                        cx,
                                    );
                                });
                            })
                    })
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(0x00e0_f2fe))
                            .text_xs()
                            .text_color(rgb(0x000e_7490))
                            .child(language.choose_owned(
                                format!("{evidence_ready}/{} 可作证据", documents.len()),
                                format!("{evidence_ready}/{} evidence-ready", documents.len()),
                            )),
                    ),
            );

        if documents.is_empty() {
            return div()
                .v_flex()
                .gap_4()
                .size_full()
                .child(header)
                .child(
                    project_empty_state(
                        language.choose("还没有授权文档", "No authorized documents yet"),
                        language.choose(
                            "导入第一份 datasheet 或参考设计后，会记录内容哈希与来源，并可被证据检索引用。",
                            "Import the first datasheet or reference design; its content hash and source are recorded and citable.",
                        ),
                    ),
                )
                .into_any_element();
        }

        let mut list = div().v_flex().gap_2();
        list = list.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
            "点击文档卡片，窗口向右展开并在右侧显示只读预览。",
            "Click a document card: the window extends right and shows a read-only preview.",
        )));
        for document in &documents {
            let integrity_verified = document_integrity.get(&document.id).copied().unwrap_or(false);
            let evidence_available = integrity_verified
                && self.workspace.is_document_evidence_available(&project.id, &document.id);
            let index_state =
                self.pdf_index_state.get(&(project.id.clone(), document.id.clone())).copied();
            let evidence_label = if evidence_available {
                if self.workspace.has_verified_datasheet_evidence(&project.id, &document.id) {
                    language
                        .choose("可作证据 · 含已校验数据", "Evidence ready — incl. verified data")
                } else {
                    language.choose("已索引，可作证据", "Indexed — evidence ready")
                }
            } else if index_state == Some(PdfIndexState::Running) {
                language.choose("正在提取全文…", "Extracting full text…")
            } else if index_state == Some(PdfIndexState::Failed) {
                language.choose("全文提取失败，不可作证据", "Text extraction failed — not evidence")
            } else if is_evidence_indexable(&document.document_kind) {
                language.choose("待索引，不可作证据", "Pending index — not evidence")
            } else {
                language.choose("等待提取器，不可作证据", "Awaiting extractor — not evidence")
            };
            let previewed = self.document_preview.as_ref().is_some_and(|preview| {
                preview.project_id == project.id && preview.document_id == document.id
            });
            let category_style = match document.category {
                DocumentCategory::Datasheet => (0x00e0_f2fe, 0x000e_7490),
                DocumentCategory::ReferenceDesign => (0x00f3_e8ff, 0x0076_2b_a3),
            };
            let opener = entity.clone();
            let click_project_id = project.id.clone();
            let click_document_id = document.id.clone();
            list = list.child(
                div()
                    .id(format!("document-card-{}", document.id))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if previewed { ACCENT } else { BORDER }))
                    .bg(rgb(CARD_BG))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                    .on_click(move |_, window, cx| {
                        opener.update(cx, |view, cx| {
                            // Re-read the record at click time so the preview reflects
                            // the current index, not this render's snapshot.
                            let document =
                                view.project_data.get(&click_project_id).and_then(|data| {
                                    data.documents
                                        .iter()
                                        .find(|document| document.id == click_document_id)
                                        .cloned()
                                });
                            if let Some(document) = document {
                                view.open_document_preview(
                                    click_project_id.clone(),
                                    &document,
                                    window,
                                    cx,
                                );
                            }
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
                                    .whitespace_normal()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(document.original_file_name.clone()),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(category_style.0))
                                    .text_color(rgb(category_style.1))
                                    .child(document.category.label()),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(if integrity_verified {
                                        0x00dc_fce7
                                    } else {
                                        0x00fe_f2f2
                                    }))
                                    .text_color(rgb(if integrity_verified {
                                        0x0016_a34a
                                    } else {
                                        0x00b9_1c1c
                                    }))
                                    .child(if integrity_verified {
                                        language.choose("完整性已验证", "Integrity verified")
                                    } else {
                                        language.choose("完整性无效", "Integrity invalid")
                                    }),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(if evidence_available {
                                        0x00e0_f2fe
                                    } else {
                                        SURFACE_BG
                                    }))
                                    .text_color(rgb(if evidence_available {
                                        0x000e_7490
                                    } else {
                                        TEXT_MUTED
                                    }))
                                    .child(evidence_label),
                            )
                            .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                format!(
                                    "{} · {} bytes",
                                    &document.content_hash[..19],
                                    document.byte_size
                                ),
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("{} · {}", document.id, document.source_locator)),
                    ),
            );
        }

        let search_panel = self.render_evidence_search_panel(project, &documents, cx);

        div()
            .v_flex()
            .gap_4()
            .w_full()
            .child(header)
            .child(search_panel)
            .child(list)
            .into_any_element()
    }
}
