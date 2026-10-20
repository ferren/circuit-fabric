//! The assistant's runtime cascader: runtime → provider → model → reasoning effort.
//!
//! The choice applies to assistant turns only and is saved with the settings, so it
//! survives restarts. Every level can follow the global configuration instead.
use super::*;
use circuitfabric_codex_runtime::{
    AssistantSelection, ReasoningEffort,
    execution::{selected_provider, supports_reasoning_effort},
};
use gpui::App;

const COLUMN_WIDTH: f32 = 184.;
const COLUMN_MAX_HEIGHT: f32 = 300.;
/// Rows rendered per catalog; larger catalogs (e.g. `OpenRouter`) are cut here.
const CATALOG_ROWS: usize = 300;

impl ControlPlaneView {
    fn update_assistant_selection(
        &mut self,
        update: impl FnOnce(&mut AssistantSelection),
        cx: &mut Context<Self>,
    ) {
        let mut selection = self.saved_settings.assistant.clone();
        update(&mut selection);
        if selection == self.saved_settings.assistant {
            return;
        }
        let update = crate::application::settings_persistence::SettingsUpdate::Assistant(selection);
        if let Err(error) = self.save_update(update) {
            self.status = format!("助手模型选择未保存：{error}");
        }
        cx.notify();
    }

    /// Starts one background catalog fetch for a saved provider the selector
    /// shows, unless its catalog is already cached, loading, or failed.
    fn ensure_model_catalog(&mut self, provider_id: &str, cx: &mut Context<Self>) {
        if self.model_catalogs.contains_key(provider_id)
            || self.model_catalog_fetches.contains_key(provider_id)
        {
            return;
        }
        let Some(provider) =
            self.saved_settings.providers.iter().find(|p| p.id == provider_id).cloned()
        else {
            return;
        };
        self.model_catalog_fetches.insert(provider.id.clone(), None);
        let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
            .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
            .flatten();
        let provider_id = provider.id.clone();
        let fetch = cx.background_spawn(super::settings::fetch_model_catalog(
            provider,
            self.saved_settings.codex.clone(),
            self.saved_settings.adapters.kimi_command.clone(),
            secrets,
        ));
        cx.spawn(async move |view, cx| {
            let outcome = fetch.await;
            view.update(cx, |view, cx| {
                match outcome {
                    Ok(choices) => {
                        view.model_catalog_fetches.remove(&provider_id);
                        view.model_catalogs.insert(provider_id, choices);
                    }
                    Err(error) => {
                        view.model_catalog_fetches.insert(provider_id, Some(error));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The cascader trigger. The popup opens toward the panel's interior: above
    /// the trigger unless the panel hangs from the top edge, aligned to the side
    /// the panel is docked against.
    pub(super) fn render_assistant_model_selector(
        &mut self,
        dock_anchor: gpui::Anchor,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.language;
        let (kind, effective) = self.saved_settings.for_assistant();
        let adapter = RuntimeAdapter::from_agent_kind(kind);
        let provider = selected_provider(&effective, kind).cloned();
        if let Some(provider) = &provider {
            self.ensure_model_catalog(&provider.id, cx);
        }
        let model = provider.as_ref().map_or("—", |p| {
            if p.model.trim().is_empty() {
                language.choose("默认模型", "default model")
            } else {
                &p.model
            }
        });
        let effort = if supports_reasoning_effort(kind) {
            reasoning_effort_label(provider.as_ref().and_then(|p| p.reasoning_effort), language)
        } else {
            language.choose("无推理等级", "no effort")
        };
        let label = format!(
            "{} / {} / {model} / {effort}",
            adapter.short_label(),
            provider.as_ref().map_or("—", |p| p.id.as_str()),
        );
        let entity = cx.entity().clone();
        gpui_component::popover::Popover::new("assistant-model-selector")
            .anchor(dock_anchor)
            .trigger(
                Button::new("assistant-model-selector-trigger")
                    .debug_selector(|| "assistant-model-selector-trigger".into())
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronsUpDown)
                    .label(label.clone())
                    .tooltip(label),
            )
            .content(move |_, _, cx| entity.read(cx).render_assistant_selector_panel(&entity))
    }

    #[allow(clippy::too_many_lines)]
    fn render_assistant_selector_panel(&self, entity: &Entity<Self>) -> Div {
        let language = self.language;
        let saved = &self.saved_settings;
        let selection = &saved.assistant;
        let default_kind = saved.adapters.default_runtime;
        let (kind, effective) = saved.for_assistant();
        let adapter = RuntimeAdapter::from_agent_kind(kind);
        let provider = selected_provider(&effective, kind).cloned();

        // Runtime: switching to another runtime drops the provider and model,
        // which may not suit the new runtime.
        let mut runtimes = vec![selector_item(
            "assistant-runtime-follow",
            format!(
                "{}（{}）",
                language.choose("跟随默认运行时", "Follow default runtime"),
                RuntimeAdapter::from_agent_kind(default_kind).short_label()
            ),
            selection.runtime.is_none(),
            true,
            selection_setter(entity, move |s| {
                if default_kind != kind {
                    s.provider_id.clear();
                    s.model.clear();
                }
                s.runtime = None;
            }),
        )];
        for candidate in RuntimeAdapter::ALL {
            let candidate_kind = candidate.agent_kind();
            runtimes.push(selector_item(
                format!("assistant-runtime-{}", candidate.backend_id()),
                candidate.label().to_owned(),
                selection.runtime == Some(candidate_kind),
                true,
                selection_setter(entity, move |s| {
                    if candidate_kind != kind {
                        s.provider_id.clear();
                        s.model.clear();
                    }
                    s.runtime = Some(candidate_kind);
                }),
            ));
        }

        // Provider: the runtime's binding, or any enabled provider it accepts.
        let binding = selected_provider(saved, kind);
        let mut providers = vec![selector_item(
            "assistant-provider-binding",
            format!(
                "{}（{}）",
                language.choose("运行时绑定", "Runtime binding"),
                binding.map_or(language.choose("不可用", "unavailable"), |p| p.id.as_str())
            ),
            selection.provider_id.is_empty()
                || !saved.providers.iter().any(|p| p.id == selection.provider_id && p.enabled),
            true,
            selection_setter(entity, |s| {
                if !s.provider_id.is_empty() {
                    s.provider_id.clear();
                    s.model.clear();
                }
            }),
        )];
        for candidate in
            saved.providers.iter().filter(|p| p.enabled && adapter.accepts(p.connection))
        {
            let id = candidate.id.clone();
            providers.push(selector_item(
                format!("assistant-provider-{id}"),
                if candidate.name == candidate.id {
                    candidate.id.clone()
                } else {
                    format!("{}（{}）", candidate.name, candidate.id)
                },
                selection.provider_id == candidate.id,
                true,
                selection_setter(entity, move |s| {
                    if s.provider_id != id {
                        s.provider_id.clone_from(&id);
                        s.model.clear();
                    }
                }),
            ));
        }

        // Model: the provider's own model, or any model its catalog lists.
        let mut models = Vec::new();
        if let Some(provider) = &provider {
            let own_model =
                saved.providers.iter().find(|p| p.id == provider.id).map_or("", |p| p.model.trim());
            models.push(selector_item(
                "assistant-model-provider-default",
                format!(
                    "{}（{}）",
                    language.choose("Provider 默认", "Provider default"),
                    if own_model.is_empty() {
                        language.choose("运行时默认", "runtime default")
                    } else {
                        own_model
                    }
                ),
                selection.model.is_empty(),
                true,
                selection_setter(entity, |s| s.model.clear()),
            ));
            let catalog = self.model_catalogs.get(&provider.id);
            if !selection.model.is_empty()
                && catalog.is_none_or(|choices| choices.iter().all(|c| c.id != selection.model))
            {
                models.push(selector_item(
                    "assistant-model-current",
                    selection.model.clone(),
                    true,
                    true,
                    selection_setter(entity, |_| {}),
                ));
            }
            match (catalog, self.model_catalog_fetches.get(&provider.id)) {
                (Some(choices), _) => {
                    for (index, choice) in choices.iter().take(CATALOG_ROWS).enumerate() {
                        let id = choice.id.clone();
                        models.push(selector_item(
                            format!("assistant-model-{index}"),
                            choice.display_name.clone().unwrap_or_else(|| choice.id.clone()),
                            selection.model == choice.id,
                            true,
                            selection_setter(entity, move |s| s.model.clone_from(&id)),
                        ));
                    }
                }
                (None, Some(Some(error))) => models.push(selector_note(format!(
                    "{}：{error}",
                    language.choose("模型列表获取失败", "Model list unavailable")
                ))),
                (None, _) => models.push(selector_note(
                    language.choose("正在获取模型列表…", "Fetching model list…").to_owned(),
                )),
            }
        } else {
            models.push(selector_note(
                language.choose("没有可用的 Provider", "No provider available").to_owned(),
            ));
        }

        // Reasoning effort: only Codex and Claude Code forward it.
        let supported = supports_reasoning_effort(kind);
        let provider_effort = provider.as_ref().and_then(|p| {
            saved.providers.iter().find(|saved| saved.id == p.id).and_then(|p| p.reasoning_effort)
        });
        let mut efforts = vec![selector_item(
            "assistant-effort-provider-default",
            format!(
                "{}（{}）",
                language.choose("Provider 默认", "Provider default"),
                reasoning_effort_label(provider_effort, language)
            ),
            selection.reasoning_effort.is_none(),
            supported,
            selection_setter(entity, |s| s.reasoning_effort = None),
        )];
        for effort in ReasoningEffort::ALL {
            efforts.push(selector_item(
                format!("assistant-effort-{}", effort.value()),
                reasoning_effort_label(Some(effort), language).to_owned(),
                selection.reasoning_effort == Some(effort),
                supported,
                selection_setter(entity, move |s| s.reasoning_effort = Some(effort)),
            ));
        }
        if !supported {
            efforts.push(selector_note(language.choose_owned(
                format!("{} 不支持推理等级", adapter.label()),
                format!("{} has no reasoning effort", adapter.label()),
            )));
        }

        div()
            .debug_selector(|| "assistant-model-selector-panel".into())
            .flex()
            .gap_1()
            .child(selector_column(
                "assistant-selector-runtime",
                language.choose("运行时", "Runtime"),
                runtimes,
            ))
            .child(selector_column(
                "assistant-selector-provider",
                language.choose("LLM Provider", "LLM provider"),
                providers,
            ))
            .child(selector_column(
                "assistant-selector-model",
                language.choose("模型", "Model"),
                models,
            ))
            .child(selector_column(
                "assistant-selector-effort",
                language.choose("推理等级", "Reasoning effort"),
                efforts,
            ))
    }
}

type ItemHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

fn selection_setter(
    entity: &Entity<ControlPlaneView>,
    update: impl Fn(&mut AssistantSelection) + 'static,
) -> ItemHandler {
    let entity = entity.clone();
    Box::new(move |_, cx| {
        entity.update(cx, |view, cx| view.update_assistant_selection(&update, cx));
    })
}

fn selector_column(id: &'static str, title: &'static str, items: Vec<AnyElement>) -> Div {
    div()
        .w(px(COLUMN_WIDTH))
        .flex_none()
        .v_flex()
        .gap_1()
        .child(
            div()
                .px_2()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_MUTED))
                .child(title),
        )
        .child(
            div()
                .id(id)
                .max_h(px(COLUMN_MAX_HEIGHT))
                .overflow_y_scroll()
                .v_flex()
                .gap_0p5()
                .children(items),
        )
}

fn selector_item(
    id: impl Into<String>,
    label: String,
    selected: bool,
    enabled: bool,
    on_click: ItemHandler,
) -> AnyElement {
    let tooltip = label.clone();
    let id: String = id.into();
    let selector = id.clone();
    div()
        .id(gpui::ElementId::Name(id.into()))
        .debug_selector(move || selector.clone())
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .truncate()
        .when(selected, |this| {
            this.bg(rgb(0x00f0_f9ff)).text_color(rgb(ACCENT)).font_weight(FontWeight::SEMIBOLD)
        })
        .when(!enabled, |this| this.text_color(rgb(TEXT_MUTED)).opacity(0.6))
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(|this| this.bg(rgb(SURFACE_BG)))
                .on_click(move |_, window, cx| on_click(window, cx))
        })
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(label)
        .into_any_element()
}

fn selector_note(text: String) -> AnyElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .text_color(rgb(TEXT_MUTED))
        .whitespace_normal()
        .child(text)
        .into_any_element()
}

/// The popup corner for a dock: it opens toward the panel's interior.
pub(super) const fn selector_anchor(dock_is_left: bool, dock_is_top: bool) -> gpui::Anchor {
    match (dock_is_left, dock_is_top) {
        (true, _) => gpui::Anchor::BottomLeft,
        (false, true) => gpui::Anchor::TopRight,
        (false, false) => gpui::Anchor::BottomRight,
    }
}
