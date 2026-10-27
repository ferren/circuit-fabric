//! End-to-end check of the supervised bridge service: launch the real binary,
//! wait for it to listen, run the WebSocket `status` round-trip, verify that a
//! port conflict exits with readable diagnostics, and stop it cleanly.

mod common;

use std::{
    path::Path,
    time::{Duration, Instant},
};

use jlcircuit_eda_bridge::supervision::BridgeProcessHandle;

use common::{RunningBridge, WsClient, write_registry};

fn wait_until_exit(handle: &mut BridgeProcessHandle, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(exit) = handle.try_exit() {
            let diagnostics = handle.exit_diagnostics();
            assert!(!exit.success(), "a conflicting bridge must exit with an error");
            return diagnostics;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the conflicting bridge did not exit within {timeout:?}");
}

#[test]
fn supervised_bridge_reports_status_and_stops_cleanly() {
    let bridge = RunningBridge::spawn();
    let address = bridge.address.clone();
    let config_path = bridge.config_path.clone();
    let dir = bridge.dir.clone();

    let report = jlcircuit_eda_bridge::probe::status(&address, Duration::from_secs(3))
        .expect("the status round-trip over WebSocket succeeds");
    assert_eq!(report.protocol_version, 1);
    assert_eq!(report.bridge_name, "CircuitFabric");
    // 能力与实际实现一致，包括受审批约束的写入、回读和回滚。
    let mut capabilities = report.capabilities.clone();
    capabilities.sort();
    assert_eq!(
        capabilities,
        vec![
            "apply",
            "bridge-chat",
            "bridge-context",
            "drc",
            "inspect",
            "preview",
            "readback",
            "rollback",
            "visual-capture"
        ]
    );
    for dishonest in ["apply", "readback", "rollback"] {
        assert!(
            report.capabilities.iter().any(|capability| capability == dishonest),
            "capabilities must report `{dishonest}`"
        );
    }
    assert!(
        report.projects.is_empty(),
        "no projects are registered in the temporary settings directory"
    );

    verify_project_selection(&address, &dir);

    // The extension's connect path: hello with an unregistered project must be
    // rejected WITH an explanatory error reply — never a silent socket drop.
    let rejection =
        jlcircuit_eda_bridge::probe::hello(&address, "default-project", Duration::from_secs(3))
            .expect_err("an unregistered project must be rejected");
    assert!(
        rejection.contains("未注册"),
        "the rejection should name the problem, got: {rejection}"
    );

    // Registering the project (the bridge re-reads the registry per message)
    // flips both the status report and hello to success.
    write_registry(&dir, "power-supply");
    let status_after = jlcircuit_eda_bridge::probe::status(&address, Duration::from_secs(3))
        .expect("status after registration");
    assert!(
        status_after.projects.iter().any(|project| project == "power-supply"),
        "status must report the registered project, got {:?}",
        status_after.projects
    );
    jlcircuit_eda_bridge::probe::hello(&address, "power-supply", Duration::from_secs(3))
        .expect("hello succeeds for a registered project");

    // A second bridge on the same port must fail loudly: it exits on its own
    // and its stderr explains the bind failure.
    let mut conflicting = BridgeProcessHandle::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge")),
        &config_path,
    )
    .expect("the conflicting process still spawns");
    let diagnostics = wait_until_exit(&mut conflicting, Duration::from_secs(10));
    assert!(
        diagnostics.contains("could not listen"),
        "diagnostics should name the bind failure, got: {diagnostics}"
    );

    let address = bridge.address.clone();
    let mut bridge = bridge;
    bridge.stop().expect("the supervised bridge stops on request");
    drop(bridge);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        jlcircuit_eda_bridge::probe::tcp_reachable(&address, Duration::from_millis(500)).is_err(),
        "the port must stop accepting after stop()"
    );
}

fn verify_project_selection(address: &str, dir: &Path) {
    use serde_json::json;

    let mut client = WsClient::connect(address, "/bridge");
    assert_eq!(client.exchange(&json!({"type":"hello"}))["type"], "hello_ack");
    assert_eq!(client.exchange(&json!({"type":"list_projects"}))["projects"], json!([]));
    assert_eq!(client.exchange(&json!({"type":"chat"}))["type"], "error");
    write_registry(dir, "first-project");
    assert_eq!(
        client.exchange(&json!({"type":"list_projects"}))["projects"],
        json!([{"id":"first-project","name":"Power Supply"}])
    );
    assert_eq!(
        client.exchange(&json!({"type":"select_project","projectId":"first-project"}))["projectId"],
        "first-project"
    );
    write_registry(dir, "second-project");
    assert_eq!(
        client.exchange(&json!({"type":"select_project","projectId":"second-project"}))["projectId"],
        "second-project"
    );
    let rejection = client.exchange(&json!({"type":"select_project","projectId":"missing"}));
    assert_eq!(rejection["type"], "error");
    assert!(rejection["message"].as_str().unwrap().contains("second-project"));
    assert_eq!(client.exchange(&json!({"type":"chat"}))["type"], "error");
    assert_eq!(
        client.exchange(&json!({"type":"select_project","projectId":"second-project"}))["type"],
        "project_selected"
    );
    // Histories are served from the selected project and only the EDA category.
    let project_root = dir.join("project-root");
    std::fs::create_dir_all(&project_root).unwrap();
    let storage = circuitfabric_project::ProjectStorage::create(
        &project_root,
        circuitfabric_contracts::Project {
            id: "second-project".into(),
            name: "Second".into(),
            description: None,
        },
    )
    .unwrap();
    for (id, category) in [
        ("eda-history", circuitfabric_project::SessionCategory::Eda),
        ("datasheet-history", circuitfabric_project::SessionCategory::Datasheet),
    ] {
        storage
            .start_categorized_session(
                circuitfabric_project::SessionSeed {
                    session_id: id.into(),
                    runtime_profile_id: "p".into(),
                    backend_id: Some("codex".into()),
                },
                category,
                None,
            )
            .unwrap();
    }
    // The listed title comes from the first user turn of the session body,
    // whitespace collapsed; the newline routes it through the multi-line form.
    storage
        .append_session_event(
            "eda-history",
            &circuitfabric_project::SessionEvent {
                timestamp_unix_seconds: 1_700_000_000,
                kind: circuitfabric_project::SessionEventKind::Turn {
                    actor: circuitfabric_project::SessionActor::User,
                    message: "检查电源  输入\n风险".into(),
                },
            },
        )
        .unwrap();
    let sessions = client.exchange(&json!({"type":"list_sessions"}));
    assert_eq!(sessions["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(sessions["sessions"][0]["sessionId"], "eda-history");
    assert_eq!(sessions["sessions"][0]["title"], "检查电源 输入 风险");
    assert_eq!(
        client.exchange(&json!({"type":"load_session","sessionId":"eda-history"}))["type"],
        "session_loaded"
    );
    let rejected = client.exchange(&json!({"type":"load_session","sessionId":"datasheet-history"}));
    assert_eq!(rejected["type"], "error");
    assert!(rejected["message"].as_str().unwrap().contains("EDA"));
    assert_eq!(
        client.exchange(&json!({"type":"load_session","sessionId":"../../outside"}))["type"],
        "error"
    );
}
