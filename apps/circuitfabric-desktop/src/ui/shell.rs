//! Navigation, fixed chrome and page composition.
use super::*;

const SIDEBAR_WIDTH: f32 = 248.;
const SIDEBAR_COLLAPSED_WIDTH: f32 = 64.;

/// One Lucide outline icon per screen, so the set shares stroke weight and style.
fn screen_icon(screen: ControlPlaneScreen) -> Icon {
    let name = match screen {
        ControlPlaneScreen::Overview => "layout-dashboard",
        ControlPlaneScreen::Projects => "folder-kanban",
        ControlPlaneScreen::Documents => "file-text",
        ControlPlaneScreen::Semantics => "circuit-board",
        ControlPlaneScreen::EdaServices => "cpu",
        ControlPlaneScreen::AgentsAndMcp => "bot",
        ControlPlaneScreen::SessionsAndTasks => "messages-square",
        ControlPlaneScreen::ChangesAndApprovals => "git-pull-request",
        ControlPlaneScreen::BomAndExport => "package",
        ControlPlaneScreen::Plugins => "puzzle",
        ControlPlaneScreen::Usage => "chart-no-axes-column",
        ControlPlaneScreen::SecretsVault => "vault",
        ControlPlaneScreen::Settings => "settings",
    };
    Icon::default().path(format!("icons/{name}.svg"))
}

impl ControlPlaneView {
    fn render_sidebar_collapse_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let collapsed = self.sidebar_collapsed;
        let label = if collapsed {
            self.language.choose("展开导航", "Expand navigation")
        } else {
            self.language.choose("收起导航", "Collapse navigation")
        };
        div()
            .id("sidebar-collapse-toggle")
            .debug_selector(|| "sidebar-collapse-toggle".into())
            .mb_2()
            .h(px(32.))
            .px_2()
            .flex()
            .items_center()
            .when(collapsed, Styled::justify_center)
            .gap_2()
            .rounded_lg()
            .text_xs()
            .text_color(rgb(SIDEBAR_GROUP))
            .cursor_pointer()
            .hover(|this| this.bg(rgb(SIDEBAR_ITEM_HOVER)).text_color(rgb(SIDEBAR_TEXT_ACTIVE)))
            .when(collapsed, |this| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                })
            })
            .on_click(move |_, _, cx| {
                entity.update(cx, |view, cx| {
                    view.sidebar_collapsed = !view.sidebar_collapsed;
                    cx.notify();
                });
            })
            .child(
                Icon::new(if collapsed {
                    IconName::PanelLeftOpen
                } else {
                    IconName::PanelLeftClose
                })
                .with_size(px(16.)),
            )
            .when(!collapsed, |this| this.child(label))
    }
}

impl Render for ControlPlaneView {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_bridge_lifecycle();
        self.refresh_message_center();
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
        let session_modal = (self.session_modal_open && self.session_replay.is_some())
            .then(|| self.render_session_modal(window, cx).into_any_element());
        let datasheet_stream_modal = (self.datasheet_stream_modal_open
            && self.datasheet_stream.is_some())
        .then(|| self.render_datasheet_stream_modal(cx).into_any_element());
        let assistant_layer = self.render_assistant_layer(cx);
        let message_panel = if self.message_panel_open {
            Some(self.render_message_panel(cx).into_any_element())
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
        let collapsed = self.sidebar_collapsed;
        let default_runtime = self.default_runtime();
        let fading_divider = || {
            div().flex_1().h(px(1.)).bg(gpui::linear_gradient(
                90.,
                gpui::linear_color_stop(rgb_alpha(SIDEBAR_DIVIDER, 1.), 0.),
                gpui::linear_color_stop(rgb_alpha(SIDEBAR_DIVIDER, 0.), 1.),
            ))
        };

        for screen in ControlPlaneScreen::ALL {
            if screen.group() != current_group {
                current_group = screen.group();
                // Collapsed, a group heading shrinks to its divider line.
                navigation = navigation.child(if collapsed {
                    div()
                        .pt_3()
                        .pb_1p5()
                        .px_2()
                        .flex()
                        .child(div().flex_1().h(px(1.)).bg(rgb(SIDEBAR_DIVIDER)))
                } else {
                    div()
                        .pt_4()
                        .pb_1p5()
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(SIDEBAR_GROUP))
                                .child(language.group_label(screen)),
                        )
                        .child(fading_divider())
                });
            }

            let selector = entity.clone();
            let active = screen == active_screen;
            let label = language.screen_label(screen);
            let is_todo = screen.is_todo();
            // Project-scoped screens stay gray and inert — no pointer,
            // hover, or click — until a project is selected.
            let unavailable = screen.requires_project() && !project_selected;
            let icon_color = if unavailable {
                rgb(SIDEBAR_GROUP)
            } else if active {
                rgb(ACCENT_SOFT)
            } else {
                rgb(SIDEBAR_TEXT)
            };
            let vault_dot = (screen == ControlPlaneScreen::SecretsVault)
                .then(|| status_dot(if vault_unlocked { 0x0022_c55e } else { 0x0094_a3b8 }));
            if collapsed {
                let tooltip = if is_todo { format!("{label}（TODO）") } else { label.to_owned() };
                navigation = navigation.child(
                    div()
                        .id(format!("nav-{}", screen.label()))
                        .relative()
                        .w_full()
                        .h(px(40.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb_alpha(ACCENT, 0.))
                        .when(active, |this| {
                            this.bg(rgb_alpha(ACCENT, 0.14)).border_color(rgb_alpha(ACCENT, 0.3))
                        })
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                        })
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
                        .when(active, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .left(px(-1.))
                                    .top(px(10.))
                                    .w(px(3.))
                                    .h(px(18.))
                                    .rounded_full()
                                    .bg(accent_gradient(180.))
                                    .shadow(accent_glow(0.8, 8.)),
                            )
                        })
                        .child(screen_icon(screen).with_size(px(18.)).text_color(icon_color))
                        .when_some(vault_dot, |this, dot| {
                            this.child(div().absolute().top(px(6.)).right(px(8.)).child(dot))
                        }),
                );
                continue;
            }
            navigation = navigation.child(
                div()
                    .id(format!("nav-{}", screen.label()))
                    .w_full()
                    .h(px(36.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb_alpha(ACCENT, 0.))
                    .when(active, |this| {
                        this.bg(gpui::linear_gradient(
                            90.,
                            gpui::linear_color_stop(rgb_alpha(ACCENT, 0.18), 0.),
                            gpui::linear_color_stop(rgb_alpha(ACCENT_ALT, 0.04), 1.),
                        ))
                        .border_color(rgb_alpha(ACCENT, 0.22))
                    })
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
                            .when(active, |this| {
                                this.bg(accent_gradient(180.)).shadow(accent_glow(0.8, 8.))
                            }),
                    )
                    .child(screen_icon(screen).with_size(px(16.)).text_color(icon_color))
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
                    .when_some(vault_dot, ParentElement::child),
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
                    .w(px(if collapsed { SIDEBAR_COLLAPSED_WIDTH } else { SIDEBAR_WIDTH }))
                    .flex_none()
                    .h_full()
                    .v_flex()
                    .py_3()
                    .px(px(if collapsed { 8. } else { 12. }))
                    .bg(gpui::linear_gradient(
                        180.,
                        gpui::linear_color_stop(rgb(SIDEBAR_BG), 0.),
                        gpui::linear_color_stop(rgb(0x0008_1626), 1.),
                    ))
                    .border_r_1()
                    .border_color(rgb(SIDEBAR_DIVIDER))
                    .text_color(rgb(SIDEBAR_TEXT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .when(collapsed, Styled::justify_center)
                            .gap_2p5()
                            .px_1()
                            .pb_4()
                            .mb_1()
                            .border_b_1()
                            .border_color(rgb(SIDEBAR_DIVIDER))
                            // This compact mark intentionally omits the logo's outer
                            // frame: the tile itself supplies the only frame at this size.
                            .child(
                                div()
                                    .size(px(42.))
                                    .flex_none()
                                    .p(px(1.5))
                                    .rounded_lg()
                                    .bg(accent_gradient(135.))
                                    .shadow(accent_glow(0.35, 18.))
                                    .child(
                                        div()
                                            .size_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_lg()
                                            .bg(rgb(SIDEBAR_BG))
                                            .child(img(self.sidebar_mark.clone()).size(px(30.))),
                                    ),
                            )
                            .when(!collapsed, |this| {
                                this.child(
                                    div()
                                        .v_flex()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_base()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(SIDEBAR_TEXT_ACTIVE))
                                                .child("CircuitFabric"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(ACCENT_SOFT))
                                                .child(language.choose(
                                                    "电路设计控制面",
                                                    "CIRCUIT CONTROL PLANE",
                                                )),
                                        ),
                                )
                            }),
                    )
                    .child(navigation)
                    .child(div().flex_1())
                    .child(self.render_sidebar_collapse_toggle(cx))
                    .child(if collapsed {
                        let label = self.runtime_check_label(default_runtime, language);
                        div()
                            .id("sidebar-runtime-status")
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(SIDEBAR_DIVIDER))
                            .bg(rgb_alpha(SIDEBAR_ITEM_ACTIVE, 0.6))
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(label.clone())
                                    .build(window, cx)
                            })
                            .child(status_dot(self.runtime_check_dot(default_runtime)))
                            .into_any_element()
                    } else {
                        div()
                            .p_2p5()
                            .v_flex()
                            .gap_1p5()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(SIDEBAR_DIVIDER))
                            .bg(rgb_alpha(SIDEBAR_ITEM_ACTIVE, 0.6))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(status_dot(self.runtime_check_dot(default_runtime)))
                                    .child(
                                        div()
                                            .text_xs()
                                            .truncate()
                                            .text_color(rgb(SIDEBAR_TEXT))
                                            .child(
                                                self.runtime_check_label(default_runtime, language),
                                            ),
                                    ),
                            )
                            .child(div().text_xs().text_color(rgb(SIDEBAR_GROUP)).child(
                                language.choose("v0.1 · 本地控制面", "v0.1 · local control plane"),
                            ))
                            .into_any_element()
                    }),
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
                            .flex_none()
                            .items_center()
                            .justify_between()
                            .relative()
                            .bg(rgb(CARD_BG))
                            // Accent hairline instead of a flat divider.
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .h(px(1.))
                                    .bg(gpui::linear_gradient(
                                        90.,
                                        gpui::linear_color_stop(rgb_alpha(ACCENT, 0.7), 0.),
                                        gpui::linear_color_stop(rgb_alpha(ACCENT_ALT, 0.05), 1.),
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .v_flex()
                                    .gap_0p5()
                                    .child({
                                        // Level 1 is the current function page; a second
                                        // level appears for drill-in detail panes and makes
                                        // level 1 clickable to navigate back.
                                        let level1 = language.screen_label(active_screen);
                                        let level2 = match (active_screen, self.agents_selection)
                                        {
                                            (
                                                ControlPlaneScreen::AgentsAndMcp,
                                                AgentsSelection::Provider,
                                            ) => Some(language.choose("LLM Provider", "LLM Provider")),
                                            (
                                                ControlPlaneScreen::AgentsAndMcp,
                                                AgentsSelection::SkillsAndMcp,
                                            ) => Some(language.choose(
                                                "技能与 MCP 授权",
                                                "Skills & MCP",
                                            )),
                                            (
                                                ControlPlaneScreen::AgentsAndMcp,
                                                AgentsSelection::BundledJev,
                                            ) => Some(language.choose(
                                                "Jev 判断工具",
                                                "Jev judgments",
                                            )),
                                            _ => None,
                                        };
                                        let mut breadcrumb =
                                            Breadcrumb::new().text_base().min_w(px(0.));
                                        breadcrumb = match level2 {
                                            Some(level2) => {
                                                let nav_back = entity.clone();
                                                breadcrumb
                                                    .child(
                                                        BreadcrumbItem::new(level1).on_click(
                                                            move |_, _, cx| {
                                                                nav_back.update(cx, |view, cx| {
                                                                    view.agents_selection =
                                                                        AgentsSelection::Runtime(
                                                                            RuntimeAdapter::CodexAppServer,
                                                                        );
                                                                    cx.notify();
                                                                });
                                                            },
                                                        ),
                                                    )
                                                    .child(BreadcrumbItem::new(level2))
                                            }
                                            None => breadcrumb.child(BreadcrumbItem::new(level1)),
                                        };
                                        breadcrumb
                                    })
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
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .pl_2p5()
                                            .pr_1()
                                            .h(px(30.))
                                            .w(px(220.))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(rgb(BORDER))
                                            .bg(rgb(SURFACE_BG))
                                            .cursor_pointer()
                                            .hover(|this| this.border_color(rgb_alpha(ACCENT, 0.6)))
                                            .on_click({
                                                let opener = entity.clone();
                                                move |_, window, cx| {
                                                    opener.update(cx, |view, cx| {
                                                        view.open_command_palette(window, cx);
                                                    });
                                                }
                                            })
                                            .child(
                                                Icon::new(IconName::Search)
                                                    .small()
                                                    .text_color(rgb(TEXT_MUTED)),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT_MUTED))
                                                    .child(language.choose(
                                                        "跳转到模块…",
                                                        "Jump to module…",
                                                    )),
                                            )
                                            .child(keycap(if cfg!(target_os = "macos") {
                                                "⌘K"
                                            } else {
                                                "Ctrl K"
                                            })),
                                    )
                                    .child(self.render_assistant_trigger(cx))
                                    .child(super::messages_panel::render_message_center_trigger(
                                        self, cx,
                                    ))
                                    .child(
                                        action_button("toggle-language")
                                            .ghost()
                                            .label(language.toggle_label())
                                            .on_click(move |_, window, cx| {
                                                entity.update(cx, |view, cx| {
                                                    view.toggle_global_language(window, cx);
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
                            .flex_none()
                            .bg(rgb(SURFACE_BG))
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
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .text_xs()
                                            .text_color(rgb(TEXT_MUTED))
                                            .child(keycap(ASSISTANT_SHORTCUT_LABEL))
                                            .child(language.choose("智能助手", "Assistant")),
                                    )
                                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose(
                                            "CircuitFabric 桌面端",
                                            "CircuitFabric desktop",
                                        ),
                                    )),
                            ),
                    )
                    .child(assistant_layer)
                    .when_some(message_panel, ParentElement::child)
                    .when_some(command_palette, ParentElement::child)
                    .when_some(vault_prompt, ParentElement::child)
                    .when_some(vault_quick_unlock, ParentElement::child)
                    .when_some(jev_backend_modal, ParentElement::child)
                    .when_some(jev_key_modal, ParentElement::child)
                    .when_some(catalog_modal, ParentElement::child)
                    .when_some(session_modal, ParentElement::child)
                    .when_some(datasheet_stream_modal, ParentElement::child),
            )
    }
}
