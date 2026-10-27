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

#[gpui::test]
fn materialization_states_are_visible_on_changes_sessions_and_eda_pages(cx: &mut TestAppContext) {
    use circuitfabric_contracts::Project;
    use circuitfabric_project::{
        ChangeSetAuditEntry, ChangeSetStage, ChangeSetStageStatus as Status, ProjectStorage,
        StoredChangeSet,
    };
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "测试".into(), description: None },
    )
    .unwrap();
    let mut record = StoredChangeSet {
        schema_version: 1,
        id: "cs".into(),
        base_snapshot_hash: "base".into(),
        target_snapshot_hash: "target".into(),
        plan_hash: "plan".into(),
        ir_diff: vec![],
        evidence: vec![],
        preview_validation: Default::default(),
        execution: Default::default(),
        observed_snapshot_hash: None,
        rollback_handle: Some("handle".into()),
        audit: vec![],
    };
    record.execution.write =
        ChangeSetStage { status: Status::Passed, detail: "已确认写入".into() };
    record.execution.readback =
        ChangeSetStage { status: Status::Inconclusive, detail: "回读缺失".into() };
    record.execution.verification =
        ChangeSetStage { status: Status::Inconclusive, detail: "尚无法判定".into() };
    store.save_change_set(&record).unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::ChangesAndApprovals);
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
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
    for status in ["inconclusive", "verified", "cancelled", "rollback_executed"] {
        if status == "verified" {
            record.execution.readback.status = Status::Passed;
            record.execution.verification.status = Status::Passed;
        }
        if status == "cancelled" || status == "rollback_executed" {
            record.audit.push(ChangeSetAuditEntry {
                timestamp_unix_seconds: 1,
                actor: "bridge".into(),
                decision: status.into(),
                reason: "验收".into(),
                observed_snapshot_hash: None,
                rollback_handle: Some("handle".into()),
            });
        }
        ProjectStorage::open(&root).unwrap().save_change_set(&record).unwrap();
        cx.update(|_, cx| view.update(cx, |view, _| view.refresh_project_data("p").unwrap()));
        for (surface, screen) in [
            ("changes", ControlPlaneScreen::ChangesAndApprovals),
            ("sessions", ControlPlaneScreen::SessionsAndTasks),
            ("eda", ControlPlaneScreen::EdaServices),
        ] {
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.navigation.screen = screen;
                    cx.notify();
                })
            });
            draw(cx);
            let selector: &'static str =
                Box::leak(format!("materialization-{surface}-cs-{status}").into_boxed_str());
            assert!(cx.debug_bounds(selector).is_some(), "{surface} must show {status}");
            if status == "inconclusive" {
                let selector: &'static str =
                    Box::leak(format!("materialization-{surface}-cs-verified").into_boxed_str());
                assert!(cx.debug_bounds(selector).is_none(), "write alone cannot look successful");
            }
        }
    }
}

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
        gpui_component::Root::new(entity, window, cx)
    });
    (view.unwrap(), cx)
}

#[gpui::test]
fn assistant_composer_grows_with_long_input_and_stops_at_its_maximum(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.toggle_assistant(window, cx)));
    draw(cx);
    let single = cx.debug_bounds("assistant-composer").expect("composer").size.height;
    let input = view.read_with(cx, |v, _| v.assistant.input.clone());
    let set = |cx: &mut VisualTestContext, text: String| {
        cx.update(|window, cx| input.update(cx, |p, cx| p.set_value(text, window, cx)));
        draw(cx);
        cx.debug_bounds("assistant-composer").unwrap().size.height
    };
    let wrapped = set(cx, "很长的一段需求描述，".repeat(12));
    assert!(wrapped > single, "soft-wrapped text grows the box: {wrapped:?} > {single:?}");
    let capped = set(cx, "第一行\n".repeat(30));
    let still_capped = set(cx, "第一行\n".repeat(60));
    assert!(capped > wrapped);
    assert_eq!(capped, still_capped, "beyond the maximum the box stops growing and scrolls");
    assert!(capped < px(220.));
}

#[gpui::test]
fn assistant_cascader_saves_each_level_and_sits_beside_the_input_on_horizontal_docks(
    cx: &mut TestAppContext,
) {
    use circuitfabric_codex_runtime::{ReasoningEffort, execution::AgentKind};
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.toggle_assistant(window, cx)));
    draw(cx);
    // Side dock: the trigger sits below the input.
    let composer = cx.debug_bounds("assistant-composer").unwrap();
    let trigger = cx.debug_bounds("assistant-model-selector-trigger").expect("trigger");
    assert!(trigger.top() >= composer.bottom());

    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("assistant-model-selector-panel").is_some(), "popup opens");
    let claude = cx.debug_bounds("assistant-runtime-claude-code").expect("runtime column");
    cx.simulate_click(claude.center(), Default::default());
    draw(cx);
    let high = cx.debug_bounds("assistant-effort-high").expect("effort column");
    cx.simulate_click(high.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.saved_settings.assistant.runtime, Some(AgentKind::Claude));
        assert_eq!(v.saved_settings.assistant.reasoning_effort, Some(ReasoningEffort::High));
        let reloaded =
            circuitfabric_codex_runtime::RuntimeSettings::load_or_default(&v.settings_path)
                .unwrap();
        assert_eq!(reloaded.assistant, v.saved_settings.assistant, "persisted");
        let (kind, effective) = reloaded.for_assistant();
        assert_eq!(kind, AgentKind::Claude);
        let provider =
            circuitfabric_codex_runtime::execution::selected_provider(&effective, kind).unwrap();
        assert_eq!(provider.reasoning_effort, Some(ReasoningEffort::High));
    });

    // Bottom dock: the trigger moves to the right of the input.
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.assistant.dock = super::assistant::AssistantDock::Bottom;
            cx.notify();
        })
    });
    draw(cx);
    let composer = cx.debug_bounds("assistant-composer").unwrap();
    let trigger = cx.debug_bounds("assistant-model-selector-trigger").unwrap();
    assert!(trigger.left() >= composer.right());
    assert!(trigger.bottom() <= composer.bottom() + px(1.));
}

#[gpui::test]
fn assistant_sessions_switch_via_the_picker_and_keep_contexts_separate(cx: &mut TestAppContext) {
    use crate::application::assistant::{
        AssistantSession, AssistantSessions, ChatMessage, Role, load_sessions,
    };
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.toggle_assistant(window, cx)));
    draw(cx);
    let message = |role: Role, text: &str| ChatMessage {
        role,
        text: text.into(),
        reasoning: String::new(),
        context: String::new(),
        pending: false,
        actions: Vec::new(),
        error: None,
        reasoning_collapsed: true,
    };
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.assistant.sessions = AssistantSessions {
                sessions: vec![
                    AssistantSession {
                        id: "s1".into(),
                        title: "电源项目".into(),
                        messages: vec![message(Role::User, "打开电源项目")],
                    },
                    AssistantSession {
                        id: "s2".into(),
                        title: "BOM 导出".into(),
                        messages: vec![
                            message(Role::User, "导出 BOM"),
                            message(Role::Assistant, "好的。"),
                        ],
                    },
                ],
                active_id: "s1".into(),
            };
            cx.notify();
        })
    });
    draw(cx);
    // The header topic opens the picker, above the composer.
    let composer = cx.debug_bounds("assistant-composer").unwrap();
    let trigger = cx.debug_bounds("assistant-session-selector-trigger").expect("trigger");
    assert!(trigger.bottom() <= composer.top(), "the picker trigger sits in the header");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("assistant-session-panel").is_some(), "picker opens");
    // Switching swaps the visible conversation's context.
    let item = cx.debug_bounds("assistant-session-item-1").expect("session row");
    cx.simulate_click(item.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.assistant.sessions.active_id, "s2");
        assert_eq!(v.assistant.sessions.active().messages.len(), 2, "context swapped");
        assert_eq!(v.assistant.sessions.sessions[0].id, "s2", "most recently used first");
    });
    assert!(cx.debug_bounds("assistant-session-panel").is_none(), "picker closes on switch");
    // Both conversations persist with their own messages and the new active one.
    let reloaded = load_sessions(&profile.0.join("assistant-history.json")).unwrap();
    assert_eq!(reloaded.active_id, "s2");
    assert_eq!(reloaded.sessions.len(), 2);
    assert_eq!(reloaded.sessions.iter().find(|s| s.id == "s1").unwrap().messages.len(), 1);
}

#[gpui::test]
fn assistant_new_session_starts_a_fresh_context_and_reuses_the_draft(cx: &mut TestAppContext) {
    use crate::application::assistant::{AssistantSession, AssistantSessions, ChatMessage, Role};
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.toggle_assistant(window, cx)));
    draw(cx);
    let message = |role: Role, text: &str| ChatMessage {
        role,
        text: text.into(),
        reasoning: String::new(),
        context: String::new(),
        pending: false,
        actions: Vec::new(),
        error: None,
        reasoning_collapsed: true,
    };
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.assistant.sessions = AssistantSessions {
                sessions: vec![AssistantSession {
                    id: "s1".into(),
                    title: "电源项目".into(),
                    messages: vec![message(Role::User, "打开电源项目")],
                }],
                active_id: "s1".into(),
            };
            cx.notify();
        })
    });
    draw(cx);
    let trigger = cx.debug_bounds("assistant-session-selector-trigger").expect("trigger");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    let new = cx.debug_bounds("action-button-assistant-session-new").expect("new-session button");
    cx.simulate_click(new.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.assistant.sessions.sessions.len(), 2);
        assert!(v.assistant.sessions.active().messages.is_empty(), "fresh context");
        assert!(v.assistant.sessions.active().title.is_empty());
        assert_ne!(v.assistant.sessions.active_id, "s1");
    });
    assert!(
        cx.debug_bounds("assistant-empty-state").is_some(),
        "the fresh conversation shows the empty state"
    );
    // Asking for another new session while on the empty draft reuses it.
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    let new = cx.debug_bounds("action-button-assistant-session-new").unwrap();
    cx.simulate_click(new.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.assistant.sessions.sessions.len(), 2, "no duplicate drafts");
    });
}

#[gpui::test]
fn assistant_message_text_selects_and_copies_to_the_clipboard(cx: &mut TestAppContext) {
    use crate::application::assistant::{AssistantSession, AssistantSessions, ChatMessage, Role};
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    // Opening the panel focuses the composer — the state a copy must escape.
    cx.update(|window, cx| view.update(cx, |v, cx| v.toggle_assistant(window, cx)));
    draw(cx);
    let text = "LM1117 是一款三端稳压器\n输出电流最高 800mA\n输入电压最高 15V";
    let message = |role: Role| ChatMessage {
        role,
        text: text.to_owned(),
        reasoning: String::new(),
        context: "总览".into(),
        pending: false,
        actions: Vec::new(),
        error: None,
        reasoning_collapsed: true,
    };
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.assistant.sessions = AssistantSessions {
                sessions: vec![AssistantSession {
                    id: "s1".into(),
                    title: "资料收集".into(),
                    messages: vec![message(Role::User), message(Role::Assistant)],
                }],
                active_id: "s1".into(),
            };
            cx.notify();
        })
    });
    draw(cx);
    let bubble = cx.debug_bounds("assistant-message-bubble-1").expect("assistant bubble");
    // Inside the bubble's px-3/py-2 padding, on the text itself.
    let start = point(bubble.left() + px(14.), bubble.top() + px(12.));
    let end = point(bubble.right() - px(14.), bubble.bottom() - px(12.));
    // A span that crosses into the next message — how a user copies a
    // conversation snippet.
    let first = cx.debug_bounds("assistant-message-bubble-0").expect("first bubble");
    let cross_start = point(first.left() + px(14.), first.top() + px(12.));
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(cx);
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_, cx| {
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), text);
    });
    // Cross-message span: both bubbles join one selection document.
    cx.simulate_mouse_down(cross_start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(cx);
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            format!("{text}\n{text}"),
            "dragging across messages copies both in order"
        );
    });
    // The per-message copy button writes regardless of focus or selection.
    let copy = cx.debug_bounds("action-button-assistant-copy-0").expect("copy button");
    cx.simulate_click(copy.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), text);
    });
}

#[gpui::test]
fn assistant_session_delete_removes_only_that_conversation(cx: &mut TestAppContext) {
    use crate::application::assistant::{AssistantSession, AssistantSessions, ChatMessage, Role};
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.toggle_assistant(window, cx)));
    draw(cx);
    let message = |role: Role, text: &str| ChatMessage {
        role,
        text: text.into(),
        reasoning: String::new(),
        context: String::new(),
        pending: false,
        actions: Vec::new(),
        error: None,
        reasoning_collapsed: true,
    };
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.assistant.sessions = AssistantSessions {
                sessions: vec![
                    AssistantSession {
                        id: "s1".into(),
                        title: "电源项目".into(),
                        messages: vec![message(Role::User, "打开电源项目")],
                    },
                    AssistantSession {
                        id: "s2".into(),
                        title: "BOM 导出".into(),
                        messages: vec![message(Role::User, "导出 BOM")],
                    },
                ],
                active_id: "s1".into(),
            };
            cx.notify();
        })
    });
    draw(cx);
    let trigger = cx.debug_bounds("assistant-session-selector-trigger").expect("trigger");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    // Deleting the active conversation moves to its neighbor.
    let delete = cx.debug_bounds("action-button-assistant-session-delete-0").expect("delete");
    cx.simulate_click(delete.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.assistant.sessions.sessions.len(), 1);
        assert_eq!(v.assistant.sessions.active_id, "s2");
    });
    assert!(
        cx.debug_bounds("assistant-session-panel").is_some(),
        "the picker stays open for more deletes"
    );
    // Deleting the last conversation leaves one fresh empty session.
    let delete = cx.debug_bounds("action-button-assistant-session-delete-0").unwrap();
    cx.simulate_click(delete.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.assistant.sessions.sessions.len(), 1);
        assert!(v.assistant.sessions.active().messages.is_empty());
    });
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
fn session_followup_composer_is_scoped_and_missing_history_keeps_the_draft(
    cx: &mut TestAppContext,
) {
    use circuitfabric_project::{ProjectStorage, SessionCategory};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project {
            id: "p-original".into(),
            name: "Original".into(),
            description: None,
        },
    )
    .unwrap();
    let settings = circuitfabric_codex_runtime::RuntimeSettings::default();
    let id = crate::application::runtime_session::start_for(
        &store,
        &settings.providers[0].id,
        "codex",
        SessionCategory::Datasheet,
        Some("original-document".into()),
    )
    .unwrap();
    crate::application::runtime_session::finish(
        &store,
        &id,
        "original prompt",
        Ok("original reply"),
        None,
    )
    .unwrap();
    let history = store.session_runtime_directory(&id).unwrap();
    std::fs::create_dir_all(&history).unwrap();
    let binding = history.join("circuitfabric-thread.json");
    std::fs::write(&binding, "{}").unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::SessionsAndTasks);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::application::project_data::attach_project_storage(
                &mut view.workspace,
                &mut view.project_storages,
                &mut view.project_data,
                store.clone(),
            )
            .unwrap();
            view.navigation.selected_project = Some("different-project".into());
            view.open_session_replay("p-original".into(), id.clone(), cx);
        })
    });
    draw(cx);
    let input_bounds = cx.debug_bounds("session-followup-input").expect("composer");
    let modal = cx.debug_bounds("session-detail-modal").unwrap();
    assert!(input_bounds.bottom() < modal.bottom());
    let prompt = view
        .read_with(cx, |v, _| v.session_replay.as_ref().unwrap().followup_prompt.clone().unwrap());
    cx.update(|window, cx| {
        prompt.update(cx, |p, cx| p.set_value("human supplement\nmore details", window, cx))
    });
    draw(cx);
    // Busy extraction must prevent continuation even if invoked programmatically.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.datasheet_extracting = true;
            v.send_session_followup(window, cx);
            assert!(v.session_followup.is_none());
            v.datasheet_extracting = false;
            cx.notify();
        })
    });
    std::fs::remove_file(&binding).unwrap();
    draw(cx);
    let send = cx.debug_bounds("action-button-send-session-followup").unwrap();
    cx.simulate_click(send.center(), Default::default());
    draw(cx);
    view.read_with(cx, |v, cx| {
        let (project, session, feedback) = v.session_followup_feedback.as_ref().unwrap();
        assert_eq!(project, "p-original");
        assert_eq!(session, &id);
        assert!(feedback.contains("原生会话历史"));
        assert!(v.session_followup.is_none());
        assert_eq!(prompt.read(cx).value().as_str(), "human supplement\nmore details");
    });
    assert!(!binding.exists(), "no replacement thread");
    assert_eq!(store.list_sessions().unwrap().sessions.len(), 1);
    assert!(!store.load_session(&id).unwrap().body.contains("human supplement"));
    // Reopening re-evaluates eligibility and never leaks a draft to a different session.
    cx.update(|_, cx| {
        view.update(cx, |v, cx| v.open_session_replay("p-original".into(), id.clone(), cx))
    });
    draw(cx);
    assert!(cx.debug_bounds("session-followup-input").is_none());
    assert!(cx.debug_bounds("session-conversation-body").is_some());
}

#[cfg(windows)]
#[path = "session_followup_tests.rs"]
mod session_followup_tests;

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
    // The detail modal opens on the readable conversation tab; the raw record is one tab away.
    assert!(cx.debug_bounds("session-conversation-body").is_some());
    assert!(cx.debug_bounds("session-replay-body").is_none());
    let raw_tab = cx.debug_bounds("session-tab-raw").expect("raw data tab");
    cx.simulate_click(raw_tab.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("session-replay-body").is_some());
    assert!(cx.debug_bounds("session-conversation-body").is_none());
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
fn session_entry_auto_refresh_lists_externally_written_eda_sessions(cx: &mut TestAppContext) {
    use circuitfabric_project::{ProjectStorage, SessionCategory, SessionSeed};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project {
            id: "p".into(),
            name: "桥接会话".into(),
            description: None,
        },
    )
    .unwrap();
    let seed = |id: &str| SessionSeed {
        session_id: id.into(),
        runtime_profile_id: "provider".into(),
        backend_id: Some("jlcircuit".into()),
    };
    store.start_categorized_session(seed("eda-before"), SessionCategory::Eda, None).unwrap();
    store
        .start_categorized_session(seed("runtime-before"), SessionCategory::Runtime, None)
        .unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::SessionsAndTasks);
    cx.simulate_resize(gpui::size(px(1400.), px(1200.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::application::project_data::attach_project_storage(
                &mut view.workspace,
                &mut view.project_storages,
                &mut view.project_data,
                store.clone(),
            )
            .unwrap();
            view.navigation.selected_project = Some("p".into());
            view.session_category_filter = SessionCategory::Eda;
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("session-row-eda-before").is_some());

    // The EDA bridge writes its record straight into the same project storage, as an
    // external process would; the desktop cache has no knowledge of it yet.
    store.start_categorized_session(seed("eda-after"), SessionCategory::Eda, None).unwrap();
    // A cache fresh enough is not rescanned on redraw.
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    assert!(
        cx.debug_bounds("session-row-eda-after").is_none(),
        "a cache inside the staleness threshold must not be rescanned"
    );

    // Age the cache past the threshold; the next render picks the bridge record up.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.project_data.get_mut("p").unwrap().loaded_at_unix_nanos = 0;
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("session-row-eda-after").is_some(),
        "a stale cache is refreshed during render and lists the bridge session"
    );
    assert!(cx.debug_bounds("session-row-eda-before").is_some());
    assert!(
        cx.debug_bounds("session-row-runtime-before").is_none(),
        "the EDA category must not mix in other categories"
    );
    assert!(view.read_with(cx, |view, _| view.project_read_errors.is_empty()));
    assert!(cx.debug_bounds("sessions-project-read-error").is_none());

    // The runtime category shows its own records and never the new EDA one.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.session_category_filter = SessionCategory::Runtime;
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("session-row-runtime-before").is_some());
    assert!(cx.debug_bounds("session-row-eda-after").is_none());
}

#[gpui::test]
fn runtime_history_only_shows_its_own_project_category_and_backend(cx: &mut TestAppContext) {
    use circuitfabric_project::{ProjectStorage, SessionCategory, SessionSeed};
    let profile = Profile::new();
    let records = [
        ("codex-current", "session-row-codex-current", SessionCategory::Runtime, Some("codex")),
        ("codex-old", "session-row-codex-old", SessionCategory::Runtime, Some("codex-app-server")),
        (
            "claude-current",
            "session-row-claude-current",
            SessionCategory::Runtime,
            Some("claude-code"),
        ),
        ("claude-old", "session-row-claude-old", SessionCategory::Runtime, Some("claude")),
        ("kimi-current", "session-row-kimi-current", SessionCategory::Runtime, Some("kimi-code")),
        ("dsh-current", "session-row-dsh-current", SessionCategory::Runtime, Some("dsh")),
        // Pre-category files load as `Legacy` but are the same runtimes'
        // sessions; they must appear on their own runtime's page, not vanish.
        ("codex-legacy", "session-row-codex-legacy", SessionCategory::Legacy, Some("codex")),
        ("kimi-legacy", "session-row-kimi-legacy", SessionCategory::Legacy, Some("kimi-code")),
        ("eda-legacy", "session-row-eda-legacy", SessionCategory::Legacy, Some("jlcircuit")),
        ("no-backend", "session-row-no-backend", SessionCategory::Runtime, None),
        (
            "unknown-backend",
            "session-row-unknown-backend",
            SessionCategory::Runtime,
            Some("unknown"),
        ),
        (
            "datasheet-codex",
            "session-row-datasheet-codex",
            SessionCategory::Datasheet,
            Some("codex"),
        ),
    ];
    let make_store = |id: &str| {
        let root = profile.0.join(id);
        std::fs::create_dir_all(&root).unwrap();
        ProjectStorage::create(
            &root,
            circuitfabric_contracts::Project { id: id.into(), name: id.into(), description: None },
        )
        .unwrap()
    };
    let store = make_store("p");
    for (id, _, category, backend) in records {
        store
            .start_categorized_session(
                SessionSeed {
                    session_id: id.into(),
                    runtime_profile_id: "zai".into(),
                    backend_id: backend.map(str::to_owned),
                },
                category,
                None,
            )
            .unwrap();
    }
    let other = make_store("other");
    other
        .start_categorized_session(
            SessionSeed {
                session_id: "other-project".into(),
                runtime_profile_id: "zai".into(),
                backend_id: Some("codex".into()),
            },
            SessionCategory::Runtime,
            None,
        )
        .unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    cx.simulate_resize(gpui::size(px(1400.), px(2200.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for storage in [store.clone(), other] {
                crate::application::project_data::attach_project_storage(
                    &mut view.workspace,
                    &mut view.project_storages,
                    &mut view.project_data,
                    storage,
                )
                .unwrap();
            }
            view.navigation.selected_project = Some("p".into());
            cx.notify();
        })
    });
    for (adapter, expected) in [
        (RuntimeAdapter::CodexAppServer, vec!["codex-current", "codex-old", "codex-legacy"]),
        (RuntimeAdapter::ClaudeCode, vec!["claude-current", "claude-old"]),
        (RuntimeAdapter::Dsh, vec!["dsh-current"]),
        (RuntimeAdapter::KimiCode, vec!["kimi-current", "kimi-legacy"]),
    ] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.agents_selection = super::AgentsSelection::Runtime(adapter);
                cx.notify();
            })
        });
        draw(cx);
        for (id, selector, _, _) in records {
            assert_eq!(
                cx.debug_bounds(selector).is_some(),
                expected.contains(&id),
                "{} history must only show its own runtime records: {id}",
                adapter.label()
            );
        }
        assert!(cx.debug_bounds("session-row-other-project").is_none());
        let selector = records.iter().find(|(id, _, _, _)| *id == expected[0]).unwrap().1;
        let row = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(row.center(), Default::default());
        draw(cx);
        assert!(cx.debug_bounds("session-detail-modal").is_some());
        view.read_with(cx, |view, _| {
            let selection = view.session_replay.as_ref().unwrap();
            assert_eq!(selection.project_id, "p");
            assert_eq!(selection.replay.metadata.session_id, expected[0]);
            let reason = selection.followup_unavailable.as_deref().unwrap();
            assert!(!reason.contains("此入口目前支持"));
            if adapter != RuntimeAdapter::CodexAppServer {
                assert!(reason.contains(adapter.label()) && reason.contains("尚未接入"));
            }
        });
        cx.update(|_, cx| view.update(cx, ControlPlaneView::close_session_replay));
        draw(cx);
    }
    // The project-wide runtime category intentionally remains cross-runtime.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.navigation.screen = ControlPlaneScreen::SessionsAndTasks;
            view.session_category_filter = SessionCategory::Runtime;
            cx.notify();
        })
    });
    draw(cx);
    for (_, selector, category, _) in records {
        assert_eq!(cx.debug_bounds(selector).is_some(), category == SessionCategory::Runtime);
    }
    assert!(cx.debug_bounds("session-row-other-project").is_none());
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

    // The default tab organizes the audit as a chat: the agent's message is a bubble
    // on the left, and the virtualized list scrolls the long turn in chunks.
    let conversation = cx.debug_bounds("session-conversation-body").expect("conversation tab");
    assert!(cx.debug_bounds("session-replay-body").is_none());
    let bubble = cx.debug_bounds("session-chat-bubble-0").expect("agent bubble");
    assert!(
        bubble.left() - conversation.left() < conversation.right() - bubble.right(),
        "agent bubbles hug the left edge: bubble={bubble:?} body={conversation:?}"
    );
    assert!(bubble.size.width <= conversation.size.width * 0.8);
    let conversation_list = view
        .read_with(cx, |view, _| view.session_replay.as_ref().unwrap().conversation_list.clone());
    let chat_before = conversation_list.logical_scroll_top();
    scroll(cx, conversation.center().x.as_f32(), conversation.center().y.as_f32());
    let chat_after = conversation_list.logical_scroll_top();
    assert!(
        chat_after.item_ix > chat_before.item_ix
            || chat_after.offset_in_item > chat_before.offset_in_item,
        "wheel must scroll the conversation list"
    );

    // The raw tab keeps the full audit record with parsed Markdown and full-text copy.
    let raw_tab = cx.debug_bounds("session-tab-raw").expect("raw data tab");
    cx.simulate_click(raw_tab.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("session-conversation-body").is_none());
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
    for adapter in [
        RuntimeAdapter::CodexAppServer,
        RuntimeAdapter::ClaudeCode,
        RuntimeAdapter::Dsh,
        RuntimeAdapter::KimiCode,
    ] {
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
fn provider_editor_model_dropdown_filters_and_never_takes_layout_space(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::ProviderConnection;
    use circuitfabric_codex_runtime::models::ModelChoice;
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    // Forty choices prove the windowed dropdown: it opens holding one page
    // of fifteen (ten visible, the rest behind the scrollbar), and paging
    // reveals the rest.
    let choices: Vec<ModelChoice> = (0..40)
        .map(|index| ModelChoice {
            id: format!("m{index:02}"),
            display_name: (index == 5).then(|| "GLM 5.3 Flash".to_owned()),
            is_default: index == 0,
            supports_vision: (index == 5).then_some(true),
        })
        .collect();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.provider_editor_open = true;
            // The mounted profile's first draft provider is the default `zai`
            // API-key provider; sign the catalog with its current draft.
            let signature = ControlPlaneView::model_catalog_signature(&view.providers[0], cx);
            view.model_choices = super::ModelChoicesState {
                pending: false,
                fetched_for: Some(signature),
                dropdown_open: false,
                generation: 0,
                choices,
                error: None,
                window: super::MODEL_PAGE_SIZE,
            };
            cx.notify();
        })
    });
    draw(cx);
    let card = cx.debug_bounds("provider-settings-card").expect("provider dialog rendered");
    let wire = cx
        .debug_bounds("action-button-provider-wire-chat")
        .expect("wire protocol control rendered");
    assert!(
        wire.size.height > px(20.) && wire.top() >= card.top() && wire.bottom() <= card.bottom(),
        "wire protocol control must stay inside the dialog: {wire:?}"
    );
    assert!(
        cx.debug_bounds("provider-model-dropdown").is_none(),
        "dropdown stays closed until the model field is focused"
    );
    let caret = cx.debug_bounds("provider-model-caret").expect("caret rendered in the field");
    let field_box = cx.debug_bounds("provider-model-box").expect("input box rendered");
    assert!(
        caret.left() >= field_box.left() && caret.right() <= field_box.right(),
        "the caret sits inside the input box: {caret:?} vs {field_box:?}"
    );

    // The caret opens the dropdown; the overlay must not move the dialog.
    cx.simulate_click(caret.center(), Default::default());
    draw(cx);
    let card_open = cx.debug_bounds("provider-settings-card").expect("provider dialog rendered");
    assert_eq!(card, card_open, "the floating dropdown takes no layout space");
    let panel = cx.debug_bounds("provider-model-dropdown").expect("dropdown panel rendered");
    let field_box = cx.debug_bounds("provider-model-box").expect("input box rendered");
    let top_gap: f32 = (panel.top() - field_box.bottom()).into();
    let left_gap: f32 = (panel.left() - field_box.left()).into();
    let width_gap: f32 = (panel.size.width - field_box.size.width).into();
    assert!(
        top_gap.abs() < 1. && left_gap.abs() < 1. && width_gap.abs() < 1.,
        "panel top edge aligns to the box's bottom-left at field width: {panel:?} vs {field_box:?}"
    );
    assert!(
        cx.debug_bounds("model-dropdown-backdrop").is_some(),
        "an outside-click backdrop exists while the dropdown is open"
    );
    // The field still holds the saved model `glm-5.3-flash`, which is not a
    // catalog id here, so it filters: nothing matches yet.
    assert!(cx.debug_bounds("model-choice-row-m00").is_none());

    // Clearing the field shows the first page: fifteen rows are loaded (ten
    // visible behind the scrollbar), the rest wait for paging.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let state = view.providers[0].model.clone();
            state.update(cx, |state, cx| state.set_value("", window, cx));
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("model-choice-row-m00").is_some());
    assert!(cx.debug_bounds("model-choice-row-m09").is_some());
    assert!(cx.debug_bounds("model-choice-row-m14").is_some());
    assert!(
        cx.debug_bounds("model-choice-row-m15").is_none(),
        "the dropdown opens holding one page of fifteen rows"
    );
    assert!(
        cx.debug_bounds("provider-model-more").is_some(),
        "a load-more affordance exists while matches remain"
    );
    let list = view.read_with(cx, |view, _| view.model_list_scroll.clone());
    let panel = cx.debug_bounds("provider-model-dropdown").unwrap();
    assert!(list.max_offset().y > px(160.), "model rows must belong to the scroll container");
    let first = cx.debug_bounds("model-choice-row-m00").unwrap();
    let tenth = cx.debug_bounds("model-choice-row-m09").unwrap();
    assert_eq!(tenth.bottom() - first.top(), px(320.), "ten full-height rows fit the viewport");
    let viewport = cx.debug_bounds("provider-model-list").expect("scroll viewport");
    assert_eq!(viewport.size.height, px(320.));
    assert!(
        cx.debug_bounds("provider-model-scrollbar").is_some(),
        "overflow has a visible scrollbar"
    );

    // Dispatch a real wheel event over a row: it moves the list without
    // moving the floating panel or the dialog.
    scroll(cx, viewport.center().x.as_f32(), viewport.center().y.as_f32());
    draw(cx);
    assert!(list.offset().y < px(0.), "wheel must move model rows");
    assert!(cx.debug_bounds("model-choice-row-m00").unwrap().top() < first.top());
    assert_eq!(cx.debug_bounds("provider-model-dropdown").unwrap(), panel);
    assert_eq!(cx.debug_bounds("provider-settings-card").unwrap(), card);
    assert_eq!(view.read_with(cx, |view, _| view.model_choices.window), 15);
    for _ in 0..3 {
        scroll(cx, viewport.center().x.as_f32(), viewport.center().y.as_f32());
        draw(cx);
    }
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.model_choices.window),
        30,
        "wheeling to the loaded bottom must load the next fifteen rows"
    );
    assert!(cx.debug_bounds("model-choice-row-m29").is_some());
    assert!(cx.debug_bounds("model-choice-row-m30").is_none());

    // Drag the actual scrollbar thumb to the bottom to load the final page.
    let viewport = cx.debug_bounds("provider-model-list").unwrap();
    let content_height = viewport.size.height + list.max_offset().y;
    let thumb_top = viewport.top() + (-list.offset().y / content_height) * viewport.size.height;
    let start = point(viewport.right() - px(5.), thumb_top + px(12.));
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    draw(cx);
    let end = point(start.x, viewport.bottom() - px(2.));
    cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    draw(cx);
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(cx);
    draw(cx);
    assert!(list.offset().y < px(-500.), "dragging the thumb must scroll the list");
    assert_eq!(
        view.read_with(cx, |view, _| view.model_choices.window),
        45,
        "dragging to the loaded bottom must load the next fifteen rows"
    );
    draw(cx);
    assert!(cx.debug_bounds("model-choice-row-m15").is_some());
    assert!(cx.debug_bounds("model-choice-row-m39").is_some());
    assert!(
        cx.debug_bounds("provider-model-more").is_none(),
        "no load-more affordance once every match is loaded"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let state = view.providers[0].model.clone();
            state.update(cx, |state, cx| state.set_value("m1", window, cx));
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("model-choice-row-m10").is_some()
            && cx.debug_bounds("model-choice-row-m11").is_some(),
        "matching models stay visible while filtering"
    );
    assert!(
        cx.debug_bounds("model-choice-row-m00").is_none(),
        "non-matching models are filtered out"
    );
    assert_eq!(list.offset().y, px(0.), "a new search starts at the top");
    assert_eq!(list.max_offset().y, px(0.), "ten matches fit without scrolling");
    assert!(cx.debug_bounds("provider-model-scrollbar").is_none());

    // Picking a row fills the field and closes the dropdown.
    let row = cx.debug_bounds("model-choice-row-m10").expect("row rendered");
    cx.simulate_click(row.center(), Default::default());
    draw(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.providers[0].model.read(cx).value().as_ref(), "m10");
            assert!(!view.model_choices.dropdown_open, "a picked row closes the dropdown");
        })
    });
    assert!(cx.debug_bounds("provider-model-dropdown").is_none());

    // Reopening with an exact catalog id in the field shows the catalog's
    // first rows again instead of filtering down to the selection.
    let caret = cx.debug_bounds("provider-model-caret").expect("caret rendered");
    cx.simulate_click(caret.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds("model-choice-row-m00").is_some(),
        "an exact selection reopens to the catalog's first rows"
    );
    assert_eq!(view.read_with(cx, |view, _| view.model_choices.window), 15);
    assert_eq!(list.offset().y, px(0.), "reopening starts on the first page");

    // A click anywhere outside the panel closes the dropdown.
    let card = cx.debug_bounds("provider-settings-card").expect("provider dialog rendered");
    cx.simulate_click(point(card.left() + px(100.), card.top() + px(20.)), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds("provider-model-dropdown").is_none(),
        "clicking outside the dropdown closes it"
    );

    // Switching the connection mode invalidates the fetch signature: the list
    // from the API-key fetch must not offer picks into a direct-mode provider.
    let caret = cx.debug_bounds("provider-model-caret").expect("caret rendered");
    cx.simulate_click(caret.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("provider-model-dropdown").is_some());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_provider_connection(ProviderConnection::CodexAccount, cx);
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("provider-model-dropdown").is_none(),
        "stale model choices must disappear when the connection changes"
    );
}

#[gpui::test]
fn provider_editor_cancel_restores_fields_for_every_dismissal(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::{ProviderConnection, ProviderWireApi, RuntimeSettings};
    let profile = Profile::new();
    let settings_path = profile.0.join("runtime.json");
    let mut settings = RuntimeSettings::default();
    settings.providers[0].api_key_environment_variable = "CF_CANCEL_TEST_UNUSED_KEY".into();
    settings.save(&settings_path).unwrap();
    let file_before = std::fs::read(&settings_path).unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.agents_selection = super::AgentsSelection::Provider;
            cx.notify();
        })
    });
    draw(cx);
    let original = cx.update(|_, cx| view.update(cx, |view, cx| view.provider_values(cx)));
    for dismissal in [
        "action-button-provider-editor-cancel-top",
        "action-button-provider-editor-cancel",
        "backdrop",
    ] {
        let open = cx.debug_bounds("action-button-open-provider-editor").unwrap();
        cx.simulate_click(open.center(), Default::default());
        draw(cx);
        let generation = view.read_with(cx, |view, _| view.model_choices.generation);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let fields = &mut view.providers[0];
                for (input, value) in [
                    (&fields.id, "cancelled-id"),
                    (&fields.name, "cancelled-name"),
                    (&fields.base_url, "https://cancelled.example/v1"),
                    (&fields.model, "cancelled-model"),
                    (&fields.api_key_environment_variable, "CANCELLED_KEY"),
                    (&fields.vision_base_url, "https://cancelled.example/vision"),
                    (&fields.vision_model, "cancelled-vision"),
                    (&fields.vision_api_key_environment_variable, "CANCELLED_VISION_KEY"),
                ] {
                    input.update(cx, |state, cx| state.set_value(value, window, cx));
                }
                fields.wire_api = ProviderWireApi::Chat;
                fields.native_vision = true;
                fields.enabled = false;
                view.set_provider_connection(ProviderConnection::CodexAccount, cx);
                view.model_catalogs.insert("cancelled-id".into(), vec![]);
                cx.notify();
            })
        });
        draw(cx);
        cx.update(|_, cx| view.update(cx, |view, cx| view.close_model_dropdown(cx)));
        draw(cx);
        let target = if dismissal == "backdrop" {
            let card = cx.debug_bounds("provider-settings-card").unwrap();
            point(card.left() - px(10.), card.top())
        } else {
            cx.debug_bounds(dismissal).unwrap().center()
        };
        cx.simulate_click(target, Default::default());
        draw(cx);
        assert!(cx.debug_bounds("provider-settings-card").is_none());
        assert_eq!(
            cx.update(|_, cx| view.update(cx, |view, cx| view.provider_values(cx))),
            original,
            "{dismissal} must restore the values used by cards and details"
        );
        assert!(
            view.read_with(cx, |view, _| view.model_choices.generation) > generation,
            "cancel must invalidate outstanding draft model fetches"
        );
        assert!(
            view.read_with(cx, |view, _| view.model_catalogs.is_empty()),
            "cancel must discard catalogs fetched for draft endpoints"
        );
        assert!(
            view.read_with(cx, |view, cx| view.providers[0].base_url.read(cx).is_editable()),
            "cancel must restore field editability after changing the connection mode"
        );
        assert_eq!(
            std::fs::read(&settings_path).unwrap(),
            file_before,
            "cancel must never write settings"
        );
    }
}

#[gpui::test]
fn provider_editor_save_commits_and_failed_save_can_be_cancelled(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::RuntimeSettings;
    let profile = Profile::new();
    let settings_path = profile.0.join("runtime.json");
    let mut settings = RuntimeSettings::default();
    settings.providers[0].api_key_environment_variable = "CF_CANCEL_TEST_UNUSED_KEY".into();
    settings.save(&settings_path).unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.agents_selection = super::AgentsSelection::Provider;
            cx.notify();
        })
    });
    draw(cx);
    let open = cx.debug_bounds("action-button-open-provider-editor").unwrap();
    cx.simulate_click(open.center(), Default::default());
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.providers[0]
                .name
                .update(cx, |state, cx| state.set_value("Saved name", window, cx));
            cx.notify();
        })
    });
    draw(cx);
    let save = cx.debug_bounds("action-button-provider-editor-save").unwrap();
    cx.simulate_click(save.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("provider-settings-card").is_none());
    assert!(view.read_with(cx, |view, _| view.provider_editor_snapshot.is_none()));
    assert_eq!(
        RuntimeSettings::load_or_default(&settings_path).unwrap().providers[0].name,
        "Saved name"
    );
    let saved_bytes = std::fs::read(&settings_path).unwrap();

    // A failed validation keeps the edited fields and the original snapshot.
    // Cancelling afterward restores the latest successful save, not startup values.
    let open = cx.debug_bounds("action-button-open-provider-editor").unwrap();
    cx.simulate_click(open.center(), Default::default());
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.providers[0].id.update(cx, |state, cx| state.set_value("", window, cx));
            view.providers[0]
                .name
                .update(cx, |state, cx| state.set_value("Failed draft", window, cx));
            cx.notify();
        })
    });
    draw(cx);
    let save = cx.debug_bounds("action-button-provider-editor-save").unwrap();
    cx.simulate_click(save.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("provider-settings-card").is_some());
    assert!(view.read_with(cx, |view, _| view.dialog_error.is_some()));
    assert_eq!(
        view.read_with(cx, |view, cx| view.providers[0].name.read(cx).value().to_string()),
        "Failed draft"
    );
    assert_eq!(std::fs::read(&settings_path).unwrap(), saved_bytes);
    let cancel = cx.debug_bounds("action-button-provider-editor-cancel-top").unwrap();
    cx.simulate_click(cancel.center(), Default::default());
    draw(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.provider_values(cx), view.saved_settings.providers);
            assert!(view.dialog_error.is_none());
        })
    });
    assert_eq!(std::fs::read(&settings_path).unwrap(), saved_bytes);
}
#[gpui::test]
fn provider_editor_model_field_click_opens_the_dropdown(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.provider_editor_open = true;
            cx.notify();
        })
    });
    draw(cx);
    let field = cx.debug_bounds("provider-model").expect("model field rendered");
    cx.simulate_click(field.center(), Default::default());
    draw(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(
                view.model_choices.dropdown_open,
                "clicking the model field opens the dropdown"
            );
        })
    });
    // The automatically started fetch signs itself with the current draft.
    assert!(cx.debug_bounds("provider-model-dropdown").is_some());
}

#[gpui::test]
fn quick_verification_sessions_are_isolated_per_runtime(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::execution::Cancellation;
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    // Two runtimes each hold an in-flight quick task with its own result.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for adapter in [RuntimeAdapter::CodexAppServer, RuntimeAdapter::KimiCode] {
                let task = view.quick_task_mut(adapter);
                task.cancel = Some(Cancellation::default());
                task.result = format!("result-{}", adapter.backend_id());
                task.identity = Some((None, None, adapter.backend_id().to_owned()));
            }
            cx.notify();
        })
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(view.quick_task_busy(RuntimeAdapter::CodexAppServer));
            assert!(view.quick_task_busy(RuntimeAdapter::KimiCode));
            assert!(!view.quick_task_busy(RuntimeAdapter::ClaudeCode));
            assert!(!view.quick_task_busy(RuntimeAdapter::Dsh));
            assert!(view.any_quick_task_running());
            assert_eq!(view.quick_task(RuntimeAdapter::KimiCode).result, "result-kimi-code");
        })
    });

    // Cancelling on the Codex page cancels only the Codex task: the other
    // runtime's run flag and result text stay untouched.
    draw(cx);
    let cancel =
        cx.debug_bounds("action-button-cancel-agent-task").expect("cancel button rendered");
    cx.simulate_click(cancel.center(), Default::default());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(
                view.quick_task(RuntimeAdapter::CodexAppServer)
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.0.load(Ordering::SeqCst))
            );
            assert!(
                !view
                    .quick_task(RuntimeAdapter::KimiCode)
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.0.load(Ordering::SeqCst)),
                "the other runtime's run is not cancelled"
            );
            assert_eq!(
                view.quick_task(RuntimeAdapter::CodexAppServer).result,
                "正在取消并清理进程…"
            );
            assert_eq!(
                view.quick_task(RuntimeAdapter::KimiCode).result,
                "result-kimi-code",
                "the other runtime's result is not overwritten"
            );
        })
    });

    // A Codex follow-up continuation blocks only the Codex slot.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.quick_task_mut(RuntimeAdapter::CodexAppServer).cancel = None;
            view.session_followup = Some(super::SessionFollowupRun {
                project_id: "p".into(),
                session_id: "s".into(),
                cancel: Cancellation::default(),
                stream: std::sync::Arc::new(std::sync::Mutex::new(
                    super::SessionFollowupStream::default(),
                )),
                displayed_revision: 0,
            });
            assert!(view.quick_task_busy(RuntimeAdapter::CodexAppServer));
            assert!(
                !view.quick_task_busy(RuntimeAdapter::ClaudeCode),
                "the follow-up blocks no other runtime's slot"
            );
        })
    });

    // Cross-runtime changes (project switch, grant edits) still cancel all.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.cancel_all_quick_tasks();
            assert!(
                view.quick_task(RuntimeAdapter::KimiCode)
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.0.load(Ordering::SeqCst))
            );
        })
    });
}

#[gpui::test]
fn provider_detail_badges_vision_from_the_model_catalog(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::models::ModelChoice;
    use circuitfabric_codex_runtime::{LlmProviderSettings, ProviderConnection, RuntimeSettings};
    let profile = Profile::new();
    {
        // A Codex account-direct provider alongside the default zai one.
        let mut settings = RuntimeSettings::default();
        let mut codex = LlmProviderSettings::default();
        codex.id = "codex".into();
        codex.connection = ProviderConnection::CodexAccount;
        codex.model = "gpt-6.1-sol".into();
        settings.providers.push(codex);
        settings.save(&profile.0.join("runtime.json")).unwrap();
    }
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.agents_selection = super::AgentsSelection::Provider;
            // The default zai provider (index 0) configures a separate vision
            // model, so it carries the badge from its own configuration.
            view.selected_provider = 0;
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("provider-vision-badge").is_some(),
        "an API-key provider with vision configured keeps its badge"
    );

    // The direct provider badges only once the fetched catalog says the
    // selected model supports vision.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selected_provider = 1;
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("provider-vision-badge").is_none(),
        "no catalog, no badge — the direct provider never guesses"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.model_catalogs.insert(
                "codex".into(),
                vec![
                    ModelChoice {
                        id: "gpt-6.1-sol".into(),
                        display_name: None,
                        is_default: true,
                        supports_vision: Some(true),
                    },
                    ModelChoice {
                        id: "gpt-6-text".into(),
                        display_name: None,
                        is_default: false,
                        supports_vision: Some(false),
                    },
                ],
            );
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("provider-vision-badge").is_some(),
        "the selected model's catalog vision support shows the badge"
    );

    // A catalog that reports no vision support for the selected model removes
    // the badge again.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let state = view.providers[1].model.clone();
            state.update(cx, |state, cx| state.set_value("gpt-6-text", window, cx));
            cx.notify();
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("provider-vision-badge").is_none(),
        "a text-only selected model shows no badge"
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
        gpui_component::Root::new(view, window, cx)
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

#[gpui::test]
fn datasheet_data_tables_keep_headers_on_one_line_and_align_columns(cx: &mut TestAppContext) {
    use circuitfabric_contracts::{
        DATASHEET_EXTRACTION_SCHEMA_VERSION, DatasheetExtraction, DatasheetOverview,
        DatasheetParameter, DatasheetPin, DatasheetPinKind,
    };
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Documents);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    let pin = |number: &str, name: &str, kind: DatasheetPinKind, description: &str| DatasheetPin {
        number: number.into(),
        name: name.into(),
        kind,
        description: description.into(),
        evidence: None,
    };
    let parameter = |name: &str, conditions: Option<&str>| DatasheetParameter {
        parameter: name.into(),
        symbol: Some("VIN".into()),
        min: Some("-0.3".into()),
        typ: None,
        max: Some("26".into()),
        unit: Some("V".into()),
        conditions: conditions.map(str::to_owned),
        evidence: None,
    };
    let extraction = std::sync::Arc::new(DatasheetExtraction {
        schema_version: DATASHEET_EXTRACTION_SCHEMA_VERSION,
        document_id: "doc-a".into(),
        content_hash: "sha256:fixture".into(),
        extracted_at_unix_seconds: 0,
        overview: DatasheetOverview {
            title: "Example step-down converter".into(),
            ..Default::default()
        },
        pins: vec![
            pin("1", "VIN", DatasheetPinKind::Power, "输入电源脚，就近接输入电容。"),
            pin(
                "AB12",
                "GPIO0_BOOT",
                DatasheetPinKind::InputOutput,
                "BGA 编号与较长名称也不能把表头挤换行。",
            ),
            pin(
                "3",
                "EN",
                DatasheetPinKind::Input,
                "使能脚，高电平有效；拉低进入关断模式，静态电流极低。",
            ),
        ],
        absolute_maximum_ratings: vec![parameter("Input voltage", None)],
        electrical_characteristics: vec![
            parameter("Quiescent current", Some("VIN = 12 V, no load")),
            parameter("Reference voltage", None),
        ],
        operating_conditions: vec![],
        notes: vec![],
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.document_preview = Some(super::DocumentPreviewSelection {
                project_id: "p".into(),
                document_id: "doc-a".into(),
                file_name: "current.pdf".into(),
                state: super::DocumentPreviewState::Unavailable { reason: "fixture".into() },
                extraction: Some(extraction),
            });
            view.preview_show_data = true;
            cx.notify();
        });
    });
    draw(cx);
    let header = cx.debug_bounds("datasheet-pin-header").expect("pin table header band");
    assert!(
        header.size.height <= px(40.),
        "pin header stays on one line, got {:?}",
        header.size.height
    );
    let description_header = cx.debug_bounds("pin-header-cell-description").unwrap();
    assert!(
        description_header.size.width > px(40.),
        "description header gets a real column width, got {:?}",
        description_header.size.width
    );
    for (header_selector, cell_selector, column) in [
        ("pin-header-cell-number", "pin-cell-number-0", "number"),
        ("pin-header-cell-name", "pin-cell-name-0", "name"),
        ("pin-header-cell-kind", "pin-cell-kind-0", "kind"),
    ] {
        let header_cell = cx.debug_bounds(header_selector).unwrap();
        let data_cell = cx.debug_bounds(cell_selector).unwrap();
        assert_eq!(header_cell.left(), data_cell.left(), "{column} column misaligned");
        assert_eq!(
            header_cell.size.width, data_cell.size.width,
            "{column} column width differs between header and row"
        );
    }
    let description_cell = cx.debug_bounds("pin-cell-description-0").unwrap();
    assert_eq!(description_header.left(), description_cell.left(), "description misaligned");
    let parameter_header =
        cx.debug_bounds("datasheet-parameter-header-electricalCharacteristics").unwrap();
    assert!(
        parameter_header.size.height <= px(40.),
        "parameter header stays on one line, got {:?}",
        parameter_header.size.height
    );
    assert!(
        cx.debug_bounds("parameter-header-cell-conditions").is_some(),
        "a conditions column appears when any row carries conditions"
    );
}

#[gpui::test]
fn semantics_timeline_marks_a_revert_to_an_earlier_state(cx: &mut TestAppContext) {
    use circuitfabric_contracts::{LogicalCircuitSnapshot, Project, SnapshotAuthority};
    use circuitfabric_project::{ProjectStorage, TimelineObservation, TimelineSource};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "timeline".into(), description: None },
    )
    .unwrap();
    let observed = |hash: &str| {
        let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        snapshot.snapshot_hash = format!("sha256:{hash}");
        snapshot
    };
    for (index, hash) in ["aa", "aa", "bb", "aa"].into_iter().enumerate() {
        let observation = TimelineObservation {
            source: TimelineSource::Inspect,
            observed_at_unix_millis: index as u64 * 1000,
            session_id: None,
            change_set_id: None,
        };
        store.record_observed_snapshot(&observed(hash), observation).unwrap();
    }
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Semantics);
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
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
    draw(cx);
    assert!(cx.debug_bounds("semantic-timeline-revisit-3").is_some(), "A -> B -> A is visible");
    assert!(cx.debug_bounds("semantic-timeline-revisit-2").is_none());
    assert!(cx.debug_bounds("semantic-timeline-revisit-4").is_none(), "unchanged inspect adds no event");
}


#[gpui::test]
fn time_zone_setting_defaults_to_system_and_persists_fixed_offsets(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::GlobalTimeZone;
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Settings);
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.time_zone),
        GlobalTimeZone::System,
        "the default display zone follows the operating system"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_time_zone(GlobalTimeZone::Offset(8 * 3600), cx);
        })
    });
    let saved = std::fs::read_to_string(profile.0.join("runtime.json")).unwrap();
    assert!(saved.contains("offset:+08:00"), "fixed offset persisted: {saved}");
    assert_eq!(
        view.read_with(cx, |view, _| view.time_zone),
        GlobalTimeZone::Offset(8 * 3600)
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_time_zone(GlobalTimeZone::System, cx);
        })
    });
    let saved = std::fs::read_to_string(profile.0.join("runtime.json")).unwrap();
    assert!(saved.contains("time_zone") && saved.contains("system"), "{saved}");
}
