//! 子任务 G 的 chat 增量转发测试：Bridge 把 `run_session_observed` 的增量按帧
//! 转发为 `chat_delta`（`kind` 区分 answer/reasoning），未在 `hello` 上声明
//! `chatDeltaKinds` 的旧客户端只收到 answer 帧，Reasoning 增量不下发。

// 共享测试辅助：不同测试二进制用到的子集不同。
#[allow(dead_code)]
mod common;

use std::{path::Path, time::Duration};

use circuitfabric_codex_runtime::{ProviderConnection, RuntimeSettings};
use circuitfabric_contracts::Project;
use circuitfabric_project::ProjectStorage;
use jlcircuit_eda_bridge::supervision::BridgeProcessHandle;
use serde_json::{Value, json};

const PROJECT: &str = "stream-p1";
const SESSION: &str = "stream-s1";
/// 模拟模型一轮输出：先思考，再分两段回答。全 ASCII，避免 PowerShell 5.1 按
/// ANSI 读取无 BOM 脚本时中文字面量失真。
const REASONING: &str = "thinking";
const ANSWER_PART_1: &str = "volt";
const ANSWER_PART_2: &str = "age high";

/// 模拟 Codex App Server：每个 chat 回合一个新进程，按序输出 reasoning 增量、
/// 两段 answer 增量、用量与完成事件。
struct StreamingRig {
    dir: std::path::PathBuf,
    address: String,
    handle: BridgeProcessHandle,
}

impl StreamingRig {
    fn new() -> Self {
        let dir = common::temp_dir();
        let address = common::free_loopback_address();
        let launcher = dir.join("chat-stream-mock.cmd");
        std::fs::write(
            &launcher,
            "@powershell -NoProfile -NonInteractive -File \"%~dp0chat-stream-mock.ps1\"\r\n",
        )
        .unwrap();
        std::fs::write(dir.join("chat-stream-mock.ps1"), r#"
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$out = [Console]::Out
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $m = $line | ConvertFrom-Json
  if ($m.method -eq 'initialize') { $out.WriteLine('{"id":' + $m.id + ',"result":{}}') }
  elseif ($m.method -eq 'thread/start' -or $m.method -eq 'thread/resume') {
    $out.WriteLine('{"id":' + $m.id + ',"result":{"thread":{"id":"eda-chat-thread"}}}')
  }
  elseif ($m.method -eq 'turn/start') {
    $out.WriteLine('{"id":' + $m.id + ',"result":{"turn":{"id":"turn"}}}')
    $out.WriteLine('{"method":"item/reasoning/summaryTextDelta","params":{"delta":"thinking"}}')
    $out.WriteLine('{"method":"item/agentMessage/delta","params":{"delta":"volt"}}')
    $out.WriteLine('{"method":"item/agentMessage/delta","params":{"delta":"age high"}}')
    $out.WriteLine('{"method":"thread/tokenUsage/updated","params":{"threadId":"eda-chat-thread","turnId":"turn","tokenUsage":{"total":{"inputTokens":3,"outputTokens":2}}}}')
    $out.WriteLine('{"method":"turn/completed","params":{"threadId":"eda-chat-thread","turn":{"id":"turn","status":"completed"}}}')
  }
  $out.Flush()
}
"#).unwrap();
        let mut settings = RuntimeSettings::default();
        settings.bridge.listen_address.clone_from(&address);
        settings.codex.command = launcher.to_string_lossy().into_owned();
        // 只运行协议夹具，不读凭据、不请求模型端点。
        settings.providers[0].connection = ProviderConnection::CodexAccount;
        let config_path = dir.join("runtime.json");
        settings.save(&config_path).unwrap();
        common::write_registry(&dir, PROJECT);
        let root = dir.join("project-root");
        std::fs::create_dir_all(&root).unwrap();
        ProjectStorage::create(
            &root,
            Project { id: PROJECT.into(), name: "Streaming".into(), description: None },
        )
        .unwrap();
        let handle = BridgeProcessHandle::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge")),
            &config_path,
        )
        .unwrap();
        common::wait_until_listening(&address, Duration::from_secs(10));
        Self { dir, address, handle }
    }
}

impl Drop for StreamingRig {
    fn drop(&mut self) {
        let _ = self.handle.stop();
        std::thread::sleep(Duration::from_millis(100));
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 连接并选择项目；`hello` 可携带 `chatDeltaKinds` 能力声明。
fn connected_client(rig: &StreamingRig, chat_delta_kinds: Option<Value>) -> common::WsClient {
    let mut client = common::WsClient::connect(&rig.address, "/bridge");
    let mut hello = json!({"type":"hello","protocolVersion":1});
    if let Some(kinds) = chat_delta_kinds {
        hello["chatDeltaKinds"] = kinds;
    }
    assert_eq!(client.exchange(&hello)["type"], "hello_ack");
    assert_eq!(
        client.exchange(&json!({"type":"select_project","projectId":PROJECT}))["type"],
        "project_selected"
    );
    client
}

/// 发送一条 chat 并收集到 `chat_completed` 为止的全部帧。
fn run_chat(client: &mut common::WsClient) -> Vec<Value> {
    client.send(&json!({"type":"chat","sessionId":SESSION,"text":"check power input"}));
    let mut frames = Vec::new();
    loop {
        let frame = client.recv();
        let finished = frame["type"] == "chat_completed";
        frames.push(frame);
        if finished {
            return frames;
        }
    }
}

/// 全部 `chat_delta` 帧的 `(kind, delta)` 序列；缺省 `kind` 视为 `answer`（旧 Bridge）。
fn deltas(frames: &[Value]) -> Vec<(String, String)> {
    frames
        .iter()
        .filter(|frame| frame["type"] == "chat_delta")
        .map(|frame| {
            (
                frame["kind"].as_str().unwrap_or("answer").to_owned(),
                frame["delta"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

#[cfg(windows)]
#[test]
fn capability_clients_receive_reasoning_and_answer_frames_in_order() {
    let rig = StreamingRig::new();
    let mut client = connected_client(&rig, Some(json!(["answer", "reasoning"])));

    let frames = run_chat(&mut client);

    assert_eq!(frames.first().unwrap()["type"], "chat_started", "帧序：{frames:?}");
    // reasoning 先于 answer；answer 增量按序拼接即最终文本。
    assert_eq!(
        deltas(&frames),
        vec![
            ("reasoning".to_owned(), REASONING.to_owned()),
            ("answer".to_owned(), ANSWER_PART_1.to_owned()),
            ("answer".to_owned(), ANSWER_PART_2.to_owned()),
        ],
        "帧序：{frames:?}"
    );
    assert!(
        frames.iter().all(|frame| frame["delta"].as_str() != Some("voltage high")),
        "完成后不得重发整段回复：{frames:?}"
    );
    assert_eq!(frames.last().unwrap()["type"], "chat_completed");

    // 持久化语义：answer 进会话轮次，思考链不落盘。
    let loaded = client.exchange(&json!({"type":"load_session","sessionId":SESSION}));
    let body = loaded["body"].as_str().unwrap_or_default();
    assert!(body.contains("voltage high"), "回复应持久化：{body}");
    assert!(!body.contains(REASONING), "思考链不得持久化：{body}");

    // 会话通道在 chat 后仍然可用。
    assert_eq!(client.exchange(&json!({"type":"ping"}))["type"], "pong");
}

#[cfg(windows)]
#[test]
fn legacy_clients_receive_answer_frames_without_reasoning() {
    let rig = StreamingRig::new();
    let mut client = connected_client(&rig, None);

    let frames = run_chat(&mut client);

    let received = deltas(&frames);
    assert_eq!(
        received,
        vec![
            ("answer".to_owned(), ANSWER_PART_1.to_owned()),
            ("answer".to_owned(), ANSWER_PART_2.to_owned()),
        ],
        "旧客户端只收到 answer 增量：{frames:?}"
    );
    assert!(
        frames.iter().all(|frame| frame["kind"] != "reasoning"),
        "未声明 chatDeltaKinds 时不得下发 reasoning 帧：{frames:?}"
    );
    assert!(received.len() > 1, "回复必须逐帧增量下发，不得一次性整段：{frames:?}");
    assert_eq!(frames.last().unwrap()["type"], "chat_completed");
}
