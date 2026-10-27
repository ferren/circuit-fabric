//! 子任务 E 的工具边界测试：模型侧只有三个受控工具，任何路径都不触发未审批
//! 写入；闭环（观测 → 提议 → 请求预览）在 MCP 协议面上完成；运行时重启后仅凭
//! 已存储引用恢复。

// 共享测试辅助：不同测试二进制用到的子集不同。
#[allow(dead_code)]
mod common;

use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use circuitfabric_contracts::Project;
use circuitfabric_project::{
    ChangeSetExecutionReport, ProjectStorage, SessionCategory, SessionSeed,
};
use jlcircuit_eda_bridge::{agent_tools, observation};
use serde_json::{Value, json};

const PROJECT: &str = "p1";
const SESSION: &str = "s1";

/// 项目 + 注册表 + 设置 + 一次持久化 inspect 观测 + EDA 会话。
struct Rig {
    dir: PathBuf,
    root: PathBuf,
    config_path: PathBuf,
    hash: String,
}

impl Rig {
    fn new() -> Self {
        let dir = common::temp_dir();
        let config_path = common::write_settings(&dir, "127.0.0.1:1");
        common::write_registry(&dir, PROJECT);
        let root = dir.join("project-root");
        std::fs::create_dir_all(&root).unwrap();
        let storage = ProjectStorage::create(
            &root,
            Project { id: PROJECT.into(), name: "测试".into(), description: None },
        )
        .unwrap();
        storage
            .start_categorized_session(
                SessionSeed {
                    session_id: SESSION.into(),
                    runtime_profile_id: "test".into(),
                    backend_id: Some("codex".into()),
                },
                SessionCategory::Eda,
                None,
            )
            .unwrap();
        let payload = json!({
            "document": {"kind":"schematic", "projectUuid":"eda-project", "pageUuid":"page-1"},
            "components": [{"primitiveId":"prim-r1", "designator":"R1", "name":"10k",
                "footprint":"0603", "x":0, "y":0, "rotation":0, "mirror":false,
                "pins":[{"number":"1", "name":"A", "x":0, "y":0}]}],
            "wires":[], "nets":[], "buses":[], "inconclusive": []
        });
        let build = observation::build_observation(&payload, None).unwrap();
        let hash = build.snapshot.snapshot_hash.clone();
        let record = json!({"schemaVersion":1, "kind":"inspect", "projectId":PROJECT, "sessionId":SESSION,
            "receivedAt":"2026-10-23T01:00:00.000Z", "observedPayload":payload,
            "snapshot":build.snapshot, "externalRefs":build.external_refs,
            "inconclusive":build.inconclusive, "validation":build.validation,
            "physicalProjection":build.physical_projection});
        observation::persist_inspect_record(&root, &record).unwrap();
        Self { dir, root, config_path, hash }
    }

    fn context(&self) -> agent_tools::ToolContext {
        agent_tools::ToolContext::new(self.config_path.clone(), PROJECT.into(), SESSION.into())
    }

    /// 一批 JSON-RPC 帧经过 serve 的应答（每次调用等价一次工具服务器会话）。
    fn serve_batch(&self, frames: &str) -> Vec<Value> {
        let mut output = Vec::new();
        agent_tools::serve(BufReader::new(frames.as_bytes()), &mut output, &self.context())
            .unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn call(&self, id: u64, tool: &str, arguments: &Value) -> Value {
        let frame = json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}});
        let replies = self.serve_batch(&format!("{frame}\n"));
        assert_eq!(replies.len(), 1);
        replies.into_iter().next().unwrap()
    }

    fn patch(&self) -> Value {
        json!({"base_snapshot_hash": self.hash,
            "operations": [{"annotate_semantic": {"key": "domain", "value": "power"}}],
            "rationale": "标注用途"})
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// 成功结果的内容文本解析为 JSON；isError 结果返回错误文本。
fn outcome(reply: &Value) -> Result<Value, String> {
    let text = reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    if reply["result"]["isError"] == true {
        Err(text)
    } else {
        Ok(serde_json::from_str(&text).unwrap())
    }
}

#[test]
fn model_completes_the_observe_propose_preview_loop_over_mcp() {
    let rig = Rig::new();
    let opening = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
    );
    let replies = rig.serve_batch(opening);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2024-11-05");
    let tools: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["eda_inspect", "eda_propose_patch", "eda_request_preview"]);

    // 观测：返回基线引用与语义快照，不接触执行器。
    let inspected = outcome(&rig.call(3, "eda_inspect", &json!({}))).unwrap();
    assert_eq!(inspected["baselineSnapshotHash"], rig.hash);
    assert!(inspected["snapshot"]["components"].as_array().is_some());

    // 提议：候选补丁被校验并持久化，不产生计划与审批。
    let proposed = outcome(&rig.call(4, "eda_propose_patch", &rig.patch())).unwrap();
    assert_eq!(proposed["status"], "recorded");
    assert_eq!(proposed["write"], "not_run");
    let candidate_hash = proposed["candidateHash"].as_str().unwrap().to_owned();
    assert!(
        observation::observations_dir(&rig.root)
            .join(format!("candidate-{}.json", &candidate_hash[7..]))
            .is_file()
    );
    let storage = ProjectStorage::open(&rig.root).unwrap();
    assert!(storage.list_change_sets().unwrap().is_empty(), "提议不得创建 ChangeSet");

    // 请求预览：生成计划与 ChangeSet；write/readback 保持 not_run，无审批。
    let preview =
        outcome(&rig.call(5, "eda_request_preview", &json!({"candidateHash": candidate_hash})))
            .unwrap();
    assert_eq!(preview["type"], "preview_result");
    assert_eq!(preview["baselineSnapshotHash"], rig.hash);
    assert_eq!(preview["approvalAllowed"], true);
    assert_eq!(preview["write"], "not_run");
    assert_eq!(preview["readback"], "not_run");
    assert_eq!(preview["verification"], "inconclusive");
    let record = &storage.list_change_sets().unwrap()[0];
    assert_eq!(record.plan_hash, preview["planHash"]);
    assert_eq!(
        record.audit.iter().map(|entry| entry.decision.as_str()).collect::<Vec<_>>(),
        ["preview"]
    );
    assert_eq!(record.execution, ChangeSetExecutionReport::default());

    // 工具请求全部进入会话审计。
    let body = storage.load_session(SESSION).unwrap().body;
    for marker in ["eda_inspect：", "eda_propose_patch：", "eda_request_preview："] {
        assert!(body.contains(marker), "审计缺少 {marker}：\n{body}");
    }

    // 回合后通知：只含本回合为该会话新建的预览。
    let before: BTreeSet<String> = BTreeSet::new();
    let created = agent_tools::previews_created_since(&rig.root, SESSION, &before).unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0]["planHash"], preview["planHash"]);
    assert_eq!(created[0]["write"], "not_run");
    let already: BTreeSet<String> =
        [created[0]["changeSetId"].as_str().unwrap().to_owned()].into_iter().collect();
    assert!(agent_tools::previews_created_since(&rig.root, SESSION, &already).unwrap().is_empty());
}

#[test]
fn component_removal_without_compensation_data_is_approvable_with_a_rollback_warning() {
    let rig = Rig::new();
    let patch = json!({"base_snapshot_hash": rig.hash,
        "operations": [{"remove_component": {"component_id": "component:R1"}}],
        "rationale": "只移除指定元件，其他对象保留"});
    let candidate = outcome(&rig.call(1, "eda_propose_patch", &patch)).unwrap();
    let preview = outcome(&rig.call(
        2,
        "eda_request_preview",
        &json!({"candidateHash": candidate["candidateHash"]}),
    ))
    .unwrap();
    assert!(preview["planHash"].as_str().is_some_and(|hash| hash.starts_with("sha256:")));
    // 夹具缺少删除补偿所需的库身份：不再阻止批准，而是给出醒目的回滚提醒。
    assert_eq!(preview["approvalAllowed"], true);
    assert_eq!(preview["plan"]["rollbackable"], false);
    assert!(
        preview["plan"]["rollbackWarnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|w| w.as_str().unwrap().contains("R1")))
    );
    assert_eq!(preview["write"], "not_run");
    assert_eq!(preview["plan"]["steps"][0]["kind"], "delete_component");
    assert_eq!(preview["plan"]["steps"][0]["logicalId"], "component:R1");
    assert!(preview["plan"]["risks"].as_array().is_some_and(|risks| !risks.is_empty()));
    let change_sets = ProjectStorage::open(&rig.root).unwrap().list_change_sets().unwrap();
    assert_eq!(change_sets.len(), 1);
    assert!(
        change_sets[0].audit.iter().all(|entry| entry.decision != "approved"),
        "预览不生成审批"
    );
    assert_eq!(change_sets[0].execution, ChangeSetExecutionReport::default());
}

#[test]
fn tool_boundary_rejects_raw_apis_stale_baselines_and_unknown_references() {
    let rig = Rig::new();
    // 原始 EDA API 形状的操作被拒绝，并逐个定位错误。
    let error = outcome(&rig.call(
        1,
        "eda_propose_patch",
        &json!({"base_snapshot_hash": rig.hash,
            "operations": [{"eda_api": {"method": "delete"}}], "rationale": "r"}),
    ))
    .unwrap_err();
    assert!(error.contains("不支持的操作类型 eda_api"), "{error}");

    // 过期基线被拒绝。
    let stale = format!("sha256:{}", "0".repeat(64));
    let error = outcome(&rig.call(
        2,
        "eda_propose_patch",
        &json!({"base_snapshot_hash": stale,
            "operations": [{"annotate_semantic": {"key": "k", "value": "v"}}], "rationale": "r"}),
    ))
    .unwrap_err();
    assert!(error.contains("最新观测快照不一致"), "{error}");

    // 不存在或畸形的候选引用被拒绝，且哈希不会被解释为路径。
    let error =
        outcome(&rig.call(3, "eda_request_preview", &json!({"candidateHash": stale}))).unwrap_err();
    assert!(error.contains("候选补丁不存在"), "{error}");
    for bad in ["../outside", "sha256:../../outside", "not-a-hash"] {
        assert!(
            outcome(&rig.call(4, "eda_request_preview", &json!({"candidateHash": bad}))).is_err()
        );
    }

    // 工具面没有第四个工具：写入/审批/回滚/原始 API 都不可达。
    for tool in ["eda_apply", "eda_approve", "eda_rollback", "eda_api", "apply"] {
        let error = outcome(&rig.call(5, tool, &json!({}))).unwrap_err();
        assert!(error.contains("未知工具"), "{tool}: {error}");
    }
}

#[test]
fn candidates_are_bound_to_their_project_and_session() {
    let rig = Rig::new();
    let proposed = outcome(&rig.call(1, "eda_propose_patch", &rig.patch())).unwrap();
    let candidate_hash = proposed["candidateHash"].as_str().unwrap();
    // 另一个会话（哪怕是同一项目）不能凭引用请求预览。
    let other = agent_tools::ToolContext::new(rig.config_path.clone(), PROJECT.into(), "s2".into());
    let mut output = Vec::new();
    let frame = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"eda_request_preview","arguments":{"candidateHash":candidate_hash}}});
    agent_tools::serve(BufReader::new(format!("{frame}\n").as_bytes()), &mut output, &other)
        .unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    let error = outcome(&reply).unwrap_err();
    assert!(error.contains("属于其他项目或会话"), "{error}");
}

#[test]
fn runtime_restart_recovers_candidates_and_plans_from_stored_references() {
    let rig = Rig::new();
    let proposed = outcome(&rig.call(1, "eda_propose_patch", &rig.patch())).unwrap();
    let candidate_hash = proposed["candidateHash"].as_str().unwrap().to_owned();
    let preview =
        outcome(&rig.call(2, "eda_request_preview", &json!({"candidateHash": candidate_hash})))
            .unwrap();
    let plan_hash = preview["planHash"].as_str().unwrap().to_owned();
    // 模拟运行时重启：丢掉全部内存状态，仅凭磁盘上的引用恢复。
    let storage = ProjectStorage::open(&rig.root).unwrap();
    let restored =
        outcome(&rig.call(3, "eda_request_preview", &json!({"candidateHash": candidate_hash})))
            .unwrap();
    assert_eq!(restored["planHash"], plan_hash, "同一候选重启后恢复同一计划");
    // 重复预览不重置既有审计。
    let record = &storage.list_change_sets().unwrap()[0];
    assert_eq!(record.audit.len(), 1);
}

#[test]
fn previews_created_since_filters_other_sessions() {
    let rig = Rig::new();
    outcome(&rig.call(1, "eda_propose_patch", &rig.patch())).unwrap();
    let before: BTreeSet<String> = BTreeSet::new();
    // 另一会话的预览不会进入本会话的回合通知。
    let other = agent_tools::ToolContext::new(rig.config_path.clone(), PROJECT.into(), "s2".into());
    let mut output = Vec::new();
    let frame = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"eda_request_preview","arguments": rig.patch()}});
    agent_tools::serve(BufReader::new(format!("{frame}\n").as_bytes()), &mut output, &other)
        .unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    outcome(&reply).unwrap();
    assert!(
        agent_tools::previews_created_since(&rig.root, SESSION, &before).unwrap().is_empty(),
        "其他会话的计划不得转发到本会话"
    );
    let for_other = agent_tools::previews_created_since(&rig.root, "s2", &before).unwrap();
    assert_eq!(for_other.len(), 1);
}

#[test]
fn no_observation_means_inspect_reports_an_actionable_error() {
    let dir = common::temp_dir();
    let config_path = common::write_settings(&dir, "127.0.0.1:1");
    common::write_registry(&dir, PROJECT);
    let root = dir.join("project-root");
    std::fs::create_dir_all(&root).unwrap();
    ProjectStorage::create(
        &root,
        Project { id: PROJECT.into(), name: "空".into(), description: None },
    )
    .unwrap();
    let context = agent_tools::ToolContext::new(config_path, PROJECT.into(), SESSION.into());
    let mut output = Vec::new();
    let frame = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"eda_inspect","arguments":{}}});
    agent_tools::serve(BufReader::new(format!("{frame}\n").as_bytes()), &mut output, &context)
        .unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    let error = outcome(&reply).unwrap_err();
    assert!(error.contains("inspect"), "{error}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn inspect_explains_legacy_missing_designators_and_identifies_cached_observations() {
    let rig = Rig::new();
    let path = rig.root.join("observations").join(format!("{}.json", &rig.hash[7..]));
    let mut record: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut frame = record["observedPayload"]["components"][0].clone();
    frame["designator"] = Value::Null;
    frame["name"] = json!("Drawing-Symbol_A4");
    record["observedPayload"]["components"].as_array_mut().unwrap().push(frame);
    record["observedPayload"]["inconclusive"] = json!(["components"]);
    record["inconclusive"] = json!(["components"]);
    // 模拟旧版没有 diagnostics 的持久化记录，读取时只生成说明，不修改磁盘基线。
    observation::persist_inspect_record(&rig.root, &record).unwrap();
    let inspected = outcome(&rig.call(1, "eda_inspect", &json!({}))).unwrap();
    assert_eq!(inspected["observationSource"], "persisted");
    assert_eq!(inspected["inconclusive"], json!(["components"]));
    assert_eq!(inspected["diagnostics"][0]["code"], "missing_designator");
    assert_eq!(inspected["diagnostics"][0]["examples"], json!(["Drawing-Symbol_A4"]));
    assert!(inspected["observationNote"].as_str().unwrap().contains("不是新调用 EDA"));
    assert!(inspected["validation"].is_array());
    let reread = jlcircuit_eda_bridge::review::load_baseline(&rig.root, &rig.hash).unwrap();
    assert!(reread.get("diagnostics").is_none(), "诊断回退不得篡改旧记录");
}

/// 真实子进程：Codex 运行时拉起工具服务器的方式（`--agent-tools` 模式）。
#[test]
fn agent_tools_binary_serves_the_loop_over_stdio() {
    let rig = Rig::new();
    let mut child = Command::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge"))
        .arg(&rig.config_path)
        .arg(agent_tools::MODE_FLAG)
        .arg("--project")
        .arg(PROJECT)
        .arg("--session")
        .arg(SESSION)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the agent tools server");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut exchange = |frame: &Value| -> Value {
        writeln!(stdin, "{frame}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let initialized = exchange(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
    assert_eq!(initialized["result"]["serverInfo"]["name"], "circuitfabric-jlc-eda-tools");
    let listed = exchange(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 3);
    let inspected = exchange(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"eda_inspect","arguments":{}}}));
    assert_eq!(outcome(&inspected).unwrap()["baselineSnapshotHash"], rig.hash);
    let proposed = exchange(&json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"eda_propose_patch","arguments": rig.patch()}}));
    let candidate = outcome(&proposed).unwrap()["candidateHash"].as_str().unwrap().to_owned();
    let preview = exchange(&json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
        "params":{"name":"eda_request_preview","arguments":{"candidateHash": candidate}}}));
    assert_eq!(outcome(&preview).unwrap()["approvalAllowed"], true);
    drop(stdin);
    assert!(child.wait().unwrap().success());

    // 参数形状非法时拒绝启动，绝不退化为监听模式。
    let output = Command::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge"))
        .arg(&rig.config_path)
        .arg(agent_tools::MODE_FLAG)
        .output()
        .expect("spawn with malformed arguments");
    assert!(!output.status.success());
}

/// Codex 运行时注入断言（Windows PowerShell 夹具）：应用服务器命令行上只出现
/// 经授权的工具服务器，工具恰好三个、环境变量为空；重启后按已存储绑定恢复会话。
#[cfg(windows)]
#[test]
fn codex_runtime_registers_only_the_authorized_agent_tools() {
    use circuitfabric_codex_runtime::{
        RuntimeSettings, ToolAuthorizationSettings,
        execution::{Cancellation, run_session_observed},
    };

    let rig = Rig::new();
    let launcher = rig.dir.join("mock.cmd");
    std::fs::write(
        &launcher,
        "@echo %* >> \"%~dp0codex-argv.txt\"\r\n@powershell -NoProfile -NonInteractive -File \"%~dp0mock.ps1\"\r\n",
    )
    .unwrap();
    std::fs::write(
        rig.dir.join("mock.ps1"),
        r#"
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$out = [Console]::Out
$history = Join-Path $env:CODEX_HOME 'mock-history.json'
$trace = Join-Path $env:CODEX_HOME 'mock-rpc.txt'
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $m = $line | ConvertFrom-Json
  Add-Content -LiteralPath $trace -Value $m.method
  if ($m.method -eq 'initialize') { $out.WriteLine('{"id":' + $m.id + ',"result":{}}') }
  elseif ($m.method -eq 'thread/start') {
    '{"turns":0}' | Set-Content -LiteralPath $history
    $out.WriteLine('{"id":' + $m.id + ',"result":{"thread":{"id":"persistent-thread"}}}')
  }
  elseif ($m.method -eq 'thread/resume') {
    if (!(Test-Path -LiteralPath $history)) { throw 'history missing' }
    if ($m.params.threadId -ne 'persistent-thread' -or $m.params.sandbox -ne 'read-only') { throw 'wrong identity or scope' }
    $out.WriteLine('{"id":' + $m.id + ',"result":{"thread":{"id":"persistent-thread"}}}')
  }
  elseif ($m.method -eq 'turn/start') {
    $state = Get-Content -LiteralPath $history -Raw | ConvertFrom-Json
    $state.turns += 1
    $state | ConvertTo-Json | Set-Content -LiteralPath $history
    $out.WriteLine('{"id":' + $m.id + ',"result":{"turn":{"id":"turn"}}}')
    $out.WriteLine('{"method":"item/agentMessage/delta","params":{"delta":"done"}}')
    $out.WriteLine('{"method":"turn/completed","params":{"threadId":"persistent-thread","turn":{"id":"turn","status":"completed"}}}')
  }
  $out.Flush()
}
"#,
    )
    .unwrap();

    let mut settings = RuntimeSettings::default();
    settings.codex.command = launcher.to_string_lossy().into_owned();
    // 复用真实会话的注入流程：项目通用 MCP 清单为空时仍提供三个内置工具。
    let grants = agent_tools::inject(
        &mut settings,
        &ToolAuthorizationSettings::default(),
        &rig.config_path,
        PROJECT,
        SESSION,
    )
    .unwrap();
    // 测试进程不是 Bridge：用构建出的真实服务器替换 current_exe。
    settings
        .catalog
        .mcp_servers
        .iter_mut()
        .find(|server| server.id == agent_tools::AGENT_TOOLS_SERVER_ID)
        .unwrap()
        .command = env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge").into();
    let secrets = {
        let mut vault = circuitfabric_codex_runtime::secrets::UnlockedVault::create(
            &rig.dir.join("secrets.vault.json"),
            "test-password",
        )
        .unwrap();
        vault.set(&settings.providers[0].api_key_environment_variable, "mock-key-value").unwrap();
        vault.values().clone()
    };
    let history = rig.dir.join("session-history");
    for prompt in ["first turn", "second turn"] {
        run_session_observed(
            &settings,
            &grants,
            prompt,
            None,
            &rig.root,
            &history,
            Some(&secrets),
            &Cancellation::default(),
            &mut |_, _| {},
            &mut |_| {},
        )
        .unwrap();
    }

    let argv = std::fs::read_to_string(rig.dir.join("codex-argv.txt")).unwrap();
    assert!(argv.contains("mcp_servers.jlc-eda-tools.enabled_tools="), "{argv}");
    for tool in ["eda_inspect", "eda_propose_patch", "eda_request_preview"] {
        assert!(argv.contains(tool), "缺少工具 {tool}：{argv}");
    }
    assert_eq!(argv.matches("approval_mode").count(), 2 * 3, "每个工具各一次、共两轮：{argv}");
    assert!(
        argv.contains("mcp_servers.jlc-eda-tools.env_vars=[]"),
        "工具进程不得携带环境变量：{argv}"
    );
    assert!(argv.contains("mcp_servers.jlc-eda-tools.required=true"), "{argv}");
    assert!(!argv.contains("mcp_servers.typesafe-jev"), "未授权服务器不得登记：{argv}");
    // 凭据只以变量名出现，值永远不进命令行。
    assert!(!argv.contains("mock-key-value"), "{argv}");

    // 第二次运行凭 circuitfabric-thread.json 的已存储绑定恢复，不另开线程。
    let trace = std::fs::read_to_string(history.join("mock-rpc.txt")).unwrap();
    assert_eq!(trace.matches("thread/start").count(), 1, "{trace}");
    assert_eq!(trace.matches("thread/resume").count(), 1, "{trace}");
    let binding = std::fs::read_to_string(history.join("circuitfabric-thread.json")).unwrap();
    assert!(binding.contains("jlc-eda-tools"), "{binding}");
    assert!(!binding.contains("mock-key-value"), "{binding}");
}

/// 从真实 /bridge chat 入口验证空项目 MCP 清单，不让测试手工授权掩盖接线错误。
#[cfg(windows)]
#[test]
fn chat_injects_builtin_tools_without_generic_project_mcp_selections() {
    use circuitfabric_codex_runtime::{ProviderConnection, RuntimeSettings};
    use jlcircuit_eda_bridge::supervision::BridgeProcessHandle;
    use std::time::Duration;

    let rig = Rig::new();
    let launcher = rig.dir.join("chat-mock.cmd");
    std::fs::write(&launcher,
        "@echo %* >> \"%~dp0chat-argv.txt\"\r\n@powershell -NoProfile -NonInteractive -File \"%~dp0chat-mock.ps1\"\r\n").unwrap();
    std::fs::write(rig.dir.join("chat-mock.ps1"), r#"
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$out = [Console]::Out
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $m = $line | ConvertFrom-Json
  if ($m.method -eq 'initialize') { $out.WriteLine('{"id":' + $m.id + ',"result":{}}') }
  elseif ($m.method -eq 'thread/start') {
    $out.WriteLine('{"id":' + $m.id + ',"result":{"thread":{"id":"eda-chat-thread"}}}')
  }
  elseif ($m.method -eq 'turn/start') {
    $out.WriteLine('{"id":' + $m.id + ',"result":{"turn":{"id":"turn"}}}')
    $out.WriteLine('{"method":"item/agentMessage/delta","params":{"delta":"done"}}')
    $out.WriteLine('{"method":"turn/completed","params":{"threadId":"eda-chat-thread","turn":{"id":"turn","status":"completed"}}}')
  }
  $out.Flush()
}
"#).unwrap();
    let mut settings = RuntimeSettings::load_or_default(&rig.config_path).unwrap();
    settings.bridge.listen_address = common::free_loopback_address();
    settings.codex.command = launcher.to_string_lossy().into_owned();
    // 只运行协议夹具，不读凭据、不请求模型端点。
    settings.providers[0].connection = ProviderConnection::CodexAccount;
    settings.save(&rig.config_path).unwrap();
    let runtime_before = std::fs::read(&rig.config_path).unwrap();
    let storage = ProjectStorage::open(&rig.root).unwrap();
    let configuration_before = std::fs::read(storage.configuration_path()).unwrap();
    assert!(storage.load_configuration().unwrap().enabled_mcp_server_ids.is_empty());
    let mut bridge = BridgeProcessHandle::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_circuitfabric-jlc-bridge")),
        &rig.config_path,
    )
    .unwrap();
    common::wait_until_listening(&settings.bridge.listen_address, Duration::from_secs(10));
    let mut client = common::WsClient::connect(&settings.bridge.listen_address, "/bridge");
    assert_eq!(client.exchange(&json!({"type":"hello"}))["type"], "hello_ack");
    assert_eq!(
        client.exchange(&json!({"type":"select_project","projectId":PROJECT}))["type"],
        "project_selected"
    );
    client.send(&json!({"type":"chat","sessionId":SESSION,"text":"preview removing R1"}));
    assert_eq!(client.recv()["type"], "chat_started");
    let answer = client.recv();
    assert_eq!(answer["type"], "chat_delta", "聊天失败：{answer}");
    assert_eq!(client.recv()["type"], "chat_completed");
    let argv = std::fs::read_to_string(rig.dir.join("chat-argv.txt")).unwrap();
    for tool in ["eda_inspect", "eda_propose_patch", "eda_request_preview"] {
        assert!(argv.contains(tool), "实际聊天命令行缺少 {tool}: {argv}");
    }
    let binding: Value = serde_json::from_slice(
        &std::fs::read(
            storage.session_runtime_directory(SESSION).unwrap().join("circuitfabric-thread.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        binding["scope"]["grants"]["authorized_mcp_server_ids"],
        json!([agent_tools::AGENT_TOOLS_SERVER_ID])
    );
    assert_eq!(binding["scope"]["servers"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(&rig.config_path).unwrap(), runtime_before);
    assert_eq!(std::fs::read(storage.configuration_path()).unwrap(), configuration_before);
    bridge.stop().unwrap();
}
