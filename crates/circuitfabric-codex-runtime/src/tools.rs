//! Tool definitions and explicit, scoped access to local skills and MCP servers.
use crate::{RuntimeError, ToolAuthorizationSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::{
    collections::BTreeSet,
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCatalog {
    #[serde(default)]
    pub skills: Vec<SkillDefinition>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub path: PathBuf,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpServerDefinition {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Names of variables inherited by the server, never their values.
    #[serde(default)]
    pub environment_variables: Vec<String>,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
const fn enabled() -> bool {
    true
}

pub(crate) fn invalid(message: impl Into<String>) -> RuntimeError {
    RuntimeError::InvalidSettings(message.into())
}

#[must_use]
pub fn valid_env_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}

impl ToolCatalog {
    /// Validate definitions without launching third-party processes.
    /// # Errors
    /// Rejects duplicate/invalid identifiers, empty commands and literal credentials.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        for ids in [
            self.skills.iter().map(|x| &x.id).collect::<Vec<_>>(),
            self.mcp_servers.iter().map(|x| &x.id).collect(),
        ] {
            let mut seen = BTreeSet::new();
            for id in ids {
                if id.is_empty()
                    || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                    || !seen.insert(id)
                {
                    return Err(invalid("工具 ID 必须唯一，且仅含字母、数字、下划线或连字符"));
                }
            }
        }
        for server in &self.mcp_servers {
            if server.command.trim().is_empty()
                || server.environment_variables.iter().any(|x| !valid_env_name(x))
            {
                return Err(invalid("MCP 需要启动命令与合法的环境变量名"));
            }
        }
        Ok(())
    }

    /// Import a skill directory or SKILL.md; duplicate identifiers update the entry.
    /// # Errors
    /// Returns file, UTF-8 or invalid identifier errors.
    pub fn import_skill(&mut self, path: &Path) -> Result<String, RuntimeError> {
        let path = if path.is_dir() { path.join("SKILL.md") } else { path.to_owned() };
        let path = fs::canonicalize(path)?;
        let text = fs::read_to_string(&path)?;
        if text.trim().is_empty() {
            return Err(invalid("SKILL.md 为空"));
        }
        let id = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("技能目录没有有效名称"))?
            .to_owned();
        let mut next = self.clone();
        next.skills.retain(|s| s.id != id);
        next.skills.push(SkillDefinition { id: id.clone(), path, enabled: true });
        next.validate()?;
        *self = next;
        Ok(id)
    }

    /// Load only explicitly authorized, enabled skills.
    /// # Errors
    /// Missing or disabled definitions and unreadable files fail closed.
    pub fn skill_instructions(
        &self,
        grants: &ToolAuthorizationSettings,
    ) -> Result<String, RuntimeError> {
        let mut result = String::new();
        for id in &grants.authorized_skill_ids {
            let skill = self
                .skills
                .iter()
                .find(|s| &s.id == id && s.enabled)
                .ok_or_else(|| invalid(format!("技能 {id} 未定义或已停用")))?;
            let text = fs::read_to_string(&skill.path)?;
            if text.len() > 256 * 1024 {
                return Err(invalid("技能文件超过 256 KiB"));
            }
            let _ = write!(result, "\n\n## Skill: {id}\n{text}");
        }
        Ok(result)
    }

    /// Discover the tools actually advertised by an authorized MCP server.
    /// # Errors
    /// Authorization, process, protocol and timeout failures are returned.
    pub fn list_tools(
        &self,
        id: &str,
        grants: &ToolAuthorizationSettings,
    ) -> Result<Value, RuntimeError> {
        self.mcp_request(id, grants, "tools/list", &json!({}))
    }

    /// Call an authorized server; re-evaluate grants at every invocation.
    /// # Errors
    /// Authorization, process, protocol and timeout failures are returned.
    pub fn call_tool(
        &self,
        id: &str,
        grants: &ToolAuthorizationSettings,
        name: &str,
        arguments: &Value,
    ) -> Result<Value, RuntimeError> {
        self.mcp_request(id, grants, "tools/call", &json!({"name":name,"arguments":arguments}))
    }

    #[allow(clippy::too_many_lines)]
    fn mcp_request(
        &self,
        id: &str,
        grants: &ToolAuthorizationSettings,
        method: &str,
        params: &Value,
    ) -> Result<Value, RuntimeError> {
        if !grants.authorized_mcp_server_ids.iter().any(|s| s == id) {
            return Err(invalid(format!("MCP {id} 未授权")));
        }
        let server = self
            .mcp_servers
            .iter()
            .find(|s| s.id == id && s.enabled)
            .ok_or_else(|| invalid(format!("MCP {id} 未定义或已停用")))?;
        let mut command = executable(&server.command);
        command.args(&server.args).env_clear();
        for name in ["PATH", "SystemRoot", "TEMP", "TMP", "HOME", "USERPROFILE"]
            .into_iter()
            .chain(server.environment_variables.iter().map(String::as_str))
        {
            if let Some(value) = env::var_os(name) {
                command.env(name, value);
            }
        }
        for name in &server.environment_variables {
            if env::var_os(name).is_none() {
                return Err(invalid(format!("缺少环境变量 {name}")));
            }
        }
        let mut child =
            command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let ownership = crate::execution::ProcessOwnership::attach(&mut child)?;
        let mut input = child.stdin.take().ok_or_else(|| invalid("MCP 未提供 stdin"))?;
        let output = child.stdout.take().ok_or_else(|| invalid("MCP 未提供 stdout"))?;
        let (sender, receiver) = mpsc::sync_channel(64);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let result = (|| {
            let mut request = |id: u64,
                               method: &str,
                               params: Value|
             -> Result<Value, RuntimeError> {
                writeln!(
                    input,
                    "{}",
                    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
                )?;
                input.flush()?;
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                loop {
                    let line = receiver
                        .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                        .map_err(|_| invalid("MCP 连接关闭或响应超时"))??;
                    let message: Value = serde_json::from_str(&line)?;
                    if message.get("method").is_some() && message.get("id").is_some() {
                        writeln!(
                            input,
                            "{}",
                            json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Client capability not supported"}})
                        )?;
                        input.flush()?;
                    } else if message["id"] == id {
                        if message.get("error").is_some() {
                            return Err(invalid("MCP 请求被拒绝"));
                        }
                        return Ok(message["result"].clone());
                    }
                }
            };
            request(
                1,
                "initialize",
                json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"CircuitFabric","version":"0.1.0"}}),
            )?;
            // End the borrow held by the request closure before sending the notification.
            writeln!(input, "{}", json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
            writeln!(input, "{}", json!({"jsonrpc":"2.0","id":2,"method":method,"params":params}))?;
            input.flush()?;
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let mut request_id = 2_u64;
            let mut tools = Vec::new();
            let mut cursors = BTreeSet::new();
            loop {
                let line = receiver
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .map_err(|_| invalid("MCP 连接关闭或响应超时"))??;
                let message: Value = serde_json::from_str(&line)?;
                if message.get("method").is_some() && message.get("id").is_some() {
                    writeln!(
                        input,
                        "{}",
                        json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Client capability not supported"}})
                    )?;
                    input.flush()?;
                } else if message["id"] == request_id {
                    if message.get("error").is_some() {
                        return Err(invalid("MCP 请求被拒绝"));
                    }
                    if method == "tools/list" {
                        let page = message["result"]["tools"]
                            .as_array()
                            .ok_or_else(|| invalid("MCP 未返回工具清单"))?;
                        tools.extend(page.iter().cloned());
                        if tools.len() > 4096 {
                            return Err(invalid("MCP 工具清单超过上限"));
                        }
                        if let Some(cursor) = message["result"]["nextCursor"].as_str() {
                            if !cursors.insert(cursor.to_owned()) || cursors.len() > 100 {
                                return Err(invalid("MCP 返回无效分页游标"));
                            }
                            request_id += 1;
                            writeln!(
                                input,
                                "{}",
                                json!({"jsonrpc":"2.0","id":request_id,"method":"tools/list","params":{"cursor":cursor}})
                            )?;
                            input.flush()?;
                            continue;
                        }
                        return Ok(json!({"tools":tools}));
                    }
                    if message["result"]["isError"] == true {
                        return Err(invalid("MCP 工具执行失败"));
                    }
                    return Ok(message["result"].clone());
                }
            }
        })();
        crate::execution::stop_child(&mut child);
        drop(ownership);
        drop(receiver);
        let _ = reader.join();
        result.map(|mut value| {
            redact_value(&mut value, server);
            value
        })
    }
}

fn redact_value(value: &mut Value, server: &McpServerDefinition) {
    match value {
        Value::String(text) => *text = crate::execution::redact(text, "", &[server]),
        Value::Array(items) => items.iter_mut().for_each(|item| redact_value(item, server)),
        Value::Object(fields) => fields.values_mut().for_each(|item| redact_value(item, server)),
        _ => {}
    }
}

/// Resolve known npm launchers without passing arguments through a shell.
pub(crate) fn executable(name: &str) -> Command {
    #[cfg(windows)]
    {
        if let Some(relative) = match name {
            "codex" => Some("node_modules/@openai/codex/bin/codex.js"),
            "dsh" => Some("node_modules/@deepseek-ai/dsh/lib/bin.js"),
            _ => None,
        } {
            for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
                let script = directory.join(relative);
                if script.is_file() {
                    let mut command = Command::new(directory.join("node.exe"));
                    command.arg(script);
                    hide(&mut command);
                    return command;
                }
            }
        }
    }
    let mut command = Command::new(name);
    hide(&mut command);
    command
}
fn hide(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revoked_and_unknown_tools_fail_before_launch() {
        let catalog = ToolCatalog {
            mcp_servers: vec![McpServerDefinition {
                id: "test".into(),
                command: "must-not-run".into(),
                args: vec![],
                environment_variables: vec![],
                enabled: true,
            }],
            ..ToolCatalog::default()
        };
        assert!(catalog.list_tools("test", &ToolAuthorizationSettings::default()).is_err());
        assert!(
            catalog
                .skill_instructions(&ToolAuthorizationSettings {
                    authorized_skill_ids: vec!["missing".into()],
                    ..ToolAuthorizationSettings::default()
                })
                .is_err()
        );
    }
    #[test]
    fn rejects_key_values_as_environment_names() {
        assert!(valid_env_name("MCP_API_KEY"));
        assert!(!valid_env_name("sk-secret-key"));
        assert!(!valid_env_name("TOKEN=value"));
        assert!(!valid_env_name("3KEY"));
    }
}
