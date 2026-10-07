//! Runtime presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn save_bridge_settings(&mut self, cx: &mut Context<Self>) {
        let update = crate::application::settings_persistence::SettingsUpdate::Bridge {
            listen_address: self.bridge_address.read(cx).value().trim().to_owned(),
        };
        self.status = match self.save_update(update) {
            Ok(()) => "已保存 Bridge 监听地址；下次启动生效。".into(),
            Err(error) => format!("Bridge 监听地址未保存：{error}"),
        };
        cx.notify();
    }

    pub(super) fn bridge_endpoint(&self) -> String {
        self.bridge_active_address
            .as_ref()
            .unwrap_or(&self.saved_settings.bridge.listen_address)
            .clone()
    }

    /// Execution reads saved configuration and never commits unrelated form drafts.
    pub(super) fn settings_for_execution(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<RuntimeSettings> {
        match RuntimeSettings::load_or_default(&self.settings_path) {
            Ok(settings) => Some(settings),
            Err(error) => {
                self.status = format!("无法读取已保存配置：{error}");
                cx.notify();
                None
            }
        }
    }

    pub(super) fn current_project_context(&self) -> Option<(ProjectId, PathBuf)> {
        let id = self.navigation.selected_project.as_ref()?;
        let storage = self.project_storages.get(id)?;
        Some((id.clone(), storage.root().to_path_buf()))
    }

    /// Starts the supervised Codex App Server child process.
    ///
    /// Uses the saved snapshot and checks for an immediate process exit.
    pub(super) fn start_codex_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.codex_process.is_some()
            || matches!(self.codex_status, RuntimeLifecycleStatus::Starting)
        {
            "未启动：Codex App Server 正在运行或正在启动。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        let project_context = self.current_project_context();
        let launch_settings = match crate::application::project_runtime::codex_for_project(
            &settings,
            project_context.as_ref().map(|(_, root)| root.as_path()),
        ) {
            Ok(settings) => settings,
            Err(reason) => {
                self.status = reason;
                cx.notify();
                return;
            }
        };
        let Some(provider) = circuitfabric_codex_runtime::execution::selected_provider(
            &settings,
            circuitfabric_codex_runtime::execution::AgentKind::Codex,
        )
        .cloned() else {
            "未启动：请先配置默认 Provider。".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.codex_status = RuntimeLifecycleStatus::Starting;
        self.codex_project_context = project_context.clone();
        self.codex_active_provider =
            Some(format!("{} / {} / {}", provider.id, provider.model, provider.base_url));
        self.status = format!("Codex App Server: {} / {}", provider.id, provider.model);
        let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
            .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
            .flatten();
        let launch = cx.background_spawn(async move {
            CodexAppServerHandle::launch_with_secrets(&launch_settings, &provider, secrets.as_ref())
                .map_err(|error| error.to_string())
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = launch.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    if view.codex_project_context != project_context
                        || view.current_project_context() != project_context
                    {
                        let stopped = match result {
                            Ok(mut handle) => handle.stop().map_err(|error| error.to_string()),
                            Err(_) => Ok(()),
                        };
                        view.codex_project_context = None;
                        view.codex_active_provider = None;
                        view.codex_status = match stopped {
                            Ok(()) => {
                                view.status =
                                    "已取消 Codex 启动并清理进程；请在当前项目重新启动。".into();
                                RuntimeLifecycleStatus::Stopped
                            }
                            Err(reason) => {
                                view.status = format!("取消 Codex 启动时清理进程失败：{reason}");
                                RuntimeLifecycleStatus::Failed { reason }
                            }
                        };
                        cx.notify();
                        return;
                    }
                    match result {
                        Ok(handle) => {
                            let pid = handle.pid();
                            view.codex_process = Some(handle);
                            view.codex_status = RuntimeLifecycleStatus::Running { pid };
                        }
                        Err(reason) => {
                            view.codex_project_context = None;
                            view.codex_active_provider = None;
                            view.codex_status =
                                RuntimeLifecycleStatus::Failed { reason: reason.clone() };
                            view.status = reason;
                        }
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

    pub(super) fn stop_codex_runtime(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.task_cancel {
            cancel.cancel();
        }
        if matches!(self.codex_status, RuntimeLifecycleStatus::Starting) {
            self.codex_project_context = None;
            "已请求取消 Codex 启动，启动结束后自动清理进程。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(mut process) = self.codex_process.take() else {
            "运行时当前未在运行。".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let pid = process.pid();
        self.codex_project_context = None;
        match process.stop() {
            Ok(()) => {
                self.codex_status = RuntimeLifecycleStatus::Stopped;
                self.codex_active_provider = None;
                self.status =
                    format!("已停止 Codex App Server（PID {pid}）。已保存的运行时设置保持不变。");
            }
            Err(error) => {
                self.codex_status = RuntimeLifecycleStatus::Failed { reason: error.to_string() };
                self.status = format!("停止失败（PID {pid}）：{error}");
            }
        }
        cx.notify();
    }

    /// Reconciles the lifecycle chip with the real process state: a process that died on its
    /// own is reported as stopped or failed instead of staying green.
    pub(super) fn refresh_codex_lifecycle(&mut self) {
        if self.codex_project_context.is_some()
            && self.codex_project_context != self.current_project_context()
        {
            self.codex_project_context = None;
            if let Some(mut process) = self.codex_process.take() {
                self.codex_active_provider = None;
                self.codex_status = match process.stop() {
                    Ok(()) => {
                        self.status =
                            "项目已切换，旧项目的 Codex 进程已清理；请在当前项目重新启动。".into();
                        RuntimeLifecycleStatus::Stopped
                    }
                    Err(error) => {
                        let reason = error.to_string();
                        self.status =
                            format!("项目已切换，但旧项目的 Codex 进程清理失败：{reason}");
                        RuntimeLifecycleStatus::Failed { reason }
                    }
                };
            }
        }
        let exit = self.codex_process.as_mut().and_then(CodexAppServerHandle::try_exit);
        if let Some(exit) = exit {
            self.codex_process = None;
            self.codex_active_provider = None;
            self.codex_project_context = None;
            if exit.success() {
                self.codex_status = RuntimeLifecycleStatus::Stopped;
            } else {
                self.codex_status = RuntimeLifecycleStatus::Failed {
                    reason: format!("进程已退出（{exit}）"),
                };
            }
        }
    }

    /// Starts the `JLCircuit` EDA bridge as a supervised child process.
    ///
    /// The bridge reads the saved snapshot, then waits for the configured port before
    /// reporting running — an early exit is reported as a failure together
    /// with the bridge's own stderr.
    pub(super) fn start_bridge_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.bridge_process.is_some()
            || matches!(self.bridge_status, RuntimeLifecycleStatus::Starting)
        {
            "未启动：bridge 服务已在运行或正在启动。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        let config_path = self.settings_path.clone();
        let address = settings.bridge.listen_address.clone();
        let launch_address = address.clone();
        self.bridge_status = RuntimeLifecycleStatus::Starting;
        self.bridge_active_address = Some(address.clone());
        self.bridge_health = BridgeHealth::Unknown;
        self.bridge_probed_at = None;
        self.status = format!("bridge 正在启动：ws://{address}/bridge");
        let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
            .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
            .flatten();
        let launch = cx.background_spawn(async move {
            BridgeProcessHandle::launch_with_vault(&config_path, secrets.as_ref()).and_then(
                |mut handle| {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline {
                        if let Some(exit) = handle.try_exit() {
                            let diagnostics = handle.exit_diagnostics();
                            let detail = if diagnostics.is_empty() {
                                String::new()
                            } else {
                                format!("：{diagnostics}")
                            };
                            return Err(format!("bridge 进程已退出（{exit}）{detail}"));
                        }
                        if probe::tcp_reachable(&launch_address, Duration::from_millis(300)).is_ok()
                        {
                            return Ok(handle);
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    let _ = handle.stop();
                    Err(format!("bridge 在 5 秒内未开始监听 {launch_address}"))
                },
            )
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = launch.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    match result {
                        Ok(handle) => {
                            let pid = handle.pid();
                            view.bridge_health = BridgeHealth::Listening { at: Instant::now() };
                            view.bridge_process = Some(handle);
                            view.bridge_status = RuntimeLifecycleStatus::Running { pid };
                            view.status =
                                format!("bridge 已启动（PID {pid}）：ws://{address}/bridge");
                        }
                        Err(reason) => {
                            view.bridge_active_address = None;
                            view.bridge_status =
                                RuntimeLifecycleStatus::Failed { reason: reason.clone() };
                            view.status = format!("bridge 启动失败：{reason}");
                        }
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

    /// Stops the bridge child process. A bridge this app did not start is
    /// reported as such — its process is never touched.
    pub(super) fn stop_bridge_service(&mut self, cx: &mut Context<Self>) {
        if matches!(self.bridge_status, RuntimeLifecycleStatus::Starting) {
            "未停止：bridge 正在启动，请稍候。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(mut process) = self.bridge_process.take() else {
            "bridge 服务当前不是由本应用启动的。".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let pid = process.pid();
        match process.stop() {
            Ok(()) => {
                self.bridge_status = RuntimeLifecycleStatus::Stopped;
                self.bridge_active_address = None;
                self.bridge_health = BridgeHealth::Unknown;
                self.bridge_probed_at = None;
                self.bridge_test = None;
                self.status = format!("已停止 bridge 服务（PID {pid}）。");
            }
            Err(error) => {
                self.bridge_status = RuntimeLifecycleStatus::Failed { reason: error.clone() };
                self.status = format!("停止 bridge 失败（PID {pid}）：{error}");
            }
        }
        cx.notify();
    }

    /// Reconciles the bridge chip with the real process state: a bridge that
    /// died on its own is reported as failed with its stderr, not left green.
    pub(super) fn refresh_bridge_lifecycle(&mut self) {
        let exit = self.bridge_process.as_mut().and_then(BridgeProcessHandle::try_exit);
        if let Some(exit) = exit {
            let diagnostics = self
                .bridge_process
                .as_mut()
                .map(BridgeProcessHandle::exit_diagnostics)
                .unwrap_or_default();
            self.bridge_process = None;
            self.bridge_active_address = None;
            if exit.success() {
                self.bridge_status = RuntimeLifecycleStatus::Stopped;
            } else {
                let detail = if diagnostics.is_empty() {
                    String::new()
                } else {
                    format!("：{diagnostics}")
                };
                self.bridge_status = RuntimeLifecycleStatus::Failed {
                    reason: format!("bridge 进程已退出（{exit}）{detail}"),
                };
            }
        }
    }

    /// Runs the automatic reachability probe while the EDA services page is
    /// visible, rate-limited to one TCP probe every 5 seconds. It never
    /// opens the WebSocket: an attached EDA client must not be disturbed,
    /// because the bridge serves one connection at a time.
    pub(super) fn maybe_probe_bridge_health(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bridge_probe_pending
            || self.bridge_probed_at.is_some_and(|at| at.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        self.bridge_probe_pending = true;
        self.bridge_probed_at = Some(Instant::now());
        let address = self.bridge_endpoint();
        let probe_task =
            cx.background_spawn(
                async move { probe::tcp_reachable(&address, Duration::from_secs(2)) },
            );
        cx.spawn_in(window, async move |view, cx| {
            let result = probe_task.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.bridge_probe_pending = false;
                    view.bridge_health = match result {
                        Ok(()) => BridgeHealth::Listening { at: Instant::now() },
                        Err(reason) => BridgeHealth::Unreachable { reason },
                    };
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    /// Manual connection test: the protocol-level `status` round-trip over
    /// the bridge WebSocket. The bridge serves one client at a time, so the
    /// test may time out while an EDA client is attached — that outcome is
    /// reported as its own error, not as "offline".
    pub(super) fn test_bridge_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.bridge_test_pending {
            return;
        }
        self.bridge_test_pending = true;
        let address = self.bridge_endpoint();
        let test_task = cx.background_spawn(async move {
            probe::status(&address, Duration::from_secs(5)).map_err(|reason| {
                // A refused connection really is offline; anything else
                // (handshake timeout, malformed reply) keeps the current
                // health — the port may still be listening.
                let offline = probe::tcp_reachable(&address, Duration::from_secs(1)).is_err();
                (reason, offline)
            })
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = test_task.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.bridge_test_pending = false;
                    let at = Instant::now();
                    match result {
                        Ok(report) => {
                            let version = report.protocol_version;
                            let bridge_name = report.bridge_name.clone();
                            view.bridge_health = BridgeHealth::Listening { at };
                            view.bridge_test = Some(BridgeTestResult { at, outcome: Ok(report) });
                            view.status =
                                format!("bridge 连接测试成功：协议 v{version} · {bridge_name}");
                        }
                        Err((reason, offline)) => {
                            if offline {
                                view.bridge_health =
                                    BridgeHealth::Unreachable { reason: reason.clone() };
                            }
                            view.bridge_test = Some(BridgeTestResult { at, outcome: Err(reason) });
                        }
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

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_agents_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let adapter_settings_dialog = self
            .adapter_settings_open
            .map(|adapter| self.render_adapter_settings_dialog(adapter, cx).into_any_element());
        let provider_dialog =
            self.provider_editor_open.then(|| self.render_provider_dialog(cx).into_any_element());

        let mut runtime_cards = div().v_flex().gap_2();
        for adapter in
            [RuntimeAdapter::CodexAppServer, RuntimeAdapter::ClaudeCode, RuntimeAdapter::Dsh]
        {
            let selector = entity.clone();
            let selected = matches!(self.agents_selection, AgentsSelection::Runtime(chosen) if chosen == adapter);
            let is_codex = adapter == RuntimeAdapter::CodexAppServer;
            let status_label =
                if is_codex { self.codex_status.clone().label(language) } else { String::new() };
            runtime_cards = runtime_cards.child(
                div()
                    .id(format!("runtime-card-{}", adapter.label()))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if selected { ACCENT } else { BORDER }))
                    .bg(rgb(if selected { 0x00f0_f9ff } else { CARD_BG }))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                    .on_click(move |_, _, cx| {
                        selector.update(cx, |view, cx| {
                            view.agents_selection = AgentsSelection::Runtime(adapter);
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(is_codex, |row| row.child(status_dot(self.codex_status.dot())))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(if selected {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .child(adapter.label()),
                            )
                            .child(div().ml_auto().flex().items_center().gap_2().when(
                                is_codex,
                                |row| {
                                    row.child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(TEXT_MUTED))
                                            .child(status_label.clone()),
                                    )
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(adapter.summary(language)),
                    ),
            );
        }

        let selected_provider = self.selected_provider.min(self.providers.len() - 1);
        let mut provider_cards = div().v_flex().gap_2();
        for (index, provider) in self.providers.iter().enumerate() {
            let selector = entity.clone();
            let selected =
                self.agents_selection == AgentsSelection::Provider && index == selected_provider;
            let provider_id = provider.id.read(cx).value().to_string();
            let provider_name = provider.name.read(cx).value().to_string();
            let provider_model = provider.model.read(cx).value().to_string();
            let is_default = provider_id == self.default_provider_id;
            provider_cards = provider_cards.child(
                div()
                    .id(format!("provider-card-{index}"))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if selected { ACCENT } else { BORDER }))
                    .bg(rgb(if selected { 0x00f0_f9ff } else { CARD_BG }))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                    .on_click(move |_, _, cx| {
                        selector.update(cx, |view, cx| {
                            view.selected_provider = index;
                            view.agents_selection = AgentsSelection::Provider;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(status_dot(if provider.enabled {
                                0x0022_c55e
                            } else {
                                0x0094_a3b8
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(if selected {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .truncate()
                                    .child(provider_name),
                            )
                            .when(is_default, |row| {
                                row.child(
                                    div()
                                        .ml_auto()
                                        .text_xs()
                                        .text_color(rgb(0x00b4_5309))
                                        .child("★"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(div().truncate().child(provider_id))
                            .child(div().truncate().child(provider_model)),
                    ),
            );
        }

        let tools_selected = self.agents_selection == AgentsSelection::SkillsAndMcp;
        let global_skill_count = self.tool_authorizations.authorized_skill_ids.len();
        let global_mcp_count = self.tool_authorizations.authorized_mcp_server_ids.len();
        let tools_project_line = match self
            .navigation
            .selected_project
            .as_deref()
            .and_then(|project_id| self.workspace.configuration(project_id))
        {
            Some(configuration) => language.choose_owned(
                format!(
                    "当前项目：{} 技能 · {} MCP",
                    configuration.enabled_skill_ids.len(),
                    configuration.enabled_mcp_server_ids.len()
                ),
                format!(
                    "Project: {} skills · {} MCP",
                    configuration.enabled_skill_ids.len(),
                    configuration.enabled_mcp_server_ids.len()
                ),
            ),
            None => language.choose("未选择项目", "No project selected").to_owned(),
        };
        let tools_selector = entity.clone();
        let tools_card = div()
            .id("tools-card")
            .v_flex()
            .gap_1()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if tools_selected { ACCENT } else { BORDER }))
            .bg(rgb(if tools_selected { 0x00f0_f9ff } else { CARD_BG }))
            .cursor_pointer()
            .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
            .on_click(move |_, _, cx| {
                tools_selector.update(cx, |view, cx| {
                    view.agents_selection = AgentsSelection::SkillsAndMcp;
                    cx.notify();
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(if tools_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .child(language.choose("技能与 MCP 授权", "Skills & MCP")),
                    )
                    .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose_owned(
                            format!("全局 {global_skill_count} · {global_mcp_count}"),
                            format!("{global_skill_count} · {global_mcp_count} global"),
                        ),
                    )),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(tools_project_line));

        let detail = match self.agents_selection {
            AgentsSelection::Runtime(RuntimeAdapter::CodexAppServer) => {
                self.render_codex_runtime_detail(cx).into_any_element()
            }
            AgentsSelection::Runtime(adapter) => {
                self.render_adapter_detail(adapter, cx).into_any_element()
            }
            AgentsSelection::Provider => self.render_provider_detail(cx).into_any_element(),
            AgentsSelection::SkillsAndMcp => self.render_skills_detail(cx).into_any_element(),
            AgentsSelection::BundledJev => self.render_bundled_jev_detail(cx).into_any_element(),
        };

        let jev_selected = self.agents_selection == AgentsSelection::BundledJev;
        let jev_summary =
            match self.catalog.mcp_servers.iter().find(|server| server.id == BUNDLED_JEV_SERVER_ID)
            {
                Some(server) => {
                    let granted = self
                        .tool_authorizations
                        .authorized_mcp_server_ids
                        .iter()
                        .any(|id| id == BUNDLED_JEV_SERVER_ID);
                    let llm =
                        circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server(server)
                            .is_some();
                    language.choose_owned(
                        format!(
                            "{} · {} · {}",
                            if server.enabled { "已启用" } else { "已停用" },
                            if granted { "已授权" } else { "未授权" },
                            if llm { "LLM" } else { "TypeSafe" }
                        ),
                        format!(
                            "{} · {} · {}",
                            if server.enabled { "enabled" } else { "disabled" },
                            if granted { "authorized" } else { "not authorized" },
                            if llm { "LLM" } else { "TypeSafe" }
                        ),
                    )
                }
                None => language.choose("未注册", "not registered").to_owned(),
            };
        let jev_selector = entity.clone();
        let jev_card = div()
            .id("jev-card")
            .v_flex()
            .gap_1()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if jev_selected { ACCENT } else { BORDER }))
            .bg(rgb(if jev_selected { 0x00f0_f9ff } else { CARD_BG }))
            .cursor_pointer()
            .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
            .on_click(move |_, _, cx| {
                jev_selector.update(cx, |view, cx| {
                    view.agents_selection = AgentsSelection::BundledJev;
                    cx.notify();
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(if jev_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .child(language.choose("Jev 判断工具", "Jev judgments")),
                    )
                    .child(
                        div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(jev_summary),
                    ),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "TypeSafe 或第三方 LLM 的类型化判断（evaluate）",
                "Typed judgments via TypeSafe or a third-party LLM (evaluate)",
            )));

        let add_provider = entity.clone();
        workspace_page("agents-tools-page").relative()
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
                                    .child(language
                                        .choose("智能体与工具", "Agents & tools")),
                            )
                            .child(
                                div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                    language.choose(
                                        "在各详情页保存对应配置；技能/MCP 启停与授权即时保存。API Key 以变量名引用。",
                                        "Save each configuration in its detail pane; skill/MCP toggles and grants save immediately. API keys use variable names.",
                                    ),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap_4()
                    .when(!tools_selected, |row| row.child(
                        div()
                            .id("agents-list-scroll")
                            .min_h(px(0.))
                            .scroll_y()
                            .w(px(320.))
                            .flex_none()
                            .v_flex()
                            .gap_2()
                            .child(agents_group_label(
                                language.choose("运行时端点", "Runtime endpoints"),
                            ))
                            .child(runtime_cards)
                            .child(agents_group_label(
                                language.choose("LLM Provider", "LLM providers"),
                            ))
                            .child(provider_cards)
                            .child(
                                action_button("add-provider")
                                    .label(language.choose("添加 Provider", "Add provider"))
                                    .on_click(move |_, window, cx| {
                                        add_provider.update(cx, |view, cx| {
                                            view.add_provider(window, cx);
                                        });
                                    }),
                            )
                            .child(agents_group_label(
                                language.choose("技能与 MCP", "Skills & MCP"),
                            ))
                            .child(tools_card)
                            .child(jev_card),
                    ))
                    .child(detail),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
            .when_some(adapter_settings_dialog, ParentElement::child)
            .when_some(provider_dialog, ParentElement::child)
    }

    /// The EDA services page is the multi-EDA service registry, laid out like
    /// Agents & tools: the left list manages one card per EDA backend service
    /// (only `JLCircuit` is implemented today), the right pane shows the
    /// selected service's settings, supervised lifecycle, and monitoring.
    pub(super) fn render_eda_services_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.maybe_probe_bridge_health(window, cx);
        let entity = cx.entity().clone();
        let language = self.language;
        let selector = entity;

        let jlc_selected = self.eda_services_selection == EdaServiceSelection::JlcircuitBridge;
        let lifecycle_label = self.bridge_status.clone().label(language);
        let health = self.bridge_health.clone();
        let address = self.bridge_endpoint();
        let jlc_card = div()
            .id("eda-service-jlcircuit")
            .v_flex()
            .gap_1()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if jlc_selected { ACCENT } else { BORDER }))
            .bg(rgb(if jlc_selected { 0x00f0_f9ff } else { CARD_BG }))
            .cursor_pointer()
            .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
            .on_click(move |_, _, cx| {
                selector.update(cx, |view, cx| {
                    view.eda_services_selection = EdaServiceSelection::JlcircuitBridge;
                    cx.notify();
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(status_dot(health.dot()))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(if jlc_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .child(EdaServiceSelection::JlcircuitBridge.label()),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(lifecycle_label),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .truncate()
                    .child(format!("ws://{address}/bridge")),
            );

        // Deliberately not selectable: further backends plug into this same
        // service model later, but nothing claims to work today.
        let planned_card = div()
            .v_flex()
            .gap_1()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .opacity(0.7)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose("更多 EDA 后端", "More EDA backends")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .whitespace_normal()
                    .child(language.choose(
                        "KiCad 等后续按同一服务模型接入：左侧注册、启停与监测。",
                        "KiCad and others will plug into the same service model later: registered, supervised, and monitored from this list.",
                    )),
            );

        let detail = match self.eda_services_selection {
            EdaServiceSelection::JlcircuitBridge => {
                self.render_eda_bridge_detail(cx).into_any_element()
            }
        };

        workspace_page("eda-services-page")
            .min_w(px(720.))
            .relative()
            .v_flex()
            .gap_4()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("EDA 服务", "EDA services")),
                    )
                    .child(
                        div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                            language.choose(
                                "EDA 后端服务在这里注册、启停与监测；智能体运行时端点在「智能体与工具」页。",
                                "EDA backend services are registered, supervised, and monitored here; agent runtime endpoints live on the Agents & tools page.",
                            ),
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
                            .id("eda-list-scroll")
                            .min_h(px(0.))
                            .scroll_y()
                            .w(px(320.))
                            .flex_none()
                            .v_flex()
                            .gap_2()
                            .child(agents_group_label(language.choose(
                                "已实现",
                                "Available",
                            )))
                            .child(jlc_card)
                            .child(agents_group_label(language.choose(
                                "规划中",
                                "Planned",
                            )))
                            .child(planned_card),
                    )
                    .child(detail),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
    }

    /// The `JLCircuit` bridge service detail: settings, supervised lifecycle,
    /// and layered health monitoring with the protocol-level connection test.
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_eda_bridge_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let bridge_saver = entity.clone();
        let bridge_starter = entity.clone();
        let bridge_stopper = entity.clone();
        let bridge_tester = entity;
        let status = self.bridge_status.clone();
        let is_running = self.bridge_process.is_some();
        let is_starting = matches!(self.bridge_status, RuntimeLifecycleStatus::Starting);
        let is_test_pending = self.bridge_test_pending;
        let address = self.bridge_endpoint();
        let health = self.bridge_health.clone();
        let externally_owned =
            matches!(health, BridgeHealth::Listening { .. }) && !is_running && !is_starting;
        let last_test = self
            .bridge_test
            .as_ref()
            .map(|test| (elapsed_label(language, test.at.elapsed()), test.outcome.clone()));
        let capabilities_reported = last_test.as_ref().is_some_and(|(_, outcome)| outcome.is_ok());

        // Capabilities are shown only when the bridge itself reported them
        // in a successful connection test — never claimed on its behalf.
        let mut capability_items = div().flex().flex_wrap().gap_1p5();
        if let Some((_, Ok(report))) = &last_test {
            for capability in &report.capabilities {
                capability_items = capability_items.child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .text_xs()
                        .bg(rgb(0x00e0_f2fe))
                        .text_color(rgb(0x000e_7490))
                        .child(capability.clone()),
                );
            }
        }
        let test_line = match &last_test {
            None => language
                .choose("尚未执行连接测试。", "No connection test has been run yet.")
                .to_owned(),
            Some((elapsed, Ok(report))) => format!(
                "{} · {elapsed}{} · {} · {} v{}",
                language.choose("已连接", "Connected"),
                language.choose(" 前", " ago"),
                report.bridge_name,
                language.choose("协议", "protocol"),
                report.protocol_version,
            ),
            Some((elapsed, Err(reason))) => format!(
                "{} · {elapsed}{} · {reason}",
                language.choose("连接失败", "Connection test failed"),
                language.choose(" 前", " ago"),
            ),
        };
        // The EDA extension's hello is rejected unless its projectId is
        // registered, so the connection test reports that gate too — an
        // empty list explains an EDA-side connect failure up front.
        let projects_note = match &last_test {
            Some((_, Ok(report))) if !report.projects.is_empty() => Some((
                language.choose_owned(
                    format!(
                        "已注册项目（EDA 插件连接时填写这些 ID）：{}",
                        report.projects.join("、")
                    ),
                    format!(
                        "Registered projects (use these IDs in the EDA extension): {}",
                        report.projects.join(", ")
                    ),
                ),
                TEXT_SECONDARY,
            )),
            Some((_, Ok(_))) => Some((
                language.choose(
                    "连接成功，但尚无已注册项目：EDA 插件可连接，发送请求前请先在「项目」页创建或打开项目，再刷新项目列表。",
                    "Connected, but no registered projects: the EDA extension's connection will be rejected — create or open a project on the Projects page first.",
                )
                .to_owned(),
                0x00b4_5309,
            )),
            _ => None,
        };

        detail_pane("eda-bridge-detail")
            .gap_3()
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
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("JLCircuit EDA bridge"),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(0x00e0_f2fe))
                                    .text_color(rgb(0x000e_7490))
                                    .child(language.choose("本地 WebSocket", "Local WebSocket")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(SURFACE_BG))
                            .child(status_dot(status.dot()))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(status.label(language)),
                            ),
                    ),
            )
            .child(
                div().text_sm().text_color(rgb(TEXT_SECONDARY)).whitespace_normal().child(
                    language.choose_owned(
                        format!("JLCircuit 插件与本机 `circuitfabric-jlc-bridge` 进程之间的 WebSocket 传输。bridge 作为受监管的子进程在这里启动与停止，并读取这里保存的监听地址；EDA 插件连接 ws://{address}/bridge。"),
                        format!("The WebSocket transport between the JLCircuit plugin and the local `circuitfabric-jlc-bridge` process. The bridge is started and stopped here as a supervised child process and reads the listen address saved here; the EDA extension connects to ws://{address}/bridge."),
                    ),
                ),
            )
            .child(labeled_field(
                language.choose("bridge 监听地址", "Bridge listen address"),
                "bridge-address",
                Some(language.choose(
                    "仅保存此服务的监听地址（如 127.0.0.1:49630）；下次启动生效，运行中需重启。",
                    "Saves only this service's listen address (e.g. 127.0.0.1:49630); applies at the next start. Restart a running service.",
                )),
                &self.bridge_address,
            ))
            .child(Self::save_state_note(
                self.bridge_address.read(cx).value().trim() != self.saved_settings.bridge.listen_address,
                language,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        action_button("save-eda-bridge")
                            .primary()
                            .label(language.choose("保存 Bridge 地址", "Save Bridge address"))
                            .on_click(move |_, _, cx| {
                                bridge_saver.update(cx, ControlPlaneView::save_bridge_settings);
                            }),
                    )
                    .child(
                        action_button("start-eda-bridge")
                            .disabled(is_running || is_starting)
                            .label(language.choose("启动服务", "Start service"))
                            .on_click(move |_, window, cx| {
                                bridge_starter.update(cx, |view, cx| {
                                    view.start_bridge_service(window, cx);
                                });
                            }),
                    )
                    .child(
                        action_button("stop-eda-bridge")
                            .disabled(!is_running)
                            .label(language.choose("停止服务", "Stop service"))
                            .on_click(move |_, _, cx| {
                                bridge_stopper
                                    .update(cx, ControlPlaneView::stop_bridge_service);
                            }),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("服务监测", "Service monitoring")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(status_dot(health.dot()))
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_normal()
                                    .child(health.label(language)),
                            ),
                    )
                    .when(externally_owned, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .whitespace_normal()
                                .child(language.choose(
                                    "端口可达，但服务不是由本应用启动的（例如手动启动的 bridge）；「停止服务」只作用于本应用启动的进程。",
                                    "The port answers, but the service was not started by this app (e.g. a manually launched bridge); Stop service only affects processes this app started.",
                                )),
                        )
                    })
                    .child(
                        div().flex().items_center().gap_2().child(
                            action_button("test-eda-bridge")
                                .disabled(is_test_pending)
                                .label(if is_test_pending {
                                    language.choose("测试中…", "Testing…")
                                } else {
                                    language.choose("测试连接", "Test connection")
                                })
                                .on_click(move |_, window, cx| {
                                    bridge_tester.update(cx, |view, cx| {
                                        view.test_bridge_connection(window, cx);
                                    });
                                }),
                        ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .whitespace_normal()
                            .child(test_line),
                    )
                    .when_some(projects_note, |this, (note, color)| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(rgb(color))
                                .whitespace_normal()
                                .child(note),
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .whitespace_normal()
                            .child(language.choose(
                                "连接测试会临时占用 bridge 的唯一 WebSocket 连接；若 EDA 插件正连接中，测试可能超时，这不代表服务离线。",
                                "The connection test temporarily occupies the bridge's single WebSocket connection; if the EDA plugin is attached the test may time out, which does not mean the service is offline.",
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(language.choose(
                                "已上报能力（连接测试成功后显示）",
                                "Reported capabilities (shown after a successful connection test)",
                            )),
                    )
                    .when(capabilities_reported, |this| this.child(capability_items))
                    .when(!capabilities_reported, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .whitespace_normal()
                                .child(language.choose(
                                    "能力尚未上报，不展示为可执行。",
                                    "No capabilities reported yet; nothing is shown as executable.",
                                )),
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .whitespace_normal()
                            .child(language.choose(
                                "最后回读：尚未上报（等待 bridge 协议支持回读结果上报）。",
                                "Last readback: not reported yet (pending bridge protocol support for readback reporting).",
                            )),
                    ),
            )
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_codex_runtime_detail(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let status = self.codex_status.clone();
        let is_running = self.codex_process.is_some();
        let is_starting = matches!(self.codex_status, RuntimeLifecycleStatus::Starting);
        let starter = entity.clone();
        let binding_saver = entity.clone();
        let stopper = entity;
        let launch_provider = circuitfabric_codex_runtime::execution::selected_provider(
            &self.saved_settings,
            circuitfabric_codex_runtime::execution::AgentKind::Codex,
        );
        let launch_provider_summary = match launch_provider {
            Some(provider) => {
                format!("`{}`（{} · {}）", provider.id, provider.name, provider.model)
            }
            None => language
                .choose(
                    "尚未配置（请先添加 Provider）",
                    "none configured yet (add a provider first)",
                )
                .to_owned(),
        };
        let active_provider_note = self
            .codex_active_provider
            .clone()
            .map(|provider| {
                format!("｜{}{provider}", language.choose("当前进程：", "current process: "))
            })
            .unwrap_or_default();
        let codex_dirty = self.runtime_dirty(RuntimeAdapter::CodexAppServer, cx);
        let codex_binding = self.codex_provider.read(cx).value().trim().to_owned();
        let codex_binding_display = if codex_binding.is_empty() {
            language.choose("默认 Provider", "Default provider").to_owned()
        } else {
            codex_binding
        };
        detail_pane("codex-detail")
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
                                    .child("Codex App Server"),
                            )
                            .child(
                                div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                    language.choose(
                                        "本地 stdio JSON-RPC 端点；由 CircuitFabric 以子进程方式启动与停止。",
                                        "Local stdio JSON-RPC endpoint; started and stopped as a CircuitFabric child process.",
                                    ),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(rgb(SURFACE_BG))
                            .child(status_dot(status.dot()))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(status.label(language)),
                            ),
                    ),
            )
            .child(
                Self::adapter_section_card(
                    "1",
                    language.choose("运行 · 启动与停止", "Run · start and stop"),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            action_button("start-codex-runtime")
                                .primary()
                                .disabled(is_running || is_starting || self.current_project_context().is_none())
                                .label(language.choose("启动", "Start"))
                                .on_click(move |_, window, cx| {
                                    starter.update(cx, |view, cx| {
                                        view.start_codex_runtime(window, cx);
                                    });
                                }),
                        )
                        .child(
                            action_button("stop-codex-runtime")
                                .disabled(!is_running && !is_starting && self.task_cancel.is_none())
                                .label(language.choose("停止", "Stop"))
                                .on_click(move |_, _, cx| {
                                    stopper.update(
                                        cx,
                                        ControlPlaneView::stop_codex_runtime,
                                    );
                                }),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(format!(
                            "{}{}{active_provider_note}",
                            language.choose(
                                "下次启动 Provider：",
                                "Next launch provider: "
                            ),
                            launch_provider_summary,
                        )),
                )
                .child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "启动使用当前项目根目录；请先在「项目」页打开项目。切换项目时清理旧进程，在新项目重新启动。退出应用时清理子进程。",
                            "Start uses the current project root; open a project first. Switching projects cleans up the old process; start again for the new project. Child processes are cleaned up on app exit.",
                        ),
                    ),
                )
                .child(settings_summary_row(
                    language.choose("项目根目录（自动）", "Project root (automatic)"),
                    self.current_project_context().map_or_else(
                        || language.choose("未选择项目，请先打开项目", "No project selected; open a project first").to_owned(),
                        |(_, root)| root.display().to_string(),
                    ),
                )),
            )
            .child(
                Self::adapter_section_card(
                    "2",
                    language.choose(
                        "设置 · 命令与 Provider 关联",
                        "Settings · command and provider binding",
                    ),
                )
                .child(settings_summary_row(
                    language.choose("命令", "Command"),
                    self.command.read(cx).value().to_string(),
                ))
                .child(settings_summary_row(
                    language.choose("Provider", "Provider"),
                    codex_binding_display,
                ))
                .child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "JLC bridge 监听地址属于 EDA 侧服务，在「EDA 服务」页配置。",
                            "The JLC bridge listen address belongs to the EDA side and is configured on the EDA services page.",
                        ),
                    ),
                )
                .child(Self::save_state_note(codex_dirty, language))
                .child(
                    action_button("open-codex-settings")
                        .primary()
                        .label(language.choose("编辑设置…", "Edit settings…"))
                        .on_click(move |_, _, cx| {
                            binding_saver.update(cx, |view, cx| {
                                view.adapter_settings_open =
                                    Some(RuntimeAdapter::CodexAppServer);
                                view.dialog_error = None;
                                cx.notify();
                            });
                        }),
                ),
            )
            .child(
                Self::adapter_section_card(
                    "3",
                    language.choose("快速验证 · 提交一个任务", "Quick check · run one task"),
                )
                .child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "用已保存的配置发起一次性任务，验证端到端链路（模型、密钥、工具授权）；结果只显示在下方，不会修改任何配置。",
                            "Run a one-off task with the saved configuration to verify the end-to-end path (model, secrets, tool grants); the result appears below and changes no configuration.",
                        ),
                    ),
                )
                .child(self.render_task_controls(RuntimeAdapter::CodexAppServer, cx)),
            )
    }

    pub(super) fn run_agent_task(
        &mut self,
        adapter: RuntimeAdapter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use circuitfabric_codex_runtime::execution::{AgentKind, Cancellation};
        if self.task_cancel.is_some() {
            return;
        }
        if self.navigation.selected_project.is_some() && self.current_project_context().is_none() {
            self.status = "未执行：当前项目根目录未加载，请重新打开项目。".into();
            cx.notify();
            return;
        }
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        let grants = self.effective_grants();
        let user_prompt = self.task_prompt.read(cx).value().to_string();
        let image_text = self.task_image.read(cx).value().to_string();
        let image = if adapter == RuntimeAdapter::CodexAppServer && !image_text.trim().is_empty() {
            Some(PathBuf::from(image_text))
        } else {
            None
        };
        let mut prompt = user_prompt.clone();
        if let Some(instructions) = self
            .navigation
            .selected_project
            .as_ref()
            .and_then(|id| self.workspace.configuration(id))
            .and_then(|c| c.agent_instructions.as_ref())
        {
            prompt = format!("{instructions}\n\n{prompt}");
        }
        let kind = match adapter {
            RuntimeAdapter::CodexAppServer => AgentKind::Codex,
            RuntimeAdapter::ClaudeCode => AgentKind::Claude,
            RuntimeAdapter::Dsh => AgentKind::Dsh,
        };
        let cancel = Cancellation::default();
        self.task_cancel = Some(cancel.clone());
        let project = self.navigation.selected_project.clone();
        // Project scope: the agent process works in the project root, and the run is
        // audited as a session record inside that project.
        let project_scope = project
            .as_ref()
            .and_then(|id| self.project_storages.get(id))
            .map(|storage| (storage.clone(), storage.root().to_path_buf()));
        let provider = circuitfabric_codex_runtime::execution::selected_provider(&settings, kind);
        let profile_id = provider.map_or_else(|| "default".to_owned(), |p| p.id.clone());
        let snapshot = provider
            .map_or_else(String::new, |p| format!("{} / {} / {}", p.id, p.model, p.base_url));
        self.task_result = format!(
            "正在调用 {}：{snapshot}。使用已保存快照；修改配置在下次任务生效。",
            adapter.label()
        );
        let session = project_scope.as_ref().and_then(|(storage, _)| {
            match crate::application::runtime_session::start(
                storage,
                &profile_id,
                adapter.backend_id(),
            ) {
                Ok(session_id) => Some((storage.clone(), session_id)),
                Err(error) => {
                    self.status = format!("任务继续执行，但项目会话记录创建失败：{error}");
                    None
                }
            }
        });
        self.task_identity = Some((
            project.clone(),
            session.as_ref().map(|(_, id)| id.clone()),
            adapter.backend_id().to_owned(),
        ));
        if let Some(project_id) = &project {
            if let Err(error) = self.refresh_project_data(project_id) {
                self.status = format!("任务已启动，但会话投影未刷新：{error}");
            }
        }
        let project_root = project_scope.as_ref().map(|(_, root)| root.clone());
        let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
        let work = cx.background_spawn(async move {
            let mut usage = None;
            let result = circuitfabric_codex_runtime::execution::run_task_observed(
                &settings,
                kind,
                &grants,
                &prompt,
                image.as_deref(),
                project_root.as_deref(),
                secrets.as_ref(),
                &cancel,
                &mut |_, _| {},
                &mut |reported| usage = Some(reported),
            );
            let persistence_error = session.as_ref().and_then(|(storage, session_id)| {
                let error_text = result.as_ref().err().map(ToString::to_string);
                let outcome =
                    result.as_deref().map_err(|_| error_text.as_deref().unwrap_or_default());
                crate::application::runtime_session::finish(
                    storage,
                    session_id,
                    &user_prompt,
                    outcome,
                    usage,
                )
                .err()
            });
            (result, persistence_error)
        });
        cx.spawn_in(window, async move |view, cx| {
            let (result, persistence_error) = work.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.task_cancel = None;
                    view.task_identity = None;
                    if view.navigation.selected_project == project {
                        view.task_result = match &result {
                            Ok(output) => format!("任务完成（{snapshot}）\n{output}"),
                            Err(error) => format!("任务未完成（{snapshot}）：{error}"),
                        };
                    }
                    if let Some(error) = persistence_error {
                        view.status = format!("项目会话记录未写入：{error}");
                    }
                    if let Some(project) = &project
                        && let Err(error) = view.refresh_project_data(project)
                    {
                        view.status = format!("项目会话列表未刷新：{error}");
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

    pub(super) fn render_task_controls(
        &mut self,
        adapter: RuntimeAdapter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let runner = cx.entity().clone();
        let stopper = runner.clone();
        let working_directory_note = self
            .navigation
            .selected_project
            .as_ref()
            .zip(
                self.navigation
                    .selected_project
                    .as_ref()
                    .and_then(|id| self.project_storages.get(id)),
            )
            .map_or_else(
                || "工作目录：隔离临时目录（未选择项目）".to_owned(),
                |(_, storage)| format!("工作目录（项目根目录）：{}", storage.root().display()),
            );
        div()
            .v_flex()
            .gap_2()
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(working_directory_note))
            .child(labeled_field("任务", "runtime-task", None, &self.task_prompt))
            .when(adapter == RuntimeAdapter::CodexAppServer, |panel| {
                panel.child(labeled_field("图片（可选）", "runtime-image", None, &self.task_image))
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        action_button("run-agent-task")
                            .label("执行任务")
                            .primary()
                            .disabled(self.task_cancel.is_some())
                            .on_click(move |_, window, cx| {
                                runner.update(cx, |view, cx| {
                                    view.run_agent_task(adapter, window, cx);
                                });
                            }),
                    )
                    .child(
                        action_button("cancel-agent-task")
                            .label("取消任务")
                            .disabled(self.task_cancel.is_none())
                            .on_click(move |_, _, cx| {
                                stopper.update(cx, |view, cx| {
                                    if let Some(cancel) = &view.task_cancel {
                                        cancel.cancel();
                                    }
                                    view.task_result = "正在取消并清理进程…".into();
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .id("runtime-task-result")
                    .v_flex()
                    .gap_0p5()
                    .max_h(px(320.))
                    .scroll_y()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .child(div().text_sm().whitespace_normal().child(self.task_result.clone())),
            )
    }

    /// The “run agents inside this project” block of the project's agent tab.
    pub(super) fn render_project_agents_section(
        &mut self,
        project: &Project,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let adapter = self.project_agent_adapter;
        let mut selector = div().flex().gap_2().flex_wrap();
        for candidate in
            [RuntimeAdapter::CodexAppServer, RuntimeAdapter::ClaudeCode, RuntimeAdapter::Dsh]
        {
            let chooser = entity.clone();
            selector = selector.child(
                action_button(format!("project-agent-adapter-{}", candidate.backend_id()))
                    .label(candidate.label())
                    .when(candidate == adapter, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            view.project_agent_adapter = candidate;
                            cx.notify();
                        });
                    }),
            );
        }
        let root_note = self.project_storages.get(&project.id).map_or_else(
            || "项目根目录未打开，无法在项目内运行。".to_owned(),
            |storage| format!("任务工作目录（项目根目录）：{}", storage.root().display()),
        );
        div()
            .v_flex()
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(
                div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                    language.choose("在项目中运行智能体", "Run agents in this project"),
                ),
            )
            .child(selector)
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(root_note))
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                language.choose(
                    "智能体进程以项目根目录为工作目录；提示词自动附加项目级说明；工具授权取全局与项目 allowlist 的交集，项目不能扩大全局授权。每次运行都会写入项目会话记录，可在“会话”标签回放。",
                    "Agent processes run with the project root as their working directory; project instructions are prepended to the prompt; tool grants are the intersection of global and project allowlists, so a project never expands global authorization. Every run is recorded as a project session, replayable in the Sessions tab.",
                ),
            ))
            .child(self.render_task_controls(adapter, cx))
    }
}
