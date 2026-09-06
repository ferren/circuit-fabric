//! Health probes for a running `circuitfabric-jlc-bridge`.
//!
//! Two levels, because the bridge serves one WebSocket client at a time:
//! a plain TCP probe is safe to run automatically (it never disturbs an
//! attached EDA client), while the protocol-level `status` round-trip is a
//! deliberate connection test — it opens the `/bridge` WebSocket, asks for
//! the bridge's protocol version and reported capabilities, and closes.

// WebSocket length prefixes are range-checked before they are narrowed.
#![allow(clippy::cast_possible_truncation)]

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

/// What a successful connection test learned from the bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeStatusReport {
    pub protocol_version: u64,
    pub bridge_name: String,
    pub capabilities: Vec<String>,
    /// Registered project IDs — the gate the EDA extension's `hello` depends on.
    pub projects: Vec<String>,
}

/// Checks whether anything is listening on the bridge address.
///
/// # Errors
///
/// Returns an error when the address is malformed or the connection is
/// refused or times out.
pub fn tcp_reachable(address: &str, timeout: Duration) -> Result<(), String> {
    let socket: SocketAddr =
        address.parse().map_err(|error| format!("bridge 地址无效（{address}）：{error}"))?;
    TcpStream::connect_timeout(&socket, timeout)
        .map(|_| ())
        .map_err(|error| format!("无法连接 {address}：{error}"))
}

/// Performs the protocol-level connection test: WebSocket handshake plus a
/// `status` message, returning the bridge's self-reported version,
/// capabilities, and registered project IDs.
///
/// # Errors
///
/// Returns an error when the address is malformed, the bridge is not
/// reachable, the handshake times out (typically because an EDA client is
/// already attached — the bridge serves one connection at a time), or the
/// reply is not a valid `status_ack`.
pub fn status(address: &str, timeout: Duration) -> Result<BridgeStatusReport, String> {
    let mut stream = open_bridge_socket(address, timeout)?;
    write_client_frame(&mut stream, 0x1, br#"{"type":"status"}"#)?;
    let reply =
        read_text_frame(&mut stream)?.ok_or_else(|| "bridge 在应答前关闭了连接".to_owned())?;
    let message: Value = serde_json::from_str(&reply)
        .map_err(|error| format!("bridge 应答不是有效 JSON：{error}"))?;
    if message.get("type").and_then(Value::as_str) != Some("status_ack") {
        return Err(format!(
            "bridge 应答了意外的消息：{}",
            message.get("type").and_then(Value::as_str).unwrap_or("(无 type)")
        ));
    }
    Ok(BridgeStatusReport {
        protocol_version: message
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        bridge_name: message.get("bridge").and_then(Value::as_str).unwrap_or_default().to_owned(),
        capabilities: message
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|items| {
                items.iter().filter_map(|item| item.as_str().map(ToOwned::to_owned)).collect()
            })
            .unwrap_or_default(),
        projects: message
            .get("projects")
            .and_then(Value::as_array)
            .map(|items| {
                items.iter().filter_map(|item| item.as_str().map(ToOwned::to_owned)).collect()
            })
            .unwrap_or_default(),
    })
}

/// Performs the same `hello` handshake the EDA extension sends, for the exact
/// connect path an EDA client takes.
///
/// # Errors
///
/// Returns an error when the transport fails, or carries the bridge's own
/// error message (for example an unregistered project ID) verbatim.
pub fn hello(address: &str, project_id: &str, timeout: Duration) -> Result<(), String> {
    let mut stream = open_bridge_socket(address, timeout)?;
    let hello = serde_json::json!({
        "type": "hello",
        "protocolVersion": 1,
        "projectId": project_id,
    });
    write_client_frame(&mut stream, 0x1, hello.to_string().as_bytes())?;
    let reply =
        read_text_frame(&mut stream)?.ok_or_else(|| "bridge 在应答前关闭了连接".to_owned())?;
    let message: Value = serde_json::from_str(&reply)
        .map_err(|error| format!("bridge 应答不是有效 JSON：{error}"))?;
    match message.get("type").and_then(Value::as_str) {
        Some("hello_ack") => Ok(()),
        Some("error") => Err(message
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("bridge 拒绝了连接")
            .to_owned()),
        other => Err(format!("bridge 应答了意外的消息：{other:?}")),
    }
}

/// Connects to the bridge and completes the `/bridge` WebSocket upgrade.
fn open_bridge_socket(address: &str, timeout: Duration) -> Result<TcpStream, String> {
    let socket: SocketAddr =
        address.parse().map_err(|error| format!("bridge 地址无效（{address}）：{error}"))?;
    let mut stream = TcpStream::connect_timeout(&socket, timeout)
        .map_err(|error| format!("无法连接 {address}：{error}"))?;
    stream.set_read_timeout(Some(timeout)).map_err(|error| error.to_string())?;
    stream.set_write_timeout(Some(timeout)).map_err(|error| error.to_string())?;

    let key = handshake_key();
    let request = format!(
        "GET /bridge HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(|error| error.to_string())?;
    let headers = read_http_headers(&mut stream)?;
    if !headers.starts_with("HTTP/1.1 101") {
        let status_line = headers.lines().next().unwrap_or_default();
        return Err(format!("bridge 拒绝了 WebSocket 握手：{status_line}"));
    }
    Ok(stream)
}

/// Non-crypto randomness for the handshake key and frame masks: these only
/// need to differ between probes, never to be secret.
fn pseudo_random_bytes<const N: usize>() -> [u8; N] {
    let nanos = u64::from(
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.subsec_nanos()),
    );
    let mut state = (nanos ^ (u64::from(std::process::id()) << 32)) | 1;
    let mut bytes = [0_u8; N];
    for byte in &mut bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state.to_le_bytes()[0];
    }
    bytes
}

/// A base64 `Sec-WebSocket-Key` of the length RFC 6455 expects.
fn handshake_key() -> String {
    base64_encode(&pseudo_random_bytes::<16>())
}

fn read_http_headers(stream: &mut TcpStream) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    while bytes.len() < 16_384 {
        stream.read_exact(&mut byte).map_err(|error| {
            format!("WebSocket 握手超时或失败（{error}）；bridge 可能正被 EDA 客户端占用")
        })?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).map_err(|error| error.to_string());
        }
    }
    Err("bridge HTTP 响应头超过 16 KiB".to_owned())
}

/// Writes one masked frame, as a WebSocket client must. `opcode` is `0x1`
/// for text or `0xA` for a pong reply.
fn write_client_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> Result<(), String> {
    let mask = pseudo_random_bytes::<4>();
    stream.write_all(&[0x80 | opcode]).map_err(|error| error.to_string())?;
    let length = payload.len();
    if length <= 125 {
        let length = u8::try_from(length).expect("length checked <= 125");
        stream.write_all(&[0x80 | length]).map_err(|error| error.to_string())?;
    } else if length <= 65_535 {
        let length = u16::try_from(length).expect("length checked <= 65535");
        stream
            .write_all(&[0x80 | 0x7E])
            .and_then(|()| stream.write_all(&length.to_be_bytes()))
            .map_err(|error| error.to_string())?;
    } else {
        stream
            .write_all(&[0x80 | 0x7F])
            .and_then(|()| stream.write_all(&(length as u64).to_be_bytes()))
            .map_err(|error| error.to_string())?;
    }
    stream.write_all(&mask).map_err(|error| error.to_string())?;
    let masked: Vec<u8> =
        payload.iter().enumerate().map(|(index, byte)| byte ^ mask[index % 4]).collect();
    stream.write_all(&masked).and_then(|()| stream.flush()).map_err(|error| error.to_string())
}

/// Reads one text frame from the server; `None` on a close frame.
fn read_text_frame(stream: &mut TcpStream) -> Result<Option<String>, String> {
    let mut header = [0_u8; 2];
    if let Err(error) = stream.read_exact(&mut header) {
        return Err(format!("等待 bridge 应答超时或失败（{error}）"));
    }
    let opcode = header[0] & 0x0f;
    if opcode == 0x8 {
        return Ok(None);
    }
    let masked = header[1] & 0x80 != 0;
    let mut length = u64::from(header[1] & 0x7f);
    if length == 126 {
        let mut extended = [0_u8; 2];
        stream.read_exact(&mut extended).map_err(|error| error.to_string())?;
        length = u64::from(u16::from_be_bytes(extended));
    } else if length == 127 {
        let mut extended = [0_u8; 8];
        stream.read_exact(&mut extended).map_err(|error| error.to_string())?;
        length = u64::from_be_bytes(extended);
    }
    if length > 1_048_576 {
        return Err("bridge 应答帧超过 1 MiB".to_owned());
    }
    let mut mask = [0_u8; 4];
    if masked {
        stream.read_exact(&mut mask).map_err(|error| error.to_string())?;
    }
    let mut payload = vec![0_u8; length as usize];
    stream.read_exact(&mut payload).map_err(|error| error.to_string())?;
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    if opcode == 0x9 {
        let _ = write_client_frame(stream, 0xA, &payload);
        return read_text_frame(stream);
    }
    if opcode != 0x1 {
        return Err("bridge 发送了非文本帧".to_owned());
    }
    String::from_utf8(payload).map(Some).map_err(|error| error.to_string())
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 { ALPHABET[(value & 63) as usize] as char } else { '=' });
    }
    output
}
