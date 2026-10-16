//! End-to-end modal continuation against a local, non-networked App Server fixture.
use super::*;

#[gpui::test]
fn restores_native_context_and_completion_keeps_modal_closed(cx: &mut TestAppContext) {
    use circuitfabric_codex_runtime::execution::{Cancellation, run_session_observed};
    use circuitfabric_project::{ProjectStorage, SessionCategory};
    let profile = Profile::new();
    let launcher = profile.0.join("mock.cmd");
    std::fs::write(&launcher, "@powershell -NoProfile -NonInteractive -File \"%~dp0mock.ps1\"\r\n")
        .unwrap();
    std::fs::write(profile.0.join("mock.ps1"), r#"
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$out = [Console]::Out
$saved = Join-Path $env:CODEX_HOME 'mock-first.txt'
$trace = Join-Path $env:CODEX_HOME 'trace.txt'
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $m = $line | ConvertFrom-Json
  Add-Content -LiteralPath $trace -Value $m.method
  if ($m.method -eq 'initialize') { $out.WriteLine('{"id":' + $m.id + ',"result":{}}') }
  elseif ($m.method -eq 'thread/start') {
    $out.WriteLine('{"id":' + $m.id + ',"result":{"thread":{"id":"original-thread"}}}')
  }
  elseif ($m.method -eq 'thread/resume') {
    if (!(Test-Path -LiteralPath $saved) -or $m.params.threadId -ne 'original-thread') { throw 'original context missing' }
    $out.WriteLine('{"id":' + $m.id + ',"result":{"thread":{"id":"original-thread"}}}')
  }
  elseif ($m.method -eq 'turn/start') {
    [System.IO.File]::WriteAllText((Join-Path $env:CODEX_HOME 'mock-last-prompt.txt'), $m.params.input[0].text, [System.Text.Encoding]::UTF8)
    if (!(Test-Path -LiteralPath $saved)) { Set-Content -LiteralPath $saved -Value $m.params.input[0].text }
    $first = Get-Content -LiteralPath $saved -Raw
    $out.WriteLine('{"id":' + $m.id + ',"result":{"turn":{"id":"turn"}}}')
    $reply = 'Restored: ' + $first + ' / Next: ' + $m.params.input[0].text
    $proposal = Join-Path $env:CODEX_HOME 'mock-proposed-reply.txt'
    if ($m.params.input[0].text -match 'remove row using conversation|Generate or update the document overview:') { $reply = [System.IO.File]::ReadAllText($proposal, [System.Text.Encoding]::UTF8) }
    $out.WriteLine((@{method='item/agentMessage/delta';params=@{delta=$reply}} | ConvertTo-Json -Compress -Depth 6))
    $out.WriteLine('{"method":"item/reasoning/summaryTextDelta","params":{"delta":"**Check document**\n\nRuntime summary retained."}}')
    $out.WriteLine('{"method":"turn/completed","params":{"threadId":"original-thread","turn":{"id":"turn","status":"completed"}}}')
  }
  $out.Flush()
}
"#).unwrap();
    let mut settings = circuitfabric_codex_runtime::RuntimeSettings::default();
    settings.codex.command = launcher.to_string_lossy().into_owned();
    // Jev is authorized for the extraction controller, while its model thread
    // uses no tools. This distinction must survive the history modal entry point.
    let jev_id = circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID;
    settings.tools.authorized_mcp_server_ids = vec![jev_id.into()];
    settings.catalog.mcp_servers = vec![circuitfabric_codex_runtime::tools::McpServerDefinition {
        id: jev_id.into(),
        display_name: "Jev".into(),
        command: "fixture-jev-must-not-be-launched-by-model".into(),
        args: vec![],
        environment_variables: vec![],
        enabled: true,
    }];
    settings.save(&profile.0.join("runtime.json")).unwrap();
    let mut vault =
        UnlockedVault::create(&profile.0.join("vault.bin"), "mock-followup-passphrase").unwrap();
    vault.set(&settings.providers[0].api_key_environment_variable, "mock-test-key").unwrap();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project { id: "p".into(), name: "p".into(), description: None },
    )
    .unwrap();
    let mut configuration = store.load_configuration().unwrap();
    configuration.enabled_mcp_server_ids = vec![jev_id.into()];
    store.save_configuration(&configuration).unwrap();
    let source = profile.0.join("chip.pdf");
    std::fs::write(&source, crate::application::datasheet_dialogue::tests::minimal_pdf()).unwrap();
    let document = store
        .import_document(&source, circuitfabric_project::DocumentCategory::Datasheet, "fixture")
        .unwrap()
        .document;
    let mut data = crate::application::datasheet_dialogue::DialogueDocument::prepare(
        &store,
        "p",
        &document.id,
    )
    .unwrap()
    .base;
    data.pins.push(serde_json::from_value(serde_json::json!({"number":"1","name":"VDD","kind":"power","description":"Supply","evidence":"1 VDD Supply"})).unwrap());
    store.save_datasheet_extraction(&data).unwrap();
    let id = crate::application::runtime_session::start_for(
        &store,
        &settings.providers[0].id,
        "codex",
        SessionCategory::Datasheet,
        Some(document.id.clone()),
    )
    .unwrap();
    let history = store.session_runtime_directory(&id).unwrap();
    let initial = run_session_observed(
        &settings,
        &Default::default(),
        "remember document facts",
        None,
        store.root(),
        &history,
        Some(vault.values()),
        &Cancellation::default(),
        &mut |_, _| {},
        &mut |_| {},
    )
    .unwrap();
    crate::application::runtime_session::finish(
        &store,
        &id,
        "remember document facts",
        Ok(&initial),
        None,
    )
    .unwrap();
    // The first server process has exited. The UI must resume its native thread.
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
            view.vault = Some(vault);
            assert_eq!(view.effective_grants_for(&"p".into()).authorized_mcp_server_ids, [jev_id]);
            view.navigation.selected_project = Some("another-project".into());
            view.open_session_replay("p".into(), id.clone(), cx);
        })
    });
    draw(cx);
    let input = view
        .read_with(cx, |v, _| v.session_replay.as_ref().unwrap().followup_prompt.clone().unwrap());
    cx.update(|window, cx| {
        input.update(cx, |p, cx| p.set_value("followup supplement", window, cx));
        view.update(cx, |view, cx| {
            view.send_session_followup(window, cx);
            assert!(view.session_followup.is_some());
            assert!(store.load_session(&id).unwrap().body.contains("followup supplement"));
            view.close_session_replay(cx);
        });
        input.update(cx, |p, cx| p.set_value("next unsent draft", window, cx));
    });
    draw(cx);
    view.read_with(cx, |v, cx| {
        assert!(v.session_followup.is_none());
        assert!(!v.session_modal_open, "completion must not reopen a dismissed modal");
        assert_eq!(input.read(cx).value().as_str(), "next unsent draft");
    });
    let replay = store.load_session(&id).unwrap();
    assert_eq!(replay.metadata.status, circuitfabric_project::SessionStatus::Completed);
    assert_eq!(replay.metadata.subject_id.as_deref(), Some(document.id.as_str()));
    assert!(replay.body.contains("Restored: ") && replay.body.contains("remember document facts"));
    let reopened = ProjectStorage::open(&root).unwrap().load_session(&id).unwrap();
    assert!(reopened.body.contains("· 思考摘要 · 消息"));
    assert!(super::super::session_markdown::conversation_rows(&reopened.body).iter().any(|r| matches!(r,
        super::super::session_markdown::ConversationRow::Reasoning { text, .. } if text.contains("Runtime summary retained."))));
    assert!(replay.body.contains("Next: ") && replay.body.contains("followup supplement"));
    assert!(!replay.body.contains("next unsent draft"));
    assert_eq!(store.list_sessions().unwrap().sessions.len(), 1);
    let trace = std::fs::read_to_string(history.join("trace.txt")).unwrap();
    assert_eq!(trace.matches("thread/start").count(), 1);
    assert_eq!(trace.matches("thread/resume").count(), 1);

    // The model requests one deletion; the app commits it and refreshes the data column.
    let reply = format!(
        "删除建议。\n```circuitfabric-datasheet-edit\n{}\n```",
        serde_json::json!({"documentId":document.id,"contentHash":document.content_hash,"operations":[{"op":"remove","table":"pins","index":0}]})
    );
    std::fs::write(history.join("mock-proposed-reply.txt"), reply).unwrap();
    cx.update(|_, cx| view.update(cx, |v, cx| v.open_session_replay("p".into(), id.clone(), cx)));
    draw(cx);
    let input = view
        .read_with(cx, |v, _| v.session_replay.as_ref().unwrap().followup_prompt.clone().unwrap());
    assert!(cx.debug_bounds("session-datasheet-panel").is_some());
    cx.update(|window, cx| {
        input.update(cx, |p, cx| p.set_value("remove row using conversation", window, cx));
    });
    let input_bounds = cx.debug_bounds("session-followup-input").unwrap();
    cx.simulate_click(input_bounds.center(), Default::default());
    cx.simulate_keystrokes("shift-enter");
    assert!(view.read_with(cx, |v, _| v.session_followup.is_none()), "Shift+Enter is a newline");
    assert!(view.read_with(cx, |_, cx| input.read(cx).value().contains('\n')));
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(
        store.load_datasheet_extraction(&document.id).unwrap().unwrap().pins.is_empty(),
        "{:?} / {}",
        view.read_with(cx, |v, _| v.session_followup_feedback.clone()),
        store
            .load_session(&id)
            .unwrap()
            .body
            .chars()
            .rev()
            .take(1800)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    );
    view.read_with(cx, |v, _| {
        let selection = v.session_replay.as_ref().unwrap();
        assert!(selection.datasheet.as_ref().unwrap().pins.is_empty());
        assert!(selection.replay.body.contains("circuitfabric-datasheet-edit"));
        assert!(!selection.conversation.iter().any(|row| matches!(row,
            super::super::session_markdown::ConversationRow::Turn { text, .. } if text.contains("```circuitfabric-datasheet-edit"))));
        assert!(v.session_followup_feedback.as_ref().unwrap().2.contains("修改已保存"));
    });
    let raw_tab = cx.debug_bounds("session-tab-raw").unwrap();
    cx.simulate_click(raw_tab.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("session-followup-input").is_none());
    assert!(cx.debug_bounds("session-datasheet-panel").is_none());
    assert!(cx.debug_bounds("session-replay-body").is_some());

    // The overview action uses the current display language and keeps a draft.
    let overview = circuitfabric_contracts::DatasheetOverview {
        title: "Supply controller".into(),
        description: "A device that controls the supply. ".repeat(30),
        features: vec!["Supply control".into()],
        ..Default::default()
    };
    let reply = format!(
        "Overview generated.\n```circuitfabric-datasheet-edit\n{}\n```",
        serde_json::json!({
        "documentId":document.id,"contentHash":document.content_hash,"operations":[],"overview":overview})
    );
    std::fs::write(history.join("mock-proposed-reply.txt"), reply).unwrap();
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.language = UiLanguage::English;
            v.open_session_replay("p".into(), id.clone(), cx);
        })
    });
    draw(cx);
    let input = view
        .read_with(cx, |v, _| v.session_replay.as_ref().unwrap().followup_prompt.clone().unwrap());
    cx.update(|window, cx| input.update(cx, |p, cx| p.set_value("keep this draft", window, cx)));
    let button = cx.debug_bounds("action-button-session-generate-overview").unwrap();
    cx.simulate_click(button.center(), Default::default());
    draw(cx);
    assert_eq!(input.read_with(cx, |p, _| p.value().to_string()), "keep this draft");
    let saved = store.load_datasheet_extraction(&document.id).unwrap().unwrap();
    assert_eq!(
        saved.overview,
        overview,
        "{:?} / {}",
        view.read_with(cx, |v, _| v.session_followup_feedback.clone()),
        store
            .load_session(&id)
            .unwrap()
            .body
            .chars()
            .rev()
            .take(2400)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    );
    assert!(saved.pins.is_empty());
    let model_prompt = std::fs::read_to_string(history.join("mock-last-prompt.txt")).unwrap();
    assert!(model_prompt.contains("in English, the current application display language"));
    assert!(cx.debug_bounds("session-document-overview").unwrap().size.height <= px(180.));
    view.read_with(cx, |v, _| {
        assert!(v.session_followup.is_none());
        assert!(v.session_followup_feedback.as_ref().unwrap().2.contains("未调用 Jev"));
        assert_eq!(
            v.session_replay.as_ref().unwrap().datasheet.as_ref().unwrap().overview,
            overview
        );
    });

    // A changed model must not send a new turn or lose the submitted message.
    settings.providers[0].model = "changed-model".into();
    settings.save(&profile.0.join("runtime.json")).unwrap();
    cx.update(|_, cx| view.update(cx, |v, cx| v.open_session_replay("p".into(), id.clone(), cx)));
    draw(cx);
    let input = view
        .read_with(cx, |v, _| v.session_replay.as_ref().unwrap().followup_prompt.clone().unwrap());
    cx.update(|window, cx| {
        input.update(cx, |p, cx| p.set_value("retry this message", window, cx));
        view.update(cx, |v, cx| v.send_session_followup(window, cx));
    });
    draw(cx);
    view.read_with(cx, |v, cx| {
        assert!(v.session_followup.is_none());
        assert_eq!(input.read(cx).value().as_str(), "retry this message");
        assert!(
            v.session_followup_feedback.as_ref().unwrap().2.contains("Provider 或工具授权已变化")
        );
    });
    assert_eq!(store.load_session(&id).unwrap().body.matches("retry this message").count(), 1);
    let trace = std::fs::read_to_string(history.join("trace.txt")).unwrap();
    assert_eq!(trace.matches("thread/start").count(), 1);
    assert_eq!(trace.matches("turn/start").count(), 4);
}

#[gpui::test]
fn live_replies_follow_the_tail_pause_when_reading_and_raw_tab_is_read_only(
    cx: &mut TestAppContext,
) {
    use circuitfabric_project::{ProjectStorage, SessionCategory};
    use std::sync::{Arc, Mutex};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project { id: "p".into(), name: "p".into(), description: None },
    )
    .unwrap();
    let settings = circuitfabric_codex_runtime::RuntimeSettings::default();
    let id = crate::application::runtime_session::start_for(
        &store,
        &settings.providers[0].id,
        "codex",
        SessionCategory::Runtime,
        None,
    )
    .unwrap();
    crate::application::runtime_session::finish(
        &store,
        &id,
        "question",
        Ok(&"History\n\n".repeat(300)),
        None,
    )
    .unwrap();
    let native = store.session_runtime_directory(&id).unwrap();
    std::fs::create_dir_all(&native).unwrap();
    std::fs::write(native.join("circuitfabric-thread.json"), "{}").unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::SessionsAndTasks);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    let stream = Arc::new(Mutex::new(super::super::SessionFollowupStream {
        answer: "First live segment".into(),
        reasoning: "Runtime reasoning summary\n\n".repeat(80),
        revision: 1,
        ..Default::default()
    }));
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            crate::application::project_data::attach_project_storage(
                &mut v.workspace,
                &mut v.project_storages,
                &mut v.project_data,
                store,
            )
            .unwrap();
            v.open_session_replay("p".into(), id.clone(), cx);
            v.session_followup = Some(super::super::SessionFollowupRun {
                project_id: "p".into(),
                session_id: id,
                cancel: Default::default(),
                stream: stream.clone(),
                displayed_revision: 0,
            });
            v.session_replay
                .as_ref()
                .unwrap()
                .conversation_list
                .set_follow_mode(gpui::FollowMode::Tail);
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("session-jump-to-latest").is_none());
    let reasoning = cx.debug_bounds("session-reasoning-panel").unwrap();
    assert!(reasoning.size.height <= px(112.));
    let list =
        view.read_with(cx, |v, _| v.session_replay.as_ref().unwrap().conversation_list.clone());
    let outer_before = list.logical_scroll_top();
    cx.simulate_event(ScrollWheelEvent {
        position: reasoning.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-90.))),
        ..Default::default()
    });
    draw(cx);
    assert_eq!(
        (list.logical_scroll_top().item_ix, list.logical_scroll_top().offset_in_item),
        (outer_before.item_ix, outer_before.offset_in_item),
        "reasoning scrolls independently of the conversation"
    );
    assert!(list.is_following_tail());
    assert!(view.read_with(cx,|v,_|v.session_replay.as_ref().unwrap().conversation.iter().any(|r|matches!(r,
        super::super::session_markdown::ConversationRow::Turn{text,..} if text.contains("First live segment")))));
    {
        let mut s = stream.lock().unwrap();
        s.answer.push_str(&"\n\nStreaming next paragraph".repeat(200));
        s.revision += 1;
    }
    draw(cx);
    assert!(list.is_following_tail());
    let area = cx.debug_bounds("session-conversation-body").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: area.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(600.))),
        ..Default::default()
    });
    draw(cx);
    assert!(!list.is_following_tail(), "reading older messages pauses following");
    let jump = cx.debug_bounds("session-jump-to-latest").unwrap();
    assert!((jump.center().x - area.center().x).abs() < px(2.));
    let before = list.logical_scroll_top();
    {
        let mut s = stream.lock().unwrap();
        s.answer.push_str(&"\n\nLater streaming text".repeat(100));
        s.revision += 1;
    }
    draw(cx);
    assert!(!list.is_following_tail());
    assert_eq!(list.logical_scroll_top().item_ix, before.item_ix);
    assert!(cx.debug_bounds("session-jump-to-latest").is_some());
    let jump = cx.debug_bounds("session-jump-to-latest").unwrap();
    cx.simulate_click(jump.center(), Default::default());
    draw(cx);
    assert!(list.is_following_tail(), "returning to the end re-engages follow mode");
    assert!(cx.debug_bounds("session-jump-to-latest").is_none());
    let raw = cx.debug_bounds("session-tab-raw").unwrap();
    cx.simulate_click(raw.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds("session-followup-input").is_none());
    assert!(cx.debug_bounds("action-button-send-session-followup").is_none());
}
