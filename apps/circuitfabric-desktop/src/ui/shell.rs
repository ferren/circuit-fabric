//! Navigation, fixed chrome and page composition.
use super::*;

impl Render for ControlPlaneView {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_codex_lifecycle();
        self.refresh_bridge_lifecycle();
        self.sync_window_title(window);
        let command_palette = if self.command_palette_open {
            Some(self.render_command_palette(cx).into_any_element())
        } else {
            None
        };
        let vault_prompt =
            if self.vault_prompt_open && self.vault.is_none() && self.vault_file_exists {
                Some(self.render_vault_prompt(cx).into_any_element())
            } else {
                None
            };
        let vault_quick_unlock =
            if self.vault_quick_unlock_open && self.vault.is_none() && self.vault_file_exists {
                Some(self.render_vault_quick_unlock(cx).into_any_element())
            } else {
                None
            };
        let catalog_modal = if self.catalog_editor.is_some() {
            Some(self.render_catalog_modal(cx).into_any_element())
        } else {
            None
        };
        let jev_backend_modal = if self.jev_backend_modal_open {
            Some(self.render_jev_backend_modal(cx).into_any_element())
        } else {
            None
        };
        let jev_key_modal = if self.jev_key_modal_open {
            Some(self.render_jev_key_modal(cx).into_any_element())
        } else {
            None
        };
        let document_preview_pane = if self.document_preview.is_some() {
            Some(self.render_document_preview_pane(window, cx))
        } else {
            None
        };
        let entity = cx.entity().clone();
        let active_screen = self.navigation.screen;
        let language = self.language;
        let selected_project_label = self
            .navigation
            .selected_project
            .as_deref()
            .and_then(|id| self.workspace.project(id))
            .map(|project| format!("{} · {}", project.name, project.id))
            .unwrap_or_else(|| language.choose("未选择项目", "No project selected").to_owned());
        let mut navigation = div().v_flex().gap_0p5();
        let mut current_group = "";
        let vault_unlocked = self.vault.is_some();
        let project_selected = self.navigation.selected_project.is_some();

        for screen in ControlPlaneScreen::ALL {
            if screen.group() != current_group {
                current_group = screen.group();
                navigation = navigation.child(
                    div()
                        .pt_4()
                        .pb_1()
                        .px_3()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(SIDEBAR_GROUP))
                        .child(language.group_label(screen)),
                );
            }

            let selector = entity.clone();
            let active = screen == active_screen;
            let label = language.screen_label(screen);
            let is_todo = screen.is_todo();
            // Project-scoped screens stay gray and inert — no pointer,
            // hover, or click — until a project is selected.
            let unavailable = screen.requires_project() && !project_selected;
            navigation = navigation.child(
                div()
                    .id(format!("nav-{}", screen.label()))
                    .w_full()
                    .h(px(36.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_md()
                    .when(active, |this| this.bg(rgb(SIDEBAR_ITEM_ACTIVE)))
                    .when(!unavailable, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.bg(rgb(SIDEBAR_ITEM_HOVER)))
                            .active(|this| this.bg(rgb(SIDEBAR_ITEM_PRESSED)))
                            .on_click(move |_, _, cx| {
                                selector.update(cx, |view, cx| {
                                    view.navigation.screen = screen;
                                    cx.notify();
                                });
                            })
                    })
                    .child(
                        div()
                            .w(px(3.))
                            .h(px(18.))
                            .rounded_full()
                            .flex_none()
                            .when(active, |this| this.bg(rgb(ACCENT))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .font_weight(if active {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if unavailable {
                                rgb(SIDEBAR_GROUP)
                            } else if active {
                                rgb(SIDEBAR_TEXT_ACTIVE)
                            } else {
                                rgb(SIDEBAR_TEXT)
                            })
                            .child(label),
                    )
                    .when(is_todo, |this| {
                        this.child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if active {
                                    rgb(ACCENT_SOFT)
                                } else {
                                    rgb(SIDEBAR_GROUP)
                                })
                                .child("TODO"),
                        )
                    })
                    .when(screen == ControlPlaneScreen::SecretsVault, |this| {
                        this.child(status_dot(if vault_unlocked {
                            0x0022_c55e
                        } else {
                            0x0094_a3b8
                        }))
                    }),
            );
        }

        let page = match active_screen {
            ControlPlaneScreen::Overview => self.overview_page(window, cx).into_any_element(),
            ControlPlaneScreen::Semantics => self.render_semantics_page(cx).into_any_element(),
            ControlPlaneScreen::BomAndExport => self.render_bom_export_page(cx).into_any_element(),
            ControlPlaneScreen::Projects => {
                self.render_projects_page(window, cx).into_any_element()
            }
            ControlPlaneScreen::Documents => self.render_documents_page(cx).into_any_element(),
            ControlPlaneScreen::EdaServices => {
                self.render_eda_services_page(window, cx).into_any_element()
            }
            ControlPlaneScreen::AgentsAndMcp => {
                self.render_agents_page(window, cx).into_any_element()
            }
            ControlPlaneScreen::SessionsAndTasks => {
                self.render_sessions_page(cx).into_any_element()
            }
            ControlPlaneScreen::SecretsVault => {
                self.render_secrets_page(window, cx).into_any_element()
            }
            ControlPlaneScreen::Usage => {
                self.render_usage_audit_page(window, cx).into_any_element()
            }
            ControlPlaneScreen::Plugins => self.render_plugins_page(cx).into_any_element(),
            ControlPlaneScreen::ChangesAndApprovals => {
                self.render_changes_approvals_page(cx).into_any_element()
            }
            ControlPlaneScreen::Settings => self.render_settings_page(cx).into_any_element(),
        };

        div()
            .size_full()
            .flex()
            .bg(rgb(SURFACE_BG))
            .text_color(rgb(TEXT_PRIMARY))
            .child(crate::pdf_text_layer::selection_gesture_guard(self.selection_pressed.clone()))
            .child(
                // Sidebar
                div()
                    .w(px(248.))
                    .flex_none()
                    .h_full()
                    .v_flex()
                    .p_3()
                    .bg(rgb(SIDEBAR_BG))
                    .text_color(rgb(SIDEBAR_TEXT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_1()
                            .pb_4()
                            .mb_1()
                            .border_b_1()
                            .border_color(rgb(SIDEBAR_DIVIDER))
                            // This compact mark intentionally omits the logo's outer
                            // frame: the tile itself supplies the only frame at this size.
                            .child(
                                div()
                                    .size(px(40.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(rgb(SIDEBAR_ITEM_ACTIVE))
                                    .border_1()
                                    .border_color(rgb(ACCENT_SOFT))
                                    .child(img(self.sidebar_mark.clone()).size(px(32.))),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(SIDEBAR_TEXT_ACTIVE))
                                            .child("CircuitFabric"),
                                    )
                                    .child(div().text_xs().text_color(rgb(SIDEBAR_GROUP)).child(
                                        language.choose("电路设计控制面", "Circuit control plane"),
                                    )),
                            ),
                    )
                    .child(navigation)
                    .child(div().flex_1())
                    .child(
                        div()
                            .px_1()
                            .pt_3()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(status_dot(self.codex_status.dot()))
                                    .child(div().text_xs().text_color(rgb(SIDEBAR_TEXT)).child(
                                        match &self.codex_status {
                                            RuntimeLifecycleStatus::Starting => {
                                                language.choose("Codex 启动中…", "Codex starting…")
                                            }
                                            RuntimeLifecycleStatus::Running { .. } => {
                                                language.choose("Codex 运行中", "Codex running")
                                            }
                                            RuntimeLifecycleStatus::Stopped => {
                                                language.choose("运行时离线", "Runtime offline")
                                            }
                                            RuntimeLifecycleStatus::Failed { .. } => {
                                                language.choose(
                                                    "Codex 启动失败",
                                                    "Codex failed to start",
                                                )
                                            }
                                        },
                                    )),
                            )
                            .child(div().text_xs().text_color(rgb(SIDEBAR_GROUP)).child(
                                language.choose("v0.1 · 本地控制面", "v0.1 · local control plane"),
                            )),
                    ),
            )
            .child(
                // Main column
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .v_flex()
                    .bg(rgb(CARD_BG))
                    .relative()
                    .child(
                        // Top bar
                        div()
                            .h(px(56.))
                            .px_5()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT_PRIMARY))
                                            .child(language.screen_label(active_screen)),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(0.))
                                            .truncate()
                                            .text_xs()
                                            .text_color(rgb(TEXT_MUTED))
                                            .child(selected_project_label),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("command-palette-trigger")
                                            .self_start()
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .px_2()
                                            .h(px(28.))
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(BORDER))
                                            .bg(rgb(SURFACE_BG))
                                            .cursor_pointer()
                                            .hover(|this| this.bg(rgb(BORDER)))
                                            .on_click({
                                                let opener = entity.clone();
                                                move |_, window, cx| {
                                                    opener.update(cx, |view, cx| {
                                                        view.open_command_palette(window, cx);
                                                    });
                                                }
                                            })
                                            .child(
                                                div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                                    if cfg!(target_os = "macos") {
                                                        "⌘K"
                                                    } else {
                                                        "Ctrl K"
                                                    },
                                                ),
                                            ),
                                    )
                                    .child(
                                        action_button("toggle-language")
                                            .ghost()
                                            .label(language.toggle_label())
                                            .on_click(move |_, _, cx| {
                                                entity.update(cx, |view, cx| {
                                                    view.toggle_global_language(cx);
                                                });
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        // Scrollable content plus the docked document preview, when one
                        // is open. Vertical-only scrolling keeps the page width locked to
                        // the viewport, so text nodes receive a definite wrap width and
                        // reflow instead of stretching the workspace sideways; the
                        // preview pane sits beside the scroll host, not inside it, and
                        // scrolls independently at full window height. The divider
                        // between them is draggable and resizes the pane.
                        div()
                            .flex()
                            .flex_1()
                            .min_h(px(0.))
                            .on_drag_move::<DraggedPreviewSplit>(cx.listener(
                                |view, event: &DragMoveEvent<DraggedPreviewSplit>, _, cx| {
                                    let row_right = f32::from(event.bounds.right());
                                    let row_width =
                                        f32::from(event.bounds.right() - event.bounds.left());
                                    let pointer_x = f32::from(event.event.position.x);
                                    let upper = (row_width * 0.8).min(DOCUMENT_PREVIEW_MAX_WIDTH);
                                    view.document_preview_width = (row_right - pointer_x).clamp(
                                        DOCUMENT_PREVIEW_MIN_WIDTH,
                                        upper.max(DOCUMENT_PREVIEW_MIN_WIDTH),
                                    );
                                    cx.notify();
                                },
                            ))
                            .on_drop::<DraggedPreviewSplit>(cx.listener(
                                |_view, _event, _window, cx| {
                                    cx.notify();
                                },
                            ))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .min_h(px(0.))
                                    .h_full()
                                    .overflow_hidden()
                                    .bg(rgb(SURFACE_BG))
                                    .child(layout::page_host(active_screen, page)),
                            )
                            .when_some(document_preview_pane, |row, pane| {
                                row.child(
                                    div()
                                        .id("document-preview-split-handle")
                                        .w(px(6.))
                                        .flex_none()
                                        .h_full()
                                        .cursor_col_resize()
                                        .bg(rgb(BORDER))
                                        .hover(|this| this.bg(rgb(ACCENT_SOFT)))
                                        .block_mouse_except_scroll()
                                        .on_click(cx.listener(
                                            |view, event: &ClickEvent, _window, cx| {
                                                if event.click_count() >= 2 {
                                                    view.document_preview_width =
                                                        DOCUMENT_PREVIEW_WIDTH;
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .on_drag(DraggedPreviewSplit, |_, _, _, cx| {
                                            cx.new(|_| gpui::Empty)
                                        }),
                                )
                                .child(pane)
                            }),
                    )
                    .child(
                        // Status bar
                        div()
                            .h(px(28.))
                            .px_5()
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(rgb(CARD_BG))
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(status_dot(0x0094_a3b8))
                                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose(
                                            "Bridge 未连接 · 验证未运行",
                                            "Bridge not connected · Verification not run",
                                        ),
                                    )),
                            )
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose("CircuitFabric 桌面端", "CircuitFabric desktop"),
                            )),
                    )
                    .when_some(command_palette, ParentElement::child)
                    .when_some(vault_prompt, ParentElement::child)
                    .when_some(vault_quick_unlock, ParentElement::child)
                    .when_some(jev_backend_modal, ParentElement::child)
                    .when_some(jev_key_modal, ParentElement::child)
                    .when_some(catalog_modal, ParentElement::child),
            )
    }
}
