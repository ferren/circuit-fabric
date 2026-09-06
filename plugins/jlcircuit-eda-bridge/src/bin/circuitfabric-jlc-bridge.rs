//! Loopback WebSocket bridge between the JLC EDA extension and Codex App Server.
//!
//! This deliberately small transport is dependency-free: it accepts one local
//! WebSocket client at a time and never exposes an EDA or Codex credential over
//! the network. It is a first vertical slice, not the eventual supervisor.

#![allow(
    clippy::cast_possible_truncation,
    clippy::chunks_exact_to_as_chunks,
    clippy::needless_pass_by_value
)]

use std::{
    collections::BTreeMap,
    env,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
};

use circuitfabric_codex_runtime::{
    RuntimeSettings,
    execution::{AgentKind, Cancellation, run_task},
};
use circuitfabric_project::{ProjectRegistry, ProjectStorage};
use serde_json::{Value, json};

const BRIDGE_PROTOCOL_VERSION: u64 = 1;

fn main() -> Result<(), String> {
    let config_path = env::args().nth(1).map_or_else(RuntimeSettings::default_path, PathBuf::from);
    let settings =
        RuntimeSettings::load_or_default(&config_path).map_err(|error| error.to_string())?;
    settings.validate().map_err(|error| error.to_string())?;
    let listener = TcpListener::bind(&settings.bridge.listen_address).map_err(|error| {
        format!("could not listen on {}: {error}", settings.bridge.listen_address)
    })?;
    println!(
        "CircuitFabric JLC bridge listening on ws://{}/bridge",
        settings.bridge.listen_address
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = serve_connection(stream, &config_path) {
                    eprintln!("bridge client disconnected: {error}");
                }
            }
            Err(error) => eprintln!("bridge accept error: {error}"),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn serve_connection(mut stream: TcpStream, config_path: &std::path::Path) -> Result<(), String> {
    websocket_handshake(&mut stream)?;
    let mut threads = BTreeMap::<String, String>::new();
    let mut previous_snapshot = None;
    let mut project_id: Option<String> = None;

    while let Some(raw) = read_text_frame(&mut stream)? {
        let message: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        match message.get("type").and_then(Value::as_str) {
            Some("hello") => {
                let requested_project = message
                    .get("projectId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "hello requires a projectId".to_owned())?;
                if project_id.as_deref() != Some(requested_project) {
                    threads.clear();
                }
                let registry = ProjectRegistry::load(config_path.with_file_name("projects.json"))
                    .map_err(|e| e.to_string())?;
                if registry.root_for(requested_project).is_none() {
                    return Err("项目未注册，拒绝连接".to_owned());
                }
                project_id = Some(requested_project.to_owned());
                send_json(
                    &mut stream,
                    json!({
                        "type": "hello_ack",
                        "protocolVersion": BRIDGE_PROTOCOL_VERSION,
                        "bridge": "CircuitFabric",
                    }),
                )?;
            }
            Some("chat") => {
                let project_id =
                    project_id.as_deref().ok_or_else(|| "send hello before chat".to_owned())?;
                let session_id = message
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "chat requires a sessionId".to_owned())?;
                let text = message
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "chat requires text".to_owned())?;
                let settings =
                    RuntimeSettings::load_or_default(config_path).map_err(|e| e.to_string())?;
                let registry = ProjectRegistry::load(config_path.with_file_name("projects.json"))
                    .map_err(|e| e.to_string())?;
                let root = registry.root_for(project_id).ok_or_else(|| "项目未注册".to_owned())?;
                let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
                let configuration = storage.load_configuration().map_err(|e| e.to_string())?;
                let mut grants = settings.tools.clone();
                grants.authorized_skill_ids.extend(configuration.enabled_skill_ids);
                grants.authorized_mcp_server_ids.extend(configuration.enabled_mcp_server_ids);
                grants.authorized_skill_ids.sort();
                grants.authorized_skill_ids.dedup();
                grants.authorized_mcp_server_ids.sort();
                grants.authorized_mcp_server_ids.dedup();
                let thread_id = format!("{project_id}:{session_id}");
                send_json(
                    &mut stream,
                    json!({
                        "type": "chat_started", "sessionId": session_id, "threadId": thread_id,
                    }),
                )?;
                let prompt = format!(
                    "You are assisting inside JLC EDA for CircuitFabric project `{project_id}`. \
                     Treat any EDA operation as a proposal unless the user explicitly requests a confirmed write.\n\n{text}"
                );
                let prompt =
                    format!("{}\n{prompt}", configuration.agent_instructions.unwrap_or_default());
                let cancel = Cancellation::default();
                let done = std::sync::atomic::AtomicBool::new(false);
                let project_path = storage.configuration_path();
                let configuration_files = [config_path.to_owned(), project_path];
                let snapshots = configuration_files
                    .iter()
                    .map(std::fs::read)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                if previous_snapshot.as_ref() != Some(&snapshots) {
                    threads.clear();
                }
                previous_snapshot = Some(snapshots.clone());
                let previous = threads.get(&thread_id).cloned().unwrap_or_default();
                let prompt = format!("{previous}\n\nUser: {prompt}");
                if prompt.len() > 256 * 1024 {
                    return Err("会话上下文过长，请新建会话".to_owned());
                }
                let result = std::thread::scope(|scope| {
                    scope.spawn(|| {
                        while !done.load(std::sync::atomic::Ordering::SeqCst) {
                            if configuration_files.iter().zip(&snapshots).any(|(path, original)| {
                                std::fs::read(path).ok().as_ref() != Some(original)
                            }) {
                                cancel.cancel();
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    });
                    let result = run_task(&settings, AgentKind::Codex, &grants, &prompt, &cancel);
                    done.store(true, std::sync::atomic::Ordering::SeqCst);
                    result
                });
                match result {
                    Ok(output) => {
                        threads.insert(thread_id, format!("{prompt}\n\nAssistant: {output}"));
                        send_json(
                            &mut stream,
                            json!({"type":"chat_delta","sessionId":session_id,"delta":output}),
                        )?;
                    }
                    Err(error) => {
                        send_json(
                            &mut stream,
                            json!({"type":"error","sessionId":session_id,"message":error.to_string()}),
                        )?;
                        continue;
                    }
                }
                send_json(
                    &mut stream,
                    json!({ "type": "chat_completed", "sessionId": session_id }),
                )?;
            }
            Some("ping") => send_json(&mut stream, json!({ "type": "pong" }))?,
            _ => send_json(
                &mut stream,
                json!({ "type": "error", "message": "unsupported bridge message" }),
            )?,
        }
    }
    Ok(())
}

fn websocket_handshake(stream: &mut TcpStream) -> Result<(), String> {
    let request = read_http_headers(stream)?;
    if !request.starts_with("GET /bridge ") {
        return Err("only GET /bridge is accepted".to_owned());
    }
    let key = request
        .lines()
        .find_map(|line| line.strip_prefix("Sec-WebSocket-Key: "))
        .ok_or_else(|| "missing Sec-WebSocket-Key".to_owned())?;
    let accept =
        base64_encode(&sha1(format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes()));
    write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())
}

fn read_http_headers(stream: &mut TcpStream) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    while bytes.len() < 16_384 {
        stream.read_exact(&mut byte).map_err(|error| error.to_string())?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).map_err(|error| error.to_string());
        }
    }
    Err("HTTP headers exceed 16 KiB".to_owned())
}

fn read_text_frame(stream: &mut TcpStream) -> Result<Option<String>, String> {
    let mut header = [0_u8; 2];
    match stream.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.to_string()),
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
        return Err("bridge frame exceeds 1 MiB".to_owned());
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
        write_frame(stream, 0xA, &payload)?;
        return read_text_frame(stream);
    }
    if opcode != 0x1 {
        return Err("only text WebSocket frames are supported".to_owned());
    }
    String::from_utf8(payload).map(Some).map_err(|error| error.to_string())
}

fn send_json(stream: &mut TcpStream, value: Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    write_frame(stream, 0x1, &encoded)
}

fn write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> Result<(), String> {
    stream.write_all(&[0x80 | opcode]).map_err(|error| error.to_string())?;
    match payload.len() {
        0..=125 => stream.write_all(&[payload.len() as u8]),
        126..=65_535 => stream
            .write_all(&[126])
            .and_then(|()| stream.write_all(&(payload.len() as u16).to_be_bytes())),
        _ => stream
            .write_all(&[127])
            .and_then(|()| stream.write_all(&(payload.len() as u64).to_be_bytes())),
    }
    .and_then(|()| stream.write_all(payload))
    .and_then(|()| stream.flush())
    .map_err(|error| error.to_string())
}

#[allow(clippy::many_single_char_names)]
fn sha1(input: &[u8]) -> [u8; 20] {
    let mut data = input.to_vec();
    let length_bits = (data.len() as u64) * 8;
    data.push(0x80);
    while (data.len() % 64) != 56 {
        data.push(0);
    }
    data.extend_from_slice(&length_bits.to_be_bytes());
    let mut h = [0x6745_2301_u32, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    for chunk in data.chunks_exact(64) {
        let mut w = [0_u32; 80];
        for (index, word) in w[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes(
                chunk[index * 4..index * 4 + 4].try_into().expect("fixed chunk"),
            );
        }
        for index in 16..80 {
            w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (index, word) in w.into_iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp =
                a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut output = [0_u8; 20];
    for (index, word) in h.into_iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_accept_hash_matches_the_rfc_example() {
        let value =
            base64_encode(&sha1(b"dGhlIHNhbXBsZSBub25jZQ==258EAFA5-E914-47DA-95CA-C5AB0DC85B11"));
        assert_eq!(value, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }
}
