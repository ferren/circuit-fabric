//! Jev presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn persist_catalog(
        &mut self,
        previous: circuitfabric_codex_runtime::tools::ToolCatalog,
        cx: &mut Context<Self>,
    ) -> bool {
        let saved = match self.save_update(
            crate::application::settings_persistence::SettingsUpdate::Catalog(self.catalog.clone()),
        ) {
            Ok(()) => {
                if let Some(cancel) = &self.task_cancel {
                    cancel.cancel();
                }
                self.status = "已保存定义；当前任务已请求取消，下次任务使用新配置".into();
                true
            }
            Err(error) => {
                self.catalog = previous;
                self.status = format!("未保存：{error}");
                false
            }
        };
        cx.notify();
        saved
    }

    /// Enable/disable switch for the bundled Jev server definition.
    pub(super) fn toggle_jev_enabled(&mut self, cx: &mut Context<Self>) {
        let previous = self.catalog.clone();
        if let Some(server) =
            self.catalog.mcp_servers.iter_mut().find(|server| server.id == BUNDLED_JEV_SERVER_ID)
        {
            server.enabled = !server.enabled;
        }
        self.persist_catalog(previous, cx);
    }

    /// Global-scope grant for the bundled Jev server, mirroring
    /// `authorize_tool`/`revoke_tool` for one fixed id.
    pub(super) fn set_jev_authorization(&mut self, authorized: bool, cx: &mut Context<Self>) {
        let previous = self.tool_authorizations.clone();
        {
            let list = &mut self.tool_authorizations.authorized_mcp_server_ids;
            if authorized {
                if !list.iter().any(|id| id == BUNDLED_JEV_SERVER_ID) {
                    list.push(BUNDLED_JEV_SERVER_ID.to_owned());
                    list.sort();
                }
            } else {
                list.retain(|id| id != BUNDLED_JEV_SERVER_ID);
            }
        }
        match self.save_update(
            crate::application::settings_persistence::SettingsUpdate::Authorizations(
                self.tool_authorizations.clone(),
            ),
        ) {
            Ok(()) => {
                if let Some(cancel) = &self.task_cancel {
                    cancel.cancel();
                }
                self.status = if authorized {
                    format!("已全局授权 {BUNDLED_JEV_SERVER_ID}；当前任务已请求取消。")
                } else {
                    format!("已撤销 {BUNDLED_JEV_SERVER_ID} 的全局授权；当前任务已请求取消。")
                };
            }
            Err(error) => {
                self.tool_authorizations = previous;
                self.status = format!("未保存：{error}");
            }
        }
        cx.notify();
    }

    pub(super) fn set_jev_project_authorization(
        &mut self,
        authorized: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self.navigation.selected_project.clone() else {
            self.status = "请先选择项目。".to_owned();
            cx.notify();
            return;
        };
        let Some(storage) = self.project_storages.get(&project_id) else {
            self.status = "当前项目尚未打开。".to_owned();
            cx.notify();
            return;
        };
        let mut configuration =
            self.workspace.configuration(&project_id).cloned().unwrap_or_default();
        let list = &mut configuration.enabled_mcp_server_ids;
        if authorized {
            if !list.iter().any(|id| id == BUNDLED_JEV_SERVER_ID) {
                list.push(BUNDLED_JEV_SERVER_ID.to_owned());
                list.sort();
            }
        } else {
            list.retain(|id| id != BUNDLED_JEV_SERVER_ID);
        }
        match storage.save_configuration(&configuration) {
            Ok(()) => match self.workspace.set_configuration(&project_id, configuration) {
                Ok(()) => {
                    self.invalidate_tool_runs();
                    self.status = format!(
                        "项目 `{project_id}` Jev 授权已{}。",
                        if authorized { "开启" } else { "撤销" }
                    )
                }
                Err(error) => self.status = format!("项目配置已保存，但内存未刷新：{error}"),
            },
            Err(error) => self.status = format!("项目 Jev 授权未保存：{error}"),
        }
        cx.notify();
    }

    /// Store `TYPESAFE_API_KEY` into the unlocked vault; the value never
    /// lands in configuration files.
    /// Store the active backend's key variable into the unlocked vault;
    /// returns whether the value was saved (so dialogs can close).
    pub(super) fn save_jev_api_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let name = self
            .catalog
            .mcp_servers
            .iter()
            .find(|server| server.id == circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID)
            .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
            .map_or_else(
                || "TYPESAFE_API_KEY".to_owned(),
                |settings| settings.api_key_environment_variable,
            );
        let value = self.jev_api_key.read(cx).value().trim().to_owned();
        if value.is_empty() {
            self.status = format!("未保存：请粘贴 {name} 的值。");
            cx.notify();
            return false;
        }
        let Some(vault) = self.vault.as_mut() else {
            "未保存：请先在「密钥保险库」页解锁保险库，或改用同名环境变量。"
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let saved = match vault.set(&name, &value) {
            Ok(()) => {
                self.vault_index = vault.values().names().cloned().collect();
                self.jev_api_key.update(cx, |state, cx| state.set_value("", window, cx));
                self.status = format!("已保存 {name}（值已加密写入保险库）。");
                true
            }
            Err(error) => {
                self.status = format!("未保存：{error}");
                false
            }
        };
        cx.notify();
        saved
    }

    /// Re-run bundled binary detection after the catalog entry was removed
    /// or the binary was built after the app started.
    pub(super) fn reregister_jev(&mut self, cx: &mut Context<Self>) {
        let previous = self.catalog.clone();
        self.catalog.ensure_bundled();
        if self.catalog.mcp_servers.iter().any(|server| server.id == BUNDLED_JEV_SERVER_ID) {
            self.persist_catalog(previous, cx);
        } else {
            concat!(
                "未找到 evaluate 二进制：请先运行 scripts/build-typesafe-mcp.ps1，",
                "或设置 CIRCUITFABRIC_TYPESAFE_MCP_PATH 指向它。"
            )
            .clone_into(&mut self.status);
            cx.notify();
        }
    }

    pub(super) fn jev_settings_from_form(
        &self,
        cx: &Context<Self>,
    ) -> Result<circuitfabric_codex_runtime::judge::LlmJudgeSettings, String> {
        let mut settings = self.jev_llm_settings.clone();
        settings.base_url = self.jev_base_url.read(cx).value().trim().to_owned();
        settings.model = self.jev_model.read(cx).value().trim().to_owned();
        settings.api_key_environment_variable = self.jev_key_env.read(cx).value().trim().to_owned();
        settings.timeout_seconds = self
            .jev_timeout
            .read(cx)
            .value()
            .parse()
            .map_err(|_| "超时必须为 5～180 秒".to_owned())?;
        settings.malformed_retries = self
            .jev_retries
            .read(cx)
            .value()
            .parse()
            .map_err(|_| "结构修正重试必须为 0～3 次".to_owned())?;
        settings.validate().map_err(|error| error.to_string())?;
        Ok(settings)
    }

    /// Persist the chosen backend branch; returns whether it was saved
    /// (so the configuration dialog can close on success only).
    pub(super) fn apply_jev_backend(&mut self, llm: bool, cx: &mut Context<Self>) -> bool {
        use circuitfabric_codex_runtime::tools::{BUNDLED_JEV_SERVER_ID, bundled_mcp_servers};
        let result = if llm {
            self.jev_settings_from_form(cx).and_then(|settings| {
                std::env::current_exe().map_err(|error| error.to_string()).and_then(|path| {
                    settings.server(&path, true).map_err(|error| error.to_string())
                })
            })
        } else {
            bundled_mcp_servers().into_iter().find(|s| s.id == BUNDLED_JEV_SERVER_ID)
                .ok_or_else(|| "未找到 TypeSafe evaluate 二进制，请先构建或设置 CIRCUITFABRIC_TYPESAFE_MCP_PATH".to_owned())
        };
        match result {
            Ok(mut server) => {
                let previous = self.catalog.clone();
                if let Some(settings) =
                    circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server(&server)
                {
                    self.catalog.llm_judge = Some(settings);
                }
                if let Some(existing) =
                    self.catalog.mcp_servers.iter().find(|s| s.id == BUNDLED_JEV_SERVER_ID)
                {
                    server.enabled = existing.enabled;
                }
                self.catalog.mcp_servers.retain(|s| s.id != BUNDLED_JEV_SERVER_ID);
                self.catalog.mcp_servers.push(server);
                if self.persist_catalog(previous, cx) {
                    self.jev_llm_draft = llm;
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                self.status = format!("判断后端未保存：{error}");
                cx.notify();
                false
            }
        }
    }

    /// Compact status chip for the overview row: green while `good` is
    /// `Some(true)`, red while `Some(false)`, neutral gray when `None`.
    pub(super) fn jev_chip(
        id: &'static str,
        label: String,
        good: Option<bool>,
    ) -> impl IntoElement {
        let (background, foreground) = match good {
            Some(true) => (0x00dc_fce7, 0x0016_a34a),
            Some(false) => (0x00fe_e2e2, 0x00b9_1c1c),
            None => (SURFACE_BG, TEXT_MUTED),
        };
        div()
            .id(id)
            .px_2()
            .py_0p5()
            .rounded_sm()
            .text_xs()
            .bg(rgb(background))
            .text_color(rgb(foreground))
            .child(label)
    }

    /// Numbered section heading used across the Jev settings page.
    pub(super) fn jev_section_header(
        zh: &'static str,
        en: &'static str,
        language: UiLanguage,
    ) -> Div {
        div().text_sm().font_weight(FontWeight::MEDIUM).child(language.choose(zh, en))
    }

    /// Fixed-width caption for one segmented mode row.
    pub(super) fn jev_mode_label(label: &'static str) -> Div {
        div().w(px(84.)).flex_none().text_sm().text_color(rgb(TEXT_SECONDARY)).child(label)
    }

    /// One option of an in-place segmented control; `active` marks the
    /// selected branch and `apply` runs when the option is clicked.
    pub(super) fn jev_option(
        id: &'static str,
        label: &'static str,
        active: bool,
        entity: &Entity<Self>,
        apply: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let entity = entity.clone();
        div()
            .id(id)
            .self_start()
            .flex_none()
            .px_3()
            .py_1p5()
            .rounded_md()
            .border_1()
            .border_color(rgb(if active { ACCENT } else { BORDER }))
            .bg(rgb(if active { 0x00f0_f9ff } else { CARD_BG }))
            .text_sm()
            .text_color(rgb(if active { TEXT_PRIMARY } else { TEXT_MUTED }))
            .cursor_pointer()
            .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
            .child(label)
            .on_click(move |_, _, cx| {
                entity.update(cx, |view, cx| {
                    apply(view, cx);
                    cx.notify();
                });
            })
    }

    /// Free MCP-level check: initialize + tools/list, no billable call.
    pub(super) fn jev_discover_button(
        language: UiLanguage,
        entity: &Entity<Self>,
    ) -> impl IntoElement {
        let tester = entity.clone();
        action_button("jev-test")
            .label(language.choose("连接并发现工具（不计费）", "Connect and discover tools (free)"))
            .on_click(move |_, window, cx| {
                tester.update(cx, |view, cx| {
                    let catalog = view.catalog.clone();
                    let grants = view.effective_grants();
                    let secrets = view.vault.as_ref().map(|vault| vault.values().clone());
                    let id = BUNDLED_JEV_SERVER_ID.to_owned();
                    view.status = "正在连接 MCP…".into();
                    let work = cx.background_spawn(async move {
                        catalog
                            .list_tools_with_secrets(&id, &grants, secrets.as_ref())
                            .map(|value| value.to_string())
                            .map_err(|error| error.to_string())
                    });
                    cx.spawn_in(window, async move |view, cx| {
                        let result = work.await;
                        cx.update(|_, cx| {
                            view.update(cx, |view, cx| {
                                view.status = match result {
                                    Ok(tools) => format!("MCP 已连接，工具：{tools}"),
                                    Err(error) => format!("MCP 连接失败：{error}"),
                                };
                                cx.notify();
                            })
                            .ok();
                        })
                        .ok();
                    })
                    .detach();
                    cx.notify();
                });
            })
    }

    /// Billable end-to-end judgment check through the saved backend.
    pub(super) fn jev_real_test_button(
        language: UiLanguage,
        entity: &Entity<Self>,
    ) -> impl IntoElement {
        let tester = entity.clone();
        action_button("jev-real-test")
            .label(language.choose("测试真实判断（消耗额度）", "Run a real judgment (billed)"))
            .on_click(move |_, window, cx| {
                tester.update(cx, |view, cx| {
                    let catalog = view.catalog.clone();
                    let grants = view.effective_grants();
                    let secrets = view.vault.as_ref().map(|vault| vault.values().clone());
                    view.status = "正在测试已保存的判断后端（会调用模型）…".into();
                    let work = cx.background_spawn(async move {
                        catalog
                            .call_tool_with_secrets(
                                circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID,
                                &grants,
                                "evaluate",
                                &serde_json::json!({"state":"The LED is on.","questions":{"led_on":{"type":"noul","instructions":"Does the evidence state that the LED is on?"}}}),
                                secrets.as_ref(),
                            )
                            .map_err(|error| error.to_string())
                            .and_then(|value| {
                                if value["isError"] == true {
                                    Err(value["content"][0]["text"]
                                        .as_str()
                                        .unwrap_or("判断失败")
                                        .to_owned())
                                } else {
                                    Ok(value.to_string())
                                }
                            })
                    });
                    cx.spawn_in(window, async move |view, cx| {
                        let result = work.await;
                        cx.update(|_, cx| {
                            view.update(cx, |view, cx| {
                                view.status = match result {
                                    Ok(value) => format!("判断测试成功：{value}"),
                                    Err(error) => format!("判断测试失败：{error}"),
                                };
                                cx.notify();
                            })
                            .ok();
                        })
                        .ok();
                    })
                    .detach();
                    cx.notify();
                });
            })
    }

    /// Third-party-LLM form body used inside the backend dialog: provider
    /// presets, endpoint and answer semantics. No apply button — the
    /// dialog footer owns 保存/取消.
    pub(super) fn render_jev_llm_configuration(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use circuitfabric_codex_runtime::judge::{AnswerMode, OutputFormat};
        let entity = cx.entity().clone();
        let language = self.language;
        let presets = div().flex().flex_wrap().gap_2().children(
            [
                (
                    "deepseek",
                    language.choose("DeepSeek", "DeepSeek").to_owned(),
                    "https://api.deepseek.com/v1",
                    "deepseek-flash",
                    "DEEPSEEK_API_KEY",
                ),
                (
                    "zai-coding",
                    language.choose("z.ai 编程包", "z.ai Coding Plan").to_owned(),
                    "https://api.z.ai/api/coding/paas/v4",
                    "glm-5.3-flash",
                    "ZAI_API_KEY",
                ),
                (
                    "zai-standard",
                    language.choose("z.ai 按量付费", "z.ai Pay-as-you-go").to_owned(),
                    "https://api.z.ai/api/paas/v4",
                    "glm-4.7",
                    "ZAI_API_KEY",
                ),
            ]
            .into_iter()
            .map(|(id, label, url, model, key)| {
                let selector = entity.clone();
                action_button(id).label(label).on_click(move |_, window, cx| {
                    selector.update(cx, |view, cx| {
                        view.jev_base_url.update(cx, |s, cx| s.set_value(url, window, cx));
                        view.jev_model.update(cx, |s, cx| s.set_value(model, window, cx));
                        view.jev_key_env.update(cx, |s, cx| s.set_value(key, window, cx));
                        view.jev_llm_settings.output_format = OutputFormat::JsonObject;
                        view.status = "已填入预设，可按账户修改模型；点击「保存」生效。".into();
                        cx.notify();
                    });
                })
            }),
        );
        div().v_flex().gap_3()
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "服务商预设：",
                    "Provider presets:",
                )))
                .child(presets))
            .child(labeled_field(
                "Base URL",
                "jev-llm-url",
                Some(language.choose(
                    "兼容 OpenAI Chat Completions，自动追加 /chat/completions",
                    "OpenAI Chat Completions compatible; /chat/completions is appended",
                )),
                &self.jev_base_url,
            ))
            .child(labeled_field(language.choose("判断模型", "Judge model"), "jev-llm-model", None, &self.jev_model))
            .child(labeled_field(
                language.choose("密钥变量名", "API key variable"),
                "jev-llm-env",
                Some(language.choose(
                    "其值在主列表「API 密钥」行更换",
                    "change its value via the “API key” row of the main list",
                )),
                &self.jev_key_env,
            ))
            .child(div().flex().gap_3()
                .child(labeled_field(language.choose("单次请求超时（秒）", "Request timeout (s)"), "jev-llm-timeout", None, &self.jev_timeout))
                .child(labeled_field(language.choose("结构修正重试", "Malformed retries"), "jev-llm-retries", None, &self.jev_retries)))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(Self::jev_mode_label(language.choose("回答模式", "Answer mode")))
                .child(Self::jev_option("jev-mode-probabilities",
                    language.choose("概率分布", "Probabilities"),
                    self.jev_llm_settings.answer_mode == AnswerMode::Probabilities,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.answer_mode = AnswerMode::Probabilities; }))
                .child(Self::jev_option("jev-mode-discrete",
                    language.choose("离散值", "Discrete"),
                    self.jev_llm_settings.answer_mode == AnswerMode::Discrete,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.answer_mode = AnswerMode::Discrete; })))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(Self::jev_mode_label(language.choose("输出格式", "Output format")))
                .child(Self::jev_option("jev-format-object", "JSON Object",
                    self.jev_llm_settings.output_format == OutputFormat::JsonObject,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.output_format = OutputFormat::JsonObject; }))
                .child(Self::jev_option("jev-format-schema", "JSON Schema",
                    self.jev_llm_settings.output_format == OutputFormat::JsonSchema,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.output_format = OutputFormat::JsonSchema; }))
                .child(Self::jev_option("jev-format-prompted", "Prompted JSON",
                    self.jev_llm_settings.output_format == OutputFormat::Prompted,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.output_format = OutputFormat::Prompted; })))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(Self::jev_mode_label(language.choose("概率归一化", "Normalize")))
                .child(Self::jev_option("jev-normalize-on",
                    language.choose("开", "On"),
                    self.jev_llm_settings.normalize_probabilities,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.normalize_probabilities = true; }))
                .child(Self::jev_option("jev-normalize-off",
                    language.choose("关", "Off"),
                    !self.jev_llm_settings.normalize_probabilities,
                    &entity,
                    move |view, _cx| { view.jev_llm_settings.normalize_probabilities = false; })))
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "概率是 LLM 估计、未经校准；离散模式的 0/1 只表示选择。JSON Schema 需服务商支持，不支持时可选 Prompted JSON（仍会严格校验）。",
                "Probabilities are uncalibrated LLM estimates; discrete 0/1 only encodes a selection. JSON Schema needs provider support; otherwise choose Prompted JSON (still strictly validated).")))
    }

    /// The bundled Jev judgment tool page: a read-mostly settings list
    /// where every row shows the saved value, a dialog for the two
    /// multi-field edits (backend, key), and immediate toggles for the
    /// gates. Saved state is always visible — no inline save buttons.
    pub(super) fn render_bundled_jev_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let server = self
            .catalog
            .mcp_servers
            .iter()
            .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
            .cloned();
        let enabled = server.as_ref().is_some_and(|server| server.enabled);
        let authorized = self
            .tool_authorizations
            .authorized_mcp_server_ids
            .iter()
            .any(|id| id == BUNDLED_JEV_SERVER_ID);
        let has_project = self.navigation.selected_project.is_some();
        let project_authorized = self
            .navigation
            .selected_project
            .as_ref()
            .and_then(|id| self.workspace.configuration(id))
            .is_some_and(|configuration| {
                configuration.enabled_mcp_server_ids.iter().any(|id| id == BUNDLED_JEV_SERVER_ID)
            });
        let llm_settings = server
            .as_ref()
            .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server);
        let saved_llm = llm_settings.is_some();
        let key_name = llm_settings
            .as_ref()
            .map_or("TYPESAFE_API_KEY", |s| s.api_key_environment_variable.as_str())
            .to_owned();
        let key_source = secret_source(&key_name, self.vault.as_ref().map(UnlockedVault::values));
        let key_present = key_source.is_some();
        let definition_line = server.as_ref().map_or_else(String::new, |server| {
            if server.args.is_empty() {
                server.command.clone()
            } else {
                format!("{} {}", server.command, server.args.join(" "))
            }
        });

        // At-a-glance state so the list below can be read as a checklist.
        let backend_chip = llm_settings.as_ref().map_or_else(
            || language.choose("后端：TypeSafe Jev", "Backend: TypeSafe Jev").to_owned(),
            |s| {
                format!(
                    "{}：{} · {}",
                    language.choose("后端：第三方 LLM", "Backend: third-party LLM"),
                    s.model,
                    s.base_url
                )
            },
        );
        let overview = div()
            .v_flex()
            .gap_2()
            .child(Self::jev_section_header("状态总览", "Overview", language))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(Self::jev_chip("jev-chip-backend", backend_chip, None))
                    .child(Self::jev_chip(
                        "jev-chip-enabled",
                        language.choose("已启用", "enabled").to_owned(),
                        enabled.then_some(true),
                    ))
                    .child(Self::jev_chip(
                        "jev-chip-grant",
                        language
                            .choose(
                                if authorized {
                                    "全局授权：已授权"
                                } else {
                                    "全局授权：未授权"
                                },
                                if authorized { "global grant: on" } else { "global grant: off" },
                            )
                            .to_owned(),
                        Some(authorized),
                    ))
                    .child(Self::jev_chip(
                        "jev-chip-project-grant",
                        if has_project {
                            language
                                .choose(
                                    if project_authorized {
                                        "当前项目：已授权"
                                    } else {
                                        "当前项目：未授权"
                                    },
                                    if project_authorized {
                                        "project grant: on"
                                    } else {
                                        "project grant: off"
                                    },
                                )
                                .to_owned()
                        } else {
                            language
                                .choose("项目授权：未选择项目", "project grant: no project")
                                .to_owned()
                        },
                        if has_project { Some(project_authorized) } else { None },
                    ))
                    .child(Self::jev_chip(
                        "jev-chip-key",
                        format!("{} {key_name}", if key_present { "✔" } else { "✘" }),
                        Some(key_present),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(format!("{BUNDLED_JEV_SERVER_ID} · {definition_line}")),
            );

        // The settings list: value = what is saved right now.
        let backend_value = llm_settings.as_ref().map_or_else(
            || {
                language
                    .choose("TypeSafe Jev（随附二进制）", "TypeSafe Jev (bundled binary)")
                    .to_owned()
            },
            |s| {
                format!(
                    "{} · {} · {}",
                    language.choose("第三方 LLM", "third-party LLM"),
                    s.model,
                    s.base_url
                )
            },
        );
        let backend_edit = entity.clone();
        let key_edit = entity.clone();
        let toggler = entity.clone();
        let grant_toggle = entity.clone();
        let project_grant_toggle = entity.clone();
        let backend_row = Self::jev_setting_row(
            "jev-row-backend",
            language.choose("判断后端", "Backend"),
            backend_value,
            None,
            action_button("jev-edit-backend").label(language.choose("编辑", "Edit")).on_click(
                move |_, window, cx| {
                    backend_edit.update(cx, |view, cx| {
                        view.jev_llm_draft = saved_llm;
                        view.reset_jev_form_to_saved(window, cx);
                        view.jev_backend_modal_open = true;
                        cx.notify();
                    });
                },
            ),
        );
        let key_row = Self::jev_setting_row(
            "jev-row-key",
            language.choose("API 密钥", "API key"),
            format!(
                "{} {key_name} · {}",
                if key_present { "✔" } else { "✘" },
                match key_source {
                    Some(SecretSource::Vault) => language.choose("保险库", "vault"),
                    Some(SecretSource::Environment) => {
                        language.choose("环境变量", "environment")
                    }
                    None => language.choose("未找到", "not found"),
                },
            ),
            Some(key_present),
            action_button("jev-edit-key").label(language.choose("更换", "Replace")).on_click(
                move |_, window, cx| {
                    key_edit.update(cx, |view, cx| {
                        view.jev_api_key.update(cx, |state, cx| {
                            state.set_value("", window, cx);
                        });
                        view.jev_key_modal_open = true;
                        cx.notify();
                    });
                },
            ),
        );
        let enable_row = Self::jev_setting_row(
            "jev-row-enabled",
            language.choose("启用状态", "Enabled"),
            language
                .choose(
                    if enabled { "已启用" } else { "已停用" },
                    if enabled { "enabled" } else { "disabled" },
                )
                .to_owned(),
            enabled.then_some(true),
            action_button("jev-toggle-enabled")
                .label(language.choose(
                    if enabled { "停用" } else { "启用" },
                    if enabled { "Disable" } else { "Enable" },
                ))
                .on_click(move |_, _, cx| {
                    toggler.update(cx, |view, cx| {
                        view.toggle_jev_enabled(cx);
                    });
                }),
        );
        let grant_row = Self::jev_setting_row(
            "jev-row-grant",
            language.choose("全局授权", "Global grant"),
            language
                .choose(
                    if authorized { "已授权" } else { "未授权" },
                    if authorized { "authorized" } else { "not authorized" },
                )
                .to_owned(),
            Some(authorized),
            action_button("jev-toggle-authorization")
                .label(language.choose(
                    if authorized { "撤销" } else { "授权" },
                    if authorized { "Revoke" } else { "Authorize" },
                ))
                .on_click(move |_, _, cx| {
                    grant_toggle.update(cx, |view, cx| {
                        view.set_jev_authorization(!authorized, cx);
                    });
                }),
        );
        let project_row = Self::jev_setting_row(
            "jev-row-project-grant",
            language.choose("项目授权", "Project grant"),
            if has_project {
                language
                    .choose(
                        if project_authorized {
                            "当前项目：已授权"
                        } else {
                            "当前项目：未授权"
                        },
                        if project_authorized {
                            "current project: authorized"
                        } else {
                            "current project: not authorized"
                        },
                    )
                    .to_owned()
            } else {
                language.choose("请先在「项目」页选择项目", "select a project first").to_owned()
            },
            if has_project { Some(project_authorized) } else { None },
            action_button("jev-toggle-project-authorization")
                .disabled(!has_project)
                .label(language.choose(
                    if project_authorized { "撤销" } else { "授权当前项目" },
                    if project_authorized { "Revoke" } else { "Authorize project" },
                ))
                .on_click(move |_, _, cx| {
                    project_grant_toggle.update(cx, |view, cx| {
                        view.set_jev_project_authorization(!project_authorized, cx);
                    });
                }),
        );
        let settings_list = div()
            .v_flex()
            .gap_2()
            .child(Self::jev_section_header("配置", "Settings", language))
            .child(backend_row)
            .child(key_row)
            .child(enable_row)
            .child(grant_row)
            .child(project_row)
            .child(
                div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "「判断后端」「API 密钥」在弹窗中修改，点「保存」后立即生效并写回 runtime.json；启用与授权点击后即刻生效（会取消进行中的任务）。列表显示的永远是已保存的值。",
                    "“Backend” and “API key” are edited in dialogs and applied on 保存 (persisted to runtime.json immediately); enable/grant toggles apply instantly (canceling the running task). The list always shows the saved values.",
                )),
            );

        // Verification: free discovery first, then one billed judgment.
        let verify_section = div()
            .v_flex()
            .gap_2()
            .child(Self::jev_section_header("连接与测试", "Connect & test", language))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(Self::jev_discover_button(language, &entity))
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "仅握手与列出工具，不计费。",
                            "Handshake and tools/list only; no billable call.",
                        ),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(Self::jev_real_test_button(language, &entity))
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "通过已保存的后端真实调用一次判断，结果见底部状态栏。",
                            "One real judgment through the saved backend; the result lands in the status bar.",
                        ),
                    )),
            );

        let notes_section = div()
            .v_flex()
            .gap_1()
            .child(Self::jev_section_header("使用要点", "Usage notes", language))
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "evaluate 返回类型化判断：noul（0~1 是/否概率）、choice（多选一+概率分布）、score（量表评分）；接近 0.5 表示不确定而非中等。",
                "evaluate returns typed judgments: noul (0~1 yes/no probability), choice (one option + distribution), score (rubric scale); near 0.5 means uncertain, not medium.",
            )))
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "阈值判断（例如 noul > 0.8 才放行）应写在工作流代码里，而不是依赖模型自觉。",
                "Threshold decisions (e.g. proceed only when noul > 0.8) belong in workflow code, not in the model's discretion.",
            )));

        let content = if server.is_some() {
            div()
                .v_flex()
                .gap_4()
                .child(overview)
                .child(settings_list)
                .child(verify_section)
                .child(notes_section)
                .into_any_element()
        } else {
            let register = entity.clone();
            div()
                .v_flex()
                .gap_3()
                .child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "尚未注册：未检测到随附的 evaluate 二进制。构建后会自动注册，也可以手动重新检测。",
                    "Not registered yet: the bundled evaluate binary was not detected. It registers automatically once built; you can also re-detect manually.",
                )))
                .child(
                    action_button("jev-reregister")
                        .label(language.choose("重新检测并注册", "Re-detect and register"))
                        .on_click(move |_, _, cx| {
                            register.update(cx, |view, cx| {
                                view.reregister_jev(cx);
                            });
                        }),
                )
                .into_any_element()
        };

        div()
            .id("jev-settings-detail")
            .scroll_y()
            .min_h(px(0.))
            .flex_1()
            .min_w(px(0.))
            .v_flex()
            .gap_4()
            .p_5()
            .rounded_xl()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                        language.choose("Jev 判断工具", "Jev judgment tool"),
                    ))
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                        language.choose(
                            "各配置项以列表呈现，列表值始终等于已保存状态；「判断后端」与「API 密钥」通过弹窗修改。evaluate 始终以相同接口返回类型化判断，供智能体和数据手册复核调用。",
                            "Settings appear as a list whose values always mirror what is saved; “backend” and “API key” are edited through dialogs. evaluate keeps one interface returning typed judgments for agents and datasheet review.",
                        ),
                    )),
            )
            .child(content)
    }

    /// One row of the Jev settings list: label, the saved value (always
    /// visible so save state is unambiguous), and the row action.
    pub(super) fn jev_setting_row(
        id: &'static str,
        label: &str,
        value: String,
        value_positive: Option<bool>,
        action: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_3()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(
                div()
                    .w(px(96.))
                    .flex_none()
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(label.to_owned()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_sm()
                    .whitespace_normal()
                    .text_color(rgb(match value_positive {
                        Some(true) => 0x0016_a34a,
                        Some(false) => 0x00dc_2626,
                        None => TEXT_SECONDARY,
                    }))
                    .child(value),
            )
            .child(action)
    }

    /// Reset the LLM form inputs to the saved configuration so a canceled
    /// dialog session never leaks its edits into the next one.
    pub(super) fn reset_jev_form_to_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = self
            .catalog
            .mcp_servers
            .iter()
            .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
            .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
            .or_else(|| self.catalog.llm_judge.clone())
            .unwrap_or_default();
        self.jev_llm_settings = saved.clone();
        self.jev_base_url
            .update(cx, |state, cx| state.set_value(saved.base_url.as_str(), window, cx));
        self.jev_model.update(cx, |state, cx| state.set_value(saved.model.as_str(), window, cx));
        self.jev_key_env.update(cx, |state, cx| {
            state.set_value(saved.api_key_environment_variable.as_str(), window, cx);
        });
        self.jev_timeout.update(cx, |state, cx| {
            state.set_value(saved.timeout_seconds.to_string().as_str(), window, cx);
        });
        self.jev_retries.update(cx, |state, cx| {
            state.set_value(saved.malformed_retries.to_string().as_str(), window, cx);
        });
    }

    /// Backend configuration dialog: pick a branch, edit the LLM form,
    /// 保存 applies it to the catalog; 取消 keeps the saved backend.
    pub(super) fn render_jev_backend_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let draft_llm = self.jev_llm_draft;
        let typesafe_ready = circuitfabric_codex_runtime::tools::bundled_mcp_servers()
            .into_iter()
            .find(|s| s.id == BUNDLED_JEV_SERVER_ID)
            .is_some_and(|s| std::path::Path::new(&s.command).is_file());
        let llm_form = self.render_jev_llm_configuration(cx).into_any_element();
        let cancel = entity.clone();
        let save = entity.clone();
        // Failure messages of the two save handlers, shown inside the dialog.
        let error_line = (self.status.starts_with("未保存")
            || self.status.starts_with("判断后端未保存"))
        .then(|| self.status.clone());
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("jev-backend-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00_0f17_2ab3))
                    .occlude(),
            )
            .child(
                div()
                    .id("jev-backend-modal")
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
                        div().v_flex().gap_1()
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(language.choose("配置判断后端", "Configure the judgment backend")))
                            .child(div().text_sm().whitespace_normal().text_color(rgb(TEXT_SECONDARY))
                                .child(language.choose(
                                    "选择 TypeSafe Jev 或第三方 LLM；修改仅在点击「保存」后生效，取消则保持现状。",
                                    "Choose TypeSafe Jev or a third-party LLM; changes apply only on 保存 — cancel keeps the current setup.",
                                ))),
                    )
                    .child(
                        capped_scroll_body("jev-backend-form-area", px(440.))
                            .v_flex()
                            .gap_3()
                            .child(
                                div().flex().flex_wrap().gap_2()
                                    .child(Self::jev_option(
                                        "jev-select-typesafe",
                                        language.choose("TypeSafe Jev（随附二进制）", "TypeSafe Jev (bundled)"),
                                        !draft_llm,
                                        &entity,
                                        move |view, _cx| {
                                            view.jev_llm_draft = false;
                                        },
                                    ))
                                    .child(Self::jev_option(
                                        "jev-select-llm",
                                        language.choose("第三方 LLM 模拟", "Third-party LLM"),
                                        draft_llm,
                                        &entity,
                                        move |view, _cx| {
                                            view.jev_llm_draft = true;
                                        },
                                    )),
                            )
                            .child(if draft_llm {
                                llm_form
                            } else {
                                div().v_flex().gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(if typesafe_ready { 0x0016_a34a } else { 0x00dc_2626 }))
                                            .child(if typesafe_ready {
                                                language.choose(
                                                    "✔ 已找到随附的 evaluate 二进制",
                                                    "✔ Bundled evaluate binary found",
                                                )
                                            } else {
                                                language.choose(
                                                    "✘ 未找到二进制：请运行 scripts/build-typesafe-mcp.ps1，或设置 CIRCUITFABRIC_TYPESAFE_MCP_PATH",
                                                    "✘ Binary not found: run scripts/build-typesafe-mcp.ps1 or set CIRCUITFABRIC_TYPESAFE_MCP_PATH",
                                                )
                                            }),
                                    )
                                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose(
                                            "官方校准判断；保存后请在列表「API 密钥」行收录 TYPESAFE_API_KEY。",
                                            "Calibrated official judgments; after saving, record TYPESAFE_API_KEY via the “API key” row of the list.",
                                        ),
                                    ))
                                    .into_any_element()
                            }),
                    )
                    .when_some(error_line, |this, message| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(message),
                        )
                    })
                    .child(
                        div().flex().items_center().justify_end().gap_2()
                            .child(
                                action_button("jev-backend-cancel")
                                    .ghost()
                                    .label(language.choose("取消", "Cancel"))
                                    .on_click(move |_, _, cx| {
                                        cancel.update(cx, |view, cx| {
                                            view.jev_backend_modal_open = false;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                action_button("jev-backend-save")
                                    .primary()
                                    .label(language.choose("保存", "Save"))
                                    .on_click(move |_, _, cx| {
                                        save.update(cx, |view, cx| {
                                            let llm = view.jev_llm_draft;
                                            if view.apply_jev_backend(llm, cx) {
                                                view.jev_backend_modal_open = false;
                                            }
                                        });
                                    }),
                            ),
                    ),
            )
    }

    /// Key-replacement dialog for the saved backend's key variable.
    pub(super) fn render_jev_key_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let llm_settings = self
            .catalog
            .mcp_servers
            .iter()
            .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
            .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server);
        let key_name = llm_settings
            .as_ref()
            .map_or("TYPESAFE_API_KEY", |s| s.api_key_environment_variable.as_str())
            .to_owned();
        let cancel = entity.clone();
        let save = entity.clone();
        // Failure messages of the two save handlers, shown inside the dialog.
        let error_line = (self.status.starts_with("未保存")
            || self.status.starts_with("判断后端未保存"))
        .then(|| self.status.clone());
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("jev-key-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00_0f17_2ab3))
                    .occlude(),
            )
            .child(
                div()
                    .id("jev-key-modal")
                    .relative()
                    .occlude()
                    .w(px(480.))
                    .v_flex()
                    .gap_4()
                    .p_5()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(SURFACE_BG))
                    .shadow_lg()
                    .child(
                        div().v_flex().gap_1()
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(language.choose("更换 API 密钥", "Replace the API key")))
                            .child(div().text_sm().whitespace_normal().text_color(rgb(TEXT_SECONDARY))
                                .child(format!(
                                    "{}：{key_name}。{}",
                                    language.choose("密钥变量", "Key variable"),
                                    language.choose(
                                        "值只写入密钥保险库（或同名环境变量），绝不进配置文件；需先解锁保险库。",
                                        "Values go only to the vault (or a same-named environment variable), never config files; unlock the vault first.",
                                    ),
                                ))),
                    )
                    .children(self.secret_source_hint(&key_name, &entity))
                    .child(labeled_field("API Key", "jev-api-key", None, &self.jev_api_key))
                    .when_some(error_line, |this, message| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(message),
                        )
                    })
                    .child(
                        div().flex().items_center().justify_end().gap_2()
                            .child(
                                action_button("jev-key-cancel")
                                    .ghost()
                                    .label(language.choose("取消", "Cancel"))
                                    .on_click(move |_, _, cx| {
                                        cancel.update(cx, |view, cx| {
                                            view.jev_key_modal_open = false;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                action_button("jev-key-save")
                                    .primary()
                                    .label(language.choose("保存到密钥保险库", "Save to vault"))
                                    .on_click(move |_, window, cx| {
                                        save.update(cx, |view, cx| {
                                            if view.save_jev_api_key(window, cx) {
                                                view.jev_key_modal_open = false;
                                            }
                                        });
                                    }),
                            ),
                    ),
            )
    }

    pub(super) fn render_skills_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_tool_management(cx)
    }
}
