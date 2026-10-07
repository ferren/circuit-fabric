//! Authorization presentation and event handlers.
use super::*;

impl ControlPlaneView {
    /// Splits a tool-authorization input into IDs: commas (ASCII, full-width, and
    /// ideographic), semicolons, or whitespace separate items, and empty segments drop out.
    /// IDs themselves may never contain whitespace, so the split is unambiguous.
    pub(super) fn parse_tool_ids(raw: &str) -> Vec<String> {
        raw.split(|character: char| {
            matches!(character, ',' | '，' | '、' | ';') || character.is_whitespace()
        })
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
    }

    /// Authorizes one or more skills / MCP servers in the selected scope, persisting
    /// immediately. Several IDs can be pasted at once; each becomes its own list row.
    pub(super) fn authorize_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let requested = Self::parse_tool_ids(&self.new_tool_id.read(cx).value());
        if requested.is_empty() {
            "未授权：请输入至少一个 ID（多项之间用逗号或空格分隔）。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let kind = self.new_tool_kind;
        if requested.iter().any(|id| match kind {
            ToolAuthorizationKind::Skill => {
                !self.catalog.skills.iter().any(|s| &s.id == id && s.enabled)
            }
            ToolAuthorizationKind::McpServer => {
                !self.catalog.mcp_servers.iter().any(|s| &s.id == id && s.enabled)
            }
        }) {
            "未授权：请先导入技能或保存 MCP 定义，并启用所选条目。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let kind_label = Self::tool_kind_label(kind, self.language);
        match self.new_tool_scope {
            ToolScope::Global => {
                let previous = self.tool_authorizations.clone();
                let list = Self::global_tool_list_mut(&mut self.tool_authorizations, kind);
                let mut authorized: Vec<String> = Vec::new();
                let mut skipped = 0_usize;
                for id in requested {
                    if list.contains(&id) {
                        skipped += 1;
                    } else {
                        list.push(id.clone());
                        authorized.push(id);
                    }
                }
                if authorized.is_empty() {
                    self.status = format!("未新增：所填{kind_label}均已授权（全局作用域）。");
                    cx.notify();
                    return;
                }
                list.sort();
                list.dedup();
                let skipped_note = if skipped == 0 {
                    String::new()
                } else {
                    format!("（另跳过已授权的 {skipped} 项）")
                };
                match self.save_update(
                    crate::application::settings_persistence::SettingsUpdate::Authorizations(
                        self.tool_authorizations.clone(),
                    ),
                ) {
                    Ok(()) => {
                        self.new_tool_id.update(cx, |state, cx| state.set_value("", window, cx));
                        self.status = format!(
                            "已授权 {} 项{kind_label}（全局作用域）：{}{skipped_note}。已写入 {}。",
                            authorized.len(),
                            authorized.join("、"),
                            self.settings_path.display()
                        );
                    }
                    Err(error) => {
                        self.tool_authorizations = previous;
                        self.status = format!("全局授权未保存：{error}");
                    }
                }
            }
            ToolScope::Project => {
                let Some(project_id) = self.navigation.selected_project.clone() else {
                    "未授权：请先在「项目」页选择一个项目。".clone_into(&mut self.status);
                    cx.notify();
                    return;
                };
                let Some(storage) = self.project_storages.get(&project_id).cloned() else {
                    "未授权：项目根目录未打开。".clone_into(&mut self.status);
                    cx.notify();
                    return;
                };
                let mut configuration =
                    self.workspace.configuration(&project_id).cloned().unwrap_or_default();
                let list = match kind {
                    ToolAuthorizationKind::Skill => &mut configuration.enabled_skill_ids,
                    ToolAuthorizationKind::McpServer => &mut configuration.enabled_mcp_server_ids,
                };
                let mut authorized: Vec<String> = Vec::new();
                let mut skipped = 0_usize;
                for id in requested {
                    if list.contains(&id) {
                        skipped += 1;
                    } else {
                        list.push(id.clone());
                        authorized.push(id);
                    }
                }
                if authorized.is_empty() {
                    self.status =
                        format!("未新增：所填{kind_label}均已授权（项目 `{project_id}` 作用域）。");
                    cx.notify();
                    return;
                }
                list.sort();
                list.dedup();
                let skipped_note = if skipped == 0 {
                    String::new()
                } else {
                    format!("（另跳过已授权的 {skipped} 项）")
                };
                match storage.save_configuration(&configuration) {
                    Ok(()) => {
                        match self.workspace.set_configuration(&project_id, configuration.clone()) {
                            Ok(()) => {
                                self.invalidate_tool_runs();
                                self.new_tool_id
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                self.status = format!(
                                    "已授权 {} 项{kind_label}（项目 `{project_id}` 作用域）：{}{skipped_note}。已写入 {}。",
                                    authorized.len(),
                                    authorized.join("、"),
                                    storage.configuration_path().display()
                                );
                            }
                            Err(error) => {
                                self.status = format!(
                                    "文件已保存，但项目内存未刷新；请重新打开项目：{error}"
                                );
                            }
                        }
                    }
                    Err(error) => self.status = format!("未授权：{error}"),
                }
            }
        }
        cx.notify();
    }

    /// Revokes one skill or MCP server authorization, persisting immediately.
    pub(super) fn revoke_tool(
        &mut self,
        scope: ToolScope,
        kind: ToolAuthorizationKind,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(cancel) = &self.task_cancel {
            cancel.cancel();
        }
        let kind_label = Self::tool_kind_label(kind, self.language);
        match scope {
            ToolScope::Global => {
                let previous = self.tool_authorizations.clone();
                let list = Self::global_tool_list_mut(&mut self.tool_authorizations, kind);
                list.retain(|existing| existing.as_str() != id);
                match self.save_update(
                    crate::application::settings_persistence::SettingsUpdate::Authorizations(
                        self.tool_authorizations.clone(),
                    ),
                ) {
                    Ok(()) => {
                        self.status = format!(
                            "已撤销{kind_label} `{id}` 的全局授权，已写入 {}。",
                            self.settings_path.display()
                        );
                    }
                    Err(error) => {
                        self.tool_authorizations = previous;
                        self.status = format!("撤销授权未保存：{error}");
                    }
                }
            }
            ToolScope::Project => {
                let Some(project_id) = self.navigation.selected_project.clone() else {
                    "未撤销：当前未选择项目。".clone_into(&mut self.status);
                    cx.notify();
                    return;
                };
                let Some(storage) = self.project_storages.get(&project_id).cloned() else {
                    "未撤销：项目根目录未打开。".clone_into(&mut self.status);
                    cx.notify();
                    return;
                };
                let mut configuration =
                    self.workspace.configuration(&project_id).cloned().unwrap_or_default();
                let list = match kind {
                    ToolAuthorizationKind::Skill => &mut configuration.enabled_skill_ids,
                    ToolAuthorizationKind::McpServer => &mut configuration.enabled_mcp_server_ids,
                };
                list.retain(|existing| existing.as_str() != id);
                match storage.save_configuration(&configuration) {
                    Ok(()) => {
                        match self.workspace.set_configuration(&project_id, configuration.clone()) {
                            Ok(()) => {
                                self.invalidate_tool_runs();
                                self.status = format!(
                                    "已撤销{kind_label} `{id}` 在项目 `{project_id}` 中的授权，已写入 {}。",
                                    storage.configuration_path().display()
                                );
                            }
                            Err(error) => {
                                self.status = format!(
                                    "文件已保存，但项目内存未刷新；请重新打开项目：{error}"
                                );
                            }
                        }
                    }
                    Err(error) => self.status = format!("未撤销：{error}"),
                }
            }
        }
        cx.notify();
    }

    const fn tool_kind_label(kind: ToolAuthorizationKind, language: UiLanguage) -> &'static str {
        match kind {
            ToolAuthorizationKind::Skill => language.choose("技能", "skill"),
            ToolAuthorizationKind::McpServer => language.choose("MCP 服务器", "MCP server"),
        }
    }

    pub(super) fn global_tool_list_mut(
        tools: &mut ToolAuthorizationSettings,
        kind: ToolAuthorizationKind,
    ) -> &mut Vec<String> {
        match kind {
            ToolAuthorizationKind::Skill => &mut tools.authorized_skill_ids,
            ToolAuthorizationKind::McpServer => &mut tools.authorized_mcp_server_ids,
        }
    }

    /// Global tool grants narrowed by the selected project's allowlist.
    ///
    /// A project can only restrict what the global runtime already authorized, never expand
    /// it; a missing project configuration grants no project tools.
    pub(super) fn effective_grants(&self) -> ToolAuthorizationSettings {
        let Some(project_id) = self.navigation.selected_project.as_ref() else {
            return self.catalog.effective_grants(&self.tool_authorizations, None);
        };
        self.effective_grants_for(project_id)
    }

    pub(super) fn effective_grants_for(&self, project_id: &ProjectId) -> ToolAuthorizationSettings {
        let project = self
            .workspace
            .configuration(project_id)
            .map(|configuration| ToolAuthorizationSettings {
                authorized_skill_ids: configuration.enabled_skill_ids.clone(),
                authorized_mcp_server_ids: configuration.enabled_mcp_server_ids.clone(),
            })
            .unwrap_or_default();
        self.catalog.effective_grants(&self.tool_authorizations, Some(&project))
    }
}
