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
