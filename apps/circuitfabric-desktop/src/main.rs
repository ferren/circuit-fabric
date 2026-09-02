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

    use circuitfabric_codex_runtime::RuntimeSettings;
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

    struct ControlPlaneView {
        logo: Arc<Image>,
        command: Entity<InputState>,
        working_directory: Entity<InputState>,
        model: Entity<InputState>,
        api_key_environment_variable: Entity<InputState>,
        bridge_address: Entity<InputState>,
        settings_path: std::path::PathBuf,
        status: String,
    }

    impl ControlPlaneView {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let settings_path = RuntimeSettings::default_path();
            let settings = RuntimeSettings::load_or_default(&settings_path).unwrap_or_default();
            let mut field = |value: String, placeholder: &'static str, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    InputState::new(window, cx).default_value(value).placeholder(placeholder)
                })
            };
            Self {
                logo: Arc::new(Image::from_bytes(ImageFormat::Png, APP_LOGO.to_vec())),
                command: field(settings.codex.command, "codex", cx),
                working_directory: field(
                    settings.codex.working_directory.display().to_string(),
                    "working directory",
                    cx,
                ),
                model: field(settings.codex.model.unwrap_or_default(), "default model", cx),
                api_key_environment_variable: field(
                    settings.codex.api_key_environment_variable,
                    "OPENAI_API_KEY",
                    cx,
                ),
                bridge_address: field(settings.bridge.listen_address, "127.0.0.1:49630", cx),
                settings_path,
                status: "尚未保存。API Key 仅由环境变量提供，不会写入配置文件。".to_owned(),
            }
        }

        fn save_settings(&mut self, cx: &mut Context<Self>) {
            let mut settings = RuntimeSettings::default();
            settings.codex.command = self.command.read(cx).value().to_string();
            settings.codex.working_directory =
                self.working_directory.read(cx).value().to_string().into();
            settings.codex.model = (!self.model.read(cx).value().is_empty())
                .then(|| self.model.read(cx).value().to_string());
            settings.codex.api_key_environment_variable =
                self.api_key_environment_variable.read(cx).value().to_string();
            settings.bridge.listen_address = self.bridge_address.read(cx).value().to_string();
            self.status = match settings.save(&self.settings_path) {
                Ok(()) => format!(
                    "已保存到 {}。现在可启动 circuitfabric-jlc-bridge。",
                    self.settings_path.display()
                ),
                Err(error) => format!("未保存：{error}"),
            };
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
                            .w(px(480.))
                            .v_flex()
                            .gap_3()
                            .child(Self::field("Codex command", "codex-command", &self.command))
                            .child(Self::field(
                                "Working directory",
                                "working-directory",
                                &self.working_directory,
                            ))
                            .child(Self::field("Model (optional)", "model", &self.model))
                            .child(Self::field(
                                "API key environment variable",
                                "api-key-env",
                                &self.api_key_environment_variable,
                            ))
                            .child(Self::field(
                                "JLC bridge address",
                                "bridge-address",
                                &self.bridge_address,
                            ))
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
