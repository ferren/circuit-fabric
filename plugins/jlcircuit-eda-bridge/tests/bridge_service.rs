//! End-to-end check of the supervised bridge service: launch the real binary,
//! wait for it to listen, run the WebSocket `status` round-trip, verify that a
//! port conflict exits with readable diagnostics, and stop it cleanly.

use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use circuitfabric_codex_runtime::RuntimeSettings;
use jlcircuit_eda_bridge::{probe, supervision::BridgeProcessHandle};

fn temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir()
        .join(format!("circuitfabric-bridge-test-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temporary settings directory");
    dir
}

fn free_loopback_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral loopback port");
    let address = listener.local_addr().expect("read the ephemeral address").to_string();
    drop(listener);
    address
}

fn write_settings(dir: &Path, address: &str) -> PathBuf {
    let mut settings = RuntimeSettings::default();
    settings.bridge.listen_address = address.to_owned();
    let path = dir.join("runtime.json");
    settings.save(&path).expect("default settings with a loopback address are valid");
    path
}

fn write_registry(dir: &Path, project_id: &str) {
    let document = serde_json::json!({
        "schemaVersion": 1,
        "projects": [{
            "projectId": project_id,
            "canonicalRootPath": dir.join("project-root").display().to_string(),
            "displayName": "Power Supply",
            "lastOpenedUnixSeconds": 0,
        }],
    });
    std::fs::write(dir.join("projects.json"), document.to_string())
        .expect("write the project registry");
}

fn wait_until_listening(address: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if probe::tcp_reachable(address, Duration::from_millis(300)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the bridge did not start listening on {address} within {timeout:?}");
}

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
    let dir = temp_dir();
    let address = free_loopback_address();
    let config_path = write_settings(&dir, &address);

    let mut bridge = BridgeProcessHandle::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge")),
        &config_path,
    )
    .expect("the bridge binary launches");
    wait_until_listening(&address, Duration::from_secs(10));

    let report = probe::status(&address, Duration::from_secs(3))
        .expect("the status round-trip over WebSocket succeeds");
    assert_eq!(report.protocol_version, 1);
    assert_eq!(report.bridge_name, "CircuitFabric");
    for expected in ["inspect", "apply", "readback", "drc", "visual-capture", "bridge-chat"] {
        assert!(
            report.capabilities.iter().any(|capability| capability == expected),
            "capabilities must report `{expected}`, got {:?}",
            report.capabilities
        );
    }
    assert!(
        report.projects.is_empty(),
        "no projects are registered in the temporary settings directory"
    );

    verify_project_selection(&address, &dir);

    // The extension's connect path: hello with an unregistered project must be
    // rejected WITH an explanatory error reply — never a silent socket drop.
    let rejection = probe::hello(&address, "default-project", Duration::from_secs(3))
        .expect_err("an unregistered project must be rejected");
    assert!(
        rejection.contains("未注册"),
        "the rejection should name the problem, got: {rejection}"
    );

    // Registering the project (the bridge re-reads the registry per message)
    // flips both the status report and hello to success.
    write_registry(&dir, "power-supply");
    let status_after =
        probe::status(&address, Duration::from_secs(3)).expect("status after registration");
    assert!(
        status_after.projects.iter().any(|project| project == "power-supply"),
        "status must report the registered project, got {:?}",
        status_after.projects
    );
    probe::hello(&address, "power-supply", Duration::from_secs(3))
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

    bridge.stop().expect("the supervised bridge stops on request");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        probe::tcp_reachable(&address, Duration::from_millis(500)).is_err(),
        "the port must stop accepting after stop()"
    );

    std::fs::remove_dir_all(&dir).ok();
}

fn verify_project_selection(address: &str, dir: &Path) {
    use serde_json::json;
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    stream.write_all(b"GET /bridge HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
    }
    assert!(headers.starts_with(b"HTTP/1.1 101"));
    let mut exchange = |message: serde_json::Value| {
        let payload = message.to_string();
        assert!(payload.len() < 126);
        let length = u8::try_from(payload.len()).unwrap();
        stream.write_all(&[0x81, 0x80 | length, 0, 0, 0, 0]).unwrap();
        stream.write_all(payload.as_bytes()).unwrap();
        let mut header = [0; 2];
        stream.read_exact(&mut header).unwrap();
        let length = if header[1] == 126 {
            let mut bytes = [0; 2];
            stream.read_exact(&mut bytes).unwrap();
            usize::from(u16::from_be_bytes(bytes))
        } else {
            usize::from(header[1])
        };
        let mut reply = vec![0; length];
        stream.read_exact(&mut reply).unwrap();
        serde_json::from_slice::<serde_json::Value>(&reply).unwrap()
    };
    assert_eq!(exchange(json!({"type":"hello"}))["type"], "hello_ack");
    assert_eq!(exchange(json!({"type":"list_projects"}))["projects"], json!([]));
    assert_eq!(exchange(json!({"type":"chat"}))["type"], "error");
    write_registry(dir, "first-project");
    assert_eq!(
        exchange(json!({"type":"list_projects"}))["projects"],
        json!([{"id":"first-project","name":"Power Supply"}])
    );
    assert_eq!(
        exchange(json!({"type":"select_project","projectId":"first-project"}))["projectId"],
        "first-project"
    );
    write_registry(dir, "second-project");
    assert_eq!(
        exchange(json!({"type":"select_project","projectId":"second-project"}))["projectId"],
        "second-project"
    );
    let rejection = exchange(json!({"type":"select_project","projectId":"missing"}));
    assert_eq!(rejection["type"], "error");
    assert!(rejection["message"].as_str().unwrap().contains("second-project"));
    assert_eq!(exchange(json!({"type":"chat"}))["type"], "error");
    assert_eq!(
        exchange(json!({"type":"select_project","projectId":"second-project"}))["type"],
        "project_selected"
    );
}
