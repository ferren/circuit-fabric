//! Message-center UI behaviour: source grouping, unread badge, deep links,
//! read state, and clearing — over a fixture project with a pending change
//! set, an observation record carrying diagnostics, and a bridge transition.
use super::*;

fn pending_change_set() -> circuitfabric_project::StoredChangeSet {
    circuitfabric_project::StoredChangeSet {
        schema_version: 1,
        id: "cs-pending".into(),
        base_snapshot_hash: "base".into(),
        target_snapshot_hash: "target".into(),
        plan_hash: "plan".into(),
        ir_diff: vec![],
        evidence: vec![],
        preview_validation: Default::default(),
        execution: Default::default(),
        observed_snapshot_hash: None,
        rollback_handle: None,
        audit: vec![],
    }
}

#[gpui::test]
fn message_center_groups_sources_badges_and_deep_links(cx: &mut TestAppContext) {
    use circuitfabric_contracts::Project;
    use circuitfabric_project::ProjectStorage;

    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "测试".into(), description: None },
    )
    .unwrap();
    store.save_change_set(&pending_change_set()).unwrap();
    let observations = jlcircuit_eda_bridge::observation::observations_dir(&root);
    std::fs::create_dir_all(observations.join("inspect-history")).unwrap();
    std::fs::write(
        observations.join("inspect-history").join("0001-msg.json"),
        serde_json::json!({
            "kind": "inspect",
            "snapshot": {"snapshot_hash": "sha256:abc123def456"},
            "diagnostics": [
                {"face": "components", "code": "missing_designator", "count": 2, "message": "2 个元件缺少位号"}
            ],
            "inconclusive": [],
            "validation": [],
        })
        .to_string(),
    )
    .unwrap();

    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
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
            view.message_center.scan_now(&view.project_storages, &view.project_data);
            view.note_bridge_event(crate::application::messages::BridgeEvent::Running {
                pid: 4242,
            });
            cx.notify();
        })
    });
    draw(cx);

    // The bell entry is always visible; the badge counts the three unread
    // rows (pending approval, observation diagnostics, bridge event).
    assert!(cx.debug_bounds("message-center-trigger").is_some(), "bell entry renders");
    assert!(cx.debug_bounds("message-center-unread").is_some(), "unread badge renders");

    let trigger = cx.debug_bounds("message-center-trigger").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("message-panel").is_some(), "panel opens from the bell");
    for group in ["ChangesAndApprovals", "CircuitSemantics", "EdaServices"] {
        let selector: &'static str =
            Box::leak(format!("message-group-{group}").into_boxed_str());
        assert!(cx.debug_bounds(selector).is_some(), "{group} group renders");
    }
    let pending_row = cx
        .debug_bounds("message-entry-pending_p_cs-pending")
        .expect("pending approval entry renders");
    let observation_row = cx
        .debug_bounds("message-entry-observation_p_inspect-history_0001-msg_json")
        .expect("observation diagnostics entry renders");

    // Deep link: the pending entry lands on the changes page with the change
    // set preselected, and clicking marks it read.
    cx.simulate_click(pending_row.center(), Default::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.navigation.screen, ControlPlaneScreen::ChangesAndApprovals);
        assert_eq!(
            view.selected_change_set,
            Some(("p".to_owned(), "cs-pending".to_owned()))
        );
        assert!(!view.message_panel_open, "the panel closes after a deep link");
        assert_eq!(view.message_center.unread_count(), 2, "the clicked row is read");
    });

    // The changes page still offers the review surface for that record.
    assert!(
        cx.debug_bounds("action-button-open-approval-drawer").is_some(),
        "review stays on the changes page"
    );
    for stage in ["write", "readback", "verification"] {
        let selector: &'static str = Box::leak(format!("changes-stage-{stage}").into_boxed_str());
        assert!(cx.debug_bounds(selector).is_some(), "the {stage} stage stays visible");
    }

    // Reopen and follow the observation entry into the semantics page.
    let trigger = cx.debug_bounds("message-center-trigger").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    cx.simulate_click(observation_row.center(), Default::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.navigation.screen, ControlPlaneScreen::Semantics);
        assert_eq!(
            view.selected_semantic_snapshot,
            Some(("p".to_owned(), "sha256:abc123def456".to_owned()))
        );
    });

    // Mark all read empties the badge; clear empties the list but keeps the
    // live pending entry (which is now read) — history is not replayed.
    let trigger = cx.debug_bounds("message-center-trigger").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    let mark_all = cx.debug_bounds("action-button-messages-mark-all-read").unwrap();
    cx.simulate_click(mark_all.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("message-center-unread").is_none(), "badge clears on mark-all-read");
    let clear = cx.debug_bounds("action-button-messages-clear").unwrap();
    cx.simulate_click(clear.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds("message-entry-observation_p_inspect-history_0001-msg_json").is_none(),
        "clear removes persisted events"
    );
    assert!(
        cx.debug_bounds("message-entry-pending_p_cs-pending").is_some(),
        "the live pending entry stays while the change set is still pending"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.message_center.unread_count(), 0, "the live entry stayed read");
    });
}

#[gpui::test]
fn message_panel_closes_from_the_backdrop_and_escape(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.message_panel_open = true;
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("message-panel").is_some());

    // The transparent backdrop closes the panel on an outside click.
    let backdrop = cx.debug_bounds("message-panel-backdrop").unwrap();
    cx.simulate_click(backdrop.center(), Default::default());
    draw(cx);
    view.read_with(cx, |view, _| assert!(!view.message_panel_open));

    // Escape closes it too, through the same keystroke path as the palette.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.message_panel_open = true;
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("message-panel").is_some());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_command_keystroke(
                &gpui::KeystrokeEvent {
                    keystroke: gpui::Keystroke::parse("escape").unwrap(),
                    input_preference: gpui::InputPreference::KeyBindings,
                    action: None,
                    context_stack: Vec::new(),
                },
                window,
                cx,
            );
        })
    });
    draw(cx);
    view.read_with(cx, |view, _| assert!(!view.message_panel_open, "escape closes the panel"));
}
