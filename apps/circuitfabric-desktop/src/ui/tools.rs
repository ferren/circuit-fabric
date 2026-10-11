//! Tool catalog presentation and event handlers.
use super::*;
#[derive(Clone)]
pub(super) enum CatalogEditor {
    Skill(Option<String>),
    Mcp(Option<String>),
    Delete(ToolAuthorizationKind, String),
}

pub(super) struct ToolReport {
    pub(super) text: String,
    pub(super) tools: Option<serde_json::Value>,
    pub(super) cancel: Option<circuitfabric_codex_runtime::execution::Cancellation>,
}

impl ControlPlaneView {
    // Resource fields must not inherit the generic settings field's flex growth
    // or 240px minimum width: this pane also runs inside small windows.
    pub(super) fn resource_field(
        label: &'static str,
        id: &'static str,
        _hint: Option<&'static str>,
        state: &Entity<InputState>,
    ) -> Div {
        div()
            .v_flex()
            .gap_1()
            .flex_none()
            .min_w(px(0.))
            .w_full()
            .child(div().text_sm().child(label))
            .child(
                div()
                    .h(px(36.))
                    .min_w(px(0.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .child(
                        InputBase::new(id)
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .flex()
                            .items_center()
                            .child(state.clone()),
                    ),
            )
    }
    pub(super) fn invalidate_tool_runs(&mut self) {
        for cancel in [&self.task_cancel, &self.datasheet_cancel].into_iter().flatten() {
            cancel.cancel();
        }
        for report in self.tool_reports.values() {
            if let Some(cancel) = &report.cancel {
                cancel.cancel();
            }
        }
        self.tool_reports.clear();
        self.tool_revision += 1;
    }

    pub(super) fn selected_resource(&self) -> Option<String> {
        match self.new_tool_kind {
            ToolAuthorizationKind::Skill => self.selected_skill.clone(),
            ToolAuthorizationKind::McpServer => self.selected_mcp.clone(),
        }
    }

    pub(super) fn resource_granted(
        &self,
        scope: ToolScope,
        kind: ToolAuthorizationKind,
        id: &str,
    ) -> bool {
        match scope {
            ToolScope::Global => {
                self.tool_authorizations.ids_for_kind(kind).iter().any(|s| s == id)
            }
            ToolScope::Project => self
                .navigation
                .selected_project
                .as_ref()
                .and_then(|p| self.workspace.configuration(p))
                .is_some_and(|config| {
                    match kind {
                        ToolAuthorizationKind::Skill => &config.enabled_skill_ids,
                        ToolAuthorizationKind::McpServer => &config.enabled_mcp_server_ids,
                    }
                    .iter()
                    .any(|s| s == id)
                }),
        }
    }

    pub(super) fn resource_scope_summary(
        &self,
        kind: ToolAuthorizationKind,
        id: &str,
        enabled: bool,
    ) -> String {
        let global = self.resource_granted(ToolScope::Global, kind, id);
        let project = self.resource_granted(ToolScope::Project, kind, id);
        let effective =
            enabled && global && (self.navigation.selected_project.is_none() || project);
        let language = self.language;
        let (applied, not_applied) =
            (language.choose("已应用", "granted"), language.choose("未应用", "not granted"));
        let project_state = if self.navigation.selected_project.is_none() {
            language.choose("未选择", "no project")
        } else if project {
            applied
        } else {
            not_applied
        };
        format!(
            "{}{} · {}{} · {}{}",
            language.choose("全局：", "Global: "),
            if global { applied } else { not_applied },
            language.choose("当前项目：", "Project: "),
            project_state,
            language.choose("最终权限：", "Effective: "),
            if effective {
                language.choose("允许", "allowed")
            } else {
                language.choose("禁止", "denied")
            }
        )
    }

    pub(super) fn open_catalog_editor(
        &mut self,
        editor: CatalogEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, name, source, args, env) = match &editor {
            CatalogEditor::Mcp(Some(id)) => {
                let Some(server) = self.catalog.mcp_servers.iter().find(|s| &s.id == id) else {
                    return;
                };
                (
                    server.id.clone(),
                    server.display_name.clone(),
                    server.command.clone(),
                    serde_json::to_string(&server.args).unwrap_or_else(|_| "[]".into()),
                    server.environment_variables.join(","),
                )
            }
            CatalogEditor::Skill(Some(id)) => {
                let Some(skill) = self.catalog.skills.iter().find(|s| &s.id == id) else {
                    return;
                };
                (
                    id.clone(),
                    String::new(),
                    skill.path.display().to_string(),
                    "[]".into(),
                    String::new(),
                )
            }
            _ => (String::new(), String::new(), String::new(), "[]".into(), String::new()),
        };
        for (input, value) in [
            (&self.catalog_id, id),
            (&self.catalog_name, name),
            (&self.catalog_source, source),
            (&self.catalog_args, args),
            (&self.catalog_env, env),
        ] {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.catalog_editor = Some(editor);
        self.catalog_error = None;
        cx.notify();
    }

    pub(super) fn pick_skill_source(
        &mut self,
        folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language;
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(language.choose("选择技能目录或 SKILL.md", "Choose skill folder or SKILL.md"))
            .set_parent(window);
        let filter_name = language.choose("技能说明", "Skill description");
        cx.spawn_in(window, async move |view, cx| {
            let selected = if folder {
                dialog.pick_folder().await
            } else {
                dialog.add_filter(filter_name, &["md"]).pick_file().await
            };
            let Some(file) = selected else {
                return;
            };
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.catalog_source.update(cx, |state, cx| {
                        state.set_value(file.path().display().to_string(), window, cx)
                    });
                    view.catalog_error = None;
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn save_catalog_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.catalog_editor.clone() else {
            return;
        };
        self.catalog_error = None;
        self.tool_feedback = self.language.choose("正在校验并保存…", "Validating and saving…").into();
        let result = (|| -> Result<(), String> {
            if let CatalogEditor::Delete(kind, id) = &editor {
                // Prune every opened/registered project's grants. A failing project
                // write keeps the definition and dialog, and reports partial progress.
                let storages = self.project_storages.clone();
                for (project_id, storage) in storages {
                    let mut config =
                        self.workspace.configuration(&project_id).cloned().unwrap_or_default();
                    let ids = match kind {
                        ToolAuthorizationKind::Skill => &mut config.enabled_skill_ids,
                        ToolAuthorizationKind::McpServer => &mut config.enabled_mcp_server_ids,
                    };
                    if !ids.contains(id) {
                        continue;
                    }
                    ids.retain(|s| s != id);
                    storage.save_configuration(&config).map_err(|e| {
                        self.language.choose_owned(
                            format!("项目 {project_id} 撤销失败：{e}；已撤销的其他项目保持撤销，定义尚未删除"),
                            format!(
                                "Revocation failed for project {project_id}: {e}; other revocations stand, definition not yet deleted"
                            ),
                        )
                    })?;
                    self.workspace
                        .set_configuration(&project_id, config)
                        .map_err(|e| e.to_string())?;
                    self.invalidate_tool_runs();
                }
                self.save_update(
                    crate::application::settings_persistence::SettingsUpdate::RemoveResource {
                        kind: *kind,
                        id: id.clone(),
                    },
                )
                .map_err(|e| {
                    self.language.choose_owned(
                        format!("定义未删除：{e}；已保存的项目撤销保持生效"),
                        format!("Definition not deleted: {e}; saved project revocations remain in effect"),
                    )
                })?;
                self.catalog = self.saved_settings.catalog.clone();
                self.tool_authorizations = self.saved_settings.tools.clone();
                match kind {
                    ToolAuthorizationKind::Skill => self.selected_skill = None,
                    ToolAuthorizationKind::McpServer => self.selected_mcp = None,
                }
                return Ok(());
            }
            let mut next = self.catalog.clone();
            match editor {
                CatalogEditor::Skill(original) => {
                    let path =
                        PathBuf::from(self.catalog_source.read(cx).value().trim().to_string());
                    let enabled = original
                        .as_ref()
                        .and_then(|id| next.skills.iter().find(|s| &s.id == id))
                        .is_none_or(|s| s.enabled);
                    if let Some(original) = &original {
                        next.skills.retain(|s| &s.id != original);
                    }
                    let id = next.import_skill(&path).map_err(|e| e.to_string())?;
                    if original.as_ref().is_some_and(|old| old != &id) {
                        return Err(self.language.choose(
                            "编辑不能改变技能标识；请单独添加新技能，然后删除旧技能",
                            "Editing cannot change the skill id; add the new skill separately, then delete the old one",
                        ).into());
                    }
                    if let Some(skill) = next.skills.iter_mut().find(|s| s.id == id) {
                        skill.enabled = enabled;
                    }
                    self.save_update(
                        crate::application::settings_persistence::SettingsUpdate::Catalog(next),
                    )
                    .map_err(|e| e.to_string())?;
                    self.selected_skill = Some(id);
                }
                CatalogEditor::Mcp(original) => {
                    let id = self.catalog_id.read(cx).value().trim().to_string();
                    if original.as_ref().is_some_and(|old| old != &id) {
                        return Err(self.language.choose(
                            "编辑不能改变标识；请新增一个 server 后删除旧项",
                            "Editing cannot change the id; add a new server, then delete the old one",
                        ).into());
                    }
                    if original.is_none() && next.mcp_servers.iter().any(|s| s.id == id) {
                        return Err(self.language.choose(
                            "标识已存在，请选择列表中的条目编辑",
                            "Id already exists; pick the existing entry to edit",
                        ).into());
                    }
                    let args =
                        serde_json::from_str::<Vec<String>>(&self.catalog_args.read(cx).value())
                            .map_err(|_| {
                                self.language
                                    .choose("参数必须是字符串 JSON 数组", "Arguments must be a JSON array of strings")
                                    .to_owned()
                            })?;
                    let enabled =
                        next.mcp_servers.iter().find(|s| s.id == id).is_none_or(|s| s.enabled);
                    next.mcp_servers.retain(|s| s.id != id);
                    next.removed_bundled_servers.retain(|s| s != &id);
                    next.mcp_servers.push(
                        circuitfabric_codex_runtime::tools::McpServerDefinition {
                            id: id.clone(),
                            display_name: self.catalog_name.read(cx).value().trim().to_string(),
                            command: self.catalog_source.read(cx).value().trim().to_string(),
                            args,
                            environment_variables: Self::parse_tool_ids(
                                &self.catalog_env.read(cx).value(),
                            ),
                            enabled,
                        },
                    );
                    self.save_update(
                        crate::application::settings_persistence::SettingsUpdate::Catalog(next),
                    )
                    .map_err(|e| e.to_string())?;
                    self.selected_mcp = Some(id);
                }
                CatalogEditor::Delete(..) => unreachable!(),
            }
            self.catalog = self.saved_settings.catalog.clone();
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.catalog_editor = None;
                self.tool_feedback = self.language.choose(
                    "操作已保存；定义和授权分别管理。旧操作已取消，下次任务使用新配置。",
                    "Saved; definitions and authorizations are managed separately. Previous operations were cancelled; the next task uses the new configuration.",
                ).into();
            }
            Err(error) => {
                self.catalog_error = Some(error.clone());
                self.tool_feedback = self
                    .language
                    .choose_owned(format!("未保存：{error}"), format!("Not saved: {error}"));
            }
        }
        self.status = self.tool_feedback.clone();
        cx.notify();
    }

    pub(super) fn toggle_selected_resource(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_resource() else {
            return;
        };
        let previous = self.catalog.clone();
        match self.new_tool_kind {
            ToolAuthorizationKind::Skill => {
                if let Some(s) = self.catalog.skills.iter_mut().find(|s| s.id == id) {
                    s.enabled = !s.enabled;
                }
            }
            ToolAuthorizationKind::McpServer => {
                if let Some(s) = self.catalog.mcp_servers.iter_mut().find(|s| s.id == id) {
                    s.enabled = !s.enabled;
                }
            }
        }
        self.persist_catalog(previous, cx);
        self.tool_feedback = self.status.clone();
    }

    pub(super) fn apply_selected_resource(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_resource() else {
            return;
        };
        if self.resource_granted(self.new_tool_scope, self.new_tool_kind, &id) {
            self.revoke_tool(self.new_tool_scope, self.new_tool_kind, &id, cx);
        } else {
            self.new_tool_id.update(cx, |state, cx| state.set_value(id, window, cx));
            self.authorize_tool(window, cx);
        }
        self.tool_feedback = self.status.clone();
    }

    pub(super) fn verify_selected_resource(
        &mut self,
        call: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_resource() else {
            return;
        };
        let grants = self.effective_grants();
        if self.new_tool_kind == ToolAuthorizationKind::Skill {
            let mut only = grants;
            only.authorized_skill_ids.retain(|s| s == &id);
            let result = if only.authorized_skill_ids.is_empty() {
                Err(self.language.choose(
                    "技能尚未获得最终生效权限，请检查全局、项目和启用状态",
                    "Skill has no effective permission yet; check global, project, and enabled state",
                ).into())
            } else {
                self.catalog
                    .skill_instructions(&only)
                    .map(|text| {
                        self.language.choose_owned(
                            format!("已实际读取并加载 {} 字节技能指令；下次任务会注入运行时。", text.len()),
                            format!(
                                "Read and loaded {} bytes of skill instructions; injected into the runtime on the next task.",
                                text.len()
                            ),
                        )
                    })
                    .map_err(|e| e.to_string())
            };
            self.tool_feedback = result
                .unwrap_or_else(|e| self.language.choose_owned(format!("加载失败：{e}"), format!("Load failed: {e}")));
            self.tool_reports.insert(
                format!("skill:{id}"),
                ToolReport { text: self.tool_feedback.clone(), tools: None, cancel: None },
            );
            cx.notify();
            return;
        }
        let key = format!("mcp:{id}");
        if self.tool_reports.get(&key).is_some_and(|r| r.cancel.is_some()) {
            return;
        }
        let invocation = if call {
            let name = self.tool_call_name.read(cx).value().trim().to_string();
            if !self
                .tool_reports
                .get(&key)
                .and_then(|r| r.tools.as_ref())
                .and_then(|v| v["tools"].as_array())
                .is_some_and(|tools| tools.iter().any(|t| t["name"] == name))
            {
                self.tool_feedback = self.language.choose(
                    "调用失败：请先发现工具，并选择真实工具名称",
                    "Call failed: discover tools first and pick a real tool name",
                ).into();
                cx.notify();
                return;
            }
            let Ok(arguments) =
                serde_json::from_str::<serde_json::Value>(&self.tool_call_args.read(cx).value())
            else {
                self.tool_feedback = self
                    .language
                    .choose("调用失败：参数必须是 JSON 对象", "Call failed: arguments must be a JSON object")
                    .into();
                cx.notify();
                return;
            };
            if !arguments.is_object() {
                self.tool_feedback = self
                    .language
                    .choose("调用失败：参数必须是 JSON 对象", "Call failed: arguments must be a JSON object")
                    .into();
                cx.notify();
                return;
            }
            Some((name, arguments))
        } else {
            None
        };
        let tools = self.tool_reports.get(&key).and_then(|r| r.tools.clone());
        let cancel = circuitfabric_codex_runtime::execution::Cancellation::default();
        self.tool_feedback = if call {
            self.language.choose("正在调用工具…", "Calling tool…")
        } else {
            self.language.choose("正在连接并发现工具…", "Connecting and discovering tools…")
        }
        .into();
        self.tool_reports.insert(
            key.clone(),
            ToolReport { text: self.tool_feedback.clone(), tools, cancel: Some(cancel.clone()) },
        );
        let revision = self.tool_revision;
        let catalog = self.catalog.clone();
        let secrets = self.vault.as_ref().map(|v| v.values().clone());
        let work = cx.background_spawn(async move {
            catalog
                .request_with_cancellation(
                    &id,
                    &grants,
                    invocation.as_ref().map(|(n, a)| (n.as_str(), a)),
                    secrets.as_ref(),
                    &cancel,
                )
                .map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    if view.tool_revision != revision {
                        return;
                    }
                    let Some(report) = view.tool_reports.get_mut(&key) else {
                        return;
                    };
                    report.cancel = None;
                    report.text = match result {
                        Ok(value) if call => {
                            let payload =
                                serde_json::to_string_pretty(&value).unwrap_or_default();
                            view.language.choose_owned(
                                format!("调用成功：{payload}"),
                                format!("Call succeeded: {payload}"),
                            )
                        }
                        Ok(value) => {
                            let count = value["tools"].as_array().map_or(0, Vec::len);
                            report.tools = Some(value);
                            view.language.choose_owned(
                                format!("连接验证成功，发现 {count} 个真实工具（每次操作重新连接）"),
                                format!(
                                    "Connection verified; {count} real tools discovered (each operation reconnects)"
                                ),
                            )
                        }
                        Err(error) => {
                            report.tools = None;
                            if call {
                                view.language.choose_owned(
                                    format!("调用失败：{error}"),
                                    format!("Call failed: {error}"),
                                )
                            } else {
                                view.language.choose_owned(
                                    format!("连接失败：{error}"),
                                    format!("Connect failed: {error}"),
                                )
                            }
                        }
                    };
                    view.tool_feedback = report.text.clone();
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_tool_management(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let kind = self.new_tool_kind;
        let selected = self.selected_resource();
        let search = self.catalog_search.read(cx).value().to_lowercase();
        let mut resources: Vec<(String, String, String, bool)> = match kind {
            ToolAuthorizationKind::Skill => self
                .catalog
                .skills
                .iter()
                .map(|s| {
                    let name = s.preview().map_or_else(|_| s.id.clone(), |p| p.name);
                    (s.id.clone(), name, s.path.display().to_string(), s.enabled)
                })
                .collect(),
            ToolAuthorizationKind::McpServer => self
                .catalog
                .mcp_servers
                .iter()
                .map(|s| {
                    (
                        s.id.clone(),
                        if s.display_name.is_empty() {
                            s.id.clone()
                        } else {
                            s.display_name.clone()
                        },
                        s.command.clone(),
                        s.enabled,
                    )
                })
                .collect(),
        };
        let total = resources.len();
        resources.retain(|(id, name, source, _)| {
            format!("{id} {name} {source}").to_lowercase().contains(&search)
        });
        let mut rows = div().v_flex().gap_2();
        for (id, name, source, enabled) in resources {
            let chooser = entity.clone();
            let choose_id = id.clone();
            rows = rows.child(
                div()
                    .id(format!("resource-{id}"))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if selected.as_ref() == Some(&id) { ACCENT } else { BORDER }))
                    .bg(rgb(CARD_BG))
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            match kind {
                                ToolAuthorizationKind::Skill => {
                                    view.selected_skill = Some(choose_id.clone())
                                }
                                ToolAuthorizationKind::McpServer => {
                                    view.selected_mcp = Some(choose_id.clone())
                                }
                            }
                            view.tool_feedback = view
                                .language
                                .choose(
                                    "已选择；列表及详情显示已保存内容。",
                                    "Selected; list and detail show saved state.",
                                )
                                .into();
                            cx.notify();
                        });
                    })
                    .child(div().text_sm().truncate().child(format!("{name} · {id}")))
                    .child(div().text_xs().truncate().text_color(rgb(TEXT_MUTED)).child(source))
                    .child(div().text_xs().child(if enabled {
                        language.choose("已保存 · 已启用", "Saved · enabled")
                    } else {
                        language.choose("已保存 · 已停用", "Saved · disabled")
                    }))
                    .child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .child(self.resource_scope_summary(kind, &id, enabled)),
                    ),
            );
        }
        if total == 0 {
            rows = rows
                .child(language.choose("列表为空，点击添加导入第一个资源。", "List is empty — click Add to import the first resource."));
        }
        let add = entity.clone();
        let mut list = div()
            .w(px(235.))
            .min_w(px(160.))
            .h_full()
            .min_h(px(0.))
            .v_flex()
            .gap_2()
            .child(
                div().flex_none().child(
                    action_button("resource-add")
                        .primary()
                        .label(match kind {
                            ToolAuthorizationKind::Skill => {
                                language.choose("添加技能", "Add skill")
                            }
                            ToolAuthorizationKind::McpServer => "添加 MCP server",
                        })
                        .on_click(move |_, window, cx| {
                            add.update(cx, |view, cx| {
                                view.open_catalog_editor(
                                    match kind {
                                        ToolAuthorizationKind::Skill => CatalogEditor::Skill(None),
                                        ToolAuthorizationKind::McpServer => {
                                            CatalogEditor::Mcp(None)
                                        }
                                    },
                                    window,
                                    cx,
                                );
                            });
                        }),
                ),
            )
            .child(Self::resource_field(
                language.choose("搜索", "Search"),
                "resource-search",
                None,
                &self.catalog_search,
            ))
            .child(div().text_xs().child(language.choose_owned(
                format!("共 {total} 项；无匹配时可清空搜索"),
                format!("{total} total; clear the search when nothing matches"),
            )))
            .child(
                div()
                    .id(if kind == ToolAuthorizationKind::Skill {
                        "skill-list-scroll"
                    } else {
                        "mcp-list-scroll"
                    })
                    .flex_1()
                    .min_h(px(0.))
                    .scroll_y()
                    .child(rows),
            );
        list = list.overflow_hidden();
        let mut detail = div()
            .id(format!("resource-detail-{kind:?}-{selected:?}"))
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .scroll_y()
            .v_flex()
            .p_3()
            .gap_3()
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_lg();
        if let Some(id) = selected {
            let enabled = match kind {
                ToolAuthorizationKind::Skill => {
                    self.catalog.skills.iter().find(|s| s.id == id).is_some_and(|s| s.enabled)
                }
                ToolAuthorizationKind::McpServer => {
                    self.catalog.mcp_servers.iter().find(|s| s.id == id).is_some_and(|s| s.enabled)
                }
            };
            detail = detail.child(div().text_lg().child(id.clone())).child(
                div()
                    .text_sm()
                    .whitespace_normal()
                    .child(self.resource_scope_summary(kind, &id, enabled)),
            );
            let edit = entity.clone();
            let toggle = entity.clone();
            let delete = entity.clone();
            let delete_id = id.clone();
            let edit_id = id.clone();
            let bundled = kind == ToolAuthorizationKind::McpServer && id == BUNDLED_JEV_SERVER_ID;
            detail = detail.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(action_button("resource-edit").label(language.choose("编辑", "Edit")).on_click(
                        move |_, window, cx| {
                            edit.update(cx, |view, cx| {
                                if bundled {
                                    view.agents_selection = AgentsSelection::BundledJev;
                                    view.reset_jev_form_to_saved(window, cx);
                                    view.jev_backend_modal_open = true;
                                    cx.notify();
                                } else {
                                    view.open_catalog_editor(
                                        match kind {
                                            ToolAuthorizationKind::Skill => {
                                                CatalogEditor::Skill(Some(edit_id.clone()))
                                            }
                                            ToolAuthorizationKind::McpServer => {
                                                CatalogEditor::Mcp(Some(edit_id.clone()))
                                            }
                                        },
                                        window,
                                        cx,
                                    );
                                }
                            });
                        },
                    ))
                    .child(
                        action_button("resource-toggle")
                            .label(if enabled {
                                language.choose("停用", "Disable")
                            } else {
                                language.choose("启用", "Enable")
                            })
                            .on_click(move |_, _, cx| {
                                toggle.update(cx, |view, cx| view.toggle_selected_resource(cx));
                            }),
                    )
                    .child(action_button("resource-delete").label(language.choose("删除", "Delete")).on_click(
                        move |_, window, cx| {
                            delete.update(cx, |view, cx| {
                                view.open_catalog_editor(
                                    CatalogEditor::Delete(kind, delete_id.clone()),
                                    window,
                                    cx,
                                )
                            });
                        },
                    )),
            );
            let mut scopes = div().flex().flex_wrap().gap_2();
            for (scope, label) in [
                (ToolScope::Global, language.choose("全局", "Global")),
                (ToolScope::Project, language.choose("当前项目", "Current project")),
            ]
            {
                let chooser = entity.clone();
                scopes = scopes.child(
                    action_button(format!("resource-scope-{label}"))
                        .label(label)
                        .when(self.new_tool_scope == scope, ButtonVariants::primary)
                        .on_click(move |_, _, cx| {
                            chooser.update(cx, |view, cx| {
                                view.new_tool_scope = scope;
                                cx.notify();
                            });
                        }),
                );
            }
            let apply = entity.clone();
            let granted = self.resource_granted(self.new_tool_scope, kind, &id);
            scopes = scopes.child(
                action_button("resource-apply")
                    .primary()
                    .label(if granted {
                        language.choose("撤销所选范围", "Revoke for scope")
                    } else {
                        language.choose("应用到所选范围", "Apply to scope")
                    })
                    .on_click(move |_, window, cx| {
                        apply.update(cx, |view, cx| view.apply_selected_resource(window, cx));
                    }),
            );
            detail = detail.child(scopes);
            let verify = entity.clone();
            detail = detail.child(
                action_button("resource-verify")
                    .label(if kind == ToolAuthorizationKind::Skill {
                        language.choose("验证加载", "Verify loading")
                    } else {
                        language.choose("连接并发现工具", "Connect & discover tools")
                    })
                    .on_click(move |_, window, cx| {
                        verify.update(cx, |view, cx| {
                            view.verify_selected_resource(false, window, cx)
                        });
                    }),
            );
            match kind {
                ToolAuthorizationKind::Skill => {
                    if let Some(skill) = self.catalog.skills.iter().find(|s| s.id == id) {
                        detail = detail.child(settings_summary_row(
                            language.choose("来源", "Source"),
                            skill.path.display().to_string(),
                        ));
                        match skill.preview() {
                            Ok(preview) => {
                                detail = detail
                                    .child(settings_summary_row(
                                        language.choose("名称", "Name"),
                                        preview.name,
                                    ))
                                    .child(settings_summary_row(
                                        language.choose("说明", "Description"),
                                        preview.description,
                                    ))
                                    .child(language.choose(
                                        "内容预览（仅查看，不授予使用权限）",
                                        "Content preview (view only; grants no permission)",
                                    ))
                                    .child(
                                        div()
                                            .w_full()
                                            .overflow_x_hidden()
                                            .whitespace_normal()
                                            .text_sm()
                                            .child(preview.content),
                                    );
                            }
                            Err(error) => {
                                detail = detail.child(language.choose_owned(
                                    format!("配置无效或来源不可读：{error}"),
                                    format!("Invalid configuration or unreadable source: {error}"),
                                ))
                            }
                        }
                        detail = detail.child(div().text_xs().whitespace_normal().child(
                            language.choose(
                                "当前仅将 SKILL.md 指令注入运行时；附属脚本和资源不复制、不自动运行，相对资源没有通用加载保证。",
                                "Only SKILL.md instructions are injected into the runtime; sidecar scripts and assets are neither copied nor auto-run, and relative assets have no generic loading guarantee.",
                            ),
                        ));
                    }
                }
                ToolAuthorizationKind::McpServer => {
                    if let Some(server) = self.catalog.mcp_servers.iter().find(|s| s.id == id) {
                        detail = detail
                            .child(settings_summary_row(
                                language.choose("传输", "Transport"),
                                language
                                    .choose("stdio（当前唯一支持的方式）", "stdio (the only transport currently supported)")
                                    .to_owned(),
                            ))
                            .child(settings_summary_row(
                                language.choose("名称", "Name"),
                                server.display_name.clone(),
                            ))
                            .child(settings_summary_row(
                                language.choose("命令", "Command"),
                                server.command.clone(),
                            ))
                            .child(settings_summary_row(
                                language.choose("参数", "Arguments"),
                                serde_json::to_string(&server.args).unwrap_or_default(),
                            ))
                            .child(settings_summary_row(
                                language.choose("认证变量", "Auth variables"),
                                server.environment_variables.join(", "),
                            ));
                        for name in &server.environment_variables {
                            if let Some(hint) = self.secret_source_hint(name, &entity) {
                                detail = detail.child(hint);
                            }
                        }
                    }
                    if let Some(tools) = self
                        .tool_reports
                        .get(&format!("mcp:{id}"))
                        .and_then(|r| r.tools.as_ref())
                        .and_then(|v| v["tools"].as_array())
                    {
                        for (index, tool) in tools.iter().enumerate() {
                            let choose = entity.clone();
                            let name = tool["name"].as_str().unwrap_or_default().to_owned();
                            let selected_name = name.clone();
                            detail =
                                detail
                                    .child(
                                        action_button(format!("discovered-tool-{index}"))
                                            .label(name)
                                            .on_click(move |_, window, cx| {
                                                choose.update(cx, |view, cx| {
                                                    view.tool_call_name.update(cx, |input, cx| {
                                                        input.set_value(
                                                            selected_name.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(div().whitespace_normal().text_xs().child(
                                        serde_json::to_string_pretty(tool).unwrap_or_default(),
                                    ));
                        }
                        let caller = entity.clone();
                        detail = detail
                            .child(Self::resource_field(
                                language.choose("已发现工具名称", "Discovered tool name"),
                                "tool-call-name",
                                None,
                                &self.tool_call_name,
                            ))
                            .child(Self::resource_field(
                                language.choose("调用参数（JSON 对象）", "Call arguments (JSON object)"),
                                "tool-call-args",
                                None,
                                &self.tool_call_args,
                            ))
                            .child(action_button("resource-call").label(language.choose("调用工具", "Call tool")).on_click(
                                move |_, window, cx| {
                                    caller.update(cx, |view, cx| {
                                        view.verify_selected_resource(true, window, cx)
                                    });
                                },
                            ));
                    }
                }
            }
            let key = format!(
                "{}:{id}",
                if kind == ToolAuthorizationKind::Skill { "skill" } else { "mcp" }
            );
            detail = detail.child(
                div().text_sm().whitespace_normal().child(
                    self.tool_reports.get(&key).map_or_else(
                        || self.language.choose("尚未加载 / 尚未验证连接", "Not loaded / connection not verified").into(),
                        |r| r.text.clone(),
                    ),
                ),
            );
        } else {
            detail = detail.child(self.language.choose(
                "尚未选择资源；在左侧选择一项查看详情和应用授权。",
                "No resource selected; pick one on the left to see details and apply authorizations.",
            ));
        }
        let mut tabs = TabBar::new("tool-kind").underline().flex_none();
        for (tab, label) in [
            (ToolAuthorizationKind::Skill, language.choose("技能", "Skills")),
            (ToolAuthorizationKind::McpServer, "MCP server"),
        ] {
            let chooser = entity.clone();
            tabs = tabs.child(
                Tab::new()
                    .label(label)
                    .selected(kind == tab)
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            view.new_tool_kind = tab;
                            view.tool_feedback = view
                                .language
                                .choose(
                                    "已切换列表；请选择资源查看已保存配置和验证状态。",
                                    "List switched; pick a resource to see saved configuration and verification status.",
                                )
                                .into();
                            cx.notify();
                        });
                    }),
            );
        }
        div().flex_1().h_full().min_h(px(0.)).min_w(px(0.)).v_flex().gap_2().p_3().overflow_hidden()
            .child(tabs)
            .child(div().flex_none().text_xs().whitespace_normal().child(language.choose(
                "应用 = 保存所选范围的授权，并在下次任务生成运行时配置。项目最终权限为全局与当前项目的交集，再过滤停用/缺失定义。保存、启用、授权、加载/连接分别显示。配置、权限或项目变化会取消既有操作；已完成的外部副作用无法回滚。",
                "Apply = save the authorization for the chosen scope and generate the runtime configuration for the next task. The project's effective permission is the intersection of global and current project, then filtered by disabled or missing definitions. Save, enable, authorize, and load/connect are reported separately. Configuration, permission, or project changes cancel in-flight operations; completed external side effects cannot be rolled back.",
            )))
            .child(div().flex_1().min_h(px(0.)).min_w(px(0.)).flex().gap_3().overflow_hidden().child(list).child(detail))
            .child(div().id("resource-operation-feedback").flex_none().max_h(px(60.)).scroll_y().text_xs().whitespace_normal().child(self.tool_feedback.clone()))
    }

    pub(super) fn render_catalog_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.catalog_editor.clone().expect("open catalog editor");
        let entity = cx.entity().clone();
        let language = self.language;
        let skill = matches!(editor, CatalogEditor::Skill(_));
        let delete = matches!(editor, CatalogEditor::Delete(..));
        let title = if delete {
            language.choose("删除资源", "Delete resource")
        } else if skill {
            language.choose("导入 / 编辑技能", "Import / edit skill")
        } else {
            language.choose("新增 / 编辑 MCP server", "Add / edit MCP server")
        };
        let mut fields =
            div().id("catalog-editor-fields").flex_1().min_h(px(0.)).scroll_y().v_flex().gap_3();
        match editor {
            CatalogEditor::Delete(_, id) => {
                fields = fields.child(language.choose_owned(
                    format!("删除 {id} 的定义及全局和已打开项目授权；保留源文件。其他未打开项目的旧引用不会获得全局权限。内置 Jev 删除后也不会在重启时自动恢复。"),
                    format!(
                        "Deletes the definition of {id} plus its global and opened-project authorizations; source files are kept. Stale references in unopened projects gain no global permission. The bundled Jev is not restored on restart after deletion."
                    ),
                ));
            }
            CatalogEditor::Skill(_) => {
                let folder = entity.clone();
                let file = entity.clone();
                fields = fields
                    .child(Self::resource_field(
                        language.choose("实际技能目录或 SKILL.md", "Skill folder or SKILL.md"),
                        "catalog-source",
                        None,
                        &self.catalog_source,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(action_button("skill-pick-folder").label(language.choose("选择目录", "Choose folder")).on_click(
                                move |_, window, cx| {
                                    folder.update(cx, |view, cx| {
                                        view.pick_skill_source(true, window, cx)
                                    });
                                },
                            ))
                            .child(
                                action_button("skill-pick-file").label(language.choose("选择 SKILL.md", "Choose SKILL.md")).on_click(
                                    move |_, window, cx| {
                                        file.update(cx, |view, cx| {
                                            view.pick_skill_source(false, window, cx)
                                        });
                                    },
                                ),
                            ),
                    )
                    .child(language.choose(
                        "保存时校验文件、标识和 256 KiB 大小限制。仅保存定义，不自动授权。",
                        "Validated on save: file, id, and the 256 KiB size limit. Saves the definition only; no automatic authorization.",
                    ));
                let source = PathBuf::from(self.catalog_source.read(cx).value().trim().to_string());
                let mut preview_catalog =
                    circuitfabric_codex_runtime::tools::ToolCatalog::default();
                if !source.as_os_str().is_empty() {
                    match preview_catalog.import_skill(&source) {
                        Ok(id) => {
                            if let Some(preview) = preview_catalog.skills[0].preview().ok() {
                                fields = fields
                                    .child(language.choose_owned(
                                        format!(
                                            "技能标识：{id}\n名称：{}\n说明：{}",
                                            preview.name, preview.description
                                        ),
                                        format!(
                                            "Skill id: {id}\nName: {}\nDescription: {}",
                                            preview.name, preview.description
                                        ),
                                    ))
                                    .child(
                                        div().text_xs().whitespace_normal().child(preview.content),
                                    );
                            }
                        }
                        Err(error) => fields = fields.child(language.choose_owned(
                            format!("校验未通过：{error}"),
                            format!("Validation failed: {error}"),
                        )),
                    }
                }
            }
            CatalogEditor::Mcp(_) => {
                fields = fields
                    .child(Self::resource_field(
                        language.choose("显示名称", "Display name"),
                        "catalog-name",
                        None,
                        &self.catalog_name,
                    ))
                    .child(Self::resource_field(
                        language.choose("唯一标识（编辑时保留）", "Unique id (kept when editing)"),
                        "catalog-id",
                        None,
                        &self.catalog_id,
                    ))
                    .child(language.choose(
                        "传输方式：stdio（当前唯一支持，不支持 HTTP/SSE 端点）",
                        "Transport: stdio (the only one supported; HTTP/SSE endpoints are not supported)",
                    ))
                    .child(Self::resource_field(
                        language.choose("启动程序（不经过 shell）", "Launch program (not run through a shell)"),
                        "catalog-source",
                        None,
                        &self.catalog_source,
                    ))
                    .child(Self::resource_field(
                        language.choose("参数（字符串 JSON 数组）", "Arguments (JSON array of strings)"),
                        "catalog-args",
                        None,
                        &self.catalog_args,
                    ))
                    .child(Self::resource_field(
                        language.choose("认证变量名（逗号分隔）", "Auth variable names (comma-separated)"),
                        "catalog-env",
                        None,
                        &self.catalog_env,
                    ))
                    .child(language.choose(
                        "密钥只从保险库或环境变量获取；不要将密钥写入名称、命令或参数。",
                        "Secrets come only from the vault or environment variables; never put secrets in names, commands, or arguments.",
                    ));
                for name in Self::parse_tool_ids(&self.catalog_env.read(cx).value()) {
                    if let Some(hint) = self.secret_source_hint(&name, &entity) {
                        fields = fields.child(hint);
                    }
                }
            }
        }
        let save = entity.clone();
        let cancel = entity;
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                div()
                    .id("catalog-editor-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00_0f17_2ab3))
                    .occlude(),
            )
            .child(
                div()
                    .id("catalog-editor-modal")
                    .relative()
                    .occlude()
                    .w(px(540.))
                    .max_w_full()
                    .h(px(580.))
                    .max_h_full()
                    .v_flex()
                    .gap_3()
                    .p_4()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(SURFACE_BG))
                    .shadow_lg()
                    .child(div().flex_none().text_lg().child(title))
                    .child(fields)
                    .when_some(self.catalog_error.clone(), |this, error| {
                        this.child(
                            div()
                                .flex_none()
                                .max_h(px(65.))
                                .overflow_hidden()
                                .text_sm()
                                .whitespace_normal()
                                .text_color(rgb(0x00b4_5309))
                                .child(language.choose_owned(
                                    format!("未保存：{error}"),
                                    format!("Not saved: {error}"),
                                )),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                action_button("catalog-editor-cancel")
                                    .label(language.choose("取消（丢弃草稿）", "Cancel (discard draft)"))
                                    .on_click(move |_, _, cx| {
                                        cancel.update(cx, |view, cx| {
                                            view.catalog_editor = None;
                                            view.catalog_error = None;
                                            view.tool_feedback = view
                                                .language
                                                .choose(
                                                    "已取消；已保存定义保持原值。",
                                                    "Cancelled; saved definitions keep their values.",
                                                )
                                                .into();
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                action_button("catalog-editor-save")
                                    .primary()
                                    .label(if delete {
                                        language.choose("确认删除", "Confirm delete")
                                    } else {
                                        language.choose("保存", "Save")
                                    })
                                    .on_click(move |_, _, cx| {
                                        save.update(cx, |view, cx| view.save_catalog_editor(cx));
                                    }),
                            ),
                    ),
            )
    }
}
