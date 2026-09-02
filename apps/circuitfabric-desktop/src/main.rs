//! Desktop control-plane entry point.
//!
//! The native GPUI view is feature-gated so the semantic core and plugins can be developed and
//! tested without a graphics stack. Enable it with `--features native-ui` after fetching the
//! GPUI dependencies pinned in `Cargo.lock`.

#![deny(unsafe_code)]

use circuitfabric_contracts::ProjectId;

#[cfg(all(feature = "native-ui", windows))]
#[allow(unsafe_code)]
mod windows_icon;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlPlaneScreen {
    Projects,
    Documents,
    EdaServices,
    AgentsAndMcp,
    SessionsAndTasks,
    Usage,
    SemanticQueryAndBom,
}

#[derive(Debug, Default)]
pub struct DesktopShell {
    selected_project: Option<ProjectId>,
}

impl DesktopShell {
    #[must_use]
    pub fn selected_project(&self) -> Option<&str> {
        self.selected_project.as_deref()
    }

    pub fn select_project(&mut self, project_id: ProjectId) {
        self.selected_project = Some(project_id);
    }
}

#[cfg(not(feature = "native-ui"))]
fn main() {
    println!("CircuitFabric desktop scaffold. Rebuild with --features native-ui to start GPUI.");
}

#[cfg(feature = "native-ui")]
#[allow(clippy::too_many_lines)]
fn main() {
    use std::sync::Arc;

    use circuitfabric_codex_runtime::{LlmProviderSettings, RuntimeSettings};
    use gpui::{
        AppContext, Context, Entity, Image, ImageFormat, InteractiveElement, IntoElement,
        ParentElement, Render, Styled, Window, WindowOptions, div, img, px, rgb,
    };
    use gpui_component::{
        Root, StyledExt,
        button::{Button, ButtonVariants},
        input::{Input, InputState},
    };

    const APP_LOGO: &[u8] =
        include_bytes!("../../../assets/branding/circuitfabric-logo-v3-framed-transparent.png");

    struct ProviderFields {
        id: Entity<InputState>,
        name: Entity<InputState>,
        base_url: Entity<InputState>,
        model: Entity<InputState>,
        api_key_environment_variable: Entity<InputState>,
        supports_vision: bool,
        vision_base_url: Entity<InputState>,
        vision_model: Entity<InputState>,
        vision_api_key_environment_variable: Entity<InputState>,
        enabled: bool,
    }

    struct ControlPlaneView {
        logo: Arc<Image>,
        command: Entity<InputState>,
        working_directory: Entity<InputState>,
        bridge_address: Entity<InputState>,
        providers: Vec<ProviderFields>,
        default_provider_id: String,
        selected_provider: usize,
        settings_path: std::path::PathBuf,
        status: String,
    }

    impl ControlPlaneView {
        fn input(
            window: &mut Window,
            value: String,
            placeholder: &'static str,
            cx: &mut Context<Self>,
        ) -> Entity<InputState> {
            cx.new(|cx| InputState::new(window, cx).default_value(value).placeholder(placeholder))
        }

        fn provider_fields(
            window: &mut Window,
            provider: LlmProviderSettings,
            cx: &mut Context<Self>,
        ) -> ProviderFields {
            ProviderFields {
                id: Self::input(window, provider.id, "provider-id", cx),
                name: Self::input(window, provider.name, "Provider name", cx),
                base_url: Self::input(window, provider.base_url, "https://api.example.com/v1", cx),
                model: Self::input(window, provider.model, "model-name", cx),
                api_key_environment_variable: Self::input(
                    window,
                    provider.api_key_environment_variable,
                    "API_KEY_ENVIRONMENT_VARIABLE",
                    cx,
                ),
                supports_vision: provider.supports_vision,
                vision_base_url: Self::input(
                    window,
                    provider.vision_base_url.unwrap_or_default(),
                    "https://api.example.com/v1",
                    cx,
                ),
                vision_model: Self::input(
                    window,
                    provider.vision_model.unwrap_or_default(),
                    "vision-model-name",
                    cx,
                ),
                vision_api_key_environment_variable: Self::input(
                    window,
                    provider.vision_api_key_environment_variable.unwrap_or_default(),
                    "VISION_API_KEY_ENVIRONMENT_VARIABLE",
                    cx,
                ),
                enabled: provider.enabled,
            }
        }

        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let settings_path = RuntimeSettings::default_path();
            let settings = RuntimeSettings::load_or_default(&settings_path).unwrap_or_default();
            let providers = settings
                .providers
                .iter()
                .cloned()
                .map(|provider| Self::provider_fields(window, provider, cx))
                .collect();
            Self {
                logo: Arc::new(Image::from_bytes(ImageFormat::Png, APP_LOGO.to_vec())),
                command: Self::input(window, settings.codex.command, "codex", cx),
                working_directory: Self::input(
                    window,
                    settings.codex.working_directory.display().to_string(),
                    "working directory",
                    cx,
                ),
                bridge_address: Self::input(
                    window,
                    settings.bridge.listen_address,
                    "127.0.0.1:49630",
                    cx,
                ),
                providers,
                default_provider_id: settings.default_provider_id,
                selected_provider: 0,
                settings_path,
                status: "尚未保存。Provider 只保存 API Key 环境变量名，不会保存密钥值。".to_owned(),
            }
        }

        fn provider_values(&self, cx: &Context<Self>) -> Vec<LlmProviderSettings> {
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

        fn optional_value(value: String) -> Option<String> {
            (!value.trim().is_empty()).then_some(value)
        }

        fn add_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.status = "已添加 Provider，请填写配置后保存。".to_owned();
            cx.notify();
        }

        fn remove_provider(&mut self, cx: &mut Context<Self>) {
            if self.providers.len() <= 1 {
                self.status = "至少保留一个 Provider。".to_owned();
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

        fn set_default_provider(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.default_provider_id = self.providers[selected].id.read(cx).value().to_string();
            self.status =
                format!("默认 Provider 已设为 `{}`，保存后生效。", self.default_provider_id);
            cx.notify();
        }

        fn toggle_provider(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.providers[selected].enabled = !self.providers[selected].enabled;
            self.status = format!(
                "Provider `{}` 已{}。",
                self.providers[selected].id.read(cx).value(),
                if self.providers[selected].enabled { "启用" } else { "停用" }
            );
            cx.notify();
        }

        fn toggle_vision(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.providers[selected].supports_vision = !self.providers[selected].supports_vision;
            self.status = format!(
                "Provider `{}` 的 Vision 已{}。",
                self.providers[selected].id.read(cx).value(),
                if self.providers[selected].supports_vision { "启用" } else { "停用" }
            );
            cx.notify();
        }

        fn save_settings(&mut self, cx: &mut Context<Self>) {
            let providers = self.provider_values(cx);
            if providers.is_empty() {
                self.status = "未保存：至少需要一个 Provider。".to_owned();
                cx.notify();
                return;
            }
            let default_provider_id =
                if providers.iter().any(|provider| provider.id == self.default_provider_id) {
                    self.default_provider_id.clone()
                } else {
                    providers[0].id.clone()
                };
            let mut settings = RuntimeSettings::default();
            settings.codex.command = self.command.read(cx).value().to_string();
            settings.codex.working_directory =
                self.working_directory.read(cx).value().to_string().into();
            if let Some(provider) =
                providers.iter().find(|provider| provider.id == default_provider_id)
            {
                settings.codex.model = Some(provider.model.clone());
                settings.codex.api_key_environment_variable =
                    provider.api_key_environment_variable.clone();
            }
            settings.default_provider_id = default_provider_id.clone();
            settings.providers = providers;
            settings.bridge.listen_address = self.bridge_address.read(cx).value().to_string();
            self.status = match settings.save(&self.settings_path) {
                Ok(()) => format!(
                    "已保存到 {}。默认 Provider：{}。现在可启动 circuitfabric-jlc-bridge。",
                    self.settings_path.display(),
                    default_provider_id
                ),
                Err(error) => format!("未保存：{error}"),
            };
            self.default_provider_id = default_provider_id;
            cx.notify();
        }

        fn field(
            label: &'static str,
            id: &'static str,
            state: &Entity<InputState>,
        ) -> impl IntoElement {
            div().v_flex().gap_1().child(div().text_sm().child(label)).child(
                div()
                    .id(id)
                    .w_full()
                    .h_8()
                    .px_2()
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(rgb(0x00cb_d5e1))
                    .child(Input::new(state)),
            )
        }
    }

    impl Render for ControlPlaneView {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let selected = self.selected_provider.min(self.providers.len() - 1);
            let provider = &self.providers[selected];
            let selected_provider_id = provider.id.read(cx).value().to_string();
            let selected_is_default = selected_provider_id == self.default_provider_id;
            let mut provider_list = div().v_flex().gap_1();
            for (index, provider) in self.providers.iter().enumerate() {
                let provider_id = provider.id.read(cx).value().to_string();
                let label = if provider_id == self.default_provider_id {
                    format!("★ {provider_id} (默认)")
                } else {
                    provider_id
                };
                let selector = entity.clone();
                provider_list = provider_list.child(
                    Button::new(format!("select-provider-{index}")).label(label).on_click(
                        move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_provider = index;
                                cx.notify();
                            });
                        },
                    ),
                );
            }
            let add_provider = entity.clone();
            let remove_provider = entity.clone();
            let set_default = entity.clone();
            let toggle_provider = entity.clone();
            let toggle_vision = entity.clone();
            div().v_flex().size_full().items_center().justify_center().bg(rgb(0x00f4_f7ff)).child(
                div()
                    .v_flex()
                    .items_center()
                    .gap_3()
                    .p_8()
                    .bg(rgb(0x00ff_ffff))
                    .rounded_xl()
                    .shadow_lg()
                    .child(img(self.logo.clone()).size(px(176.)))
                    .child(div().text_xl().child("CircuitFabric"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x004b_5563))
                            .child("Codex App Server + JLC EDA local bridge"),
                    )
                    .child(
                        div()
                            .w(px(720.))
                            .v_flex()
                            .gap_3()
                            .child(Self::field("Codex command", "codex-command", &self.command))
                            .child(Self::field(
                                "Working directory",
                                "working-directory",
                                &self.working_directory,
                            ))
                            .child(Self::field(
                                "JLC bridge address",
                                "bridge-address",
                                &self.bridge_address,
                            ))
                            .child(div().text_lg().child("LLM Provider 管理"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x004b_5563))
                                    .child("可添加多个 OpenAI-compatible Provider；API Key 只填写环境变量名，不在此界面保存密钥值。"),
                            )
                            .child(provider_list)
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("add-provider")
                                            .label("添加 Provider")
                                            .on_click(move |_, window, cx| {
                                                add_provider.update(cx, |view, cx| {
                                                    view.add_provider(window, cx);
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new("remove-provider")
                                            .label("删除当前")
                                            .on_click(move |_, _, cx| {
                                                remove_provider.update(cx, ControlPlaneView::remove_provider);
                                            }),
                                    ),
                            )
                            .child(div().text_sm().child(format!("当前编辑：{}", selected_provider_id)))
                            .child(Self::field("Provider ID", "provider-id", &provider.id))
                            .child(Self::field("显示名称", "provider-name", &provider.name))
                            .child(Self::field("LLM Base URL", "provider-base-url", &provider.base_url))
                            .child(Self::field("LLM Model", "provider-model", &provider.model))
                            .child(Self::field(
                                "LLM API Key 环境变量名",
                                "provider-api-key-env",
                                &provider.api_key_environment_variable,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("toggle-provider")
                                            .label(if provider.enabled { "当前：已启用" } else { "当前：已停用" })
                                            .on_click(move |_, _, cx| {
                                                toggle_provider.update(cx, ControlPlaneView::toggle_provider);
                                            }),
                                    )
                                    .child(
                                        Button::new("set-default-provider")
                                            .label(if selected_is_default { "当前为默认 Provider" } else { "设为默认 Provider" })
                                            .on_click(move |_, _, cx| {
                                                set_default.update(cx, ControlPlaneView::set_default_provider);
                                            }),
                                    ),
                            )
                            .child(div().text_lg().child("Vision 配置"))
                            .child(Self::field(
                                "Vision Base URL",
                                "vision-base-url",
                                &provider.vision_base_url,
                            ))
                            .child(Self::field("Vision LLM Model", "vision-model", &provider.vision_model))
                            .child(Self::field(
                                "Vision API Key 环境变量名",
                                "vision-api-key-env",
                                &provider.vision_api_key_environment_variable,
                            ))
                            .child(
                                Button::new("toggle-vision")
                                    .label(if provider.supports_vision {
                                        "Vision：已启用"
                                    } else {
                                        "Vision：已停用"
                                    })
                                    .on_click(move |_, _, cx| {
                                        toggle_vision.update(cx, ControlPlaneView::toggle_vision);
                                    }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x004b_5563))
                                    .child(self.status.clone()),
                            )
                            .child(
                                Button::new("save-runtime")
                                    .primary()
                                    .label("Save App Server settings")
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, ControlPlaneView::save_settings);
                                    }),
                            ),
                    ),
            )
        }
    }

    gpui_platform::application().run(move |cx| {
        gpui_component::init(cx);
        cx.set_app_identity("io.circuitfabric.desktop", "CircuitFabric");
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    app_id: Some("io.circuitfabric.desktop".to_owned()),
                    ..Default::default()
                },
                |window, cx| {
                    #[cfg(windows)]
                    windows_icon::apply(window)
                        .expect("failed to apply the CircuitFabric Windows window icon");

                    let view = cx.new(|cx| ControlPlaneView::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open CircuitFabric desktop window");
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_project_is_shell_state_not_circuit_state() {
        let mut shell = DesktopShell::default();
        shell.select_project("project-1".to_owned());

        assert_eq!(shell.selected_project(), Some("project-1"));
    }
}
