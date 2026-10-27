//! Shared helpers for the bridge integration tests: a small WebSocket client
//! (handshake + masked text frames with 16/64-bit extended lengths) and a
//! supervised bridge process bound to an ephemeral loopback address.

// 帧编解码里长度收窄前都已做范围检查。
#![allow(clippy::cast_possible_truncation)]

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use circuitfabric_codex_runtime::RuntimeSettings;
use jlcircuit_eda_bridge::{probe, supervision::BridgeProcessHandle};
use serde_json::Value;

pub fn temp_dir() -> PathBuf {
    // Windows 时钟粒度约 15ms：并行测试会在同一纳秒读数下撞名，必须再叠加序号。
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir()
        .join(format!("circuitfabric-bridge-test-{}-{unique}-{sequence}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temporary settings directory");
    dir
}

pub fn free_loopback_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral loopback port");
    let address = listener.local_addr().expect("read the ephemeral address").to_string();
    drop(listener);
    address
}

pub fn write_settings(dir: &Path, address: &str) -> PathBuf {
    let mut settings = RuntimeSettings::default();
    address.clone_into(&mut settings.bridge.listen_address);
    let path = dir.join("runtime.json");
    settings.save(&path).expect("default settings with a loopback address are valid");
    path
}

pub fn write_registry(dir: &Path, project_id: &str) {
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

pub fn wait_until_listening(address: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if probe::tcp_reachable(address, Duration::from_millis(300)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the bridge did not start listening on {address} within {timeout:?}");
}

/// A running bridge binary on an ephemeral loopback address, stopped and
/// cleaned up on drop.
pub struct RunningBridge {
    pub dir: PathBuf,
    pub address: String,
    #[allow(dead_code)]
    pub config_path: PathBuf,
    handle: BridgeProcessHandle,
}

impl RunningBridge {
    pub fn spawn() -> Self {
        let dir = temp_dir();
        let address = free_loopback_address();
        let config_path = write_settings(&dir, &address);
        let handle = BridgeProcessHandle::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge")),
            &config_path,
        )
        .expect("the bridge binary launches");
        wait_until_listening(&address, Duration::from_secs(10));
        Self { dir, address, config_path, handle }
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) -> Result<(), String> {
        self.handle.stop()
    }
}

impl Drop for RunningBridge {
    fn drop(&mut self) {
        let _ = self.handle.stop();
        std::thread::sleep(Duration::from_millis(100));
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// Minimal dependency-free WebSocket client speaking masked client frames.
pub struct WsClient {
    stream: TcpStream,
}

impl WsClient {
    /// Connects and completes the handshake, asserting the server accepted it.
    pub fn connect(address: &str, path: &str) -> Self {
        let mut stream = TcpStream::connect(address).expect("connect to the bridge");
        stream.set_read_timeout(Some(Duration::from_secs(10))).expect("set read timeout");
        handshake_on(&mut stream, path, true);
        Self { stream }
    }

    /// Connects and completes the handshake, asserting the server rejected it.
    #[allow(dead_code)]
    pub fn connect_rejected(address: &str, path: &str) {
        let mut stream = TcpStream::connect(address).expect("connect to the bridge");
        stream.set_read_timeout(Some(Duration::from_secs(10))).expect("set read timeout");
        handshake_on(&mut stream, path, false);
    }

    pub fn send(&mut self, message: &Value) {
        let payload = message.to_string();
        let bytes = payload.as_bytes();
        let mut frame = Vec::with_capacity(bytes.len() + 14);
        frame.push(0x81);
        match bytes.len() {
            0..=125 => frame.push(0x80 | bytes.len() as u8),
            126..=65_535 => {
                frame.push(0x80 | 0x7E);
                frame.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
            }
            _ => {
                frame.push(0x80 | 0x7F);
                frame.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            }
        }
        // 测试用的固定掩码：只需满足「客户端帧必须掩码」的协议要求。
        let mask = [0xA5_u8, 0x5A, 0xC3, 0x3C];
        frame.extend_from_slice(&mask);
        for (index, byte) in bytes.iter().enumerate() {
            frame.push(byte ^ mask[index % 4]);
        }
        self.stream.write_all(&frame).expect("send a masked text frame");
    }

    /// Receives the next text frame; fails the test on timeout or non-text frame.
    pub fn recv(&mut self) -> Value {
        match self.recv_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => panic!("connection closed while a frame was expected"),
            Err(error) => panic!("timed out or failed while receiving a frame: {error}"),
        }
    }

    /// Receives one frame; `Ok(None)` marks a close frame (graceful close).
    pub fn recv_frame(&mut self) -> std::io::Result<Option<Value>> {
        let mut header = [0_u8; 2];
        self.stream.read_exact(&mut header)?;
        let opcode = header[0] & 0x0f;
        if opcode == 0x8 {
            return Ok(None);
        }
        assert_eq!(opcode, 0x1, "expected a text frame, got opcode {opcode:#x}");
        let masked = header[1] & 0x80 != 0;
        let mut length = u64::from(header[1] & 0x7f);
        if length == 126 {
            let mut bytes = [0_u8; 2];
            self.stream.read_exact(&mut bytes)?;
            length = u64::from(u16::from_be_bytes(bytes));
        } else if length == 127 {
            let mut bytes = [0_u8; 8];
            self.stream.read_exact(&mut bytes)?;
            length = u64::from_be_bytes(bytes);
        }
        let mut mask = [0_u8; 4];
        if masked {
            self.stream.read_exact(&mut mask)?;
        }
        let mut payload = vec![0_u8; length as usize];
        self.stream.read_exact(&mut payload)?;
        if masked {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        Ok(Some(serde_json::from_slice(&payload).expect("the reply is valid JSON")))
    }

    pub fn exchange(&mut self, message: &Value) -> Value {
        self.send(message);
        self.recv()
    }

    /// Asserts that no frame arrives within `timeout` (restores the default
    /// read timeout afterwards).
    #[allow(dead_code)]
    pub fn expect_silence(&mut self, timeout: Duration) {
        self.stream.set_read_timeout(Some(timeout)).expect("set short read timeout");
        let result = self.recv_frame();
        let _ = self.stream.set_read_timeout(Some(Duration::from_secs(10)));
        match result {
            Ok(None) => panic!("connection closed while silence was expected"),
            Ok(Some(frame)) => panic!("expected no frame, got: {frame}"),
            Err(_) => {}
        }
    }
}

fn handshake_on(stream: &mut TcpStream, path: &str, expect_accept: bool) {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).expect("send the handshake request");
    let mut headers = Vec::new();
    let mut byte = [0_u8; 1];
    while !headers.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("read the handshake response");
        headers.push(byte[0]);
    }
    if expect_accept {
        assert!(
            headers.starts_with(b"HTTP/1.1 101"),
            "the handshake was rejected: {}",
            String::from_utf8_lossy(&headers)
        );
    } else {
        assert!(
            !headers.starts_with(b"HTTP/1.1 101"),
            "the handshake for `{path}` should have been rejected"
        );
    }
}
