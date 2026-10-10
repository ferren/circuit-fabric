//! Exercise actual GPUI layout and wheel dispatch, including shared helper identities.
use super::layout::*;
use super::{ControlPlaneView, RuntimeAdapter, UiLanguage, action_button};
use crate::application::navigation::ControlPlaneScreen;
use circuitfabric_codex_runtime::secrets::UnlockedVault;
use gpui::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta,
    ScrollWheelEvent, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use gpui_component::StyledExt;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_PROFILE: AtomicU64 = AtomicU64::new(0);
struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cf-ui-layout-{}-{}",
            std::process::id(),
            NEXT_PROFILE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mount_profile<'a>(
    cx: &'a mut TestAppContext,
    profile: &Profile,
    screen: ControlPlaneScreen,
) -> (gpui::Entity<ControlPlaneView>, &'a mut VisualTestContext) {
    use gpui::AppContext;
    cx.update(gpui_component::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut view = ControlPlaneView::new_with_paths(
                profile.0.join("runtime.json"),
                profile.0.join("vault.bin"),
                window,
                cx,
            );
            view.navigation.screen = screen;
            view.vault_prompt_open = false;
            view
        });
        view = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx).bordered(false)
    });
    (view.unwrap(), cx)
}

#[gpui::test]
fn datasheet_stream_follows_only_at_bottom_and_expands_live(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Documents);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(
        "FIRST OUTPUT\n思考内容与输出记录\n".repeat(200),
    ));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.document_preview = Some(super::DocumentPreviewSelection {
                project_id: "p".into(),
                document_id: "doc-a".into(),
                file_name: "current.pdf".into(),
                state: super::DocumentPreviewState::Unavailable { reason: "fixture".into() },
                extraction: None,
            });
            view.preview_show_data = true;
            view.datasheet_stream = Some(("p".into(), "doc-a".into(), buffer.clone()));
            view.datasheet_extracting = true;
            view.datasheet_extract_started = Some(std::time::Instant::now());
            cx.notify();
        });
    });
    draw(cx);
    let inline = view.read_with(cx, |view, _| view.datasheet_stream_scroll.clone());
    assert!(inline.max_offset().y > px(2000.), "full history must exceed the old 2000-char tail");
    assert_eq!(inline.offset().y, -inline.max_offset().y);
    let initial_extent = inline.max_offset().y;
    buffer.lock().unwrap().push_str(&"新增输出\n".repeat(30));
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    assert!(inline.max_offset().y > initial_extent);
    assert_eq!(inline.offset().y, -inline.max_offset().y, "new output follows at the bottom");

    let body = cx.debug_bounds("datasheet-stream-body").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: body.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(90.))),
        ..Default::default()
    });
    draw(cx);
    let reading_offset = inline.offset();
    assert!(reading_offset.y > -inline.max_offset().y, "wheel can scroll up into the history");
    buffer.lock().unwrap().push_str(&"阅读期间的实时输出\n".repeat(20));
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    assert_eq!(inline.offset(), reading_offset, "streaming must preserve the reader's position");
    inline.set_offset(point(px(0.), px(0.)));
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    assert_eq!(inline.offset().y, px(0.), "the beginning of the log remains accessible");
    inline.scroll_to_bottom();
    draw(cx);
    buffer.lock().unwrap().push_str(&"恢复跟随\n".repeat(20));
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    assert_eq!(inline.offset().y, -inline.max_offset().y, "returning to bottom resumes following");

    let expand = cx.debug_bounds("expand-datasheet-stream").unwrap();
    cx.simulate_click(expand.center(), Default::default());
    draw(cx);
    let modal = cx.debug_bounds("datasheet-stream-modal").expect("expand opens the dialog");
    assert!(modal.size.width > body.size.width);
    let expanded = view.read_with(cx, |view, _| view.datasheet_stream_modal_scroll.clone());
    assert_eq!(expanded.offset().y, -expanded.max_offset().y);
    let modal_extent = expanded.max_offset().y;
    let inline_offset = inline.offset();
    buffer.lock().unwrap().push_str(&"弹窗实时更新\n".repeat(40));
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    assert!(expanded.max_offset().y > modal_extent, "open dialog reads live output");
    assert_eq!(expanded.offset().y, -expanded.max_offset().y);
    assert!(inline.offset().y < inline_offset.y, "inline and dialog both receive new output");

    let modal_body = cx.debug_bounds("datasheet-stream-modal-body").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: modal_body.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(90.))),
        ..Default::default()
    });
    draw(cx);
    let expanded_reading_offset = expanded.offset();
    let inline_offset = inline.offset();
    assert!(expanded_reading_offset.y > -expanded.max_offset().y);
    buffer.lock().unwrap().push_str(&"最终输出\n".repeat(30));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.datasheet_extracting = false;
            cx.notify();
        })
    });
    draw(cx);
    assert_eq!(expanded.offset(), expanded_reading_offset, "dialog scrolling is independent");
    assert!(inline.offset().y < inline_offset.y, "completion includes the last output");

    cx.simulate_resize(gpui::size(px(780.), px(540.)));
    draw(cx);
    let modal = cx.debug_bounds("datasheet-stream-modal").unwrap();
    assert!(modal.right() <= px(780.) && modal.bottom() <= px(540.));
    assert!(cx.debug_bounds("datasheet-stream-modal-body").unwrap().size.height > px(100.));
    let close = cx.debug_bounds("close-datasheet-stream").unwrap();
    cx.simulate_click(close.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("datasheet-stream-modal").is_none());
    assert_eq!(view.read_with(cx, |view, _| view.datasheet_extracting), false);
    assert!(cx.debug_bounds("datasheet-stream-body").is_some());
}

#[gpui::test]
fn session_entry_renders_only_selected_category_and_opens_the_original_category(
    cx: &mut TestAppContext,
) {
    use circuitfabric_project::{ProjectStorage, SessionCategory, SessionSeed};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project {
            id: "p".into(),
            name: "分类会话".into(),
            description: None,
        },
    )
    .unwrap();
    let rows = [
        ("eda", SessionCategory::Eda),
        ("datasheet", SessionCategory::Datasheet),
        ("runtime", SessionCategory::Runtime),
        ("legacy", SessionCategory::Legacy),
    ];
    for (id, category) in rows {
        store
            .start_categorized_session(
                SessionSeed {
                    session_id: id.into(),
                    runtime_profile_id: "provider".into(),
                    backend_id: Some("codex".into()),
                },
                category,
                None,
            )
            .unwrap();
    }
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::SessionsAndTasks);
    cx.simulate_resize(gpui::size(px(1400.), px(1200.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::application::project_data::attach_project_storage(
                &mut view.workspace,
                &mut view.project_storages,
                &mut view.project_data,
                store,
            )
            .unwrap();
            view.navigation.selected_project = Some("p".into());
            cx.notify();
        })
    });
    for (selected_id, category) in rows {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.session_category_filter = category;
                cx.notify();
            })
        });
        draw(cx);
        for (id, _) in rows {
            let selector = match id {
                "eda" => "session-row-eda",
                "datasheet" => "session-row-datasheet",
                "runtime" => "session-row-runtime",
                "legacy" => "session-row-legacy",
                _ => unreachable!(),
            };
            assert_eq!(
                cx.debug_bounds(selector).is_some(),
                id == selected_id,
                "entry must render only the current category: {selected_id} / {id}"
            );
        }
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.open_session_replay("p".into(), "runtime".into(), cx))
    });
    assert_eq!(view.update(cx, |view, _| view.session_category_filter), SessionCategory::Runtime);
    draw(cx);
    assert!(cx.debug_bounds("session-replay-body").is_some());
    cx.update(|_, cx| view.update(cx, ControlPlaneView::close_session_replay));
    draw(cx);
    assert!(cx.debug_bounds("session-detail-modal").is_none());
    assert_eq!(
        view.read_with(cx, |view, _| view
            .session_replay
            .as_ref()
            .unwrap()
            .replay
            .metadata
            .session_id
            .clone()),
        "runtime",
        "closing details must keep the record selected for continuation"
    );
}

#[gpui::test]
fn document_session_history_opens_formatted_scrollable_modal_without_changing_document(
    cx: &mut TestAppContext,
) {
    use circuitfabric_project::{
        ProjectStorage, SessionActor, SessionCategory, SessionEvent, SessionEventKind, SessionSeed,
    };
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project {
            id: "p".into(),
            name: "文档会话".into(),
            description: None,
        },
    )
    .unwrap();
    for (id, category, document) in [
        ("current", SessionCategory::Datasheet, Some("doc-a")),
        ("other-document", SessionCategory::Datasheet, Some("doc-b")),
        ("runtime", SessionCategory::Runtime, None),
    ] {
        store
            .start_categorized_session(
                SessionSeed {
                    session_id: id.into(),
                    runtime_profile_id: "provider".into(),
                    backend_id: Some("codex".into()),
                },
                category,
                document.map(str::to_owned),
            )
            .unwrap();
    }
    let message = format!(
        "# Extraction result\n\n**Readable** and *formatted* with `inline code`.\n\n- First item\n\n> Evidence\n\n```json\n{{\"pins\": []}}\n```\n\n| Name | Value |\n| --- | --- |\n| Voltage | 3.3 V |\n\n{}\n\nFINAL MARKER",
        "## Step\n\nDetails\n\n".repeat(140)
    );
    store
        .append_session_event(
            "current",
            &SessionEvent {
                timestamp_unix_seconds: 100,
                kind: SessionEventKind::Turn { actor: SessionActor::Agent, message },
            },
        )
        .unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Documents);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::application::project_data::attach_project_storage(
                &mut view.workspace,
                &mut view.project_storages,
                &mut view.project_data,
                store,
            )
            .unwrap();
            view.navigation.selected_project = Some("p".into());
            view.document_preview = Some(super::DocumentPreviewSelection {
                project_id: "p".into(),
                document_id: "doc-a".into(),
                file_name: "current.pdf".into(),
                state: super::DocumentPreviewState::Unavailable { reason: "fixture".into() },
                extraction: None,
            });
            view.preview_show_data = true;
            cx.notify();
        });
    });
    draw(cx);
    let row = cx.debug_bounds("session-row-current").expect("document session list row");
    assert!(cx.debug_bounds("session-row-other-document").is_none());
    assert!(cx.debug_bounds("session-row-runtime").is_none());
    assert!(row.size.height > px(40.), "history rows need a visible click target: {row:?}");
    let preview_offset = view.read_with(cx, |view, _| view.preview_scroll.offset());
    cx.simulate_click(row.center(), Default::default());
    draw(cx);
    let modal = cx.debug_bounds("session-detail-modal").expect("click opens window-level modal");
    assert!(
        modal.size.width > px(800.),
        "modal must provide more reading width than the document pane"
    );
    let body = cx.debug_bounds("session-replay-body").unwrap();
    let header = cx.debug_bounds("session-modal-header").unwrap();
    let markdown =
        view.read_with(cx, |view, _| view.session_replay.as_ref().unwrap().markdown.clone());
    let before = cx.update(|_, cx| markdown.read(cx).list_state().logical_scroll_top());
    scroll(cx, body.center().x.as_f32(), body.center().y.as_f32());
    let after = cx.update(|_, cx| markdown.read(cx).list_state().logical_scroll_top());
    assert!(
        after.item_ix > before.item_ix || after.offset_in_item > before.offset_in_item,
        "wheel must scroll the modal's rich text"
    );
    assert_eq!(cx.debug_bounds("session-modal-header").unwrap(), header);
    assert_eq!(view.read_with(cx, |view, _| view.preview_scroll.offset()), preview_offset);
    assert_eq!(
        view.read_with(cx, |view, _| view.document_preview.as_ref().unwrap().document_id.clone()),
        "doc-a"
    );
    cx.update(|_, cx| {
        markdown.update(cx, |state, cx| {
            state.select_all(cx);
            let text = state.selected_text();
            assert!(text.contains("Extraction result"));
            assert!(text.contains("Readable and formatted with inline code"));
            assert!(text.contains("Voltage") && text.contains("3.3 V"));
            assert!(
                text.contains("FINAL MARKER"),
                "long histories must not be capped at 120 blocks"
            );
            assert!(!text.contains("**Readable**") && !text.contains("```json"));
            assert!(state.list_state().max_offset_for_scrollbar().y > px(0.));
        });
    });
    cx.simulate_resize(gpui::size(px(780.), px(540.)));
    draw(cx);
    let modal = cx.debug_bounds("session-detail-modal").unwrap();
    let close = cx.debug_bounds("action-button-close-session-replay").unwrap();
    assert!(modal.right() <= px(780.) && modal.bottom() <= px(540.));
    assert!(cx.debug_bounds("session-replay-body").unwrap().size.height > px(100.));
    assert!(close.bottom() < modal.bottom());
    cx.simulate_click(close.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("session-detail-modal").is_none());
    assert!(
        cx.debug_bounds("session-row-current").is_some(),
        "closing keeps the original history visible"
    );
}

#[gpui::test]
fn real_vault_page_allocates_height_to_its_panels(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::SecretsVault);
    for exists in [false, true] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.vault_file_exists = exists;
                cx.notify();
            })
        });
        draw(cx);
        let panes = cx.debug_bounds("vault-pane-row").expect("vault panels rendered");
        assert!(panes.size.height > px(200.), "vault panels must have usable height: {panes:?}");
        let field = cx
            .debug_bounds(if exists { "vault-unlock-password" } else { "vault-create-password" })
            .unwrap();
        assert!(
            field.size.height > px(30.)
                && field.top() >= panes.top()
                && field.bottom() <= panes.bottom(),
            "vault form must be visible inside the panels: {field:?}"
        );
    }
    let vault =
        UnlockedVault::create(&profile.0.join("vault.bin"), "layout-test-passphrase").unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.vault = Some(vault);
            cx.notify();
        })
    });
    draw(cx);
    let panes = cx.debug_bounds("vault-pane-row").unwrap();
    let field = cx.debug_bounds("secret-name").expect("unlocked vault editor rendered");
    assert!(
        field.size.height > px(30.)
            && field.top() >= panes.top()
            && field.bottom() <= panes.bottom(),
        "unlocked editor must remain visible: {field:?}"
    );
}

#[gpui::test]
fn real_runtime_settings_dialogs_show_their_form_body(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    for adapter in [RuntimeAdapter::CodexAppServer, RuntimeAdapter::ClaudeCode, RuntimeAdapter::Dsh]
    {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.adapter_settings_open = Some(adapter);
                cx.notify();
            })
        });
        draw(cx);
        let card = cx.debug_bounds("adapter-settings-card").expect("runtime dialog rendered");
        assert!(card.size.height > px(250.), "runtime form collapsed for {adapter:?}: {card:?}");
        let command = cx.debug_bounds("codex-command").expect("runtime command field rendered");
        assert!(
            command.size.height > px(30.)
                && command.top() >= card.top()
                && command.bottom() <= card.bottom(),
            "runtime command must be visible inside the dialog: {command:?}"
        );
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.adapter_settings_open = None;
            view.provider_editor_open = true;
            cx.notify();
        })
    });
    draw(cx);
    let card = cx.debug_bounds("provider-settings-card").expect("provider dialog rendered");
    assert!(card.size.height > px(250.), "provider form collapsed: {card:?}");
    let field = cx.debug_bounds("provider-id").expect("provider ID field rendered");
    assert!(
        field.size.height > px(30.) && field.top() >= card.top() && field.bottom() <= card.bottom(),
        "provider field must be visible inside the dialog: {field:?}"
    );
}

#[gpui::test]
fn provider_quick_actions_persist_the_change_a_restart_would_read_back(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::{LlmProviderSettings, RuntimeSettings};
    let profile = Profile::new();
    let settings_path = profile.0.join("runtime.json");
    {
        // The reported state: a newly added provider ended up as the default.
        let mut settings = RuntimeSettings::default();
        let mut added = LlmProviderSettings::default();
        added.id = "added".into();
        settings.providers.push(added);
        settings.default_provider_id = "added".into();
        settings.save(&settings_path).unwrap();
    }
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    let persisted = || RuntimeSettings::load_or_default(&settings_path).unwrap();
    let persisted_flag = |id: &str| {
        persisted()
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .unwrap_or_else(|| panic!("provider `{id}` must stay configured"))
            .enabled
    };

    // Switching the default back to the previous provider must reach disk without
    // the "保存 Provider 列表" dialog; bootstrap reads exactly this file on launch.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.default_provider_id, "added");
            view.selected_provider = 0;
            view.set_default_provider(cx);
            assert_eq!(view.default_provider_id, "zai");
        })
    });
    assert_eq!(persisted().default_provider_id, "zai");

    // Disabling a non-default provider persists the same immediate way.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selected_provider = 1;
            view.toggle_provider(cx);
        })
    });
    assert!(!persisted_flag("added"));

    // Disabling the default provider is invalid; the transaction and the draft roll back.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selected_provider = 0;
            view.toggle_provider(cx);
            assert!(view.providers[0].enabled, "rejected toggle must roll back the draft");
        })
    });
    assert!(persisted_flag("zai"));

    // Removing a saved provider persists immediately and keeps one provider configured.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selected_provider = 1;
            view.remove_provider(cx);
            assert_eq!(view.providers.len(), 1);
        })
    });
    assert_eq!(
        persisted().providers.iter().map(|provider| provider.id.as_str()).collect::<Vec<_>>(),
        ["zai"]
    );
    assert_eq!(persisted().default_provider_id, "zai");
}

#[gpui::test]
fn native_vision_grays_vision_fields_while_keeping_their_values(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::RuntimeSettings;
    let profile = Profile::new();
    let settings_path = profile.0.join("runtime.json");
    {
        // Separate-vision era values: the provider routes images through a
        // distinct vision model that must survive the native-vision switch.
        let mut settings = RuntimeSettings::default();
        settings.providers[0].vision_model = Some("kept-vision-model".into());
        settings.save(&settings_path).unwrap();
    }
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selected_provider = 0;
            assert!(!view.providers[0].native_vision);
            assert!(
                view.providers[0].vision_model.read(cx).is_editable(),
                "separate-vision fields must be editable when the LLM lacks native vision"
            );
            assert_eq!(
                view.providers[0].vision_model.read(cx).value().to_string(),
                "kept-vision-model"
            );
            // Declaring native vision grays the separate fields in place…
            view.toggle_vision(cx);
            assert!(view.providers[0].native_vision);
            assert!(
                !view.providers[0].vision_model.read(cx).is_editable(),
                "native vision must lock the separate vision fields"
            );
            // …without losing their stored content, and saving keeps both.
            assert_eq!(
                view.providers[0].vision_model.read(cx).value().to_string(),
                "kept-vision-model"
            );
            view.save_providers_checked(cx).expect("provider list saves");
        })
    });
    let persisted = RuntimeSettings::load_or_default(&settings_path).unwrap();
    assert!(persisted.providers[0].native_vision);
    assert_eq!(persisted.providers[0].vision_model.as_deref(), Some("kept-vision-model"));
    // Turning native vision off restores editing with the values intact.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.toggle_vision(cx);
            assert!(!view.providers[0].native_vision);
            assert!(view.providers[0].vision_model.read(cx).is_editable());
            assert_eq!(
                view.providers[0].vision_model.read(cx).value().to_string(),
                "kept-vision-model"
            );
        })
    });
}

#[gpui::test]
fn settings_actions_keep_intrinsic_width_in_both_languages(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Settings);
    for language in [UiLanguage::SimplifiedChinese, UiLanguage::English] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.language = language;
                cx.notify();
            })
        });
        draw(cx);
        let content = cx.debug_bounds("global-data-directory").unwrap();
        for selector in [
            "action-button-save-global-preferences",
            "action-button-cycle-log-level",
            "action-button-settings-toggle-language",
        ] {
            let button = cx.debug_bounds(selector).expect("settings action rendered");
            assert!(
                button.size.width > px(20.) && button.size.width < content.size.width / 2.,
                "settings action must not stretch across the content in {language:?}: {button:?}"
            );
        }
    }
}

struct ButtonWidthFixture;
impl Render for ButtonWidthFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(600.))
            .v_flex()
            .child(action_button("width-column").label("保存"))
            .child(
                div()
                    .flex()
                    .h(px(60.))
                    .items_center()
                    .child(action_button("width-row").label("保存")),
            )
            .child(
                div()
                    .w(px(300.))
                    .v_flex()
                    .child(action_button("width-narrow-column").label("保存")),
            )
    }
}

#[gpui::test]
fn action_width_depends_on_content_across_parent_layouts(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|_, _| ButtonWidthFixture);
    draw(cx);
    let column = cx.debug_bounds("action-button-width-column").unwrap();
    assert!(column.size.width > px(20.) && column.size.width < px(150.));
    for selector in ["action-button-width-row", "action-button-width-narrow-column"] {
        assert_eq!(cx.debug_bounds(selector).unwrap().size.width, column.size.width);
    }
}

#[gpui::test]
fn usage_page_renders_empty_zero_charts_and_resets_pagination_after_filtering(
    cx: &mut TestAppContext,
) {
    use crate::application::usage_audit::{
        AuditKind, AuditRecord, AuditSourceContext, UsageAuditModel, UsageRecord,
    };
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Usage);
    // Painting culls primitives outside the viewport; keep all four chart bodies visible.
    cx.simulate_resize(gpui::size(px(1400.), px(2200.)));
    draw(cx);
    for selector in
        ["usage-daily-chart", "usage-project-chart", "usage-provider-chart", "usage-runtime-chart"]
    {
        assert!(
            cx.debug_bounds(selector).unwrap().size.height > px(30.),
            "empty chart must remain readable"
        );
    }
    for selector in [
        "usage-daily-plot",
        "usage-project-chart-donut",
        "usage-provider-chart-donut",
        "usage-runtime-chart-donut",
    ] {
        assert!(
            cx.debug_bounds(selector).unwrap().size.height > px(150.),
            "empty data must show a complete chart scaffold"
        );
    }
    assert!(cx.debug_bounds("usage-daily-empty").is_some());
    assert!(
        cx.debug_bounds("usage-daily-chart").unwrap().top()
            < cx.debug_bounds("usage-summary").unwrap().top(),
        "charts should precede text summaries"
    );
    assert_usage_series_painted(cx);
    let audit = AuditRecord {
        timestamp_unix_seconds: 0,
        timestamp_known: true,
        kind: AuditKind::Approval,
        summary: "approved".into(),
        source: AuditSourceContext {
            project_id: "p".into(),
            provider_id: "provider".into(),
            runtime_id: "codex".into(),
            session_id: "s".into(),
            source_type: "session".into(),
            source_id: "s".into(),
            locator: "sessions/s.md#body-line:1".into(),
        },
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.usage_audit_cached = Some((
                std::time::Instant::now(),
                UsageAuditModel {
                    usage: vec![UsageRecord {
                        project_id: "p".into(),
                        provider_id: "provider".into(),
                        runtime_id: "codex".into(),
                        session_id: "s".into(),
                        timestamp_unix_seconds: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        usage_reported: true,
                        source_locator: "sessions/s.md#front-matter:usage".into(),
                    }],
                    audit: vec![audit; 201],
                    diagnostics: Vec::new(),
                },
            ));
            view.audit_page = 4;
            cx.notify();
        });
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.audit_page), 4);
    for selector in
        ["usage-daily-chart", "usage-project-chart", "usage-provider-chart", "usage-runtime-chart"]
    {
        assert!(
            cx.debug_bounds(selector).unwrap().size.height > px(30.),
            "zero chart must remain readable"
        );
    }
    assert!(
        cx.debug_bounds("usage-daily-empty").is_none(),
        "recorded zero usage must use the real dataset"
    );
    assert!(cx.debug_bounds("usage-daily-zero").is_some());
    assert_usage_series_painted(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let model = &mut view.usage_audit_cached.as_mut().unwrap().1;
            model.usage[0].input_tokens = 100;
            model.usage[0].output_tokens = 50;
            cx.notify();
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("usage-daily-zero").is_none());
    assert_usage_series_painted(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.audit_filter.update(cx, |input, cx| input.set_value("missing-source", window, cx));
            view.usage_filter.update(cx, |input, cx| input.set_value("missing-source", window, cx));
            cx.notify();
        });
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.audit_page), 0);
    assert!(cx.debug_bounds("usage-daily-empty").is_some());
    assert!(
        cx.debug_bounds("usage-daily-plot").unwrap().size.height > px(150.),
        "an empty filter result uses the same default scaffold"
    );
    assert_usage_series_painted(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.usage_audit_cached.as_ref().unwrap().1.usage.len()),
        1,
        "default scaffold must not insert sample records"
    );
}

fn assert_usage_series_painted(cx: &mut VisualTestContext) {
    let plot = cx.debug_bounds("usage-daily-plot").unwrap();
    cx.update(|window, _| {
        let scale = window.scale_factor();
        for color in [super::ACCENT, 0x008b_5cf6] {
            let color: gpui::Background = super::rgb(color).into();
            let dots = window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.background == color
                        && quad.bounds.size.width.as_f32() <= 8. * scale
                        && quad.bounds.origin.x.as_f32() >= plot.left().as_f32() * scale
                        && quad.bounds.origin.y.as_f32() >= plot.top().as_f32() * scale
                        && quad.bounds.origin.y.as_f32() < plot.bottom().as_f32() * scale
                })
                .count();
            assert!(dots >= 2, "both series must paint visible data points, not only text or container bounds: plot={plot:?}, dots={dots}, color={color:?}");
        }
    });
}

#[gpui::test]
fn overview_native_charts_draw_zero_and_real_data_and_keep_narrow_scrolling(
    cx: &mut TestAppContext,
) {
    use circuitfabric_contracts::{
        ConstraintResult, FactStatus, LogicalCircuitSnapshot, Project, Severity, SnapshotAuthority,
    };
    use circuitfabric_project::{DocumentCategory, SessionSeed};
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1450.), px(1500.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.bridge_probed_at = Some(std::time::Instant::now());
            cx.notify();
        })
    });
    draw(cx);
    for selector in ["overview-fact-ring", "overview-session-plot", "overview-resource-chart"] {
        assert!(cx.debug_bounds(selector).unwrap().size.height > px(100.));
    }
    assert_overview_line_painted(cx, true);
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = circuitfabric_project::ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "真实项目".into(), description: None },
    )
    .unwrap();
    let source = profile.0.join("evidence.md");
    std::fs::write(&source, "evidence").unwrap();
    store.import_document(source, DocumentCategory::Datasheet, "ui fixture").unwrap();
    store
        .start_session(SessionSeed {
            session_id: "s".into(),
            runtime_profile_id: "local".into(),
            backend_id: None,
        })
        .unwrap();
    let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
    snapshot.snapshot_hash = "tip".into();
    for (index, status) in
        [FactStatus::Passed, FactStatus::Failed, FactStatus::Inconclusive, FactStatus::NotRun]
            .into_iter()
            .enumerate()
    {
        snapshot.constraints.push(ConstraintResult {
            constraint_id: index.to_string(),
            layer: "logic".into(),
            subject_refs: vec![],
            status,
            severity: Severity::Warning,
            evidence_refs: vec![],
            snapshot_hash: "tip".into(),
            explanation: "check".into(),
            recommendation: None,
        });
    }
    std::fs::write(root.join("logic/snapshots/tip.json"), serde_json::to_vec(&snapshot).unwrap())
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::application::project_data::attach_project_storage(
                &mut view.workspace,
                &mut view.project_storages,
                &mut view.project_data,
                store,
            )
            .unwrap();
            cx.notify();
        })
    });
    draw(cx);
    assert_overview_line_painted(cx, true);
    let bars = cx.debug_bounds("overview-bars-p").unwrap();
    cx.update(|window, _| {
        let scale = window.scale_factor();
        for color in super::overview_charts::RESOURCE_COLORS {
            let color: gpui::Background = super::rgb(color).into();
            assert!(
                window.painted_quads().iter().any(|q| q.background == color
                    && q.bounds.size.width.as_f32() > 10. * scale
                    && q.bounds.size.height.as_f32() == 16. * scale
                    && q.bounds.origin.y.as_f32() >= bars.top().as_f32() * scale),
                "each real resource must draw a colored bar"
            );
        }
    });
    assert_eq!(view.update(cx, |view, _| view.overview_model().facts), [1; 4]);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.project_read_errors.insert("p".into(), "corrupt".into());
            cx.notify();
        })
    });
    draw(cx);
    assert_overview_line_painted(cx, false);
    assert_eq!(view.update(cx, |view, _| view.overview_model().resources[0].counts), None);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.project_read_errors.clear();
            cx.notify();
        })
    });
    cx.simulate_resize(gpui::size(px(850.), px(700.)));
    draw(cx);
    let ring = cx.debug_bounds("overview-fact-ring").unwrap();
    let plot = cx.debug_bounds("overview-session-plot").unwrap();
    assert_eq!(ring.size.height, px(160.));
    assert_eq!(plot.size.height, px(160.));
    assert!(plot.top() > ring.bottom(), "narrow window stacks chart panels");
    assert!(plot.right() <= px(850.), "plot remains inside narrow window");
    let heading = cx.debug_bounds("overview-project-kpi").unwrap();
    scroll(cx, 650., 600.);
    assert!(
        cx.debug_bounds("overview-project-kpi").unwrap().top() < heading.top(),
        "flow page scrolls rather than compressing chart canvases"
    );
}

fn assert_overview_line_painted(cx: &mut VisualTestContext, expected: bool) {
    let plot = cx.debug_bounds("overview-session-plot").unwrap();
    assert!(
        plot.size.width > px(200.),
        "line canvas must have actual horizontal space, not seven dots on one vertical line"
    );
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let color: gpui::Background = super::rgb(super::ACCENT).into();
        let dots: Vec<_> = window
            .painted_quads()
            .into_iter()
            .filter(|q| {
                q.background == color
                    && q.bounds.size.width.as_f32() == 6. * scale
                    && q.bounds.size.height.as_f32() == 6. * scale
                    && q.bounds.origin.x.as_f32() >= plot.left().as_f32() * scale
                    && q.bounds.origin.y.as_f32() >= plot.top().as_f32() * scale
                    && q.bounds.origin.y.as_f32() < plot.bottom().as_f32() * scale
            })
            .collect();
        assert_eq!(
            dots.len() >= 7,
            expected,
            "known data draws seven real daily points; unknown source draws no zero series"
        );
        if expected {
            let left =
                dots.iter().map(|q| q.bounds.origin.x.as_f32()).fold(f32::INFINITY, f32::min);
            let right =
                dots.iter().map(|q| q.bounds.origin.x.as_f32()).fold(f32::NEG_INFINITY, f32::max);
            assert!(right - left > 200. * scale, "painted daily points span the date axis");
        }
    });
}

#[gpui::test]
fn overview_refresh_recovers_failed_sources_registry_and_removes_projects(cx: &mut TestAppContext) {
    use circuitfabric_contracts::Project;
    use circuitfabric_project::{ProjectRegistry, ProjectStorage};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "恢复验证".into(), description: None },
    )
    .unwrap();
    let registry_path = profile.0.join("projects.json");
    let mut registry = ProjectRegistry::default();
    registry.register(&store).unwrap();
    registry.save(&registry_path).unwrap();
    let saved_registry = std::fs::read(&registry_path).unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.bridge_probed_at = Some(std::time::Instant::now());
            view.navigation.selected_project = Some("p".into());
            cx.notify();
        })
    });
    assert!(view.update(cx, |view, _| view.overview_model().complete()));
    let old_stamp = view.read_with(cx, |view, _| view.project_data["p"].loaded_at_unix_nanos);
    let broken = root.join("logic/changesets/broken.json");
    std::fs::write(&broken, "broken").unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert_eq!(view.update(cx, |view, _| view.overview_model().resources[0].counts), None);
    assert_eq!(
        view.read_with(cx, |view, _| view.project_data["p"].loaded_at_unix_nanos),
        old_stamp
    );
    assert!(view.read_with(cx, |view, _| view.project_read_errors["p"].contains("broken.json")));
    std::fs::remove_file(broken).unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().complete()));
    assert!(view.read_with(cx, |view, _| view.project_read_errors.is_empty()));
    std::fs::write(&registry_path, "broken registry").unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().registry_unknown));
    assert!(!view.update(cx, |view, _| view.overview_model().complete()));
    std::fs::write(&registry_path, saved_registry).unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().complete()));
    view.update(cx, |view, cx| {
        view.language = super::UiLanguage::English;
        view.navigation.selected_project = Some("p".into());
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(850.), px(700.)));
    draw(cx);
    assert!(cx.debug_bounds("overview-session-plot").unwrap().size.width > px(200.));
    ProjectRegistry::default().save(&registry_path).unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().resources.is_empty()));
    assert!(view.read_with(cx, |view, _| view.navigation.selected_project.is_none()));
    assert!(root.exists(), "removal from registry does not delete domain files");
}

struct SessionScrollFixture {
    markdown: gpui::Entity<gpui_component::text::TextViewState>,
}

impl Render for SessionScrollFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            gpui_component::text::TextView::new(&self.markdown)
                .w_full()
                .h_full()
                .p_4()
                .scrollable(true)
                .selectable(true),
        )
    }
}

/// Exercise the same list offsets used by scrollbar dragging, with a cold first
/// layout followed by many positions. Print timing as evidence; the regression
/// assertion uses block boundaries rather than a machine-dependent frame limit.
#[gpui::test]
fn session_scrollbar_uses_independent_message_blocks_and_keeps_its_extent(cx: &mut TestAppContext) {
    use gpui::AppContext;
    use gpui_component::text::TextViewState;
    cx.update(gpui_component::init);
    let raw = if let Some(path) = std::env::var_os("CF_SESSION_SCROLL_FIXTURE") {
        let raw = std::fs::read_to_string(path).unwrap();
        raw.split_once("\n---\n").map_or(raw.clone(), |(_, body)| body.to_owned())
    } else {
        let paragraph = "Readable 中文 extraction detail. ".repeat(400);
        let mut raw = String::from("## 轮次与工具调用\n\n");
        for turn in 0..8 {
            raw.push_str(&format!("- **1970-01-01T00:00:0{turn}Z** · 智能体 · {paragraph}\n"));
        }
        raw.push_str("- **1970-01-01T00:00:09Z** · 智能体 · 消息\n\n");
        raw.push_str(&"    ## Step\n\n    **Formatted** details\n\n".repeat(300));
        raw.push_str("    FINAL MARKER\n");
        raw
    };
    let event_count = raw.lines().filter(|line| line.starts_with("- **")).count();
    let source = super::session_markdown::presentation_source(&raw);
    let original = cx.new(|cx| TextViewState::markdown(&raw, cx));
    let corrected = cx.new(|cx| TextViewState::markdown(&source, cx));
    let mut fixture = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| SessionScrollFixture { markdown: original.clone() });
        fixture = Some(view.clone());
        gpui_component::Root::new(view, window, cx).bordered(false)
    });
    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    draw(cx);
    let baseline_count = cx.update(|_, cx| original.read(cx).list_state().item_count());
    let before = measure_session_drag(cx, &original);
    fixture.unwrap().update(cx, |fixture, cx| {
        fixture.markdown = corrected.clone();
        cx.notify();
    });
    draw(cx);
    let fixed_count = cx.update(|_, cx| corrected.read(cx).list_state().item_count());
    assert!(
        fixed_count > baseline_count,
        "the audit envelope must not collapse all messages into one list block"
    );
    assert!(
        fixed_count >= event_count * 2,
        "each audit event needs its own header and body blocks"
    );
    let after = measure_session_drag(cx, &corrected);
    eprintln!(
        "Session scrollbar layout comparison: chars={}, blocks={}→{}, median_us={}→{}, p95_us={}→{}",
        raw.len(),
        baseline_count,
        fixed_count,
        before.0,
        after.0,
        before.1,
        after.1
    );
    // Full-history copy still works even though blocks outside the viewport are culled.
    corrected.update(cx, |state, cx| {
        state.select_all(cx);
        let text = state.selected_text();
        assert!(!text.is_empty());
        if raw.contains("FINAL MARKER") {
            assert!(text.contains("FINAL MARKER"));
        }
    });
}

fn measure_session_drag(
    cx: &mut VisualTestContext,
    markdown: &gpui::Entity<gpui_component::text::TextViewState>,
) -> (u128, u128) {
    let list = cx.update(|_, cx| markdown.read(cx).list_state().clone());
    let extent = list.max_offset_for_scrollbar().y;
    assert!(extent > px(0.));
    list.scrollbar_drag_started();
    let mut times = Vec::new();
    for index in 0..12 {
        let started = std::time::Instant::now();
        let fraction = f32::from((index * 7 % 12) as u8) / 11.;
        list.set_offset_from_scrollbar(point(px(0.), -extent * fraction));
        markdown.update(cx, |_, cx| cx.notify());
        draw(cx);
        times.push(started.elapsed().as_micros());
        assert_eq!(
            list.max_offset_for_scrollbar().y,
            extent,
            "scrollbar extent must remain stable during dragging"
        );
    }
    list.scrollbar_drag_ended();
    times.sort_unstable();
    (times[times.len() / 2], times[times.len() * 95 / 100])
}

struct AutoHeightDialog;
impl Render for AutoHeightDialog {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .v_flex()
            .child(row("auto-dialog-header", 30.))
            .child(
                capped_scroll_body("auto-dialog-body", px(200.))
                    .v_flex()
                    .child(row("auto-dialog-first", 120.))
                    .child(row("auto-dialog-last", 480.)),
            )
            .child(row("auto-dialog-footer", 40.))
    }
}

#[gpui::test]
fn auto_height_dialog_body_grows_to_its_cap_and_still_scrolls(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|_, _| AutoHeightDialog);
    draw(cx);
    let footer = cx.debug_bounds("auto-dialog-footer").unwrap();
    assert_eq!(footer.top(), px(230.));
    let first = cx.debug_bounds("auto-dialog-first").unwrap();
    scroll(cx, 100., 100.);
    assert!(cx.debug_bounds("auto-dialog-first").unwrap().top() < first.top());
    assert_eq!(cx.debug_bounds("auto-dialog-footer").unwrap(), footer);
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn scroll(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(x), px(y)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-90.))),
        ..Default::default()
    });
    draw(cx);
}

fn row(selector: &'static str, height: f32) -> Div {
    div().h(px(height)).flex_none().debug_selector(move || selector.to_owned())
}

struct LayoutFixture {
    screen: ControlPlaneScreen,
}
impl Render for LayoutFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let content = match PageLayout::for_screen(self.screen) {
            PageLayout::Flow => page("flow-content")
                .child(row("flow-first", 400.))
                .child(row("flow-last", 400.))
                .into_any_element(),
            PageLayout::Workspace => workspace_page("workspace-content")
                .child(row("workspace-heading", 30.))
                .child(div().flex().flex_1().min_h(px(0.)).children(
                    [("left-pane", "left-first"), ("right-pane", "right-first")].into_iter().map(
                        |(id, selector)| {
                            div().id(id).flex_1().v_flex().scroll_y().child(row(selector, 600.))
                        },
                    ),
                ))
                .into_any_element(),
        };
        div()
            .w(px(500.))
            .h(px(300.))
            .v_flex()
            .child(row("fixed-header", 30.))
            .child(div().flex_1().min_h(px(0.)).child(page_host(self.screen, content)))
            .child(row("fixed-footer", 20.))
    }
}

#[gpui::test]
fn flow_page_scrolls_without_moving_shell_chrome(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) =
        cx.add_window_view(|_, _| LayoutFixture { screen: ControlPlaneScreen::Documents });
    draw(cx);
    let first = cx.debug_bounds("flow-first").unwrap();
    let header = cx.debug_bounds("fixed-header").unwrap();
    let footer = cx.debug_bounds("fixed-footer").unwrap();
    assert_eq!(footer.top(), px(280.));
    scroll(cx, 100., 120.);
    assert!(cx.debug_bounds("flow-first").unwrap().top() < first.top());
    assert_eq!(cx.debug_bounds("fixed-header").unwrap(), header);
    assert_eq!(cx.debug_bounds("fixed-footer").unwrap(), footer);
}

#[gpui::test]
fn workspace_panes_scroll_independently_and_keep_heading_visible(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) =
        cx.add_window_view(|_, _| LayoutFixture { screen: ControlPlaneScreen::AgentsAndMcp });
    draw(cx);
    let left = cx.debug_bounds("left-first").unwrap();
    let right = cx.debug_bounds("right-first").unwrap();
    let heading = cx.debug_bounds("workspace-heading").unwrap();
    scroll(cx, 100., 140.);
    assert!(cx.debug_bounds("left-first").unwrap().top() < left.top());
    assert_eq!(cx.debug_bounds("right-first").unwrap(), right);
    assert_eq!(cx.debug_bounds("workspace-heading").unwrap(), heading);
    assert_eq!(cx.debug_bounds("fixed-footer").unwrap().top(), px(280.));
}
