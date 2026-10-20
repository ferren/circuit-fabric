//! Settings presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn provider_values(&self, cx: &Context<Self>) -> Vec<LlmProviderSettings> {
        self.providers
            .iter()
            .map(|provider| LlmProviderSettings {
                id: provider.id.read(cx).value().to_string(),
                name: provider.name.read(cx).value().to_string(),
                connection: provider.connection,
                wire_api: provider.wire_api,
                base_url: provider.base_url.read(cx).value().to_string(),
                model: provider.model.read(cx).value().to_string(),
                api_key_environment_variable: provider
                    .api_key_environment_variable
                    .read(cx)
                    .value()
                    .to_string(),
                enabled: provider.enabled,
                native_vision: provider.native_vision,
                vision_base_url: Self::optional_value(
                    provider.vision_base_url.read(cx).value().to_string(),
                ),
                vision_model: Self::optional_value(
                    provider.vision_model.read(cx).value().to_string(),
                ),
                vision_api_key_environment_variable: Self::optional_value(
                    provider.vision_api_key_environment_variable.read(cx).value().to_string(),
                ),
                reasoning_effort: provider.reasoning_effort,
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
        sync_component_theme(Self::theme_is_dark(theme), cx);
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
            connection: ProviderConnection::ApiKey,
            wire_api: ProviderWireApi::Responses,
            base_url: "https://api.openai.com/v1".to_owned(),
            model: "gpt-5.4".to_owned(),
            api_key_environment_variable: "OPENAI_API_KEY".to_owned(),
            enabled: true,
            native_vision: false,
            vision_base_url: None,
            vision_model: None,
            vision_api_key_environment_variable: None,
            reasoning_effort: None,
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
        let removed = self.providers.remove(selected);
        let removed_id = removed.id.read(cx).value().to_string();
        self.model_catalogs.remove(&removed_id);
        let previous_default = self.default_provider_id.clone();
        let removed_was_default = self.default_provider_id == removed_id;
        self.selected_provider = selected.min(self.providers.len() - 1);
        if removed_was_default {
            self.default_provider_id = self.providers[0].id.read(cx).value().to_string();
        }
        if !self.saved_settings.providers.iter().any(|provider| provider.id == removed_id) {
            self.status = format!("已移除草稿 Provider `{removed_id}`；保存 Provider 列表后生效。");
            cx.notify();
            return;
        }
        match self.save_update(
            crate::application::settings_persistence::SettingsUpdate::RemoveProvider {
                id: removed_id.clone(),
            },
        ) {
            Ok(()) => {
                // The persisted replacement default is authoritative; a draft list
                // that still ends with an unsaved provider must not shadow it.
                if removed_was_default {
                    self.default_provider_id = self.saved_settings.default_provider_id.clone();
                }
                self.status = format!(
                    "已移除并保存 Provider `{removed_id}`；下次任务生效，运行中的服务须重启。"
                );
            }
            Err(error) => {
                self.providers.insert(selected, removed);
                self.selected_provider = selected;
                self.default_provider_id = previous_default;
                self.status = format!("移除未保存：{error}");
            }
        }
        cx.notify();
    }

    /// Quick actions persist immediately through section-scoped updates, so an
    /// unsaved form draft elsewhere on the page never rides along and closing the
    /// app never discards the click. Providers that exist only as drafts keep the
    /// draft semantics: they reach disk with the next "保存 Provider 列表".
    pub(super) fn set_default_provider(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let previous_default = self.default_provider_id.clone();
        let id = self.providers[selected].id.read(cx).value().to_string();
        self.default_provider_id = id.clone();
        if !self.saved_settings.providers.iter().any(|provider| provider.id == id) {
            self.status =
                format!("Provider `{id}` 尚未保存；默认项将随「保存 Provider 列表」生效。");
            cx.notify();
            return;
        }
        match self.save_update(
            crate::application::settings_persistence::SettingsUpdate::DefaultProvider {
                id: id.clone(),
            },
        ) {
            Ok(()) => {
                self.status = format!(
                    "默认 Provider 已设为 `{id}` 并保存；下次任务生效，运行中的服务须重启。"
                )
            }
            Err(error) => {
                self.default_provider_id = previous_default;
                self.status = format!("默认 Provider 未保存：{error}");
            }
        }
        cx.notify();
    }

    pub(super) fn toggle_provider(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let id = self.providers[selected].id.read(cx).value().to_string();
        self.providers[selected].enabled = !self.providers[selected].enabled;
        let enabled = self.providers[selected].enabled;
        if !self.saved_settings.providers.iter().any(|provider| provider.id == id) {
            self.status = format!("Provider `{id}` 尚未保存；启停将随「保存 Provider 列表」生效。");
            cx.notify();
            return;
        }
        let update = crate::application::settings_persistence::SettingsUpdate::ProviderEnabled {
            id: id.clone(),
            enabled,
        };
        match self.save_update(update) {
            Ok(()) => {
                self.status = format!(
                    "Provider `{id}` 已{}并保存；下次任务生效，运行中的服务须重启。",
                    if enabled { "启用" } else { "停用" }
                )
            }
            Err(error) => {
                self.providers[selected].enabled = !enabled;
                self.status = format!("未保存：{error}");
            }
        }
        cx.notify();
    }

    pub(super) fn toggle_vision(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let (provider_id, native) = {
            let fields = &mut self.providers[selected];
            fields.native_vision = !fields.native_vision;
            let native = fields.native_vision;
            // The separate vision fields gray out while the LLM handles images
            // natively; their values stay visible and come back editable when
            // native vision is turned off again.
            let vision_states = [
                fields.vision_base_url.clone(),
                fields.vision_model.clone(),
                fields.vision_api_key_environment_variable.clone(),
            ];
            for state in &vision_states {
                state.update(cx, |state, cx| state.set_disabled(native, cx));
            }
            (fields.id.read(cx).value().to_string(), native)
        };
        self.status = format!(
            "Provider `{provider_id}`：{}。修改尚未保存；请点击「保存 Provider 列表」。",
            if native {
                "LLM 本身支持视觉，图像直接使用主模型，独立 Vision 配置已灰掉（原值保留）"
            } else {
                "LLM 本身不支持视觉，图像需经独立 Vision 模型处理"
            },
        );
        cx.notify();
    }

    /// Switches the selected provider between an API-key endpoint and a direct
    /// CLI-account sign-in (Codex or Kimi). The endpoint and separate vision
    /// fields gray out in direct mode; their stored values survive and become
    /// editable again when switching back (vision fields still follow the
    /// native-vision rule).
    pub(super) fn set_provider_connection(
        &mut self,
        connection: ProviderConnection,
        cx: &mut Context<Self>,
    ) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let direct = matches!(
            connection,
            ProviderConnection::CodexAccount | ProviderConnection::KimiAccount
        );
        let (provider_id, switched) = {
            let fields = &mut self.providers[selected];
            if fields.connection == connection {
                return;
            }
            fields.connection = connection;
            let endpoint_states =
                [fields.base_url.clone(), fields.api_key_environment_variable.clone()];
            for state in &endpoint_states {
                state.update(cx, |state, cx| state.set_disabled(direct, cx));
            }
            let vision_disabled = direct || fields.native_vision;
            let vision_states = [
                fields.vision_base_url.clone(),
                fields.vision_model.clone(),
                fields.vision_api_key_environment_variable.clone(),
            ];
            for state in &vision_states {
                state.update(cx, |state, cx| state.set_disabled(vision_disabled, cx));
            }
            (fields.id.read(cx).value().to_string(), direct)
        };
        let direct_label = match connection {
            ProviderConnection::CodexAccount => "Codex 账号直连",
            ProviderConnection::KimiAccount => "Kimi 账号直连",
            ProviderConnection::ApiKey => unreachable!("an API-key switch is never direct"),
        };
        self.status = format!(
            "Provider `{provider_id}`：{}。修改尚未保存；请点击「保存 Provider 列表」。",
            if switched {
                format!(
                    "已切换为 {direct_label}，Base URL、API Key 与独立 Vision 配置不再适用（原值保留）"
                )
            } else {
                "已切换为 API Key（OpenAI 兼容）连接方式，Base URL、API Key 与 Vision 配置恢复可编辑"
                    .to_owned()
            },
        );
        cx.notify();
    }

    /// Switches the selected API-key provider between the Responses and Chat
    /// Completions protocols. Modern codex-cli builds launch only with
    /// Responses (z.ai serves a Responses endpoint at `https://api.z.ai/api/v1`);
    /// chat stays selectable for older codex installations.
    pub(super) fn set_provider_wire_api(
        &mut self,
        wire_api: ProviderWireApi,
        cx: &mut Context<Self>,
    ) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        if self.providers[selected].wire_api == wire_api {
            return;
        }
        self.providers[selected].wire_api = wire_api;
        let provider_id = self.providers[selected].id.read(cx).value().to_string();
        let wire_label = match wire_api {
            ProviderWireApi::Chat => "Chat Completions",
            ProviderWireApi::Responses => "Responses",
        };
        self.status = format!(
            "Provider `{provider_id}`：API 协议已切换为 {wire_label}；修改尚未保存，请点击「保存 Provider 列表」。",
        );
        cx.notify();
    }

    pub(super) fn set_provider_reasoning_effort(
        &mut self,
        effort: Option<circuitfabric_codex_runtime::ReasoningEffort>,
        cx: &mut Context<Self>,
    ) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        self.providers[selected].reasoning_effort = effort;
        let provider_id = self.providers[selected].id.read(cx).value().to_string();
        self.status = format!(
            "Provider `{provider_id}`：默认推理强度设为「{}」；修改尚未保存，请点击「保存 Provider 列表」。",
            reasoning_effort_label(effort, UiLanguage::SimplifiedChinese)
        );
        cx.notify();
    }

    /// Default reasoning-effort chips in the provider editor.
    fn render_reasoning_effort_choice(&self, provider: &ProviderFields, cx: &Context<Self>) -> Div {
        let language = self.language;
        let mut row = div().flex().flex_wrap().items_center().gap_2().child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(language.choose("默认推理强度", "Default reasoning effort")),
        );
        for effort in std::iter::once(None)
            .chain(circuitfabric_codex_runtime::ReasoningEffort::ALL.into_iter().map(Some))
        {
            let setter = cx.entity().clone();
            row = row.child(
                action_button(format!(
                    "provider-reasoning-{}",
                    effort.map_or("default", circuitfabric_codex_runtime::ReasoningEffort::value)
                ))
                .label(reasoning_effort_label(effort, language))
                .when(provider.reasoning_effort == effort, ButtonVariants::primary)
                .on_click(move |_, _, cx| {
                    setter.update(cx, |view, cx| view.set_provider_reasoning_effort(effort, cx));
                }),
            );
        }
        row.child(
            div()
                .flex_1()
                .min_w(px(200.))
                .text_xs()
                .text_color(rgb(TEXT_MUTED))
                .whitespace_normal()
                .child(language.choose(
                    "Codex App Server 与 Claude Code 生效；DSH 与 Kimi Code 无此设置。助手可单独覆盖。",
                    "Applies to Codex App Server and Claude Code; DSH and Kimi Code have no such setting. The assistant can override it.",
                )),
        )
    }

    /// The endpoint coordinates a fetched model catalog belongs to. Editing
    /// any of these invalidates the fetched choices, so a stale catalog never
    /// offers clicks into a changed draft.
    pub(super) fn model_catalog_signature(
        provider: &ProviderFields,
        cx: &Context<Self>,
    ) -> ModelCatalogKey {
        (
            provider.id.read(cx).value().to_string(),
            provider.connection,
            provider.base_url.read(cx).value().trim().to_owned(),
            provider.api_key_environment_variable.read(cx).value().trim().to_owned(),
        )
    }

    /// Fetches the selectable model list for the provider being edited, from
    /// the source its connection implies: the Codex App Server catalog for a
    /// Codex account, `kimi provider list --json` for a Kimi account, and the
    /// OpenAI-compatible `GET {base_url}/models` endpoint for API-key
    /// providers. The work runs in the background and starts on its own —
    /// opening the editor, switching the connection, or focusing the model
    /// field with changed endpoint coordinates all trigger it; there is no
    /// manual button. [`Self::render_model_dropdown`] turns the outcome into
    /// the floating pick-list under the model field.
    pub(super) fn fetch_provider_models(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let signature = Self::model_catalog_signature(&self.providers[selected], cx);
        if self.model_choices.pending && self.model_choices.fetched_for.as_ref() == Some(&signature)
        {
            return;
        }
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        // The fetch reads the current draft (id, connection, base URL, key
        // variable) so a freshly typed endpoint works before saving; the model
        // itself stays empty to avoid pinning a half-typed value.
        let provider = {
            let fields = &self.providers[selected];
            LlmProviderSettings {
                id: fields.id.read(cx).value().to_string(),
                name: String::new(),
                connection: fields.connection,
                wire_api: fields.wire_api,
                base_url: fields.base_url.read(cx).value().to_string(),
                model: String::new(),
                api_key_environment_variable: fields
                    .api_key_environment_variable
                    .read(cx)
                    .value()
                    .to_string(),
                enabled: true,
                native_vision: false,
                vision_base_url: None,
                vision_model: None,
                vision_api_key_environment_variable: None,
                reasoning_effort: None,
            }
        };
        self.model_choices.generation += 1;
        let generation = self.model_choices.generation;
        // Choices fetched for a different draft never appear under the new
        // signature; a same-signature refetch keeps the old list visible
        // until the fresh one lands.
        if self.model_choices.fetched_for.as_ref() != Some(&signature) {
            self.model_choices.choices.clear();
        }
        self.model_choices.pending = true;
        self.model_choices.fetched_for = Some(signature);
        self.model_choices.error = None;
        self.status = self
            .language
            .choose("正在获取模型列表…", "Fetching model list…")
            .to_owned();
        let provider_id = provider.id.clone();
        let codex_settings = settings.codex.clone();
        let kimi_command = settings.adapters.kimi_command.clone();
        let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
            .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
            .flatten();
        let fetch = cx.background_spawn(fetch_model_catalog(
            provider,
            codex_settings,
            kimi_command,
            secrets,
        ));
        cx.spawn(async move |view, cx| {
            let outcome = fetch.await;
            view.update(cx, |view, cx| {
                if view.model_choices.generation != generation {
                    return;
                }
                view.model_choices.pending = false;
                view.model_choices.window = super::MODEL_PAGE_SIZE;
                view.model_list_scroll.set_offset(gpui::Point::default());
                match outcome {
                    Ok(choices) => {
                        let count = choices.len();
                        view.model_catalogs.insert(provider_id.clone(), choices.clone());
                        view.model_choices.choices = choices;
                        view.status = format!(
                            "{}（{count}）；{}",
                            view.language.choose("模型列表已获取", "Model list fetched"),
                            view.language.choose(
                                "点击模型输入框展开下拉选择，或继续手动输入。",
                                "click the model field to pick from the dropdown, or keep typing.",
                            )
                        );
                    }
                    Err(error) => {
                        view.model_choices.choices.clear();
                        view.model_choices.error = Some(error);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Startup catalog fill for account-direct providers, so the provider
    /// detail pane can badge a vision-capable selected model without opening
    /// the editor first. Failures stay silent: no badge is shown, and the
    /// editor's own auto-fetch reports errors when it runs. Called only from
    /// the real app entry point, never from tests.
    pub(super) fn prefetch_direct_model_catalogs(&mut self, cx: &mut Context<Self>) {
        let settings = self.saved_settings.clone();
        let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
            .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
            .flatten();
        for provider in &settings.providers {
            if !matches!(
                provider.connection,
                ProviderConnection::CodexAccount | ProviderConnection::KimiAccount
            ) {
                continue;
            }
            let provider_id = provider.id.clone();
            let fetch = cx.background_spawn(fetch_model_catalog(
                provider.clone(),
                settings.codex.clone(),
                settings.adapters.kimi_command.clone(),
                secrets.clone(),
            ));
            cx.spawn(async move |view, cx| {
                if let Ok(choices) = fetch.await {
                    view.update(cx, |view, cx| {
                        view.model_catalogs.insert(provider_id, choices);
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        }
    }

    /// Opens the model catalog dropdown (the model field was focused or
    /// clicked). A catalog fetched for different endpoint coordinates — or
    /// one whose last fetch failed — is refetched automatically.
    pub(super) fn open_model_dropdown(&mut self, cx: &mut Context<Self>) {
        if !self.provider_editor_open {
            return;
        }
        self.model_choices.dropdown_open = true;
        // Every open starts from the first page at the top of the list.
        self.model_choices.window = super::MODEL_PAGE_SIZE;
        self.model_list_scroll.set_offset(gpui::Point::default());
        let selected = self.selected_provider.min(self.providers.len() - 1);
        let signature = Self::model_catalog_signature(&self.providers[selected], cx);
        let stale = self.model_choices.fetched_for.as_ref() != Some(&signature);
        if stale || self.model_choices.error.is_some() {
            self.fetch_provider_models(cx);
        }
        cx.notify();
    }

    /// Loads the dropdown's next page: the catalog is cached in full, so a
    /// page step is instant local windowing — the visible row window grows by
    /// [`super::MODEL_PAGE_SIZE`] and the next scroll lands on fresh rows.
    pub(super) fn extend_model_window(&mut self, cx: &mut Context<Self>) {
        let cap = circuitfabric_codex_runtime::models::ENDPOINT_CHOICE_LIMIT + super::MODEL_PAGE_SIZE;
        let next = (self.model_choices.window + super::MODEL_PAGE_SIZE).min(cap);
        if next == self.model_choices.window {
            return;
        }
        self.model_choices.window = next;
        cx.notify();
    }

    /// Closes the model catalog dropdown (the model field blurred, a row was
    /// picked, or the editor closed).
    pub(super) fn close_model_dropdown(&mut self, cx: &mut Context<Self>) {
        if self.model_choices.dropdown_open {
            self.model_choices.dropdown_open = false;
            cx.notify();
        }
    }

    /// The provider detail badge answer for account-direct providers: whether
    /// the cached catalog reports the selected model as vision-capable. An
    /// empty model field resolves through the catalog's default entry; an
    /// unknown catalog or model means no badge, never a guess.
    pub(super) fn direct_model_supports_vision(
        &self,
        provider: &ProviderFields,
        cx: &Context<Self>,
    ) -> bool {
        let provider_id = provider.id.read(cx).value().to_string();
        let Some(choices) = self.model_catalogs.get(&provider_id) else {
            return false;
        };
        let model = provider.model.read(cx).value().trim().to_owned();
        let choice = if model.is_empty() {
            choices.iter().find(|choice| choice.is_default)
        } else {
            choices.iter().find(|choice| choice.id == model)
        };
        choice.and_then(|choice| choice.supports_vision) == Some(true)
    }

    /// The model field with its floating catalog dropdown. The field stays
    /// free text; clicking it or its trailing caret opens the dropdown, which
    /// overlays the dialog instead of taking layout space, and closes on a
    /// row pick, on blur, or on a click anywhere outside it. The catalog
    /// arrives from the automatic background fetch.
    pub(super) fn render_model_field(
        &self,
        provider: &ProviderFields,
        cx: &Context<Self>,
    ) -> Div {
        let language = self.language;
        let entity = cx.entity().clone();
        let caret = cx.entity().clone();
        let label = if provider.connection == ProviderConnection::CodexAccount {
            language.choose("LLM 模型（留空使用 Codex 默认）", "LLM model (empty = Codex default)")
        } else if provider.connection == ProviderConnection::KimiAccount {
            language.choose("LLM 模型（留空使用 Kimi 默认）", "LLM model (empty = Kimi default)")
        } else {
            language.choose("LLM 模型", "LLM model")
        };
        let dropdown = self.render_model_dropdown(provider, cx);
        let signature = Self::model_catalog_signature(provider, cx);
        let error = if self.model_choices.dropdown_open {
            None
        } else {
            self.model_choices
                .error
                .as_ref()
                .filter(|_| self.model_choices.fetched_for.as_ref() == Some(&signature))
        };
        div()
            .debug_selector(|| "provider-model".to_owned())
            .v_flex()
            .gap_1()
            .flex_1()
            .min_w(px(240.))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id("provider-model-box")
                            .debug_selector(|| "provider-model-box".to_owned())
                            .h(px(36.))
                            .pl_2()
                            .pr_1()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(CARD_BG))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                entity.update(cx, |view, cx| view.open_model_dropdown(cx));
                            })
                            .child(
                                InputBase::new("provider-model")
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .child(provider.model.clone()),
                            )
                            .child(
                                // Web-style disclosure caret: opens the
                                // dropdown and focuses the field for immediate
                                // typing. While the dropdown is open the
                                // backdrop covers this spot, so a click there
                                // closes instead — the caret toggles.
                                div()
                                    .id("provider-model-caret")
                                    .debug_selector(|| "provider-model-caret".to_owned())
                                    .flex_none()
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_color(rgb(TEXT_MUTED))
                                    .hover(|this| this.bg(rgb(SURFACE_BG)))
                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                        caret.update(cx, |view, cx| {
                                            let selected = view
                                                .selected_provider
                                                .min(view.providers.len() - 1);
                                            let state = view.providers[selected].model.clone();
                                            state.update(cx, |state, cx| state.focus(window, cx));
                                            view.open_model_dropdown(cx);
                                        });
                                    })
                                    .child(
                                        Icon::new(IconName::ChevronDown)
                                            .with_size(gpui_component::Size::Small),
                                    ),
                            ),
                    )
                    .when_some(dropdown, |this, dropdown| this.child(dropdown)),
            )
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .debug_selector(|| "provider-model-error".to_owned())
                        .text_xs()
                        .whitespace_normal()
                        .text_color(rgb(0x00dc_2626))
                        .child(error.clone()),
                )
            })
            .when(error.is_none(), |this| {
                this.child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "点击输入框或右侧小三角展开后台获取的模型列表；输入关键字过滤，点选填入。",
                        "Click the field or its caret to open the auto-fetched model list; type to filter, click to fill.",
                    )),
                )
            })
    }

    /// Floating model catalog dropdown anchored under the model field: a
    /// deferred-draw overlay that takes no layout space and escapes the
    /// dialog's scroll clipping. The model field doubles as the search box —
    /// typing filters the locally cached catalog by substring (id or display
    /// name) and the panel shows the top matches; an empty or exactly
    /// selected value shows the catalog's first rows. Endpoints do not offer
    /// server-side name search (OpenRouter filters by modality/parameter
    /// only), so filtering applies to the fetched catalog locally. Only the
    /// catalog fetched for the current draft signature is ever shown.
    pub(super) fn render_model_dropdown(
        &self,
        provider: &ProviderFields,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !self.model_choices.dropdown_open {
            return None;
        }
        let language = self.language;
        let signature = Self::model_catalog_signature(provider, cx);
        if self.model_choices.fetched_for.as_ref() != Some(&signature) {
            return None;
        }
        let source = match provider.connection {
            ProviderConnection::CodexAccount => {
                language.choose("Codex 账号目录", "Codex account catalog")
            }
            ProviderConnection::KimiAccount => language.choose("kimi CLI 目录", "kimi CLI catalog"),
            ProviderConnection::ApiKey => language.choose("端点 GET /models", "endpoint GET /models"),
        };
        let entity = cx.entity().clone();
        let current_model = provider.model.read(cx).value().to_string();
        let mut query = current_model.trim().to_lowercase();
        // A field holding an exact catalog id is a finished selection, not a
        // search: show the catalog's first rows instead of just the match.
        if self.model_choices.choices.iter().any(|choice| choice.id == current_model.trim()) {
            query.clear();
        }
        let mut panel = div()
            .id("provider-model-dropdown")
            .debug_selector(|| "provider-model-dropdown".to_owned())
            .v_flex()
            .py_1()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .shadow_lg()
            .occlude();
        if self.model_choices.pending && self.model_choices.choices.is_empty() {
            panel = panel.child(
                div()
                    .px_3()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose("正在获取模型列表…", "Fetching model list…")),
            );
        } else if let Some(error) = &self.model_choices.error {
            panel = panel.child(
                div().px_3().py_2().text_xs().whitespace_normal().text_color(rgb(0x00dc_2626)).child(
                    format!("{source}：{error}"),
                ),
            );
        } else if self.model_choices.choices.is_empty() {
            panel = panel.child(
                div().px_3().py_2().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "未获取到模型列表；可直接输入模型名。",
                    "No model list fetched; the model name can still be typed.",
                )),
            );
        } else {
            let matches: Vec<&circuitfabric_codex_runtime::models::ModelChoice> = self
                .model_choices
                .choices
                .iter()
                .filter(|choice| {
                    query.is_empty()
                        || choice.id.to_lowercase().contains(&query)
                        || choice
                            .display_name
                            .as_deref()
                            .is_some_and(|name| name.to_lowercase().contains(&query))
                })
                .collect();
            let total = self.model_choices.choices.len();
            let truncation_note = if provider.connection == ProviderConnection::ApiKey
                && total == circuitfabric_codex_runtime::models::ENDPOINT_CHOICE_LIMIT
            {
                language.choose("（目录较大，仅收录前 1000 个）", " (large catalog, first 1000 kept)")
            } else {
                ""
            };
            panel = panel.child(
                div()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(format!(
                        "{source}：{} {total} {} {truncation_note}",
                        language.choose("共", ""),
                        language.choose("个模型", "models"),
                    )),
            );
            if matches.is_empty() {
                panel = panel.child(
                    div().px_3().py_2().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "没有匹配的模型；可直接输入完整模型名。",
                            "No matching model; the full model name can still be typed.",
                        ),
                    ),
                );
            } else {
                let rendered = matches.len().min(self.model_choices.window);
                // Ten rows stay visible; anything beyond scrolls inside the
                // panel with its own scrollbar. Reaching the loaded bottom —
                // by wheel or by dragging the scrollbar — pages in the next
                // batch of rows.
                let mut list = div()
                    .id("provider-model-list")
                    .debug_selector(|| "provider-model-list".to_owned())
                    .v_flex()
                    .flex_none()
                    .h(px(32. * rendered.min(MODEL_DROPDOWN_ROWS) as f32))
                    .overflow_y_scroll()
                    .track_scroll(&self.model_list_scroll)
                    .when(rendered > MODEL_DROPDOWN_ROWS, |this| this.pr(px(12.)));
                for choice in matches.iter().take(rendered) {
                    let chooser = entity.clone();
                    let selected = current_model == choice.id;
                    let model = choice.id.clone();
                    let row_selector = format!("model-choice-row-{}", choice.id);
                    let name =
                        choice.display_name.clone().unwrap_or_else(|| choice.id.clone());
                    let row_id = choice.id.clone();
                    let is_default = choice.is_default;
                    let vision = choice.supports_vision == Some(true);
                    let vision_unknown = choice.supports_vision.is_none();
                    list = list.child(
                        div()
                            .id(format!("model-choice-{}", choice.id))
                            .debug_selector(move || row_selector.clone())
                            .mx_1()
                            .px_2()
                            .h(px(32.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded_md()
                            .cursor_pointer()
                            .text_sm()
                            .when(selected, |this| {
                                this.bg(rgb(0x000e_7490))
                                    .text_color(rgb(CARD_BG))
                                    .font_weight(FontWeight::SEMIBOLD)
                            })
                            .when(!selected, |this| {
                                this.text_color(rgb(TEXT_PRIMARY))
                                    .hover(|this| this.bg(rgb(SURFACE_BG)))
                            })
                            // Mouse-down, not click: the pick must land before
                            // the blur the same press sends to the model field.
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                let model = model.clone();
                                chooser.update(cx, |view, cx| {
                                    let selected =
                                        view.selected_provider.min(view.providers.len() - 1);
                                    let state = view.providers[selected].model.clone();
                                    state.update(cx, |state, cx| state.set_value(model, window, cx));
                                    view.model_choices.dropdown_open = false;
                                    cx.notify();
                                });
                            })
                            .child(
                                div().flex_1().min_w(px(0.)).truncate().child(if name != row_id {
                                    format!("{name} · {row_id}")
                                } else {
                                    row_id.clone()
                                }),
                            )
                            .when(is_default, |row| {
                                row.child(
                                    div()
                                        .px_1p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .text_color(rgb(0x000e_7490))
                                        .bg(rgb(0x00e0_f2fe))
                                        .child(language.choose("默认", "default")),
                                )
                            })
                            .when(vision, |row| {
                                row.child(
                                    div()
                                        .px_1p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .text_color(rgb(0x0016_a34a))
                                        .bg(rgb(0x00dc_fce7))
                                        .child(language.choose("视觉", "vision")),
                                )
                            })
                            .when(!vision && vision_unknown, |row| {
                                row.child(
                                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose("视觉未知", "vision?"),
                                    ),
                                )
                            }),
                    );
                }
                if matches.len() > rendered {
                    let more_extender = entity.clone();
                    list = list.child(
                        div()
                            .id("provider-model-more")
                            .debug_selector(|| "provider-model-more".to_owned())
                            .mx_1()
                            .px_2()
                            .h(px(28.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .hover(|this| this.bg(rgb(SURFACE_BG)))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                more_extender.update(cx, |view, cx| view.extend_model_window(cx));
                            })
                            .child(format!(
                                "{} {rendered} / {} {}；{}",
                                language.choose("已加载", "loaded"),
                                matches.len(),
                                language.choose("个匹配", "matched"),
                                language.choose("滚动到底部或点此继续加载", "scroll to the bottom or click to load more"),
                            )),
                    );
                }
                // Read the clamped scroll position AFTER layout. Wheel events
                // and scrollbar drags both redraw this view; the scrollbar may
                // consume mouse-move events before the host sees them.
                let page_extender = entity.clone();
                let page_handle = self.model_list_scroll.clone();
                let page_window = self.model_choices.window;
                let generation = self.model_choices.generation;
                let has_more = matches.len() > rendered;
                let list_host = if rendered > MODEL_DROPDOWN_ROWS {
                    div()
                        .relative()
                        .flex_none()
                        .child(list)
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .debug_selector(|| "provider-model-scrollbar".to_owned())
                                .child(
                                    gpui_component::scroll::Scrollbar::vertical(
                                        &self.model_list_scroll,
                                    )
                                    .id("provider-model-scrollbar")
                                    .mode(gpui_component::scroll::ScrollbarMode::Always)
                                    .viewport_from_layout(),
                                ),
                        )
                        .child(
                            gpui::canvas(
                                move |_, _, cx| {
                                    let max = page_handle.max_offset().y;
                                    // GPUI offsets are negative when scrolling down.
                                    if has_more
                                        && max > px(0.)
                                        && max + page_handle.offset().y < px(64.)
                                    {
                                        cx.defer(move |cx| {
                                            page_extender.update(cx, |view, cx| {
                                                if view.model_choices.dropdown_open
                                                    && view.model_choices.generation == generation
                                                    && view.model_choices.window == page_window
                                                {
                                                    view.extend_model_window(cx);
                                                }
                                            });
                                        });
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .into_any_element()
                } else {
                    list.into_any_element()
                };
                panel = panel.child(list_host);
            }
        }
        // Flush with the field: the wrapper holds just the 36px input box, so
        // `top: 36` puts the panel's top edge exactly on the box's bottom-left
        // corner and `left/right: 0` gives it the field's own width. Deferred
        // draw escapes the dialog's scroll clipping; no layout space taken.
        Some(
            deferred(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(36.))
                    .child(panel),
            )
            .into_any_element(),
        )
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
        let providers_changed = matches!(
            &update,
            crate::application::settings_persistence::SettingsUpdate::Providers { .. }
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
        if providers_changed {
            // Endpoints may have changed; let the assistant selector refetch.
            self.model_catalog_fetches.clear();
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
            RuntimeAdapter::KimiCode => SettingsUpdate::Kimi { command, provider_id },
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
            RuntimeAdapter::KimiCode => (&self.kimi_command, &self.kimi_provider),
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
            RuntimeAdapter::KimiCode => {
                (&saved.adapters.kimi_command, &saved.adapters.kimi_provider_id)
            }
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
    /// the page a stable, scannable operation order — connection check, settings,
    /// verification — on every runtime adapter page.
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
            RuntimeAdapter::KimiCode => "kimi-provider",
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
            let compatible = adapter.accepts(provider.connection);
            let exclusive_note = (!compatible).then(|| match provider.connection {
                ProviderConnection::CodexAccount => {
                    language.choose("（仅 Codex App Server 可用）", " (Codex App Server only)")
                }
                ProviderConnection::KimiAccount => {
                    language.choose("（仅 Kimi Code 可用）", " (Kimi Code only)")
                }
                ProviderConnection::ApiKey => "",
            });
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
            let label = match exclusive_note {
                Some(note) => format!("{label}{note}"),
                None => label,
            };
            // Account-direct providers are exclusive to their own runtime, so
            // they render as muted, non-clickable chips on every other
            // adapter — a currently saved (invalid) binding stays visible
            // through its selected border until the user switches it.
            let chip = div()
                .id(format!("bind-{}-{}", adapter.backend_id(), provider.id))
                .px_3()
                .h(px(28.))
                .flex()
                .items_center()
                .rounded_full()
                .text_xs()
                .border_1()
                .border_color(rgb(if selected { 0x000e_7490 } else { BORDER }))
                .when(selected, |this| {
                    this.bg(rgb(0x000e_7490))
                        .text_color(rgb(CARD_BG))
                        .font_weight(FontWeight::SEMIBOLD)
                });
            let chip = if compatible {
                chip.cursor_pointer()
                    .when(!selected, |this| {
                        this.bg(rgb(CARD_BG)).text_color(rgb(TEXT_SECONDARY))
                    })
                    .on_click(move |_, window, cx| {
                        let id = provider.id.clone();
                        chooser.update(cx, |view, cx| {
                            let (_, state) = view.runtime_fields(adapter);
                            state.update(cx, |state, cx| state.set_value(id, window, cx));
                            cx.notify();
                        });
                    })
            } else {
                chip.bg(rgb(CARD_BG)).text_color(rgb(TEXT_MUTED))
            };
            chips = chips.child(chip.child(label));
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

    pub(super) fn open_provider_editor(&mut self, cx: &mut Context<Self>) {
        let index = self.selected_provider.min(self.providers.len() - 1);
        self.provider_editor_snapshot = Some(ProviderEditorSnapshot {
            index,
            provider: self.provider_values(cx)[index].clone(),
            catalogs: self.model_catalogs.clone(),
        });
        self.provider_editor_open = true;
        self.dialog_error = None;
        self.model_choices.dropdown_open = false;
        self.fetch_provider_models(cx);
        cx.notify();
    }

    pub(super) fn cancel_provider_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.provider_editor_open = false;
        if let Some(snapshot) = self.provider_editor_snapshot.take() {
            self.providers[snapshot.index] = Self::provider_fields(window, snapshot.provider, cx);
            self.model_catalogs = snapshot.catalogs;
        }
        // A draft catalog may still be in flight. It must not republish after
        // cancellation and change the restored provider's badges or status.
        self.model_choices = ModelChoicesState {
            generation: self.model_choices.generation + 1,
            ..Default::default()
        };
        self.model_list_scroll.set_offset(gpui::Point::default());
        self.dialog_error = None;
        self.status =
            self.language.choose("已取消 Provider 编辑。", "Provider edit cancelled.").into();
        cx.notify();
    }

    /// Modal editor for the selected provider. Saving the provider list happens inside the
    /// dialog and closes it on success; quick list actions (default, enable, remove) persist
    /// on click, so this save mainly carries the form fields and brand-new providers.
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
        let model_dropdown_open = self.model_choices.dropdown_open;
        let backdrop_closer = entity.clone();
        let connection_api_key = entity.clone();
        let connection_codex_account = entity.clone();
        let connection_kimi_account = entity.clone();
        let direct = matches!(
            provider.connection,
            ProviderConnection::CodexAccount | ProviderConnection::KimiAccount
        );
        let api_key_hint = if direct {
            None
        } else {
            self.secret_source_hint(
                &provider.api_key_environment_variable.read(cx).value(),
                &entity,
            )
        };
        let vision_key_hint = if direct {
            None
        } else {
            self.secret_source_hint(
                &provider.vision_api_key_environment_variable.read(cx).value(),
                &entity,
            )
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
                    .id("provider-editor-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x0000_0f17_2ab3))
                    .occlude()
                    .on_click(move |_, window, cx| {
                        closer.update(cx, |view, cx| view.cancel_provider_editor(window, cx));
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
                                    .on_click(move |_, window, cx| {
                                        closer_top.update(cx, |view, cx| {
                                            view.cancel_provider_editor(window, cx);
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
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(language.choose("连接方式", "Connection")),
                                    )
                                    .child(
                                        action_button("provider-connection-api-key")
                                            .label(language.choose(
                                                "API Key（OpenAI 兼容）",
                                                "API key (OpenAI-compatible)",
                                            ))
                                            .when(
                                                provider.connection == ProviderConnection::ApiKey,
                                                |button| button.primary(),
                                            )
                                            .on_click(move |_, _, cx| {
                                                connection_api_key.update(cx, |view, cx| {
                                                    let selected = view
                                                        .selected_provider
                                                        .min(view.providers.len() - 1);
                                                    if view.providers[selected].connection
                                                        != ProviderConnection::ApiKey
                                                    {
                                                        view.set_provider_connection(
                                                            ProviderConnection::ApiKey,
                                                            cx,
                                                        );
                                                        view.model_choices.dropdown_open = false;
                                                        view.fetch_provider_models(cx);
                                                    }
                                                });
                                            }),
                                    )
                                    .child(
                                        action_button("provider-connection-codex-account")
                                            .label(language.choose(
                                                "Codex 账号直连",
                                                "Codex account direct",
                                            ))
                                            .when(
                                                provider.connection
                                                    == ProviderConnection::CodexAccount,
                                                |button| button.primary(),
                                            )
                                            .on_click(move |_, _, cx| {
                                                connection_codex_account.update(cx, |view, cx| {
                                                    let selected = view
                                                        .selected_provider
                                                        .min(view.providers.len() - 1);
                                                    if view.providers[selected].connection
                                                        != ProviderConnection::CodexAccount
                                                    {
                                                        view.set_provider_connection(
                                                            ProviderConnection::CodexAccount,
                                                            cx,
                                                        );
                                                        view.model_choices.dropdown_open = false;
                                                        view.fetch_provider_models(cx);
                                                    }
                                                });
                                            }),
                                    )
                                    .child(
                                        action_button("provider-connection-kimi-account")
                                            .label(language.choose(
                                                "Kimi 账号直连",
                                                "Kimi account direct",
                                            ))
                                            .when(
                                                provider.connection
                                                    == ProviderConnection::KimiAccount,
                                                |button| button.primary(),
                                            )
                                            .on_click(move |_, _, cx| {
                                                connection_kimi_account.update(cx, |view, cx| {
                                                    let selected = view
                                                        .selected_provider
                                                        .min(view.providers.len() - 1);
                                                    if view.providers[selected].connection
                                                        != ProviderConnection::KimiAccount
                                                    {
                                                        view.set_provider_connection(
                                                            ProviderConnection::KimiAccount,
                                                            cx,
                                                        );
                                                        view.model_choices.dropdown_open = false;
                                                        view.fetch_provider_models(cx);
                                                    }
                                                });
                                            }),
                                    ),
                            )
                            .when(direct, |this| {
                                this.child(info_note(
                                    match provider.connection {
                                        ProviderConnection::CodexAccount => "直连使用本机 Codex 的 ChatGPT 账号登录态，无需 API Key 与 Base URL；模型可留空使用 Codex 默认。请先在终端运行 `codex login` 完成 chatgpt.com 网页授权。仅 Codex App Server 运行时可用。",
                                        ProviderConnection::KimiAccount => "直连使用本机 Kimi Code CLI（kimi）的登录态，无需 API Key 与 Base URL；模型可留空使用 kimi 默认。请先在终端运行 `kimi login` 完成登录。仅 Kimi Code 运行时可用。",
                                        ProviderConnection::ApiKey => unreachable!(
                                            "the API-key connection renders no direct note"
                                        ),
                                    },
                                    match provider.connection {
                                        ProviderConnection::CodexAccount => "Direct mode reuses this machine's Codex ChatGPT sign-in — no API key or base URL needed, and an empty model uses the Codex default. Run `codex login` in a terminal first to authorize via chatgpt.com. Codex App Server runtime only.",
                                        ProviderConnection::KimiAccount => "Direct mode reuses this machine's Kimi Code CLI (`kimi`) sign-in — no API key or base URL needed, and an empty model uses the kimi default. Run `kimi login` in a terminal first. Kimi Code runtime only.",
                                        ProviderConnection::ApiKey => unreachable!(
                                            "the API-key connection renders no direct note"
                                        ),
                                    },
                                    language,
                                ))
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .when(!direct, |row| {
                                        row.child(labeled_field(
                                            "LLM Base URL",
                                            "provider-base-url",
                                            None,
                                            &provider.base_url,
                                        ))
                                    })
                                    .child(self.render_model_field(provider, cx)),
                            )
                            .child(self.render_reasoning_effort_choice(provider, cx))
                            .when(!direct, |this| {
                                let wire_chat = entity.clone();
                                let wire_responses = entity.clone();
                                this.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(language.choose("API 协议", "API protocol")),
                                        )
                                        .child(
                                            action_button("provider-wire-responses")
                                                .label(language.choose(
                                                    "Responses（默认）",
                                                    "Responses (default)",
                                                ))
                                                .when(
                                                    provider.wire_api
                                                        == ProviderWireApi::Responses,
                                                    |button| button.primary(),
                                                )
                                                .on_click(move |_, _, cx| {
                                                    wire_responses.update(cx, |view, cx| {
                                                        view.set_provider_wire_api(
                                                            ProviderWireApi::Responses,
                                                            cx,
                                                        );
                                                    });
                                                }),
                                        )
                                        .child(
                                            action_button("provider-wire-chat")
                                                .label(language.choose(
                                                    "Chat Completions（旧版 Codex）",
                                                    "Chat Completions (legacy codex)",
                                                ))
                                                .when(
                                                    provider.wire_api == ProviderWireApi::Chat,
                                                    |button| button.primary(),
                                                )
                                                .on_click(move |_, _, cx| {
                                                    wire_chat.update(cx, |view, cx| {
                                                        view.set_provider_wire_api(
                                                            ProviderWireApi::Chat,
                                                            cx,
                                                        );
                                                    });
                                                }),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(200.))
                                                .text_xs()
                                                .text_color(rgb(TEXT_MUTED))
                                                .whitespace_normal()
                                                .child(language.choose(
                                                    "新版 Codex CLI 仅支持 Responses（z.ai 在 https://api.z.ai/api/v1 提供 Responses 端点）；仅旧版 Codex 可选 Chat Completions。",
                                                    "Modern codex-cli supports Responses only (z.ai serves it at https://api.z.ai/api/v1); Chat Completions is for legacy codex builds.",
                                                )),
                                        ),
                                )
                            })
                            .when(!direct, |this| {
                                this.child(labeled_field(
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
                            })
                            .when_some(api_key_hint, ParentElement::child)
                            .when(!direct, |this| {
                                this.child(
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
                                                .label(if provider.native_vision {
                                                    language.choose(
                                                        "LLM 本身支持视觉：是",
                                                        "LLM sees images natively: yes",
                                                    )
                                                } else {
                                                    language.choose(
                                                        "LLM 本身支持视觉：否",
                                                        "LLM sees images natively: no",
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
                            })
                            .when(!direct, |this| {
                                this.child(
                                    div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal()
                                        .child(language.choose(
                                            "「是」表示 LLM 本身能直接处理图像，图片直接发给主模型，无需下方独立配置；「否」表示图片需经下方独立 Vision 模型处理。",
                                            "Yes means the LLM itself processes images, so they go straight to the main model; no means images are handled by the separate vision model below.",
                                        )),
                                )
                            })
                            .when(!direct && provider.native_vision, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .whitespace_normal()
                                        .child(language.choose(
                                            "LLM 本身处理图像：下方独立 Vision 配置已灰掉停用，原值保留不丢失，关闭本开关后可继续编辑。",
                                            "The LLM handles images itself: the separate vision fields below are grayed out and disabled; their values are kept and editable again once this switch is off.",
                                        )),
                                )
                            })
                            .when(!direct, |this| {
                                this.child(
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
                            })
                            .when(!direct, |this| {
                                this.child(labeled_field(
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
                            })
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
                                    "取消会丢弃本次编辑，恢复原有内容。",
                                    "Cancel discards this edit and restores the original values.",
                                ),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        action_button("provider-editor-cancel")
                                            .label(language.choose("取消", "Cancel"))
                                            .on_click(move |_, window, cx| {
                                                saver_cancel.update(cx, |view, cx| {
                                                    view.cancel_provider_editor(window, cx);
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
                                                            view.provider_editor_snapshot = None;
                                                            view.model_choices.dropdown_open = false;
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
                    )
                    // Click-outside dismissal for the model dropdown: this
                    // card-sized backdrop paints above the dialog's normal
                    // content but below the deferred panel, so any click that
                    // does not land on the panel closes the dropdown first.
                    .when(model_dropdown_open, |card| {
                        card.child(
                            div()
                                .id("model-dropdown-backdrop")
                                .debug_selector(|| "model-dropdown-backdrop".to_owned())
                                .absolute()
                                .inset_0()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    backdrop_closer.update(cx, |view, cx| {
                                        view.close_model_dropdown(cx);
                                    });
                                }),
                        )
                    }),
            )
    }

    #[allow(clippy::too_many_lines)]
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
            RuntimeAdapter::Dsh | RuntimeAdapter::CodexAppServer | RuntimeAdapter::KimiCode => {
                language.choose(
                    "每次执行创建独立任务进程；完成、失败或取消后清理。",
                    "Each run creates an isolated task process, cleaned up on completion, failure, or cancellation.",
                )
            }
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
                    )
                    .child(self.render_runtime_header_status(adapter, cx)),
            )
            .child(self.render_runtime_check_section(adapter, "1", cx))
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
        let direct = matches!(
            provider.connection,
            ProviderConnection::CodexAccount | ProviderConnection::KimiAccount
        );
        let connection_summary = match provider.connection {
            ProviderConnection::CodexAccount => {
                language.choose("Codex 账号直连", "Codex account direct")
            }
            ProviderConnection::KimiAccount => {
                language.choose("Kimi 账号直连", "Kimi account direct")
            }
            ProviderConnection::ApiKey => {
                language.choose("API Key（OpenAI 兼容）", "API key (OpenAI-compatible)")
            }
        };
        let api_key_hint = if direct {
            None
        } else {
            self.secret_source_hint(
                &provider.api_key_environment_variable.read(cx).value(),
                &entity,
            )
        };

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
            .debug_selector(|| "provider-vision-badge".to_owned())
            .child("Vision");
        // API-key providers badge from their own vision configuration;
        // account-direct providers have none of that UI, so their badge comes
        // from the fetched model catalog: the selected model (or the catalog
        // default when the field is empty) reporting vision support.
        let vision_capable = if direct {
            self.direct_model_supports_vision(provider, cx)
        } else {
            provider.native_vision || !provider.vision_model.read(cx).value().trim().is_empty()
        };

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
                            .when(vision_capable, |row| row.child(vision_badge)),
                    ),
            )
            .child(match provider.connection {
                ProviderConnection::CodexAccount => info_note(
                    "直连使用本机 Codex 的 ChatGPT 账号登录态，无需 API Key 与 Base URL；模型可留空使用 Codex 默认。仅 Codex 运行时可用；请先在终端运行 `codex login` 完成 chatgpt.com 网页授权。",
                    "Direct mode reuses this machine's Codex ChatGPT sign-in — no API key or base URL needed, and an empty model uses the Codex default. Codex runtime only; run `codex login` in a terminal first to authorize via chatgpt.com.",
                    language,
                ),
                ProviderConnection::KimiAccount => info_note(
                    "直连使用本机 Kimi Code CLI（kimi）的登录态，无需 API Key 与 Base URL；模型可留空使用 kimi 默认。仅 Kimi 运行时可用；请先在终端运行 `kimi login` 完成登录。",
                    "Direct mode reuses this machine's Kimi Code CLI (`kimi`) sign-in — no API key or base URL needed, and an empty model uses the kimi default. Kimi runtime only; run `kimi login` in a terminal first.",
                    language,
                ),
                ProviderConnection::ApiKey => info_note(
                    "Provider 配置只保存 API Key 变量名；密钥值在密钥保险库中单独加密保存，也可由进程环境提供。",
                    "Provider configuration saves API key variable names only; values are stored separately in the encrypted vault or supplied by the process environment.",
                    language,
                ),
            })
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
                        language.choose("连接方式", "Connection"),
                        connection_summary.to_owned(),
                    ))
                    .child(settings_summary_row(
                        "Base URL",
                        if direct {
                            "—".to_owned()
                        } else {
                            provider.base_url.read(cx).value().to_string()
                        },
                    ))
                    .child(settings_summary_row(
                        language.choose("模型", "Model"),
                        provider.model.read(cx).value().to_string(),
                    ))
                    .child(settings_summary_row(
                        language.choose("API Key", "API key"),
                        if direct {
                            language.choose("（无需配置）", "(not required)").to_owned()
                        } else {
                            provider.api_key_environment_variable.read(cx).value().to_string()
                        },
                    ))
                    .when_some(api_key_hint, ParentElement::child)
                    .child({
                        let vision_summary = if direct {
                            "—".to_owned()
                        } else {
                            let vision_model =
                                provider.vision_model.read(cx).value().to_string();
                            if provider.native_vision {
                                language
                                    .choose(
                                        "LLM 本身支持视觉（图像直发主模型）",
                                        "LLM sees images natively (images go to the main model)",
                                    )
                                    .to_owned()
                            } else if vision_model.trim().is_empty() {
                                language
                                    .choose("未配置（图片任务不可用）", "Not configured (image tasks unavailable)")
                                    .to_owned()
                            } else {
                                format!(
                                    "{}（{vision_model}）",
                                    language.choose("独立 Vision 模型", "Separate vision model")
                                )
                            }
                        };
                        settings_summary_row("Vision", vision_summary)
                    }),
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
                        "默认项、启停与删除点击后立即保存；表单修改仍在「编辑 Provider…」弹窗中保存。默认项用于未指定 Provider 的运行时。",
                        "Default, enable, and removal clicks save immediately; form edits still save in the Edit provider dialog. The default applies to runtimes without an explicit provider binding.",
                    ),
                ),
            )
            .child(
                action_button("open-provider-editor")
                    .primary()
                    .label(language.choose("编辑 Provider…", "Edit provider…"))
                    .on_click(move |_, _, cx| {
                        provider_saver.update(cx, |view, cx| {
                            view.open_provider_editor(cx);
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
            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                "这些偏好会立即应用，并在下次启动时恢复。",
                "These preferences apply immediately and are restored on the next launch.",
            )))
            .child(appearance)
            .child(storage)
            .child(about)
    }
}

/// Shared catalog source for the editor's automatic fetch and the startup
/// prefetch of account-direct providers: one future per provider, resolved
/// off the UI thread. The blocking sources (app-server launch, CLI call, HTTP
/// request) are exactly what `background_spawn` is for.
pub(super) async fn fetch_model_catalog(
    provider: LlmProviderSettings,
    codex_settings: CodexAppServerSettings,
    kimi_command: String,
    secrets: Option<circuitfabric_codex_runtime::secrets::SecretValues>,
) -> Result<Vec<circuitfabric_codex_runtime::models::ModelChoice>, String> {
    match provider.connection {
        ProviderConnection::CodexAccount => circuitfabric_codex_runtime::models::list_codex_models(
            &codex_settings,
            &provider,
            secrets.as_ref(),
        ),
        ProviderConnection::KimiAccount => {
            circuitfabric_codex_runtime::models::list_kimi_models(&kimi_command)
        }
        ProviderConnection::ApiKey => {
            circuitfabric_codex_runtime::models::list_endpoint_models(&provider, secrets.as_ref())
        }
    }
    .map_err(|error| error.to_string())
}
