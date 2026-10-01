//! Exercises the real MCP child, catalog authorization, vault injection and HTTP adapter.
#![allow(clippy::needless_pass_by_value)] // Fixture helpers own their short-lived input values.
use circuitfabric_codex_runtime::{
    ToolAuthorizationSettings,
    judge::{AnswerMode, LlmJudgeSettings, OutputFormat},
    secrets::UnlockedVault,
    tools::{BUNDLED_JEV_SERVER_ID, ToolCatalog},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn completion(answers: Value) -> Value {
    json!({"choices":[{"message":{"content":json!({"answers":answers}).to_string()},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}})
}

struct Mock {
    url: String,
    worker: Option<thread::JoinHandle<Vec<Value>>>,
}
impl Mock {
    fn new(replies: Vec<(u16, Value)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, reply) in replies {
                let deadline = Instant::now() + Duration::from_secs(15);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "HTTP request never arrived");
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with("POST /v1/chat/completions "));
                let mut length = 0;
                let mut auth = false;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    if lower.trim() == "authorization: bearer test-vault-key" {
                        auth = true;
                    }
                }
                assert!(auth, "vault key did not reach the model backend");
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                requests.push(serde_json::from_slice(&body).unwrap());
                let bytes = reply.to_string();
                write!(stream,"HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).unwrap();
            }
            requests
        });
        Self { url, worker: Some(worker) }
    }
    fn finish(&mut self) -> Vec<Value> {
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Harness {
    catalog: ToolCatalog,
    grants: ToolAuthorizationSettings,
    vault: UnlockedVault,
    path: PathBuf,
}
impl Harness {
    fn new(settings: LlmJudgeSettings) -> Self {
        static NEXT_VAULT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = NEXT_VAULT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cf-judge-vault-{}-{counter}-{}.json",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let mut vault = UnlockedVault::create(&path, "judge-test-password").unwrap();
        vault.set("CF_JUDGE_TEST_KEY", "test-vault-key").unwrap();
        let server = settings
            .server(std::path::Path::new(env!("CARGO_BIN_EXE_circuitfabric-judge")), true)
            .unwrap();
        Self {
            catalog: ToolCatalog { mcp_servers: vec![server], ..Default::default() },
            grants: ToolAuthorizationSettings {
                authorized_mcp_server_ids: vec![BUNDLED_JEV_SERVER_ID.into()],
                ..Default::default()
            },
            vault,
            path,
        }
    }
    fn call(&self, arguments: Value) -> Result<Value, circuitfabric_codex_runtime::RuntimeError> {
        self.catalog.call_tool_with_secrets(
            BUNDLED_JEV_SERVER_ID,
            &self.grants,
            "evaluate",
            &arguments,
            Some(self.vault.values()),
        )
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn settings(mock: &Mock) -> LlmJudgeSettings {
    LlmJudgeSettings {
        base_url: mock.url.clone(),
        api_key_environment_variable: "CF_JUDGE_TEST_KEY".into(),
        ..Default::default()
    }
}
fn decode(result: Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}
fn noul_input() -> Value {
    json!({"state":"observed evidence","questions":{"ok":{"type":"noul","instructions":"Supported?"}}})
}

#[test]
fn real_catalog_and_mcp_evaluate_all_types_with_vault_key() {
    let mut mock = Mock::new(vec![(
        200,
        completion(
            json!({"ok":0.95,"category":{"pin":0.9,"other":0.1},"rating":{"0":0.1,"1":0.2,"2":0.7}}),
        ),
    )]);
    let mut harness = Harness::new(settings(&mock));
    let listed = harness
        .catalog
        .list_tools_with_secrets(
            BUNDLED_JEV_SERVER_ID,
            &harness.grants,
            Some(harness.vault.values()),
        )
        .unwrap();
    assert_eq!(listed["tools"][0]["name"], "evaluate");
    let result=decode(harness.call(json!({"state":"Pin 1 VIN Power input","questions":{
        "ok":{"type":"noul","instructions":"Faithful?"},
        "category":{"type":"choice","instructions":"Category?","criteria":{"pin":"Pin row","other":"Other"},"min_confidence":0.5},
        "rating":{"type":"score","instructions":"Quality?","criteria":["low","mid","high"]}
    }})).unwrap());
    assert_eq!(result["answers"]["category"]["choice"], "pin");
    assert!((result["answers"]["category"]["confidence"].as_f64().unwrap() - 0.8).abs() < 1e-9);
    assert!((result["answers"]["rating"]["score"].as_f64().unwrap() - 1.6).abs() < 1e-9);
    assert_eq!(result["meta"]["calibrated"], false);
    assert_eq!(result["meta"]["input_tokens"], 10);
    harness.grants.authorized_mcp_server_ids.clear();
    assert!(harness.call(noul_input()).unwrap_err().to_string().contains("未授权"));
    harness.grants.authorized_mcp_server_ids.push(BUNDLED_JEV_SERVER_ID.into());
    harness.catalog.mcp_servers[0].enabled = false;
    assert!(harness.call(noul_input()).is_err());
    let requests = mock.finish();
    assert_eq!(requests.len(), 1, "discovery/revocation/disable must not call the provider");
    assert_eq!(requests[0]["response_format"]["type"], "json_object");
    assert!(!harness.catalog.mcp_servers[0].args.join(" ").contains("test-vault-key"));
}

#[test]
fn malformed_response_retries_but_truncation_refusal_and_http_errors_do_not() {
    let mut mock =
        Mock::new(vec![(200, completion(json!({}))), (200, completion(json!({"ok":0.8})))]);
    let harness = Harness::new(settings(&mock));
    let result = decode(harness.call(noul_input()).unwrap());
    assert_eq!(result["answers"]["ok"]["noul"], 0.8);
    assert_eq!(result["meta"]["input_tokens"], 20);
    let requests = mock.finish();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["messages"].as_array().unwrap().len(), 3);
    for (status, reply) in [
        (
            200,
            json!({"choices":[{"finish_reason":"length","message":{"content":"{\"answers\":{\"ok\":1}}"}}]}),
        ),
        (
            200,
            json!({"choices":[{"finish_reason":"stop","message":{"refusal":"sensitive provider body"}}]}),
        ),
        (401, json!({"error":"sensitive provider body"})),
    ] {
        let mut mock = Mock::new(vec![(status, reply)]);
        let harness = Harness::new(settings(&mock));
        let error = harness.call(noul_input()).unwrap_err().to_string();
        assert!(!error.contains("sensitive provider body"));
        assert_eq!(mock.finish().len(), 1);
    }
}

#[test]
fn items_are_isolated_and_partial_failures_are_explicit() {
    let mut mock = Mock::new(vec![
        (200, completion(json!({"ok":0.9}))),
        (500, json!({"error":"unavailable"})),
    ]);
    let harness = Harness::new(settings(&mock));
    let result=decode(harness.call(json!({"state":{"policy":"common"},"items":{"a":{"record":"first"},"b":{"record":"second"}},"questions":{"ok":{"type":"noul","instructions":"Supported?"}},"include_item_usage":true})).unwrap());
    assert_eq!(result["results"]["a"]["answers"]["ok"]["noul"], 0.9);
    assert_eq!(result["results"]["a"]["usage"]["input_tokens"], 10);
    assert!(result["errors"]["b"].as_str().unwrap().contains("500"));
    assert_eq!(result["meta"]["item_count"], 2);
    let requests = mock.finish();
    let first: Value =
        serde_json::from_str(requests[0]["messages"][1]["content"].as_str().unwrap()).unwrap();
    let second: Value =
        serde_json::from_str(requests[1]["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(first["state"]["item"]["record"], "first");
    assert_eq!(second["state"]["item"]["record"], "second");
    assert_eq!(first["state"]["context"]["policy"], "common");
}

#[test]
fn error_envelopes_with_http_200_do_not_retry_and_do_not_echo_the_body() {
    for (status, reply) in [
        (200, json!({"code":401,"msg":"token expired or incorrect","success":false})),
        (200, json!({"error":{"code":"401","message":"token expired or incorrect"}})),
        (200, json!({"unrelated":"payload"})),
    ] {
        let mut mock = Mock::new(vec![(status, reply)]);
        let harness = Harness::new(settings(&mock));
        let error = harness.call(noul_input()).unwrap_err().to_string();
        assert!(!error.contains("token expired"), "provider body must not leak");
        assert!(
            error.contains("choices") || error.contains("HTTP 200"),
            "unhelpful error: {error}"
        );
        assert_eq!(mock.finish().len(), 1, "envelope errors must not be retried");
    }
}

#[test]
fn structured_and_prompted_modes_produce_discrete_answers() {
    for format in [OutputFormat::JsonSchema, OutputFormat::Prompted] {
        let mut mock = Mock::new(vec![(200, completion(json!({"ok":true})))]);
        let harness = Harness::new(LlmJudgeSettings {
            output_format: format,
            answer_mode: AnswerMode::Discrete,
            ..settings(&mock)
        });
        let result = decode(harness.call(noul_input()).unwrap());
        assert_eq!(result["answers"]["ok"]["probability_source"], "discrete_llm");
        let requests = mock.finish();
        if format == OutputFormat::JsonSchema {
            assert_eq!(requests[0]["response_format"]["json_schema"]["strict"], true);
        } else {
            assert!(requests[0].get("response_format").is_none());
        }
    }
}
