//! Commands presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn command_matches(&self, cx: &Context<Self>) -> Vec<ControlPlaneScreen> {
        let needle = self.command_search.read(cx).value().to_lowercase();
        let project_selected = self.navigation.selected_project.is_some();
        ControlPlaneScreen::ALL
            .into_iter()
            // Project-scoped screens match the sidebar's disabled state:
            // they only work with a selected project, so the palette does
            // not offer them until one is open.
            .filter(|screen| {
                (!screen.requires_project() || project_selected)
                    && (needle.is_empty()
                        || self.language.screen_label(*screen).to_lowercase().contains(&needle))
            })
            .collect()
    }

    pub(super) fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette_open = true;
        self.command_selected = 0;
        self.command_search.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open {
            self.command_palette_open = false;
            cx.notify();
        }
    }

    pub(super) fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette_open {
            self.close_command_palette(cx);
        } else {
            self.open_command_palette(window, cx);
        }
    }

    pub(super) fn command_activate(&mut self, screen: ControlPlaneScreen, cx: &mut Context<Self>) {
        self.navigation.screen = screen;
        self.close_command_palette(cx);
    }

    pub(super) fn handle_command_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;

        if self.datasheet_stream_modal_open {
            if keystroke.key == "escape" {
                self.close_datasheet_stream_modal(cx);
            }
            return;
        }

        if self.session_modal_open && self.session_replay.is_some() {
            if keystroke.key == "escape" {
                self.close_session_replay(cx);
            }
            return;
        }

        if keystroke.modifiers.secondary() && keystroke.key.eq_ignore_ascii_case("k") {
            self.toggle_command_palette(window, cx);
            return;
        }

        if self.command_palette_open {
            match keystroke.key.as_str() {
                "escape" => self.close_command_palette(cx),
                "enter" => {
                    let screen = self.command_matches(cx).get(self.command_selected).copied();
                    if let Some(screen) = screen {
                        self.command_activate(screen, cx);
                    }
                }
                "up" => {
                    let count = self.command_matches(cx).len();
                    if count > 0 {
                        self.command_selected = (self.command_selected + count - 1) % count;
                        cx.notify();
                    }
                }
                "down" => {
                    let count = self.command_matches(cx).len();
                    if count > 0 {
                        self.command_selected = (self.command_selected + 1) % count;
                        cx.notify();
                    }
                }
                _ => {}
            }
            return;
        }

        // Outside the palette, Enter presses the default button of the
        // top-most open dialog, exactly like clicking it.
        if keystroke.key == "enter" {
            self.press_default_dialog_button(window, cx);
        }
    }

    /// Triggers the default (primary) action of the currently open modal
    /// dialog: the startup vault prompt and quick unlock submit the typed
    /// password, the new-project form confirms creation. Each action
    /// reuses the button's own handler, so disabled states (e.g. a busy
    /// unlock) suppress the key the same way they suppress the click.
    pub(super) fn press_default_dialog_button(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let vault_prompt_visible =
            self.vault_prompt_open && self.vault.is_none() && self.vault_file_exists;
        let quick_unlock_visible =
            self.vault_quick_unlock_open && self.vault.is_none() && self.vault_file_exists;
        if vault_prompt_visible || quick_unlock_visible {
            self.unlock_vault(window, cx);
            return;
        }
        if self.project_form_open {
            self.create_project(window, cx);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_command_palette(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let matches = self.command_matches(cx);
        let selected = self.command_selected.min(matches.len().saturating_sub(1));

        let mut list = div().v_flex().gap_0p5().p_2();
        for (index, screen) in matches.iter().copied().enumerate() {
            let is_selected = index == selected;
            let label = language.screen_label(screen);
            let group = language.group_label(screen);
            let navigator = entity.clone();
            list = list.child(
                div()
                    .id(format!("command-item-{}", screen.label()))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .h(px(36.))
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_selected, |this| this.bg(rgb(ACCENT)).text_color(rgb(SIDEBAR_BG)))
                    .when(!is_selected, |this| this.text_color(rgb(SIDEBAR_TEXT)))
                    .when(!is_selected, |this| this.hover(|this| this.bg(rgb(SIDEBAR_ITEM_HOVER))))
                    .on_click(move |_, _, cx| {
                        navigator.update(cx, |view, cx| view.command_activate(screen, cx));
                    })
                    .child(
                        div()
                            .text_sm()
                            .font_weight(if is_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if is_selected {
                                rgb(SIDEBAR_BG)
                            } else {
                                rgb(SIDEBAR_GROUP)
                            })
                            .child(group),
                    ),
            );
        }

        let body = if matches.is_empty() {
            div()
                .px_3()
                .py_4()
                .text_sm()
                .text_color(rgb(SIDEBAR_TEXT))
                .child(language.choose("无匹配模块", "No matching modules"))
        } else {
            list
        };

        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .pt_16()
            .child(
                div()
                    .id("command-palette-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x0000_0080))
                    .occlude()
                    .on_click(move |_, _, cx| {
                        entity.update(cx, ControlPlaneView::close_command_palette);
                    }),
            )
            .child(
                div()
                    .relative()
                    .occlude()
                    .w(px(560.))
                    .v_flex()
                    .overflow_hidden()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(SIDEBAR_DIVIDER))
                    .bg(rgb(SIDEBAR_BG))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .h(px(44.))
                            .border_b_1()
                            .border_color(rgb(SIDEBAR_DIVIDER))
                            .child(
                                InputBase::new("command-palette-search")
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .text_sm()
                                    .text_color(rgb(SIDEBAR_TEXT_ACTIVE))
                                    .child(self.command_search.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(SIDEBAR_GROUP))
                                    .child(language.choose("ESC 关闭", "ESC to close")),
                            ),
                    )
                    .child(body),
            )
    }
}
