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
    /// Saved non-secret LLM judgment form, retained when switching to `TypeSafe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_judge: Option<crate::judge::LlmJudgeSettings>,
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

/// Identifier of the bundled `TypeSafe` Jev judgment server (`evaluate`).
pub const BUNDLED_JEV_SERVER_ID: &str = "typesafe-jev";

/// Environment variable consulted first when locating the bundled binary.
pub const BUNDLED_JEV_PATH_ENV: &str = "CIRCUITFABRIC_TYPESAFE_MCP_PATH";

/// MCP servers shipped inside the application bundle.
///
/// The `TypeSafe` Jev `evaluate` server (vendored under `vendor/typesafe-mcp`)
/// is included when its binary is found. Search order mirrors the `PDFium`
/// loader: `CIRCUITFABRIC_TYPESAFE_MCP_PATH`, `typesafe-mcp/evaluate` next to
/// the current executable, then `<ancestor>/native/windows-x64/typesafe-mcp/
/// evaluate` for the first few ancestors of the executable, which covers
/// `target/debug` and `target/debug/deps` during development. When no binary
/// is found the list is empty and the runtime is unaffected.
#[must_use]
pub fn bundled_mcp_servers() -> Vec<McpServerDefinition> {
    let Some(binary) = bundled_jev_binary() else { return Vec::new() };
    vec![McpServerDefinition {
        id: BUNDLED_JEV_SERVER_ID.to_owned(),
        command: binary.to_string_lossy().into_owned(),
        // `mcp` serves stdio; `--no-update-check` keeps startup offline.
        args: vec!["mcp".to_owned(), "--no-update-check".to_owned()],
        environment_variables: vec!["TYPESAFE_API_KEY".to_owned()],
        enabled: true,
    }]
}

fn bundled_jev_binary() -> Option<PathBuf> {
    let name = format!("evaluate{}", env::consts::EXE_SUFFIX);
    if let Some(explicit) = env::var_os(BUNDLED_JEV_PATH_ENV) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    let directory = env::current_exe().ok()?.parent()?.to_owned();
    for candidate in [directory.join("typesafe-mcp").join(&name), directory.join(&name)] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for ancestor in directory.ancestors().take(5) {
        let candidate =
            ancestor.join("native").join("windows-x64").join("typesafe-mcp").join(&name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

impl ToolCatalog {
    /// Register bundled MCP servers whose ids are not already defined, so an
    /// entry the user edited or disabled is never overwritten. Deleting the
    /// entry only lasts until the next load while the binary still ships.
    pub fn ensure_bundled(&mut self) {
        self.insert_bundled(&bundled_mcp_servers());
    }

    fn insert_bundled(&mut self, bundled: &[McpServerDefinition]) {
        for server in bundled {
            if !self.mcp_servers.iter().any(|existing| existing.id == server.id) {
                self.mcp_servers.push(server.clone());
            }
        }
    }

    /// Validate definitions without launching third-party processes.
    /// # Errors
    /// Rejects duplicate/invalid identifiers, empty commands and literal credentials.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if let Some(settings) = &self.llm_judge {
            settings.validate()?;
        }
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
            if server.args.first().is_some_and(|arg| arg == crate::judge::MCP_FLAG) {
                let settings = crate::judge::LlmJudgeSettings::from_server(server)
                    .ok_or_else(|| invalid("LLM 判断工具配置无效"))?;
                settings.validate()?;
                if server.args.len() != 2
                    || server.environment_variables != [settings.api_key_environment_variable]
                {
                    return Err(invalid("LLM 判断工具只允许注入配置指定的密钥变量"));
                }
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
        self.list_tools_with_secrets(id, grants, None)
    }

    /// Discover tools with the unlocked secrets vault as the fallback source
    /// for the server's whitelisted environment variables.
    /// # Errors
    /// Authorization, process, protocol and timeout failures are returned.
    pub fn list_tools_with_secrets(
        &self,
        id: &str,
        grants: &ToolAuthorizationSettings,
        secrets: Option<&crate::secrets::SecretValues>,
    ) -> Result<Value, RuntimeError> {
        self.mcp_request(id, grants, "tools/list", &json!({}), secrets)
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
        self.call_tool_with_secrets(id, grants, name, arguments, None)
    }

    /// Call a tool with the unlocked secrets vault as the fallback source for
    /// the server's whitelisted environment variables.
    /// # Errors
    /// Authorization, process, protocol and timeout failures are returned.
    pub fn call_tool_with_secrets(
        &self,
        id: &str,
        grants: &ToolAuthorizationSettings,
        name: &str,
        arguments: &Value,
        secrets: Option<&crate::secrets::SecretValues>,
    ) -> Result<Value, RuntimeError> {
        self.mcp_request(
            id,
            grants,
            "tools/call",
            &json!({"name":name,"arguments":arguments}),
            secrets,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn mcp_request(
        &self,
        id: &str,
        grants: &ToolAuthorizationSettings,
        method: &str,
        params: &Value,
        secrets: Option<&crate::secrets::SecretValues>,
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
            if let Some(value) = crate::secrets::resolve(name, secrets) {
                command.env(name, value.as_str());
            }
        }
        for name in &server.environment_variables {
            if crate::secrets::resolve(name, secrets).is_none() {
                return Err(invalid(crate::secrets::missing_variable_message(name)));
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
            let deadline = std::time::Instant::now()
                + Duration::from_secs(if method == "tools/call" { 210 } else { 20 });
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
                        // The server's own message is the only clue to the cause (for
                        // example an upstream 401/429); surface it redacted and bounded.
                        let detail = message["result"]["content"]
                            .as_array()
                            .and_then(|blocks| {
                                blocks.iter().find_map(|block| block["text"].as_str())
                            })
                            .unwrap_or_default();
                        let detail: String =
                            crate::execution::redact(detail, "", &[server], secrets)
                                .chars()
                                .take(600)
                                .collect();
                        return Err(invalid(if detail.is_empty() {
                            "MCP 工具执行失败".to_owned()
                        } else {
                            format!("MCP 工具执行失败：{detail}")
                        }));
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
            redact_value(&mut value, server, secrets);
            value
        })
    }
}

fn redact_value(
    value: &mut Value,
    server: &McpServerDefinition,
    secrets: Option<&crate::secrets::SecretValues>,
) {
    match value {
        Value::String(text) => *text = crate::execution::redact(text, "", &[server], secrets),
        Value::Array(items) => {
            for item in items {
                redact_value(item, server, secrets);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                redact_value(item, server, secrets);
            }
        }
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
    fn llm_settings_survive_native_switch_and_validate_secret_whitelist() {
        let settings = crate::judge::LlmJudgeSettings::default();
        let mut catalog = ToolCatalog {
            llm_judge: Some(settings.clone()),
            mcp_servers: vec![settings.server(Path::new("app.exe"), true).unwrap()],
            ..Default::default()
        };
        catalog.validate().unwrap();
        catalog.mcp_servers[0].environment_variables.push("UNRELATED_SECRET".into());
        assert!(catalog.validate().is_err());
        catalog.mcp_servers.clear();
        let restored: ToolCatalog =
            serde_json::from_str(&serde_json::to_string(&catalog).unwrap()).unwrap();
        assert_eq!(restored.llm_judge, Some(settings));
    }
    #[test]
    #[ignore = "requires scripts/build-typesafe-mcp.ps1 output and TYPESAFE_API_KEY in the environment"]
    fn bundled_jev_server_advertises_the_evaluate_tool() {
        let mut catalog = ToolCatalog::default();
        catalog.ensure_bundled();
        let grants = ToolAuthorizationSettings {
            authorized_mcp_server_ids: vec![BUNDLED_JEV_SERVER_ID.to_owned()],
            ..ToolAuthorizationSettings::default()
        };
        let tools = catalog.list_tools(BUNDLED_JEV_SERVER_ID, &grants).expect("list bundled tools");
        assert_eq!(tools["tools"][0]["name"], "evaluate");
    }

    #[test]
    fn tool_errors_carry_the_server_message() {
        let mut catalog = ToolCatalog::default();
        catalog.ensure_bundled();
        if !catalog.mcp_servers.iter().any(|server| server.id == BUNDLED_JEV_SERVER_ID) {
            eprintln!("bundled evaluate binary not found; skipping");
            return;
        }
        let grants = ToolAuthorizationSettings {
            authorized_mcp_server_ids: vec![BUNDLED_JEV_SERVER_ID.to_owned()],
            ..ToolAuthorizationSettings::default()
        };
        // An unknown question type is rejected locally, before any API call, so the
        // placeholder key is never sent anywhere.
        let secrets = crate::secrets::SecretValues::single("TYPESAFE_API_KEY", "placeholder-key");
        let error = catalog
            .call_tool_with_secrets(
                BUNDLED_JEV_SERVER_ID,
                &grants,
                "evaluate",
                &json!({"state":"s","questions":{"q":{"type":"bogus","instructions":"i"}}}),
                Some(&secrets),
            )
            .expect_err("invalid question type is a tool error")
            .to_string();
        assert!(error.contains("MCP 工具执行失败："), "{error}");
        assert!(error.len() > "MCP 工具执行失败：".len() + 40, "server detail missing: {error}");
    }

    #[test]
    fn rejects_key_values_as_environment_names() {
        assert!(valid_env_name("MCP_API_KEY"));
        assert!(!valid_env_name("sk-secret-key"));
        assert!(!valid_env_name("TOKEN=value"));
        assert!(!valid_env_name("3KEY"));
    }

    #[test]
    fn bundled_servers_merge_once_and_never_overwrite_user_entries() {
        let bundled = bundled_mcp_servers();
        let definition = McpServerDefinition {
            id: BUNDLED_JEV_SERVER_ID.to_owned(),
            command: "evaluate".to_owned(),
            args: vec!["mcp".to_owned()],
            environment_variables: vec!["TYPESAFE_API_KEY".to_owned()],
            enabled: true,
        };

        let mut catalog = ToolCatalog::default();
        catalog.insert_bundled(std::slice::from_ref(&definition));
        catalog.insert_bundled(std::slice::from_ref(&definition));
        assert_eq!(catalog.mcp_servers.len(), 1);

        let mut disabled = catalog.mcp_servers[0].clone();
        disabled.enabled = false;
        catalog.mcp_servers[0] = disabled;
        catalog.insert_bundled(&[definition]);
        assert!(!catalog.mcp_servers[0].enabled, "a disabled entry stays disabled");

        if let Some(server) = bundled.first() {
            assert_eq!(server.id, BUNDLED_JEV_SERVER_ID);
            assert!(server.enabled);
            assert_eq!(server.environment_variables, ["TYPESAFE_API_KEY"]);
            ToolCatalog { mcp_servers: vec![server.clone()], ..ToolCatalog::default() }
                .validate()
                .expect("the bundled definition must satisfy catalog validation");
        }
    }
}
