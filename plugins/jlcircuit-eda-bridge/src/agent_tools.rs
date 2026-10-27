//! 智能体受控工具面：模型在 EDA 会话中只能通过 `eda_inspect`、
//! `eda_propose_patch`、`eda_request_preview` 三个 MCP 工具接触设计。
//!
//! 工具服务器是 Bridge 二进制以 `--agent-tools` 模式重启的子进程，由 Codex
//! 运行时按已授权 MCP 服务器拉起（环境变量白名单为空，不携带任何凭据）。
//! 三个工具都只读写 `CircuitFabric` 项目存储：不连接执行器通道、不调用原始
//! EDA API、不产生审批，写入与回读永远由操作员经 `preview`/`approve_plan`/`apply`
//! 通道完成。每次工具请求与结果都进入 EDA 会话审计；候选补丁与计划持久化在
//! `observations/` 下，运行时重启后可仅凭这些已存储引用恢复任务。

use std::{
    collections::BTreeSet,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use circuitfabric_codex_runtime::{
    RuntimeSettings, ToolAuthorizationSettings, tools::McpServerDefinition,
};
use circuitfabric_project::{
    ChangeSetStageStatus, ProjectRegistry, ProjectStorage, ProjectStorageError, SessionCategory,
    SessionEvent, SessionEventKind, SessionSeed,
};
use serde_json::{Value, json};

use crate::{observation, plan, review};

/// 注入 Codex 运行时的 MCP 服务器标识（保留名，目录中用户条目会被覆盖）。
pub const AGENT_TOOLS_SERVER_ID: &str = "jlc-eda-tools";

/// Bridge 二进制进入工具服务器模式的命令行标志。
pub const MODE_FLAG: &str = "--agent-tools";

const EDA_INSPECT: &str = "eda_inspect";
const EDA_PROPOSE_PATCH: &str = "eda_propose_patch";
const EDA_REQUEST_PREVIEW: &str = "eda_request_preview";

/// 向运行时目录登记并授权工具服务器（仅内存，不落盘用户设置）。
///
/// `command` 是工具服务器可执行文件（生产环境为 Bridge 二进制本身）。
#[must_use]
pub fn server_definition(
    command: &Path,
    config_path: &Path,
    project_id: &str,
    session_id: &str,
) -> McpServerDefinition {
    McpServerDefinition {
        id: AGENT_TOOLS_SERVER_ID.to_owned(),
        display_name: "CircuitFabric EDA 受控工具".to_owned(),
        command: command.to_string_lossy().into_owned(),
        args: vec![
            config_path.to_string_lossy().into_owned(),
            MODE_FLAG.to_owned(),
            "--project".to_owned(),
            project_id.to_owned(),
            "--session".to_owned(),
            session_id.to_owned(),
        ],
        // 关键边界：不声明任何环境变量，工具进程拿不到 Provider 密钥或保险库内容。
        environment_variables: Vec::new(),
        enabled: true,
    }
}

/// 登记会话内置工具，并返回经过项目筛选的实际授权（仅内存副本）。
///
/// 三个 EDA 工具属于本会话的受控后端，不要求用户在通用 MCP 清单里重复勾选。
/// 只补入这个保留服务器；其他 MCP 和 skill 仍按全局与项目授权取交集。
/// # Errors
/// 当前可执行文件路径不可用时返回错误。
pub fn inject(
    settings: &mut RuntimeSettings,
    project_grants: &ToolAuthorizationSettings,
    config_path: &Path,
    project_id: &str,
    session_id: &str,
) -> Result<ToolAuthorizationSettings, String> {
    let command = std::env::current_exe().map_err(|error| error.to_string())?;
    let definition = server_definition(&command, config_path, project_id, session_id);
    definition_command_is_safe(&definition)?;
    settings.catalog.mcp_servers.retain(|server| server.id != AGENT_TOOLS_SERVER_ID);
    settings.catalog.mcp_servers.push(definition);
    let grants = &mut settings.tools.authorized_mcp_server_ids;
    if !grants.iter().any(|id| id == AGENT_TOOLS_SERVER_ID) {
        grants.push(AGENT_TOOLS_SERVER_ID.to_owned());
    }
    let mut session_grants = project_grants.clone();
    if !session_grants.authorized_mcp_server_ids.iter().any(|id| id == AGENT_TOOLS_SERVER_ID) {
        session_grants.authorized_mcp_server_ids.push(AGENT_TOOLS_SERVER_ID.to_owned());
    }
    Ok(settings.catalog.effective_grants(&settings.tools, Some(&session_grants)))
}

/// 注入前按目录规则校验定义，避免把异常参数带进运行时配置。
fn definition_command_is_safe(definition: &McpServerDefinition) -> Result<(), String> {
    let mut catalog = circuitfabric_codex_runtime::tools::ToolCatalog::default();
    catalog.mcp_servers.push(definition.clone());
    catalog.validate_secret_references(None).map_err(|error| error.to_string())
}

/// 一次 EDA 会话内工具调用的绑定上下文。
pub struct ToolContext {
    config_path: PathBuf,
    project_id: String,
    session_id: String,
}

impl ToolContext {
    #[must_use]
    pub fn new(config_path: PathBuf, project_id: String, session_id: String) -> Self {
        Self { config_path, project_id, session_id }
    }

    /// 项目根 = 注册表 `root_for`，且 manifest 项目 ID 必须一致（与 Bridge
    /// 会话通道同一规则）。
    fn root(&self) -> Result<PathBuf, String> {
        let registry =
            ProjectRegistry::load_or_default(self.config_path.with_file_name("projects.json"))
                .map_err(|error| error.to_string())?;
        let root = registry
            .root_for(&self.project_id)
            .ok_or_else(|| "项目未注册".to_owned())?
            .to_path_buf();
        let storage = ProjectStorage::open(&root).map_err(|error| error.to_string())?;
        if storage.manifest().project.id != self.project_id {
            return Err("项目注册信息与项目目录不匹配".into());
        }
        Ok(root)
    }

    /// 工具请求与结果进入 EDA 会话审计；会话缺失时按会话通道同款逻辑补建。
    fn audit(&self, text: &str, unix_seconds: u64) -> Result<(), String> {
        let root = self.root()?;
        let storage = ProjectStorage::open(&root).map_err(|error| error.to_string())?;
        match storage.load_session(&self.session_id) {
            Ok(replay) if replay.metadata.category == SessionCategory::Eda => {}
            Ok(_) => return Err("此会话不属于 EDA 类别".into()),
            Err(ProjectStorageError::SessionNotFound { .. }) => {
                storage
                    .start_categorized_session(
                        SessionSeed {
                            session_id: self.session_id.clone(),
                            runtime_profile_id: AGENT_TOOLS_SERVER_ID.to_owned(),
                            backend_id: Some("codex".to_owned()),
                        },
                        SessionCategory::Eda,
                        None,
                    )
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
        storage
            .append_session_event(
                &self.session_id,
                &SessionEvent {
                    timestamp_unix_seconds: unix_seconds,
                    kind: SessionEventKind::Note { text: text.to_owned() },
                },
            )
            .map_err(|error| error.to_string())
    }

    /// `eda_inspect`：读取最新持久化观测快照的引用与语义内容；不连接执行器。
    fn inspect(&self, unix_seconds: u64) -> Result<Value, String> {
        let root = self.root()?;
        let Some(context) = review::design_context(&root, &self.project_id)? else {
            return Err(
                "当前没有已持久化的观测快照；请让用户在 EDA 扩展中执行 inspect 后再试。".to_owned()
            );
        };
        self.audit(
            &format!("eda_inspect：baselineSnapshotHash={}", context["baselineSnapshotHash"]),
            unix_seconds,
        )?;
        Ok(context)
    }

    /// `eda_propose_patch`：校验并记录候选 IRPatch；不生成计划、不触碰 EDA。
    fn propose_patch(&self, arguments: &Value, unix_seconds: u64) -> Result<Value, String> {
        let root = self.root()?;
        let patch = review::parse_patch(arguments.clone())?;
        let latest =
            observation::latest_inspect_snapshot_hash(&root).map_err(|error| error.to_string())?;
        if latest.as_deref() != Some(patch.base_snapshot_hash.as_str()) {
            return Err(
                "base_snapshot_hash 与最新观测快照不一致；请先调用 eda_inspect 获取当前基线。"
                    .to_owned(),
            );
        }
        let record = review::load_baseline(&root, &patch.base_snapshot_hash)?;
        if record["projectId"] != self.project_id {
            return Err("观测不属于当前项目。".into());
        }
        // 纯内存身份/拓扑校验；让模型在请求预览前就能自我纠正。
        let target = review::target_snapshot(&record, &patch)?;
        let candidate_hash = persist_candidate(&root, self, &patch, unix_seconds)?;
        self.audit(
            &format!(
                "eda_propose_patch：candidateHash={candidate_hash}，base={}，ops={}，target={}",
                patch.base_snapshot_hash,
                patch.operations.len(),
                target.snapshot_hash
            ),
            unix_seconds,
        )?;
        Ok(json!({
            "status": "recorded",
            "candidateHash": candidate_hash,
            "baseSnapshotHash": patch.base_snapshot_hash,
            "targetSnapshotHash": target.snapshot_hash,
            "operations": patch.operations.len(),
            "write": "not_run",
            "readback": "not_run",
            "verification": "inconclusive",
            "note": "候选补丁已校验并记录；尚未生成计划，更未写入 EDA。用 eda_request_preview 请求预览。",
        }))
    }

    /// `eda_request_preview`：请求受控后端生成物化计划；不写入 EDA、不审批。
    fn request_preview(&self, arguments: &Value, unix_seconds: u64) -> Result<Value, String> {
        let root = self.root()?;
        let patch_value = if let Some(hash) = arguments.get("candidateHash").and_then(Value::as_str)
        {
            let record = load_candidate(&root, hash)?;
            if record["projectId"] != self.project_id || record["sessionId"] != self.session_id {
                return Err("候选补丁属于其他项目或会话。".into());
            }
            record["patch"].clone()
        } else {
            arguments.clone()
        };
        let patch = review::parse_patch(patch_value.clone())?;
        // review::preview 重新校验基线新鲜度、文档绑定与候选 IR，并持久化
        // 计划与 ChangeSet（重复预览不重置既有审批）。
        let mut reply = review::preview(
            &root,
            &self.project_id,
            &self.session_id,
            &json!({"baselineSnapshotHash": patch.base_snapshot_hash, "patch": patch_value}),
            unix_seconds,
        )?;
        reply["sessionId"] = json!(self.session_id);
        self.audit(
            &format!(
                "eda_request_preview：planHash={}，changeSetId={}，approvalAllowed={}；审批由操作员决定。",
                reply["planHash"], reply["changeSetId"], reply["approvalAllowed"]
            ),
            unix_seconds,
        )?;
        Ok(reply)
    }
}

/// 候选补丁的内容寻址：项目、会话与补丁本体一起摘要。
fn candidate_digest(context: &ToolContext, patch: &circuitfabric_contracts::IrPatch) -> String {
    let canonical = json!({
        "kind": "patch_candidate",
        "projectId": context.project_id,
        "sessionId": context.session_id,
        "patch": patch,
    });
    observation::sha256_digest(canonical.to_string().as_bytes())
}

fn candidate_path(root: &Path, candidate_hash: &str) -> Result<PathBuf, String> {
    if !review::valid_hash(candidate_hash) {
        return Err("无效的 candidateHash。".into());
    }
    Ok(observation::observations_dir(root).join(format!("candidate-{}.json", &candidate_hash[7..])))
}

fn persist_candidate(
    root: &Path,
    context: &ToolContext,
    patch: &circuitfabric_contracts::IrPatch,
    unix_seconds: u64,
) -> Result<String, String> {
    let candidate_hash = candidate_digest(context, patch);
    let record = json!({
        "schemaVersion": 1,
        "kind": "patch_candidate",
        "projectId": context.project_id,
        "sessionId": context.session_id,
        "candidateHash": candidate_hash,
        "baseSnapshotHash": patch.base_snapshot_hash,
        "patch": patch,
        "receivedAtUnixSeconds": unix_seconds,
    });
    let file_path = candidate_path(root, &candidate_hash)?;
    observation::write_atomic(
        &file_path,
        &serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(candidate_hash)
}

/// 按已存储引用恢复候选补丁；内容与绑定逐字段复核（与 `load_baseline` 同级）。
fn load_candidate(root: &Path, candidate_hash: &str) -> Result<Value, String> {
    let file_path = candidate_path(root, candidate_hash)?;
    let record: Value = serde_json::from_slice(
        &std::fs::read(&file_path).map_err(|_| "候选补丁不存在或已不可读。".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    if record["kind"] != "patch_candidate" || record["candidateHash"] != candidate_hash {
        return Err("候选补丁记录与引用不匹配。".into());
    }
    let context = ToolContext::new(
        PathBuf::new(),
        record["projectId"].as_str().unwrap_or_default().to_owned(),
        record["sessionId"].as_str().unwrap_or_default().to_owned(),
    );
    let parsed = review::parse_patch(record["patch"].clone())?;
    if candidate_digest(&context, &parsed) != candidate_hash {
        return Err("候选补丁内容摘要与引用不匹配。".into());
    }
    Ok(record)
}

/// 回合结束后转发给会话通道的预览：本回合内为该会话新建的 `ChangeSet`。
/// 重复预览同一计划不会重置审计，因此也不会重复通知。
/// # Errors
/// 读取 `ChangeSet` 或计划失败时返回错误。
pub fn previews_created_since(
    root: &Path,
    session_id: &str,
    before: &BTreeSet<String>,
) -> Result<Vec<Value>, String> {
    let storage = ProjectStorage::open(root).map_err(|error| error.to_string())?;
    let mut created: Vec<_> = storage
        .list_change_sets()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|record| record.id.starts_with("jlc-") && !before.contains(&record.id))
        .collect();
    created.sort_by(|left, right| left.id.cmp(&right.id));
    let mut replies = Vec::with_capacity(created.len());
    for record in created {
        let Some(plan) = plan::load_plan(root, &record.plan_hash).map_err(|e| e.to_string())?
        else {
            return Err("ChangeSet 缺少对应的持久化计划。".into());
        };
        if plan.session_id != session_id {
            continue;
        }
        replies.push(json!({
            "type": "preview_result",
            "ok": true,
            "planHash": record.plan_hash,
            "baselineSnapshotHash": record.base_snapshot_hash,
            "changeSetId": record.id,
            "plan": plan,
            "approvalAllowed": record.preview_validation.status == ChangeSetStageStatus::Passed,
            "write": record.execution.write.status,
            "readback": record.execution.readback.status,
            "verification": record.execution.verification.status,
        }));
    }
    Ok(replies)
}

fn tool_schemas() -> Value {
    let patch_properties = json!({
        "base_snapshot_hash": {"type": "string", "description": "必须等于 eda_inspect 返回的最新 baselineSnapshotHash。"},
        "operations": {"type": "array", "description": "外标签单键操作对象：set_component_value / rename_net / add_component / remove_component / add_net / annotate_semantic。"},
        "rationale": {"type": "string", "description": "面向审批者的变更理由。"},
    });
    json!([
        {
            "name": EDA_INSPECT,
            "description": "读取最近已保存的观测引用（baselineSnapshotHash）、语义快照、不确定面、diagnostics 和 validation。不会重新采集 EDA；读取缓存不会补齐缺失字段。按诊断说明原因，不得从元件 evidence 为空推断失败。需要新采集时请用户点击扩展里的观测当前设计。",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        },
        {
            "name": EDA_PROPOSE_PATCH,
            "description": "提交候选语义 IRPatch 并针对当前观测基线校验。只记录候选，绝不写入 EDA、不生成审批。返回 candidateHash 供 eda_request_preview 引用。",
            "inputSchema": {
                "type": "object",
                "required": ["base_snapshot_hash", "operations", "rationale"],
                "properties": patch_properties,
                "additionalProperties": false,
            },
        },
        {
            "name": EDA_REQUEST_PREVIEW,
            "description": "请求受控后端为已校验候选生成物化计划（preview）。preview 不写入 EDA；计划必须由操作员显式批准后才会进入任何写入。可传 candidateHash，或内联 base_snapshot_hash/operations/rationale。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "candidateHash": {"type": "string", "description": "eda_propose_patch 返回的候选引用。"},
                    "base_snapshot_hash": {"type": "string"},
                    "operations": {"type": "array"},
                    "rationale": {"type": "string"},
                },
                "additionalProperties": false,
            },
        },
    ])
}

fn tool_result(value: Result<Value, String>) -> Value {
    match value {
        Ok(payload) => json!({
            "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())}],
        }),
        Err(message) => json!({
            "content": [{"type": "text", "text": message}],
            "isError": true,
        }),
    }
}

/// 在 stdio 上以 MCP（换行分隔 JSON-RPC）服务三个受控工具；供 Bridge 二进制
/// 的 `--agent-tools` 模式使用。
/// # Errors
/// stdio 读写失败时返回错误；协议级错误按 JSON-RPC 错误应答，不中断服务。
pub fn serve_stdio(context: &ToolContext) -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(stdin.lock(), stdout.lock(), context)
}

/// JSON-RPC 主循环（与传输解耦，便于测试）：initialize/ping/tools/list/
/// tools/call，通知一律忽略，未知方法回 `-32601`。
/// # Errors
/// 读取或写入流失败时返回错误。
pub fn serve<R: BufRead, W: Write>(
    reader: R,
    mut writer: W,
    context: &ToolContext,
) -> Result<(), String> {
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = if let Ok(message) = serde_json::from_str(&line) {
            message
        } else {
            write_message(
                &mut writer,
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}}),
            )?;
            continue;
        };
        let Some(id) = message.get("id").cloned() else { continue };
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let response = match method {
            "initialize" => json!({"jsonrpc":"2.0","id":id,"result":{
                "protocolVersion":"2024-11-05",
                "capabilities":{"tools":{"listChanged":false}},
                "serverInfo":{"name":"circuitfabric-jlc-eda-tools","version":"1.0.0"},
            }}),
            "ping" => json!({"jsonrpc":"2.0","id":id,"result":{}}),
            "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":{"tools":tool_schemas()}}),
            "tools/call" => {
                let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let now =
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                let result = match name {
                    EDA_INSPECT => context.inspect(now),
                    EDA_PROPOSE_PATCH => context.propose_patch(&arguments, now),
                    EDA_REQUEST_PREVIEW => context.request_preview(&arguments, now),
                    other => Err(format!(
                        "未知工具 {other}；本服务器只提供 {EDA_INSPECT} / {EDA_PROPOSE_PATCH} / {EDA_REQUEST_PREVIEW}。"
                    )),
                };
                json!({"jsonrpc":"2.0","id":id,"result":tool_result(result)})
            }
            _ => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"method not found"}})
            }
        };
        write_message(&mut writer, &response)?;
    }
    Ok(())
}

fn write_message<W: Write>(writer: &mut W, message: &Value) -> Result<(), String> {
    writeln!(writer, "{message}").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_definition_carries_no_credentials_and_validates() {
        let definition = server_definition(
            Path::new("circuitfabric-jlc-bridge"),
            Path::new("runtime.json"),
            "p1",
            "s1",
        );
        assert_eq!(definition.id, AGENT_TOOLS_SERVER_ID);
        assert!(definition.environment_variables.is_empty());
        definition_command_is_safe(&definition).expect("定义必须通过目录校验");
    }

    #[test]
    fn inject_registers_and_authorizes_only_the_agent_tools_server() {
        let mut settings = RuntimeSettings::default();
        let project_grants = ToolAuthorizationSettings::default();
        inject(&mut settings, &project_grants, Path::new("runtime.json"), "p1", "s1").unwrap();
        assert_eq!(settings.catalog.mcp_servers.len(), 1);
        assert_eq!(
            settings.tools.authorized_mcp_server_ids,
            vec![AGENT_TOOLS_SERVER_ID.to_owned()]
        );
        // 重复注入保持幂等，不累积条目。
        inject(&mut settings, &project_grants, Path::new("runtime.json"), "p1", "s1").unwrap();
        assert_eq!(settings.catalog.mcp_servers.len(), 1);
        assert_eq!(settings.tools.authorized_mcp_server_ids.len(), 1);
        // 未授权时目录拒绝调用（fail closed）。
        let grants = circuitfabric_codex_runtime::ToolAuthorizationSettings::default();
        assert!(settings.catalog.list_tools(AGENT_TOOLS_SERVER_ID, &grants).is_err());
    }

    #[test]
    fn injected_tools_survive_an_empty_project_mcp_allowlist() {
        let mut settings = RuntimeSettings::default();
        let project_grants = ToolAuthorizationSettings::default();
        let effective =
            inject(&mut settings, &project_grants, Path::new("runtime.json"), "p1", "s1").unwrap();
        assert_eq!(effective.authorized_mcp_server_ids, [AGENT_TOOLS_SERVER_ID]);
        assert!(project_grants.authorized_mcp_server_ids.is_empty(), "不改动持久化项目授权");
        // 执行层再次取交集时也不能丢失内置工具。
        assert_eq!(settings.catalog.effective_grants(&settings.tools, Some(&effective)), effective);
    }

    #[test]
    fn session_injection_does_not_expand_other_project_permissions() {
        let mut settings = RuntimeSettings::default();
        for id in ["shared", "global-only", "project-only", "disabled"] {
            let mut server =
                server_definition(Path::new("unused"), Path::new("runtime.json"), "p1", "s1");
            server.id = id.to_owned();
            server.enabled = id != "disabled";
            settings.catalog.mcp_servers.push(server);
        }
        settings.tools.authorized_mcp_server_ids =
            vec!["shared".into(), "global-only".into(), "disabled".into()];
        let project_grants = ToolAuthorizationSettings {
            authorized_mcp_server_ids: vec![
                "shared".into(),
                "project-only".into(),
                "disabled".into(),
            ],
            ..ToolAuthorizationSettings::default()
        };
        let effective =
            inject(&mut settings, &project_grants, Path::new("runtime.json"), "p1", "s1").unwrap();
        assert_eq!(effective.authorized_mcp_server_ids, ["shared", AGENT_TOOLS_SERVER_ID]);
    }

    #[test]
    fn notifications_and_unknown_methods_follow_jsonrpc() {
        let context = ToolContext::new(PathBuf::new(), "p1".into(), "s1".into());
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"bogus/method\"}\n",
            "not json\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        );
        let mut output = Vec::new();
        serve(std::io::BufReader::new(input.as_bytes()), &mut output, &context).unwrap();
        let lines: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["error"]["code"], -32601);
        assert_eq!(lines[1]["error"]["code"], -32700);
        let names: Vec<&str> = lines[2]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, [EDA_INSPECT, EDA_PROPOSE_PATCH, EDA_REQUEST_PREVIEW]);
    }
}
