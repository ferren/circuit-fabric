//! Settings presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn provider_values(&self, cx: &Context<Self>) -> Vec<LlmProviderSettings> {
        self.providers
            .iter()
            .map(|provider| LlmProviderSettings {
                id: provider.id.read(cx).value().to_string(),
                name: provider.name.read(cx).value().to_string(),
                base_url: provider.base_url.read(cx).value().to_string(),
                model: provider.model.read(cx).value().to_string(),
                api_key_environment_variable: provider
                    .api_key_environment_variable
                    .read(cx)
                    .value()
                    .to_string(),
                enabled: provider.enabled,
                supports_vision: provider.supports_vision,
                vision_base_url: Self::optional_value(
                    provider.vision_base_url.read(cx).value().to_string(),
                ),
                vision_model: Self::optional_value(
                    provider.vision_model.read(cx).value().to_string(),
                ),
                vision_api_key_environment_variable: Self::optional_value(
                    provider.vision_api_key_environment_variable.read(cx).value().to_string(),
                ),
            })
            .collect()
    }

    pub(super) fn optional_value(value: String) -> Option<String> {
        (!value.trim().is_empty()).then_some(value)
    }

    pub(super) fn system_is_dark() -> bool {
        // `AppsUseLightTheme` is the Windows user preference.  On platforms
        // without this registry value, a light fallback keeps the UI usable
        // while retaining the user's explicit "follow system" choice.
        #[cfg(windows)]
        {
            let output = std::process::Command::new("reg")
                .args([
                    "query",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                    "/v",
                    "AppsUseLightTheme",
                ])
                .output();
            return output.ok().is_some_and(|result| {
                let text = String::from_utf8_lossy(&result.stdout);
                text.lines().any(|line| line.contains("AppsUseLightTheme") && line.ends_with("0x0"))
            });
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub(super) fn theme_is_dark(theme: GlobalTheme) -> bool {
        match theme {
            GlobalTheme::Light => false,
            GlobalTheme::Dark => true,
            GlobalTheme::System => Self::system_is_dark(),
        }
    }

    pub(super) fn global_preferences_from_form(&self, cx: &Context<Self>) -> GlobalPreferences {
        GlobalPreferences {
            theme: self.global_theme,
            language: self.language.preference(),
            data_directory: self.data_directory.read(cx).value().to_string().into(),
            secret_storage_provider: self.secret_storage_provider,
            log_level: self.log_level,
        }
    }

    /// Persists only global preferences, preserving any unsaved runtime form
    /// state.  No API-key values are read, displayed, or serialized here.
    pub(super) fn persist_global_preferences(&mut self, cx: &mut Context<Self>) {
        let preferences = self.global_preferences_from_form(cx);
        let result =
            RuntimeSettings::load_or_default(&self.settings_path).and_then(|mut settings| {
                settings.global_preferences = preferences;
                settings.save(&self.settings_path)
            });
        self.status = match result {
            Ok(()) => self
                .language
                .choose(
                    "全局偏好已保存；不会保存 API Key 值。",
                    "Global preferences saved; no API key values are stored.",
                )
                .to_owned(),
            Err(error) => format!(
                "{}: {error}",
                self.language.choose("全局偏好未保存", "Global preferences were not saved")
            ),
        };
        cx.notify();
    }

    pub(super) fn set_global_theme(&mut self, theme: GlobalTheme, cx: &mut Context<Self>) {
        self.global_theme = theme;
        DARK_MODE.store(Self::theme_is_dark(theme), Ordering::Relaxed);
        self.persist_global_preferences(cx);
    }

    pub(super) fn toggle_global_language(&mut self, cx: &mut Context<Self>) {
        self.language = self.language.toggled();
        self.persist_global_preferences(cx);
    }

    pub(super) fn add_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut number = self.providers.len() + 1;
        let id = loop {
            let candidate = format!("provider-{number}");
            if self.providers.iter().all(|provider| provider.id.read(cx).value() != candidate) {
                break candidate;
            }
            number += 1;
        };
        let provider = LlmProviderSettings {
            id,
            name: format!("Provider {number}"),
            base_url: "https://api.openai.com/v1".to_owned(),
            model: "gpt-5.4".to_owned(),
            api_key_environment_variable: "OPENAI_API_KEY".to_owned(),
            enabled: true,
            supports_vision: false,
            vision_base_url: None,
            vision_model: None,
            vision_api_key_environment_variable: None,
        };
        self.providers.push(Self::provider_fields(window, provider, cx));
        self.selected_provider = self.providers.len() - 1;
        self.agents_selection = AgentsSelection::Provider;
        "已添加 Provider，请填写配置后保存。".clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn remove_provider(&mut self, cx: &mut Context<Self>) {
        if self.providers.len() <= 1 {
            "至少保留一个 Provider。".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let removed_id = self.providers[selected].id.read(cx).value().to_string();
        self.providers.remove(selected);
        self.selected_provider = selected.min(self.providers.len() - 1);
        if self.default_provider_id == removed_id {
            self.default_provider_id = self.providers[0].id.read(cx).value().to_string();
        }
        self.status = format!("已移除 Provider `{removed_id}`，保存后生效。");
        cx.notify();
    }

    pub(super) fn set_default_provider(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        self.default_provider_id = self.providers[selected].id.read(cx).value().to_string();
        self.status = format!("默认 Provider 已设为 `{}`，保存后生效。", self.default_provider_id);
        cx.notify();
    }

    pub(super) fn toggle_provider(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        self.providers[selected].enabled = !self.providers[selected].enabled;
        self.status = format!(
            "Provider `{}` 的{}修改尚未保存；请点击「保存 Provider 列表」。",
            self.providers[selected].id.read(cx).value(),
            if self.providers[selected].enabled { "启用" } else { "停用" }
        );
        cx.notify();
    }

    pub(super) fn toggle_vision(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        self.providers[selected].supports_vision = !self.providers[selected].supports_vision;
        self.status = format!(
            "Provider `{}` 的 Vision {}修改尚未保存；请点击「保存 Provider 列表」。",
            self.providers[selected].id.read(cx).value(),
            if self.providers[selected].supports_vision { "启用" } else { "停用" }
        );
        cx.notify();
    }

    pub(super) fn save_update(
        &mut self,
        update: crate::application::settings_persistence::SettingsUpdate,
    ) -> Result<(), circuitfabric_codex_runtime::RuntimeError> {
        let tools_changed = matches!(
            &update,
            crate::application::settings_persistence::SettingsUpdate::Catalog(_)
                | crate::application::settings_persistence::SettingsUpdate::Authorizations(_)
                | crate::application::settings_persistence::SettingsUpdate::RemoveResource { .. }
        );
        if let crate::application::settings_persistence::SettingsUpdate::Catalog(catalog) = &update
        {
            catalog.validate_secret_references(self.vault.as_ref().map(|v| v.values()))?;
        }
        let saved =
            crate::application::settings_persistence::save_update(&self.settings_path, update)?;
        if tools_changed {
            self.invalidate_tool_runs();
        }
        self.adapters = saved.adapters.clone();
        self.saved_settings = saved;
        Ok(())
    }

    /// Persists the provider list, reporting the outcome through `status` and returning the
    /// failure so dialogs can show it inline.
    pub(super) fn save_providers_checked(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let update = crate::application::settings_persistence::SettingsUpdate::Providers {
            providers: self.provider_values(cx),
            default_provider_id: self.default_provider_id.clone(),
        };
        self.save_update(update).map_err(|error| format!("Provider 未保存：{error}"))
    }

    /// Persists one adapter's settings, reporting the outcome through `status` and returning
    /// the failure so dialogs can show it inline.
    pub(super) fn save_runtime_checked(
        &mut self,
        adapter: RuntimeAdapter,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        use crate::application::settings_persistence::SettingsUpdate;
        let (command, provider) = self.runtime_fields(adapter);
        let command = command.read(cx).value().trim().to_owned();
        let provider_id = provider.read(cx).value().trim().to_owned();
        let update = match adapter {
            RuntimeAdapter::CodexAppServer => SettingsUpdate::Codex { command, provider_id },
            RuntimeAdapter::ClaudeCode => SettingsUpdate::Claude { command, provider_id },
            RuntimeAdapter::Dsh => SettingsUpdate::Dsh { command, provider_id },
        };
        self.save_update(update).map_err(|error| {
            format!(
                "{} 配置未保存：{error}。新增 Provider 请先在 Provider 详情中保存。",
                adapter.label()
            )
        })
    }

    pub(super) fn runtime_fields(
        &self,
        adapter: RuntimeAdapter,
    ) -> (&Entity<InputState>, &Entity<InputState>) {
        match adapter {
            RuntimeAdapter::CodexAppServer => (&self.command, &self.codex_provider),
            RuntimeAdapter::ClaudeCode => (&self.claude_command, &self.claude_provider),
            RuntimeAdapter::Dsh => (&self.dsh_command, &self.dsh_provider),
        }
    }

    pub(super) fn runtime_dirty(&self, adapter: RuntimeAdapter, cx: &Context<Self>) -> bool {
        let (command, provider) = self.runtime_fields(adapter);
        let saved = &self.saved_settings;
        let (saved_command, saved_provider) = match adapter {
            RuntimeAdapter::CodexAppServer => {
                (&saved.codex.command, &saved.adapters.codex_provider_id)
            }
            RuntimeAdapter::ClaudeCode => {
                (&saved.adapters.claude_command, &saved.adapters.claude_provider_id)
            }
            RuntimeAdapter::Dsh => (&saved.adapters.dsh_command, &saved.adapters.dsh_provider_id),
        };
        command.read(cx).value().trim() != saved_command
            || provider.read(cx).value().trim() != saved_provider
    }

    pub(super) fn save_state_note(dirty: bool, language: UiLanguage) -> Div {
        div().text_xs().text_color(rgb(if dirty { 0x00b4_5309 } else { TEXT_MUTED }))
            .child(if dirty {
                language.choose("有未保存修改；切换页面会保留草稿，启动和任务使用已保存配置。", "Unsaved changes; drafts survive navigation. Starts and tasks use saved configuration.")
            } else {
                language.choose("当前配置无修改；启动和任务使用已保存配置。", "No pending changes; starts and tasks use saved configuration.")
            })
    }

    /// One numbered section card on a runtime-adapter settings page. The step badge gives
    /// the page a stable, scannable operation order — run, settings, verification for the
    /// supervised Codex endpoint; settings, verification for the per-task adapters.
    pub(super) fn adapter_section_card(step: &'static str, title: &'static str) -> Div {
        div()
            .v_flex()
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgb(0x000e_7490))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(CARD_BG))
                            .child(step),
                    )
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_PRIMARY))
                            .child(title),
                    ),
            )
    }

    /// Clickable provider binding for one runtime adapter. Chips for every saved provider
    /// (plus the default fallback) replace typo-prone free-text entry; the raw ID field
    /// stays beneath for manual override and always shows the exact value in effect.
    pub(super) fn render_provider_binding(
        &self,
        adapter: RuntimeAdapter,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let provider_state = self.runtime_fields(adapter).1.clone();
        let current = provider_state.read(cx).value().trim().to_owned();
        let saved_providers = self.saved_settings.providers.clone();
        let field_id = match adapter {
            RuntimeAdapter::CodexAppServer => "codex-provider",
            RuntimeAdapter::ClaudeCode => "claude-provider",
            RuntimeAdapter::Dsh => "dsh-provider",
        };
        let unknown_binding =
            !current.is_empty() && saved_providers.iter().all(|provider| provider.id != current);

        let mut chips = div().flex().flex_wrap().gap_2();
        let default_chooser = entity.clone();
        let default_selected = current.is_empty();
        chips = chips.child(
            div()
                .id(format!("bind-default-{}", adapter.backend_id()))
                .px_3()
                .h(px(28.))
                .flex()
                .items_center()
                .rounded_full()
                .cursor_pointer()
                .text_xs()
                .border_1()
                .border_color(rgb(if default_selected { 0x000e_7490 } else { BORDER }))
                .when(default_selected, |this| {
                    this.bg(rgb(0x000e_7490))
                        .text_color(rgb(CARD_BG))
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!default_selected, |this| {
                    this.bg(rgb(CARD_BG)).text_color(rgb(TEXT_SECONDARY))
                })
                .on_click(move |_, window, cx| {
                    default_chooser.update(cx, |view, cx| {
                        let (_, state) = view.runtime_fields(adapter);
                        state.update(cx, |state, cx| state.set_value("", window, cx));
                        cx.notify();
                    });
                })
                .child(language.choose("默认 Provider", "Default provider")),
        );
        for provider in saved_providers {
            let chooser = entity.clone();
            let selected = current == provider.id;
            let label = if provider.enabled {
                format!("{} · {}", provider.id, provider.model)
            } else {
                format!(
                    "{} · {}{}",
                    provider.id,
                    provider.model,
                    language.choose("（已停用）", " (disabled)")
                )
            };
            chips = chips.child(
                div()
                    .id(format!("bind-{}-{}", adapter.backend_id(), provider.id))
                    .px_3()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .cursor_pointer()
                    .text_xs()
                    .border_1()
                    .border_color(rgb(if selected { 0x000e_7490 } else { BORDER }))
                    .when(selected, |this| {
                        this.bg(rgb(0x000e_7490))
                            .text_color(rgb(CARD_BG))
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |this| this.bg(rgb(CARD_BG)).text_color(rgb(TEXT_SECONDARY)))
                    .on_click(move |_, window, cx| {
                        let id = provider.id.clone();
                        chooser.update(cx, |view, cx| {
                            let (_, state) = view.runtime_fields(adapter);
                            state.update(cx, |state, cx| state.set_value(id, window, cx));
                            cx.notify();
                        });
                    })
                    .child(label),
            );
        }

        div()
            .v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(language.choose("Provider 关联", "Provider binding")),
            )
            .child(chips)
            .child(labeled_field(
                language.choose(
                    "Provider ID（点选上方或手动填写，留空使用默认项）",
                    "Provider ID (pick above or type; empty uses the default)",
                ),
                field_id,
                None,
                &provider_state,
            ))
            .when(unknown_binding, |this| {
                this.child(div().text_xs().text_color(rgb(0x00b4_5309)).child(language.choose(
                    "此 ID 不在已保存的 Provider 列表中；请先在 Provider 详情中保存。",
                    "This ID is not among the saved providers; save it in provider details first.",
                )))
            })
    }

    /// Modal editor for one runtime adapter's settings. Saving happens inside the dialog and
    /// closes it on success; cancelling keeps the draft, because the fields are the same
    /// state the page summary reads.
    pub(super) fn render_adapter_settings_dialog(
        &mut self,
        adapter: RuntimeAdapter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let command = self.runtime_fields(adapter).0.clone();
        let dialog_error = self.dialog_error.clone();
        let closer = entity.clone();
        let closer_top = entity.clone();
        let saver_cancel = entity.clone();
        let saver = entity;

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("adapter-settings-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x0000_0f17_2ab3))
                    .occlude()
                    .on_click(move |_, _, cx| {
                        closer.update(cx, |view, cx| {
                            view.adapter_settings_open = None;
                            view.dialog_error = None;
                            cx.notify();
                        });
                    }),
            )
            .child(
                div()
                    .relative()
                    .occlude()
                    .w(px(640.))
                    .debug_selector(|| "adapter-settings-card".to_owned())
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
                                div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                    language.choose_owned(
                                        format!("编辑 {} 设置", adapter.label()),
                                        format!("Edit {} settings", adapter.label()),
                                    ),
                                ),
                            )
                            .child(
                                action_button("adapter-settings-cancel-top")
                                    .ghost()
                                    .label(language.choose("取消", "Cancel"))
                                    .on_click(move |_, _, cx| {
                                        closer_top.update(cx, |view, cx| {
                                            view.adapter_settings_open = None;
                                            view.dialog_error = None;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(
                        capped_scroll_body("adapter-settings-body", px(440.))
                            .v_flex()
                            .gap_3()
                            .child(labeled_field(
                                language.choose("Codex 命令", "Codex command"),
                                "codex-command",
                                None,
                                &command,
                            ))
                            .child(self.render_provider_binding(adapter, cx))
                            .child(
                                div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                    language.choose(
                                        "保存范围：此运行时的命令与 Provider 关联；工作目录自动使用当前项目根目录。下次任务生效，运行中的 Codex 进程需重启。",
                                        "Saves this runtime's command and provider binding. The working directory is the current project root. Applies to the next task; restart a running Codex process.",
                                    ),
                                ),
                            ),
                    )
                    .when_some(dialog_error, |this, error| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose(
                                    "取消会保留当前草稿，不写入设置。",
                                    "Cancelling keeps the current draft without writing settings.",
                                ),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        action_button("adapter-settings-cancel")
                                            .label(language.choose("取消", "Cancel"))
                                            .on_click(move |_, _, cx| {
                                                saver_cancel.update(cx, |view, cx| {
                                                    view.adapter_settings_open = None;
                                                    view.dialog_error = None;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(
                                        action_button("adapter-settings-save")
                                            .primary()
                                            .label(language.choose("保存并关闭", "Save and close"))
                                            .on_click(move |_, _, cx| {
                                                saver.update(cx, |view, cx| {
                                                    match view.save_runtime_checked(adapter, cx) {
                                                        Ok(()) => {
                                                            view.adapter_settings_open = None;
                                                            view.dialog_error = None;
                                                            view.status = format!(
                                                                "已保存 {} 配置；下次任务生效，运行中的服务须重启。",
                                                                adapter.label()
                                                            );
                                                        }
                                                        Err(error) => {
                                                            view.dialog_error = Some(error);
                                                        }
                                                    }
                                                    cx.notify();
                                                });
                                            }),
                                    ),
                            ),
                    ),
            )
    }

    /// Modal editor for the selected provider. Saving the provider list happens inside the
    /// dialog and closes it on success; quick list actions (default, enable, remove) stay on
    /// the page and remain drafts until this save runs.
    pub(super) fn render_provider_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let provider = &self.providers[selected];
        let dialog_error = self.dialog_error.clone();
        let closer = entity.clone();
        let closer_top = entity.clone();
        let saver = entity.clone();
        let saver_cancel = entity.clone();
        let toggle_vision = entity.clone();
        let api_key_hint = self
            .secret_source_hint(&provider.api_key_environment_variable.read(cx).value(), &entity);
        let vision_key_hint = self.secret_source_hint(
            &provider.vision_api_key_environment_variable.read(cx).value(),
            &entity,
        );

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("provider-editor-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x0000_0f17_2ab3))
                    .occlude()
                    .on_click(move |_, _, cx| {
                        closer.update(cx, |view, cx| {
                            view.provider_editor_open = false;
                            view.dialog_error = None;
                            cx.notify();
                        });
                    }),
            )
            .child(
                div()
                    .relative()
                    .occlude()
                    .w(px(680.))
                    .debug_selector(|| "provider-settings-card".to_owned())
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
                                div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                    language.choose("编辑 Provider", "Edit provider"),
                                ),
                            )
                            .child(
                                action_button("provider-editor-cancel-top")
                                    .ghost()
                                    .label(language.choose("取消", "Cancel"))
                                    .on_click(move |_, _, cx| {
                                        closer_top.update(cx, |view, cx| {
                                            view.provider_editor_open = false;
                                            view.dialog_error = None;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(
                        capped_scroll_body("provider-editor-body", px(440.))
                            .v_flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(labeled_field(
                                        "Provider ID",
                                        "provider-id",
                                        None,
                                        &provider.id,
                                    ))
                                    .child(labeled_field(
                                        language.choose("显示名称", "Display name"),
                                        "provider-name",
                                        None,
                                        &provider.name,
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(labeled_field(
                                        "LLM Base URL",
                                        "provider-base-url",
                                        None,
                                        &provider.base_url,
                                    ))
                                    .child(labeled_field(
                                        language.choose("LLM 模型", "LLM model"),
                                        "provider-model",
                                        None,
                                        &provider.model,
                                    )),
                            )
                            .child(labeled_field(
                                language.choose(
                                    "LLM API Key 环境变量名",
                                    "LLM API key environment variable",
                                ),
                                "provider-api-key-env",
                                Some(language.choose(
                                    "仅环境变量名，例如 OPENAI_API_KEY；密钥值不会出现在这里。",
                                    "Environment-variable name only, e.g. OPENAI_API_KEY; the key value never appears here.",
                                )),
                                &provider.api_key_environment_variable,
                            ))
                            .when_some(api_key_hint, ParentElement::child)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(language.choose(
                                                "Vision 配置",
                                                "Vision configuration",
                                            )),
                                    )
                                    .child(
                                        action_button("toggle-vision")
                                            .label(if provider.supports_vision {
                                                language.choose(
                                                    "Vision：已启用",
                                                    "Vision: enabled",
                                                )
                                            } else {
                                                language.choose(
                                                    "Vision：已停用",
                                                    "Vision: disabled",
                                                )
                                            })
                                            .on_click(move |_, _, cx| {
                                                toggle_vision.update(
                                                    cx,
                                                    ControlPlaneView::toggle_vision,
                                                );
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(labeled_field(
                                        "Vision Base URL",
                                        "vision-base-url",
                                        None,
                                        &provider.vision_base_url,
                                    ))
                                    .child(labeled_field(
                                        language.choose("Vision 模型", "Vision model"),
                                        "vision-model",
                                        None,
                                        &provider.vision_model,
                                    )),
                            )
                            .child(labeled_field(
                                language.choose(
                                    "Vision API Key 环境变量名",
                                    "Vision API key environment variable",
                                ),
                                "vision-api-key-env",
                                Some(language.choose(
                                    "同样只保存环境变量名。",
                                    "Also an environment-variable name only.",
                                )),
                                &provider.vision_api_key_environment_variable,
                            ))
                            .when_some(vision_key_hint, ParentElement::child)
                            .child(
                                div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                    language.choose(
                                        "保存范围：整个 Provider 列表的新增、编辑、删除、启停、Vision 配置和默认项；下次任务生效。",
                                        "Save scope: additions, edits, removals, enabled/Vision states and the default for the entire provider list; applies to the next task.",
                                    ),
                                ),
                            ),
                    )
                    .when_some(dialog_error, |this, error| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose(
                                    "取消会保留当前草稿，不写入设置。",
                                    "Cancelling keeps the current draft without writing settings.",
                                ),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        action_button("provider-editor-cancel")
                                            .label(language.choose("取消", "Cancel"))
                                            .on_click(move |_, _, cx| {
                                                saver_cancel.update(cx, |view, cx| {
                                                    view.provider_editor_open = false;
                                                    view.dialog_error = None;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(
                                        action_button("provider-editor-save")
                                            .primary()
                                            .label(language.choose(
                                                "保存 Provider 列表",
                                                "Save provider list",
                                            ))
                                            .on_click(move |_, _, cx| {
                                                saver.update(cx, |view, cx| {
                                                    match view.save_providers_checked(cx) {
                                                        Ok(()) => {
                                                            view.provider_editor_open = false;
                                                            view.dialog_error = None;
                                                            view.status = "已保存 Provider 列表及默认项；下次任务生效，运行中的服务须重启。".into();
                                                        }
                                                        Err(error) => {
                                                            view.dialog_error = Some(error);
                                                        }
                                                    }
                                                    cx.notify();
                                                });
                                            }),
                                    ),
                            ),
                    ),
            )
    }

    pub(super) fn render_adapter_detail(
        &mut self,
        adapter: RuntimeAdapter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let saver = cx.entity().clone();
        let language = self.language;
        let dirty = self.runtime_dirty(adapter, cx);
        let command_display = self.runtime_fields(adapter).0.read(cx).value().to_string();
        let binding = self.runtime_fields(adapter).1.read(cx).value().trim().to_owned();
        let binding_display = if binding.is_empty() {
            language.choose("默认 Provider", "Default provider").to_owned()
        } else {
            binding
        };
        let description = match adapter {
            RuntimeAdapter::ClaudeCode => language.choose(
                "每次执行创建独立任务进程；完成、失败或取消后清理。使用 Anthropic Messages 协议。",
                "Each run creates an isolated task process, cleaned up on completion, failure, or cancellation. Uses the Anthropic Messages protocol.",
            ),
            RuntimeAdapter::Dsh | RuntimeAdapter::CodexAppServer => language.choose(
                "每次执行创建独立任务进程；完成、失败或取消后清理。",
                "Each run creates an isolated task process, cleaned up on completion, failure, or cancellation.",
            ),
        };
        detail_pane("adapter-detail")
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
                                    .child(adapter.label()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .whitespace_normal()
                                    .child(description),
                            ),
                    ),
            )
            .child(
                Self::adapter_section_card(
                    "1",
                    language.choose(
                        "设置 · 命令与 Provider 关联",
                        "Settings · command and provider binding",
                    ),
                )
                .child(settings_summary_row(
                    language.choose("命令", "Command"),
                    command_display,
                ))
                .child(settings_summary_row(
                    language.choose("Provider", "Provider"),
                    binding_display,
                ))
                .child(Self::save_state_note(dirty, language))
                .child(
                    action_button("open-adapter-settings")
                        .primary()
                        .label(language.choose("编辑设置…", "Edit settings…"))
                        .on_click(move |_, _, cx| {
                            saver.update(cx, |view, cx| {
                                view.adapter_settings_open = Some(adapter);
                                view.dialog_error = None;
                                cx.notify();
                            });
                        }),
                ),
            )
            .child(
                Self::adapter_section_card(
                    "2",
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
                .child(self.render_task_controls(adapter, cx)),
            )
    }

    pub(super) fn render_provider_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let provider = &self.providers[selected];
        let provider_id = provider.id.read(cx).value().to_string();
        let provider_name = provider.name.read(cx).value().to_string();
        let selected_is_default = provider_id == self.default_provider_id;
        let set_default = entity.clone();
        let toggle_provider = entity.clone();
        let remove_provider = entity.clone();
        let provider_saver = entity.clone();
        let dirty = self.provider_values(cx) != self.saved_settings.providers
            || self.default_provider_id != self.saved_settings.default_provider_id;
        let api_key_hint = self
            .secret_source_hint(&provider.api_key_environment_variable.read(cx).value(), &entity);

        let default_badge = selected_is_default.then(|| {
            div()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .text_xs()
                .bg(rgb(0x00fe_f3c7))
                .text_color(rgb(0x00b4_5309))
                .child(language.choose("★ 默认", "★ Default"))
        });
        let enabled_badge = div()
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .text_xs()
            .bg(rgb(if provider.enabled { 0x00dc_fce7 } else { SURFACE_BG }))
            .text_color(rgb(if provider.enabled { 0x0016_a34a } else { TEXT_MUTED }))
            .child(if provider.enabled {
                language.choose("已启用", "Enabled")
            } else {
                language.choose("已停用", "Disabled")
            });
        let vision_badge = div()
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .text_xs()
            .bg(rgb(0x00e0_f2fe))
            .text_color(rgb(0x000e_7490))
            .child("Vision");

        detail_pane("provider-detail")
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
                                    .child(provider_name),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(provider_id),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when_some(default_badge, ParentElement::child)
                            .child(enabled_badge)
                            .when(provider.supports_vision, |row| row.child(vision_badge)),
                    ),
            )
            .child(info_note(
                "Provider 配置只保存 API Key 变量名；密钥值在密钥保险库中单独加密保存，也可由进程环境提供。",
                "Provider configuration saves API key variable names only; values are stored separately in the encrypted vault or supplied by the process environment.",
                language,
            ))
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG))
                    .child(settings_summary_row(
                        "Base URL",
                        provider.base_url.read(cx).value().to_string(),
                    ))
                    .child(settings_summary_row(
                        language.choose("模型", "Model"),
                        provider.model.read(cx).value().to_string(),
                    ))
                    .child(settings_summary_row(
                        language.choose("API Key", "API key"),
                        provider.api_key_environment_variable.read(cx).value().to_string(),
                    ))
                    .when_some(api_key_hint, ParentElement::child)
                    .child(settings_summary_row(
                        "Vision",
                        if provider.supports_vision {
                            format!(
                                "{}（{}）",
                                language.choose("已启用", "Enabled"),
                                provider.vision_model.read(cx).value()
                            )
                        } else {
                            language.choose("已停用", "Disabled").to_owned()
                        },
                    )),
            )
            .child(Self::save_state_note(dirty, language))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        action_button("set-default-provider")
                            .label(if selected_is_default {
                                language.choose("当前为默认 Provider", "Current default provider")
                            } else {
                                language.choose("设为默认 Provider", "Set as default provider")
                            })
                            .disabled(selected_is_default)
                            .on_click(move |_, _, cx| {
                                set_default.update(cx, ControlPlaneView::set_default_provider);
                            }),
                    )
                    .child(
                        action_button("toggle-provider")
                            .label(if provider.enabled {
                                language.choose("停用", "Disable")
                            } else {
                                language.choose("启用", "Enable")
                            })
                            .on_click(move |_, _, cx| {
                                toggle_provider.update(cx, ControlPlaneView::toggle_provider);
                            }),
                    )
                    .child(
                        action_button("remove-provider")
                            .danger()
                            .label(language.choose("删除", "Remove"))
                            .on_click(move |_, _, cx| {
                                remove_provider.update(cx, ControlPlaneView::remove_provider);
                            }),
                    ),
            )
            .child(
                div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    language.choose(
                        "上述默认项、启停与删除均为草稿，连同表单修改一起在「编辑 Provider…」弹窗中保存后生效。默认项用于未指定 Provider 的运行时。",
                        "Default, enabled, and removal changes are drafts; they take effect together with the form edits saved in the Edit provider dialog. The default applies to runtimes without an explicit provider binding.",
                    ),
                ),
            )
            .child(
                action_button("open-provider-editor")
                    .primary()
                    .label(language.choose("编辑 Provider…", "Edit provider…"))
                    .on_click(move |_, _, cx| {
                        provider_saver.update(cx, |view, cx| {
                            view.provider_editor_open = true;
                            view.dialog_error = None;
                            cx.notify();
                        });
                    }),
            )
    }

    pub(super) fn render_settings_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let entity = cx.entity().clone();
        let theme = self.global_theme;
        let use_vault = self.secret_storage_provider == SecretStorageProvider::EncryptedVault;
        let log_label = match self.log_level {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        };
        let next_log_level = match self.log_level {
            LogLevel::Error => LogLevel::Warn,
            LogLevel::Warn => LogLevel::Info,
            LogLevel::Info => LogLevel::Debug,
            LogLevel::Debug => LogLevel::Trace,
            LogLevel::Trace => LogLevel::Error,
        };

        let system = entity.clone();
        let light = entity.clone();
        let dark = entity.clone();
        let change_language = entity.clone();
        let vault = entity.clone();
        let environment = entity.clone();
        let change_log_level = entity.clone();
        let save = entity;

        let theme_buttons = div()
            .flex()
            .gap_2()
            .child(
                action_button("theme-system")
                    .label(language.choose("跟随系统", "System"))
                    .when(theme == GlobalTheme::System, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        system
                            .update(cx, |view, cx| view.set_global_theme(GlobalTheme::System, cx));
                    }),
            )
            .child(
                action_button("theme-light")
                    .label(language.choose("浅色", "Light"))
                    .when(theme == GlobalTheme::Light, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        light.update(cx, |view, cx| view.set_global_theme(GlobalTheme::Light, cx));
                    }),
            )
            .child(
                action_button("theme-dark")
                    .label(language.choose("深色", "Dark"))
                    .when(theme == GlobalTheme::Dark, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        dark.update(cx, |view, cx| view.set_global_theme(GlobalTheme::Dark, cx));
                    }),
            );
        let appearance = div()
            .w_full()
            .v_flex()
            .gap_3()
            .p_5()
            .bg(rgb(CARD_BG))
            .rounded_xl()
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.choose("外观与语言", "Appearance & language")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT_SECONDARY))
                    .child(language.choose("主题", "Theme")),
            )
            .child(theme_buttons)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(language.choose("界面语言", "Display language")),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "切换后完整界面立即重新渲染。",
                "The entire interface rerenders immediately.",
            )))
            .child(
                action_button("settings-toggle-language")
                    .label(language.choose("切换为 English", "Switch to 中文"))
                    .on_click(move |_, _, cx| {
                        change_language.update(cx, |view, cx| view.toggle_global_language(cx));
                    }),
            );
        let secret_buttons = div()
            .flex()
            .gap_2()
            .child(
                action_button("secret-provider-vault")
                    .label(language.choose("加密保险库", "Encrypted vault"))
                    .when(use_vault, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        vault.update(cx, |view, cx| {
                            view.secret_storage_provider = SecretStorageProvider::EncryptedVault;
                            view.persist_global_preferences(cx);
                        });
                    }),
            )
            .child(
                action_button("secret-provider-environment")
                    .label(language.choose("进程环境", "Environment"))
                    .when(!use_vault, |button| button.primary())
                    .on_click(move |_, _, cx| {
                        environment.update(cx, |view, cx| {
                            view.secret_storage_provider = SecretStorageProvider::Environment;
                            view.persist_global_preferences(cx);
                        });
                    }),
            );
        let storage = div()
            .w_full()
            .v_flex()
            .gap_3()
            .p_5()
            .bg(rgb(CARD_BG))
            .rounded_xl()
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.choose("存储与诊断", "Storage & diagnostics")),
            )
            .child(labeled_field(
                language.choose("数据目录", "Data directory"),
                "global-data-directory",
                Some(language.choose(
                    "用于 CircuitFabric 本地配置和数据。",
                    "Used for CircuitFabric local configuration and data.",
                )),
                &self.data_directory,
            ))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(language.choose("密钥存储提供方", "Secret storage provider")),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "设置页从不读取、显示或写入 API Key 值。",
                "This page never reads, displays, or writes API key values.",
            )))
            .child(secret_buttons)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(language.choose("日志级别", "Log level")),
            )
            .child(action_button("cycle-log-level").label(log_label).on_click(move |_, _, cx| {
                change_log_level.update(cx, |view, cx| {
                    view.log_level = next_log_level;
                    view.persist_global_preferences(cx);
                });
            }))
            .child(
                action_button("save-global-preferences")
                    .primary()
                    .label(language.choose("保存全局设置", "Save global settings"))
                    .on_click(move |_, _, cx| {
                        save.update(cx, |view, cx| view.persist_global_preferences(cx));
                    }),
            );
        let about =
            div()
                .w_full()
                .v_flex()
                .gap_2()
                .p_5()
                .bg(rgb(CARD_BG))
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("关于 CircuitFabric", "About CircuitFabric")),
                )
                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                    language.choose(
                        "CircuitFabric 桌面控制平面",
                        "CircuitFabric desktop control plane",
                    ),
                ))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                    "v{} · {}",
                    env!("CARGO_PKG_VERSION"),
                    language.choose("本地优先、密钥隔离", "local-first, secret-isolated")
                )));
        page("global-settings-page")
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.choose("全局设置", "Global settings")),
            )
            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                "这些偏好会立即应用，并在下次启动时恢复。",
                "These preferences apply immediately and are restored on the next launch.",
            )))
            .child(appearance)
            .child(storage)
            .child(about)
    }
}
