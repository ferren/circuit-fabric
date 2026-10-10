//! Local `Codex App Server` configuration and its JSON-RPC stdio client.
//!
//! The persisted configuration deliberately contains an environment-variable
//! *name*, never an API key value. Values are supplied by the user's process
//! environment, or by the encrypted [`secrets`] vault once the user unlocks it
//! with their vault password; either way they reach child processes strictly
//! through environment injection.

use std::{
    collections::VecDeque,
    env, fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

pub const DEFAULT_BRIDGE_ADDRESS: &str = "127.0.0.1:49630";
pub const DEFAULT_CODEX_COMMAND: &str = "codex";
pub const DEFAULT_API_KEY_ENV: &str = "OPENAI_API_KEY";
pub const DEFAULT_PROVIDER_ID: &str = "zai";

pub mod execution;
pub mod judge;
pub mod secrets;
pub mod tools;

/// Application-wide presentation and storage preferences.
///
/// These values intentionally contain no credentials.  A secret-storage choice
/// selects the source used by the desktop shell; API-key values remain either
/// in the process environment or in the separately encrypted vault.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalTheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalLanguage {
    #[default]
    SimplifiedChinese,
    English,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStorageProvider {
    /// Read credential values only from the process environment.
    Environment,
    /// Use `CircuitFabric`'s separately encrypted local vault.
    #[default]
    EncryptedVault,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

fn default_data_directory() -> PathBuf {
    RuntimeSettings::default_path()
        .parent()
        .map_or_else(|| PathBuf::from(".circuitfabric"), Path::to_path_buf)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalPreferences {
    #[serde(default)]
    pub theme: GlobalTheme,
    #[serde(default)]
    pub language: GlobalLanguage,
    #[serde(default = "default_data_directory")]
    pub data_directory: PathBuf,
    #[serde(default)]
    pub secret_storage_provider: SecretStorageProvider,
    #[serde(default)]
    pub log_level: LogLevel,
}

impl Default for GlobalPreferences {
    fn default() -> Self {
        Self {
            theme: GlobalTheme::default(),
            language: GlobalLanguage::default(),
            data_directory: default_data_directory(),
            secret_storage_provider: SecretStorageProvider::default(),
            log_level: LogLevel::default(),
        }
    }
}

/// An OpenAI-compatible model provider configured by the desktop control plane.
///
/// API keys are intentionally represented only by environment-variable names.
/// The value is supplied by the user's process environment when the Codex App
/// Server child process is launched.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LlmProviderSettings {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key_environment_variable: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision_api_key_environment_variable: Option<String>,
}

const fn default_enabled() -> bool {
    true
}

fn default_provider_id() -> String {
    DEFAULT_PROVIDER_ID.to_owned()
}

impl Default for LlmProviderSettings {
    fn default() -> Self {
        Self {
            id: DEFAULT_PROVIDER_ID.to_owned(),
            name: "Z.ai".to_owned(),
            base_url: "https://api.z.ai/api/coding/paas/v4".to_owned(),
            model: "glm-5.3-flash".to_owned(),
            api_key_environment_variable: "JLCIRCUIT_LLM_API_KEY".to_owned(),
            enabled: true,
            supports_vision: true,
            vision_base_url: Some("https://openrouter.ai/api/v1".to_owned()),
            vision_model: Some("z-ai/glm-5.3-flash".to_owned()),
            vision_api_key_environment_variable: Some("JLCIRCUIT_VISION_LLM_API_KEY".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodexAppServerSettings {
    pub command: String,
    pub working_directory: PathBuf,
    pub model: Option<String>,
    pub api_key_environment_variable: String,
}

impl Default for CodexAppServerSettings {
    fn default() -> Self {
        Self {
            command: DEFAULT_CODEX_COMMAND.to_owned(),
            working_directory: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            model: None,
            api_key_environment_variable: DEFAULT_API_KEY_ENV.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeSettings {
    pub listen_address: String,
}

impl Default for BridgeSettings {
    fn default() -> Self {
        Self { listen_address: DEFAULT_BRIDGE_ADDRESS.to_owned() }
    }
}

/// Globally authorized agent skills and MCP servers.
///
/// Like provider API keys, an authorization is only an identifier: which skills and MCP servers
/// the runtimes may load application-wide. Project-scoped authorizations live in each project's
/// own configuration instead, so the two levels never overwrite each other.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolAuthorizationSettings {
    #[serde(default)]
    pub authorized_skill_ids: Vec<String>,
    #[serde(default)]
    pub authorized_mcp_server_ids: Vec<String>,
}

impl ToolAuthorizationSettings {
    /// Returns the identifier list for one authorization kind.
    #[must_use]
    pub fn ids_for_kind(&self, kind: ToolAuthorizationKind) -> &[String] {
        match kind {
            ToolAuthorizationKind::Skill => &self.authorized_skill_ids,
            ToolAuthorizationKind::McpServer => &self.authorized_mcp_server_ids,
        }
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        for (label, ids) in
            [("skill", &self.authorized_skill_ids), ("MCP server", &self.authorized_mcp_server_ids)]
        {
            let mut seen = std::collections::BTreeSet::new();
            for id in ids {
                if id.trim().is_empty() || id.chars().any(char::is_whitespace) {
                    return Err(RuntimeError::InvalidSettings(format!(
                        "authorized {label} ID `{id}` must be non-empty and contain no whitespace"
                    )));
                }
                if !seen.insert(id.clone()) {
                    return Err(RuntimeError::InvalidSettings(format!(
                        "authorized {label} ID `{id}` is duplicated"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Which kind of tool an authorization entry names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolAuthorizationKind {
    Skill,
    McpServer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub codex: CodexAppServerSettings,
    #[serde(default = "default_provider_id")]
    pub default_provider_id: String,
    #[serde(default)]
    pub providers: Vec<LlmProviderSettings>,
    pub bridge: BridgeSettings,
    #[serde(default)]
    pub tools: ToolAuthorizationSettings,
    #[serde(default)]
    pub catalog: tools::ToolCatalog,
    #[serde(default)]
    pub adapters: execution::AdapterSettings,
    #[serde(default)]
    pub global_preferences: GlobalPreferences,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            codex: CodexAppServerSettings::default(),
            default_provider_id: default_provider_id(),
            providers: vec![LlmProviderSettings::default()],
            bridge: BridgeSettings::default(),
            tools: ToolAuthorizationSettings::default(),
            catalog: tools::ToolCatalog::default(),
            adapters: execution::AdapterSettings::default(),
            global_preferences: GlobalPreferences::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("invalid CircuitFabric runtime settings: {0}")]
    InvalidSettings(String),
    #[error("secrets vault: {0}")]
    Vault(String),
    #[error("failed to read runtime settings at {path}: {source}")]
    ReadSettings { path: PathBuf, source: std::io::Error },
    #[error("failed to parse runtime settings at {path}: {source}")]
    ParseSettings { path: PathBuf, source: serde_json::Error },
    #[error("failed to serialize runtime settings: {0}")]
    SerializeSettings(serde_json::Error),
    #[error("failed to write runtime settings at {path}: {source}")]
    WriteSettings { path: PathBuf, source: std::io::Error },
    #[error("could not launch Codex App Server using `{command}`: {source}")]
    Launch { command: String, source: std::io::Error },
    #[error("failed to stop the supervised Codex App Server process: {0}")]
    Stop(#[source] std::io::Error),
    #[error("Codex App Server did not expose {stream}")]
    MissingStream { stream: &'static str },
    #[error("failed to communicate with Codex App Server: {0}")]
    ProtocolIo(#[from] std::io::Error),
    #[error("Codex App Server returned invalid JSON: {0}")]
    ProtocolJson(#[from] serde_json::Error),
    #[error("Codex App Server rejected `{method}`: {message}")]
    Rpc { method: String, message: String },
    /// The process exited or closed stdout; `detail` carries its last stderr lines.
    #[error("Codex 进程已退出或关闭了输出（等待 `{method}` 时）{detail}")]
    Closed { method: String, detail: String },
    /// The server sent nothing at all for `seconds`; the provider stream most likely hung.
    #[error(
        "Codex App Server {seconds} 秒内没有任何响应（等待 `{method}` 时），模型服务可能卡住了"
    )]
    Stalled { method: String, seconds: u64 },
    /// The turn kept running past its upper bound ([`TURN_MAX_DURATION`] by default).
    #[error("模型调用超过 {minutes} 分钟上限仍未完成（等待 `{method}` 时）")]
    TooLong { method: String, minutes: u64 },
}

impl RuntimeError {
    /// Whether the server went silent without failing; a fresh attempt usually succeeds.
    #[must_use]
    pub fn is_stalled(&self) -> bool {
        matches!(self, Self::Stalled { .. })
    }

    /// Whether the turn hit its upper bound while still streaming, typically a model stuck
    /// in reasoning; a fresh attempt usually behaves differently.
    #[must_use]
    pub fn is_too_long(&self) -> bool {
        matches!(self, Self::TooLong { .. })
    }
}

impl RuntimeSettings {
    /// Validates the settings before they are persisted or used by a bridge.
    ///
    /// # Errors
    ///
    /// Returns an error when a required field is empty or the bridge would
    /// listen on a non-loopback address.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.codex.command.trim().is_empty() {
            return Err(RuntimeError::InvalidSettings(
                "Codex command must not be empty".to_owned(),
            ));
        }
        if self.codex.working_directory.as_os_str().is_empty() {
            return Err(RuntimeError::InvalidSettings(
                "Codex working directory must not be empty".to_owned(),
            ));
        }
        if self.global_preferences.data_directory.as_os_str().is_empty() {
            return Err(RuntimeError::InvalidSettings(
                "data directory must not be empty".to_owned(),
            ));
        }
        if !tools::valid_env_name(&self.codex.api_key_environment_variable) {
            return Err(RuntimeError::InvalidSettings(
                "API key environment-variable name must not be empty".to_owned(),
            ));
        }
        if self.bridge.listen_address.parse::<std::net::SocketAddr>().is_err() {
            return Err(RuntimeError::InvalidSettings(
                "bridge address must be an IP address and port, such as 127.0.0.1:49630".to_owned(),
            ));
        }
        if !self.bridge.listen_address.starts_with("127.") {
            return Err(RuntimeError::InvalidSettings(
                "the first bridge release only permits a loopback 127.x.x.x address".to_owned(),
            ));
        }
        self.validate_providers()?;
        self.tools.validate()?;
        self.catalog.validate()?;
        self.adapters.validate(self)?;
        Ok(())
    }

    fn validate_providers(&self) -> Result<(), RuntimeError> {
        if self.providers.is_empty() {
            return Err(RuntimeError::InvalidSettings(
                "at least one LLM provider must be configured".to_owned(),
            ));
        }
        if self.default_provider_id.trim().is_empty() {
            return Err(RuntimeError::InvalidSettings(
                "default provider ID must not be empty".to_owned(),
            ));
        }
        let mut provider_ids = std::collections::BTreeSet::new();
        for provider in &self.providers {
            if provider.id.trim().is_empty() {
                return Err(RuntimeError::InvalidSettings(
                    "provider ID must not be empty".to_owned(),
                ));
            }
            if provider.id.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
            }) {
                return Err(RuntimeError::InvalidSettings(format!(
                    "provider ID `{}` may contain only letters, numbers, `-`, and `_`",
                    provider.id
                )));
            }
            if !provider_ids.insert(provider.id.clone()) {
                return Err(RuntimeError::InvalidSettings(format!(
                    "provider ID `{}` is duplicated",
                    provider.id
                )));
            }
            if provider.name.trim().is_empty()
                || provider.base_url.trim().is_empty()
                || provider.model.trim().is_empty()
                || provider.api_key_environment_variable.trim().is_empty()
            {
                return Err(RuntimeError::InvalidSettings(format!(
                    "provider `{}` requires a name, base URL, model, and API key environment variable",
                    provider.id
                )));
            }
            if !tools::valid_env_name(&provider.api_key_environment_variable)
                || provider
                    .vision_api_key_environment_variable
                    .as_ref()
                    .is_some_and(|name| !tools::valid_env_name(name))
            {
                return Err(RuntimeError::InvalidSettings("API Key 必须填写环境变量名".to_owned()));
            }
            if !provider.base_url.starts_with("http://")
                && !provider.base_url.starts_with("https://")
            {
                return Err(RuntimeError::InvalidSettings(format!(
                    "provider `{}` base URL must start with http:// or https://",
                    provider.id
                )));
            }
            if provider.supports_vision {
                for (label, value) in [
                    ("vision base URL", provider.vision_base_url.as_deref()),
                    ("vision model", provider.vision_model.as_deref()),
                    (
                        "vision API key environment variable",
                        provider.vision_api_key_environment_variable.as_deref(),
                    ),
                ] {
                    if value.is_none_or(str::is_empty) {
                        return Err(RuntimeError::InvalidSettings(format!(
                            "provider `{}` requires a {} when vision is enabled",
                            provider.id, label
                        )));
                    }
                }
            }
        }
        if !provider_ids.contains(&self.default_provider_id) {
            return Err(RuntimeError::InvalidSettings(format!(
                "default provider `{}` is not configured",
                self.default_provider_id
            )));
        }
        if !self.default_provider().is_some_and(|provider| provider.enabled) {
            return Err(RuntimeError::InvalidSettings(format!(
                "default provider `{}` must be enabled",
                self.default_provider_id
            )));
        }
        Ok(())
    }

    #[must_use]
    pub fn default_provider(&self) -> Option<&LlmProviderSettings> {
        self.providers.iter().find(|provider| provider.id == self.default_provider_id)
    }

    fn migrate_legacy_provider(mut self) -> Self {
        if self.providers.is_empty() {
            let defaults = LlmProviderSettings::default();
            let id = if self.default_provider_id.trim().is_empty() {
                DEFAULT_PROVIDER_ID.to_owned()
            } else {
                self.default_provider_id.clone()
            };
            let provider = LlmProviderSettings {
                id: id.clone(),
                name: id,
                model: self.codex.model.clone().unwrap_or_else(|| defaults.model.clone()),
                api_key_environment_variable: self.codex.api_key_environment_variable.clone(),
                supports_vision: false,
                vision_base_url: None,
                vision_model: None,
                vision_api_key_environment_variable: None,
                ..defaults
            };
            self.default_provider_id.clone_from(&provider.id);
            self.providers.push(provider);
        }
        self
    }

    #[must_use]
    pub fn default_path() -> PathBuf {
        if let Some(app_data) = env::var_os("APPDATA") {
            return PathBuf::from(app_data).join("CircuitFabric").join("runtime.json");
        }
        PathBuf::from(".circuitfabric").join("runtime.json")
    }

    /// Loads saved settings, returning defaults when the file does not exist.
    ///
    /// Bundled MCP servers (see [`tools::bundled_mcp_servers`]) are merged
    /// into the catalog on every load without overwriting existing entries.
    ///
    /// # Errors
    ///
    /// Returns an error when an existing file cannot be read or parsed.
    pub fn load_or_default(path: &Path) -> Result<Self, RuntimeError> {
        match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str::<Self>(&raw)
                .map(Self::migrate_legacy_provider)
                .map_err(|source| RuntimeError::ParseSettings { path: path.to_owned(), source })
                .and_then(|mut settings| {
                    settings.catalog.ensure_bundled();
                    settings.validate()?;
                    Ok(settings)
                }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                let mut settings = Self::default();
                settings.catalog.ensure_bundled();
                Ok(settings)
            }
            Err(source) => Err(RuntimeError::ReadSettings { path: path.to_owned(), source }),
        }
    }

    /// Validates and writes settings as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error when validation, serialization, directory creation, or
    /// file writing fails.
    pub fn save(&self, path: &Path) -> Result<(), RuntimeError> {
        self.validate()?;
        let raw = serde_json::to_vec_pretty(self).map_err(RuntimeError::SerializeSettings)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| RuntimeError::WriteSettings {
                path: parent.to_owned(),
                source,
            })?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result = (|| {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&raw)?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|source| RuntimeError::WriteSettings { path: path.to_owned(), source })
    }
}

/// A turn fails when the server sends nothing at all for this long.
pub const TURN_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
/// Upper bound for one turn, however steadily it streams.
pub const TURN_MAX_DURATION: Duration = Duration::from_secs(20 * 60);

/// Which stream a turn delta belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnDelta {
    /// Reasoning summary (or raw reasoning) text; progress only, never part of the answer.
    Reasoning,
    /// The agent's reply.
    Answer,
}

/// A synchronous client for the documented JSONL transport of `codex app-server`.
#[derive(Debug)]
pub struct CodexAppServerClient {
    child: Child,
    ownership: Option<execution::ProcessOwnership>,
    input: BufWriter<ChildStdin>,
    output: Receiver<Result<Value, String>>,
    stderr: std::sync::Arc<std::sync::Mutex<StderrTail>>,
    idle_timeout: Duration,
    /// Upper bound for one turn; [`TURN_MAX_DURATION`] unless a caller narrows it.
    pub(crate) max_duration: Duration,
    pending: VecDeque<Value>,
    next_request_id: u64,
    cancel: execution::Cancellation,
}

/// Bounded tail of the App Server's stderr, kept to explain an unexpected exit.
#[derive(Debug, Default)]
struct StderrTail {
    lines: VecDeque<String>,
    finished: bool,
}

const STDERR_TAIL_LINES: usize = 12;
const STDERR_LINE_CHARS: usize = 300;
const STDERR_DETAIL_CHARS: usize = 800;

/// Masks credential-looking tokens: `Bearer …` values and long mixed letter/digit runs.
fn redact_stderr_line(line: &str) -> String {
    let mut bearer = false;
    line.split(' ')
        .map(|token| {
            let secret = bearer
                || (token.len() >= 24
                    && token.chars().any(|ch| ch.is_ascii_digit())
                    && token.chars().any(|ch| ch.is_ascii_alphabetic())
                    && token.chars().all(|ch| ch.is_ascii_alphanumeric() || "-_=+".contains(ch)));
            bearer = token.eq_ignore_ascii_case("bearer");
            if secret { "[redacted]" } else { token }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl CodexAppServerClient {
    /// Starts a local child process using the configured `codex` command and provider.
    ///
    /// # Errors
    ///
    /// Returns an error when the process or either required stdio stream cannot
    /// be created.
    pub fn launch(
        settings: &CodexAppServerSettings,
        provider: &LlmProviderSettings,
    ) -> Result<Self, RuntimeError> {
        Self::launch_command(
            app_server_command(settings, provider),
            &settings.command,
            execution::Cancellation::default(),
        )
    }

    pub(crate) fn launch_command(
        mut command: Command,
        name: &str,
        cancel: execution::Cancellation,
    ) -> Result<Self, RuntimeError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| RuntimeError::Launch { command: name.to_owned(), source })?;
        let ownership = execution::ProcessOwnership::attach(&mut child)?;
        let input = child.stdin.take().ok_or(RuntimeError::MissingStream { stream: "stdin" })?;
        let output = child.stdout.take().ok_or(RuntimeError::MissingStream { stream: "stdout" })?;
        let error_output =
            child.stderr.take().ok_or(RuntimeError::MissingStream { stream: "stderr" })?;
        let stderr = std::sync::Arc::new(std::sync::Mutex::new(StderrTail::default()));
        let tail = std::sync::Arc::clone(&stderr);
        // Always drained so a chatty process never blocks on a full pipe.
        std::thread::spawn(move || {
            for line in BufReader::new(error_output).lines() {
                let Ok(line) = line else { break };
                let line: String = line.trim().chars().take(STDERR_LINE_CHARS).collect();
                if line.is_empty() {
                    continue;
                }
                if let Ok(mut tail) = tail.lock() {
                    if tail.lines.len() == STDERR_TAIL_LINES {
                        tail.lines.pop_front();
                    }
                    tail.lines.push_back(redact_stderr_line(&line));
                }
            }
            if let Ok(mut tail) = tail.lock() {
                tail.finished = true;
            }
        });

        let (sender, output_messages) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let message = line.map_err(|_| "读取运行时输出失败".to_owned()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|_| "运行时返回无效 JSON".to_owned())
                });
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            ownership: Some(ownership),
            input: BufWriter::new(input),
            output: output_messages,
            stderr,
            idle_timeout: TURN_IDLE_TIMEOUT,
            max_duration: TURN_MAX_DURATION,
            pending: VecDeque::new(),
            next_request_id: 1,
            cancel,
        })
    }

    /// The last stderr lines, formatted for an error message; empty when there were none.
    /// Waits briefly for the stderr reader so lines written just before exit are included.
    fn stderr_detail(&self) -> String {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let finished = self.stderr.lock().map_or(true, |tail| tail.finished);
            if finished || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let lines = self.stderr.lock().map(|tail| tail.lines.clone()).unwrap_or_default();
        if lines.is_empty() {
            return String::new();
        }
        let joined = lines.into_iter().collect::<Vec<_>>().join(" | ");
        let start = joined.chars().count().saturating_sub(STDERR_DETAIL_CHARS);
        format!("；最后的错误输出：{}", joined.chars().skip(start).collect::<String>())
    }

    /// Performs the mandatory App Server `initialize` / `initialized` handshake.
    ///
    /// # Errors
    ///
    /// Returns an error if the server rejects or closes the JSON-RPC transport.
    pub fn initialize(&mut self) -> Result<(), RuntimeError> {
        self.request(
            "initialize",
            &json!({
                "clientInfo": {
                    "name": "circuitfabric",
                    "title": "CircuitFabric",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        )?;
        self.notify("initialized", &json!({}))
    }

    /// Creates a new App Server thread and returns its server-assigned ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the server rejects the request or omits a thread ID.
    pub fn start_thread(&mut self, model: Option<&str>) -> Result<String, RuntimeError> {
        let mut params = serde_json::Map::new();
        if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
            params.insert("model".to_owned(), Value::String(model.to_owned()));
        }
        let result = self.request("thread/start", &Value::Object(params))?;
        result.pointer("/thread/id").and_then(Value::as_str).map(ToOwned::to_owned).ok_or_else(
            || RuntimeError::Rpc {
                method: "thread/start".to_owned(),
                message: "response did not contain result.thread.id".to_owned(),
            },
        )
    }

    /// Reload a persisted conversation with the current execution boundaries.
    /// # Errors
    /// Returns the server error when history is missing; never silently starts a new thread.
    pub fn resume_thread(
        &mut self,
        thread_id: &str,
        model: &str,
        cwd: &std::path::Path,
    ) -> Result<(), RuntimeError> {
        let result = self.request(
            "thread/resume",
            &json!({
                "threadId": thread_id, "model": model, "cwd": cwd,
                "approvalPolicy": "never", "sandbox": "read-only",
            }),
        )?;
        if result.pointer("/thread/id").and_then(Value::as_str) != Some(thread_id) {
            return Err(tools::invalid("运行时恢复了不同的会话标识"));
        }
        Ok(())
    }

    /// Starts a turn and forwards streamed agent-message deltas to the callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport closes or App Server rejects the turn.
    pub fn run_turn<F>(
        &mut self,
        thread_id: &str,
        prompt: &str,
        on_delta: F,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(&str),
    {
        self.run_turn_with_input(thread_id, &[json!({"type":"text","text":prompt})], on_delta)
    }

    /// Run a turn with explicit text and image input items.
    /// # Errors
    /// Returns transport errors, cancellations and failed turn statuses.
    pub fn run_turn_with_input<F>(
        &mut self,
        thread_id: &str,
        input: &[Value],
        mut on_delta: F,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(&str),
    {
        self.run_turn_streaming(thread_id, input, |kind, delta| {
            if kind == TurnDelta::Answer {
                on_delta(delta);
            }
        })
    }

    /// Run a turn, forwarding both the reasoning-summary stream and the answer stream.
    ///
    /// The turn fails after [`TURN_IDLE_TIMEOUT`] without any server message, or after its
    /// upper bound ([`TURN_MAX_DURATION`] by default) in total, however steadily it streams.
    /// # Errors
    /// Returns transport errors, cancellations, timeouts and failed turn statuses.
    pub fn run_turn_streaming<F>(
        &mut self,
        thread_id: &str,
        input: &[Value],
        on_delta: F,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(TurnDelta, &str),
    {
        self.run_turn_observed(thread_id, input, on_delta, |_| {})
    }

    /// Runs a turn while exposing reported token totals, including those preceding failure.
    /// # Errors
    /// Returns the same transport and turn errors as `run_turn_streaming`.
    pub fn run_turn_observed<F, U>(
        &mut self,
        thread_id: &str,
        input: &[Value],
        mut on_delta: F,
        mut on_usage: U,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(TurnDelta, &str),
        U: FnMut(execution::TaskTokenUsage),
    {
        let started = self.request(
            "turn/start",
            &json!({
                "threadId": thread_id,
                "input": input,
            }),
        )?;
        let turn_id = started
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| tools::invalid("运行时未返回任务标识"))?
            .to_owned();
        let idle_timeout = self.idle_timeout;
        let max_duration = self.max_duration;
        let hard_deadline = Instant::now() + max_duration;
        let mut idle_deadline = Instant::now() + idle_timeout;
        let mut last_error_notification = None;
        loop {
            let message = self
                .read_message("turn/completed", idle_deadline.min(hard_deadline))
                .map_err(|error| match error {
                    RuntimeError::Stalled { method, .. } if Instant::now() >= hard_deadline => {
                        RuntimeError::TooLong {
                            method,
                            minutes: max_duration.as_secs().div_ceil(60),
                        }
                    }
                    RuntimeError::Stalled { method, .. } => {
                        RuntimeError::Stalled { method, seconds: idle_timeout.as_secs() }
                    }
                    other => other,
                })?;
            idle_deadline = Instant::now() + idle_timeout;
            let method = message.get("method").and_then(Value::as_str);
            if method == Some("thread/tokenUsage/updated")
                && message.pointer("/params/threadId").and_then(Value::as_str) == Some(thread_id)
                && message.pointer("/params/turnId").and_then(Value::as_str)
                    == Some(turn_id.as_str())
                && let Some(usage) = execution::TaskTokenUsage::from_codex_notification(&message)
            {
                on_usage(usage);
            }
            let kind = match method {
                Some("item/agentMessage/delta") => Some(TurnDelta::Answer),
                Some("item/reasoning/summaryTextDelta" | "item/reasoning/textDelta") => {
                    Some(TurnDelta::Reasoning)
                }
                _ => None,
            };
            if let Some(kind) = kind
                && let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str)
            {
                on_delta(kind, delta);
            }
            if method == Some("error")
                && message
                    .pointer("/params/turnId")
                    .and_then(Value::as_str)
                    .is_none_or(|id| id == turn_id)
                && let Some(detail) = message.pointer("/params/error").and_then(describe_turn_error)
            {
                last_error_notification = Some(detail);
            }
            if method == Some("turn/completed")
                && message.pointer("/params/threadId").and_then(Value::as_str) == Some(thread_id)
                && message.pointer("/params/turn/id").and_then(Value::as_str)
                    == Some(turn_id.as_str())
            {
                let turn = message.pointer("/params/turn").unwrap_or(&Value::Null);
                return match turn.get("status").and_then(Value::as_str) {
                    Some("completed") => Ok(()),
                    status => Err(RuntimeError::Rpc {
                        method: "turn/start".to_owned(),
                        message: failed_turn_message(
                            status,
                            turn.get("error"),
                            last_error_notification,
                        ),
                    }),
                };
            }
        }
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, RuntimeError> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send(&json!({ "method": method, "id": id, "params": params }))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let message =
                self.receive(method, deadline.saturating_duration_since(Instant::now()))?;
            if self.reject_server_request(&message)? {
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                if self.pending.len() >= 256 {
                    return Err(RuntimeError::InvalidSettings("运行时通知队列已满".to_owned()));
                }
                self.pending.push_back(message);
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(RuntimeError::Rpc {
                    method: method.to_owned(),
                    message: describe_rpc_error(error),
                });
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn notify(&mut self, method: &str, params: &Value) -> Result<(), RuntimeError> {
        self.send(&json!({ "method": method, "params": params }))
    }

    fn send(&mut self, message: &Value) -> Result<(), RuntimeError> {
        serde_json::to_writer(&mut self.input, &message)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        Ok(())
    }

    fn read_message(&mut self, method: &str, deadline: Instant) -> Result<Value, RuntimeError> {
        loop {
            if self.cancel.0.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(RuntimeError::Rpc {
                    method: method.to_owned(),
                    message: "任务已取消".to_owned(),
                });
            }
            if Instant::now() >= deadline {
                return Err(RuntimeError::Stalled { method: method.to_owned(), seconds: 0 });
            }
            let message = match self.pending.pop_front() {
                Some(message) => message,
                None => self.receive(method, deadline.saturating_duration_since(Instant::now()))?,
            };
            if !self.reject_server_request(&message)? {
                return Ok(message);
            }
        }
    }

    fn receive(&self, method: &str, timeout: Duration) -> Result<Value, RuntimeError> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.cancel.0.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(RuntimeError::Rpc {
                    method: method.to_owned(),
                    message: "任务已取消".to_owned(),
                });
            }
            match self.output.recv_timeout(
                Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(value) => {
                    return value.map_err(|message| RuntimeError::Rpc {
                        method: method.to_owned(),
                        message,
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(RuntimeError::Stalled {
                        method: method.to_owned(),
                        seconds: timeout.as_secs(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(RuntimeError::Closed {
                        method: method.to_owned(),
                        detail: self.stderr_detail(),
                    });
                }
            }
        }
    }

    fn reject_server_request(&mut self, message: &Value) -> Result<bool, RuntimeError> {
        if let (Some(id), Some(_)) = (message.get("id"), message.get("method")) {
            self.send(&json!({"id":id,"error":{"code":-32601,"message":"Interactive approval is unavailable; request denied"}}))?;
            return Ok(true);
        }
        Ok(false)
    }
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Rust string as TOML must not fail")
}

/// Builds the `codex app-server` command line shared by the JSON-RPC client and the supervised
/// lifecycle handle: the provider, model, base URL, and environment-variable name are forwarded
/// as `--config` overrides, so no API key value ever appears on a command line.
pub(crate) fn app_server_command(
    settings: &CodexAppServerSettings,
    provider: &LlmProviderSettings,
) -> Command {
    let mut command = tools::executable(&settings.command);
    command
        .arg("app-server")
        .arg("--stdio")
        .arg("--config")
        .arg(format!("model_provider={}", toml_string(&provider.id)))
        .arg("--config")
        .arg(format!("model={}", toml_string(&provider.model)))
        .arg("--config")
        .arg(format!("model_providers.{}.name={}", provider.id, toml_string(&provider.name)))
        .arg("--config")
        .arg(format!(
            "model_providers.{}.base_url={}",
            provider.id,
            toml_string(&provider.base_url)
        ))
        .arg("--config")
        .arg(format!(
            "model_providers.{}.env_key={}",
            provider.id,
            toml_string(&provider.api_key_environment_variable)
        ))
        .arg("--config")
        .arg(format!("model_providers.{}.wire_api=\"responses\"", provider.id))
        .current_dir(&settings.working_directory);
    command
}

/// A supervised Codex App Server child process owned by the desktop control plane.
///
/// The handle keeps the child's stdin pipe open so the server keeps waiting for JSON-RPC input;
/// dropping the handle closes that pipe, and [`Self::stop`] terminates the process explicitly.
/// This is a lifecycle surface only: JSON-RPC conversations stay owned by
/// [`CodexAppServerClient`], whether spawned here or by a bridge.
#[derive(Debug)]
pub struct CodexAppServerHandle {
    client: CodexAppServerClient,
}

impl CodexAppServerHandle {
    /// Starts the configured App Server as a keep-alive child process.
    ///
    /// Readiness requires the JSON-RPC initialization handshake. Task execution creates a
    /// separate isolated session with its own current authorization snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be spawned.
    pub fn launch(
        settings: &CodexAppServerSettings,
        provider: &LlmProviderSettings,
    ) -> Result<Self, RuntimeError> {
        Self::launch_with_secrets(settings, provider, None)
    }

    /// Starts the supervised App Server with the unlocked secrets vault
    /// overlaid onto the child environment, so the provider's referenced
    /// variable resolves even when this process does not export it.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be spawned.
    pub fn launch_with_secrets(
        settings: &CodexAppServerSettings,
        provider: &LlmProviderSettings,
        secrets: Option<&secrets::SecretValues>,
    ) -> Result<Self, RuntimeError> {
        let mut command = app_server_command(settings, provider);
        if let Some(values) = secrets {
            values.overlay_on(&mut command);
        }
        let mut client = CodexAppServerClient::launch_command(
            command,
            &settings.command,
            execution::Cancellation::default(),
        )?;
        client.initialize()?;
        Ok(Self { client })
    }

    /// Process identifier of the supervised App Server.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.client.child.id()
    }

    /// Non-blocking exit poll: `None` while the process is still running, `Some` with its final
    /// status once it has terminated.
    pub fn try_exit(&mut self) -> Option<std::process::ExitStatus> {
        self.client.child.try_wait().ok().flatten()
    }

    /// Terminates the supervised process. Already-exited processes are reaped and reported as
    /// stopped rather than as an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be killed or reaped.
    pub fn stop(&mut self) -> Result<(), RuntimeError> {
        self.client.ownership.take();
        execution::stop_child(&mut self.client.child);
        self.client.child.wait().map_err(RuntimeError::Stop)?;
        Ok(())
    }
}

impl Drop for CodexAppServerHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl Drop for CodexAppServerClient {
    fn drop(&mut self) {
        self.ownership.take();
        execution::stop_child(&mut self.child);
    }
}

fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.trim().to_owned()).filter(|text| !text.is_empty()),
        other => Some(other.to_string()),
    }
}

/// Renders App Server `codexErrorInfo`, which is either a bare tag (`"unauthorized"`) or a
/// single-key object carrying details (`{"httpConnectionFailed":{"httpStatusCode":502}}`).
fn describe_error_info(info: &Value) -> Option<String> {
    match info {
        Value::String(tag) => Some(tag.clone()),
        Value::Object(map) => {
            let (tag, details) = map.iter().next()?;
            Some(match details.get("httpStatusCode").and_then(Value::as_u64) {
                Some(status) => format!("{tag}, HTTP {status}"),
                None => tag.clone(),
            })
        }
        _ => None,
    }
}

/// Renders an App Server `TurnError` (`message`, `codexErrorInfo`, `additionalDetails`).
fn describe_turn_error(error: &Value) -> Option<String> {
    let message = error.get("message").and_then(json_text);
    let info = error.get("codexErrorInfo").and_then(describe_error_info);
    let details = error.get("additionalDetails").and_then(json_text);
    let mut text = match (message, info) {
        (Some(message), Some(info)) => format!("{message}（{info}）"),
        (Some(text), None) | (None, Some(text)) => text,
        (None, None) => return details,
    };
    if let Some(details) = details {
        text.push_str("；详情：");
        text.push_str(&details);
    }
    Some(text)
}

fn failed_turn_message(
    status: Option<&str>,
    turn_error: Option<&Value>,
    last_error_notification: Option<String>,
) -> String {
    let status = match status {
        Some("interrupted") => "任务已中断",
        Some("failed") => "任务失败",
        Some(other) => return format!("任务以未知状态 `{other}` 结束"),
        None => "任务结束但未返回状态",
    };
    match turn_error.and_then(describe_turn_error).or(last_error_notification) {
        Some(detail) => format!("{status}：{detail}"),
        None => format!("{status}：App Server 未提供错误详情"),
    }
}

/// Renders a JSON-RPC error object, keeping the server's message and data.
fn describe_rpc_error(error: &Value) -> String {
    let code = error.get("code").and_then(json_text).unwrap_or_else(|| "未知".to_owned());
    let mut text = format!("请求被拒绝（代码 {code}）");
    if let Some(message) = error.get("message").and_then(json_text) {
        text.push('：');
        text.push_str(&message);
    }
    if let Some(data) = error.get("data").and_then(json_text) {
        text.push_str("；详情：");
        text.push_str(&data);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake App Server that answers the handshake, starts a turn, streams one reasoning
    /// delta and then runs `after_delta` (PowerShell) instead of completing the turn.
    #[cfg(windows)]
    fn fake_app_server_turn(after_delta: &str) -> Result<Vec<String>, RuntimeError> {
        fake_app_server_turn_within(after_delta, TURN_MAX_DURATION)
    }

    #[cfg(windows)]
    fn fake_app_server_turn_within(
        after_delta: &str,
        max_duration: Duration,
    ) -> Result<Vec<String>, RuntimeError> {
        let script = format!(
            r#"$out = [Console]::Out
while ($null -ne ($line = [Console]::In.ReadLine())) {{
  $m = $line | ConvertFrom-Json
  if ($m.method -eq 'initialize') {{ $out.WriteLine('{{"id":' + $m.id + ',"result":{{}}}}') }}
  elseif ($m.method -eq 'thread/start') {{ $out.WriteLine('{{"id":' + $m.id + ',"result":{{"thread":{{"id":"t1"}}}}}}') }}
  elseif ($m.method -eq 'turn/start') {{
    $out.WriteLine('{{"id":' + $m.id + ',"result":{{"turn":{{"id":"u1"}}}}}}')
    $out.WriteLine('{{"method":"item/reasoning/textDelta","params":{{"delta":"thinking"}}}}')
    $out.Flush()
    {after_delta}
  }}
  $out.Flush()
}}"#
        );
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        let mut client = CodexAppServerClient::launch_command(
            command,
            "powershell",
            execution::Cancellation::default(),
        )?;
        client.idle_timeout = Duration::from_secs(2);
        client.max_duration = max_duration;
        client.initialize()?;
        let thread = client.start_thread(None)?;
        let mut deltas = Vec::new();
        client.run_turn_streaming(&thread, &[json!({"type":"text","text":"hi"})], |_, delta| {
            deltas.push(delta.to_owned());
        })?;
        Ok(deltas)
    }

    #[cfg(windows)]
    #[test]
    fn a_silent_turn_reports_a_stall_not_a_closed_connection() {
        let started = Instant::now();
        let error = fake_app_server_turn("Start-Sleep -Seconds 30").expect_err("turn must stall");
        assert!(error.is_stalled(), "{error}");
        assert!(error.to_string().contains("2 秒内没有任何响应"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(20), "stall detected by the idle timeout");
    }

    #[cfg(windows)]
    #[test]
    fn endless_reasoning_hits_the_callers_turn_limit() {
        let started = Instant::now();
        let error = fake_app_server_turn_within(
            "while ($true) { $out.WriteLine('{\"method\":\"item/reasoning/textDelta\",\"params\":{\"delta\":\"...\"}}'); $out.Flush(); Start-Sleep -Milliseconds 300 }",
            Duration::from_secs(4),
        )
        .expect_err("a turn that never answers must end");
        assert!(error.is_too_long() && !error.is_stalled(), "{error}");
        assert!(started.elapsed() < Duration::from_secs(20), "ended by the 4 s limit");
    }

    #[cfg(windows)]
    #[test]
    fn an_exiting_server_reports_its_last_stderr_lines() {
        let error = fake_app_server_turn(
            "[Console]::Error.WriteLine('stream disconnected: 502 Bad Gateway, key sk1234567890abcdefghijklmnop'); exit 3",
        )
        .expect_err("server exits mid-turn");
        assert!(matches!(error, RuntimeError::Closed { .. }), "{error}");
        let message = error.to_string();
        assert!(message.contains("502 Bad Gateway"), "{message}");
        assert!(
            !message.contains("sk1234567890"),
            "credential-looking tokens are masked: {message}"
        );
        assert!(!error.is_stalled());
    }

    #[test]
    fn stderr_redaction_masks_bearer_values_and_long_tokens() {
        assert_eq!(
            redact_stderr_line("Authorization: Bearer abc.def error 401 for model gpt-4o"),
            "Authorization: Bearer [redacted] error 401 for model gpt-4o"
        );
        assert_eq!(redact_stderr_line("key=AbC123dEf456GhI789jKl012"), "[redacted]");
        assert_eq!(
            redact_stderr_line("C:\\Users\\Ferren\\.codex\\config.toml not found"),
            "C:\\Users\\Ferren\\.codex\\config.toml not found"
        );
    }

    #[test]
    fn failed_turn_reports_server_error_message_and_info() {
        let error = json!({
            "message": "unexpected status 401 Unauthorized: invalid api key",
            "codexErrorInfo": { "httpConnectionFailed": { "httpStatusCode": 401 } },
            "additionalDetails": null,
        });

        assert_eq!(
            failed_turn_message(Some("failed"), Some(&error), None),
            "任务失败：unexpected status 401 Unauthorized: invalid api key（httpConnectionFailed, HTTP 401）"
        );
        assert_eq!(
            failed_turn_message(
                Some("failed"),
                Some(&json!({ "message": "", "codexErrorInfo": "usageLimitExceeded" })),
                None
            ),
            "任务失败：usageLimitExceeded"
        );
    }

    #[test]
    fn failed_turn_falls_back_to_error_notification_then_to_explicit_unknown() {
        assert_eq!(
            failed_turn_message(Some("failed"), None, Some("stream disconnected".to_owned())),
            "任务失败：stream disconnected"
        );
        assert_eq!(
            failed_turn_message(Some("interrupted"), Some(&Value::Null), None),
            "任务已中断：App Server 未提供错误详情"
        );
    }

    #[test]
    fn rpc_rejection_keeps_message_and_data() {
        let error = json!({ "code": -32600, "message": "model not found", "data": "gpt-x" });

        assert_eq!(
            describe_rpc_error(&error),
            "请求被拒绝（代码 -32600）：model not found；详情：gpt-x"
        );
    }

    #[test]
    fn default_settings_are_loopback_only_and_do_not_hold_a_secret() {
        let settings = RuntimeSettings::default();

        settings.validate().expect("default settings are valid");
        let encoded = serde_json::to_string(&settings).expect("settings serialize");
        assert!(encoded.contains("OPENAI_API_KEY"));
        assert!(!encoded.contains("sk-"));
    }

    #[test]
    fn global_preferences_round_trip_without_credential_values() {
        let mut settings = RuntimeSettings::default();
        settings.global_preferences.theme = GlobalTheme::Dark;
        settings.global_preferences.language = GlobalLanguage::English;
        settings.global_preferences.data_directory = PathBuf::from("D:/CircuitFabric-data");
        settings.global_preferences.secret_storage_provider = SecretStorageProvider::Environment;
        settings.global_preferences.log_level = LogLevel::Debug;

        let encoded = serde_json::to_string(&settings).expect("settings serialize");
        let restored: RuntimeSettings = serde_json::from_str(&encoded).expect("settings parse");
        assert_eq!(restored.global_preferences, settings.global_preferences);
        assert!(!encoded.contains("api-key-value"));
    }

    #[test]
    fn non_loopback_bridge_is_rejected() {
        let mut settings = RuntimeSettings::default();
        settings.bridge.listen_address = "0.0.0.0:49630".to_owned();

        assert!(matches!(settings.validate(), Err(RuntimeError::InvalidSettings(_))));
    }

    #[test]
    fn multiple_providers_can_be_validated_without_persisting_keys() {
        let mut settings = RuntimeSettings::default();
        settings.providers.push(LlmProviderSettings {
            id: "openrouter".to_owned(),
            name: "OpenRouter".to_owned(),
            base_url: "https://openrouter.ai/api/v1".to_owned(),
            model: "z-ai/glm-5.3-flash".to_owned(),
            api_key_environment_variable: "OPENROUTER_API_KEY".to_owned(),
            enabled: false,
            supports_vision: false,
            vision_base_url: None,
            vision_model: None,
            vision_api_key_environment_variable: None,
        });

        settings.validate().expect("multiple providers are valid");
        let encoded = serde_json::to_string(&settings).expect("settings serialize");
        assert!(encoded.contains("openrouter"));
        assert!(encoded.contains("vision_base_url"));
        assert!(!encoded.contains("secret-value"));
        assert!(!encoded.contains("api-key-value"));
    }

    #[test]
    fn duplicate_provider_ids_are_rejected() {
        let mut settings = RuntimeSettings::default();
        settings.providers.push(LlmProviderSettings::default());

        assert!(matches!(
            settings.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("duplicated")
        ));
    }

    #[test]
    fn old_runtime_settings_are_migrated_to_one_provider() {
        let path = std::env::temp_dir()
            .join(format!("circuitfabric-runtime-migration-{}.json", std::process::id()));
        let old_settings = r#"{
            "codex": {
                "command": "codex",
                "working_directory": ".",
                "model": "legacy-model",
                "api_key_environment_variable": "LEGACY_API_KEY"
            },
            "bridge": { "listen_address": "127.0.0.1:49630" }
        }"#;
        std::fs::write(&path, old_settings).expect("write legacy settings");

        let settings = RuntimeSettings::load_or_default(&path).expect("load legacy settings");
        let _ = std::fs::remove_file(path);
        assert_eq!(settings.providers.len(), 1);
        assert_eq!(settings.default_provider_id, DEFAULT_PROVIDER_ID);
        assert_eq!(settings.providers[0].model, "legacy-model");
        assert_eq!(settings.providers[0].api_key_environment_variable, "LEGACY_API_KEY");
        assert_eq!(
            settings.tools,
            ToolAuthorizationSettings::default(),
            "settings saved before tool authorizations existed load an empty allow-list"
        );
    }

    #[test]
    fn tool_authorizations_round_trip_without_secrets() {
        let mut settings = RuntimeSettings::default();
        settings.tools.authorized_skill_ids = vec!["evidence-search".to_owned()];
        settings.tools.authorized_mcp_server_ids = vec!["jlcircuit-bridge".to_owned()];

        settings.validate().expect("tool authorizations are valid");
        let encoded = serde_json::to_string(&settings).expect("settings serialize");
        assert!(encoded.contains("evidence-search"));
        assert!(encoded.contains("jlcircuit-bridge"));
        let reloaded: RuntimeSettings = serde_json::from_str(&encoded).expect("settings parse");
        assert_eq!(reloaded.tools, settings.tools);
        assert_eq!(reloaded.tools.ids_for_kind(ToolAuthorizationKind::Skill), ["evidence-search"]);
    }

    #[test]
    fn duplicate_or_whitespace_tool_authorizations_are_rejected() {
        let duplicated = RuntimeSettings {
            tools: ToolAuthorizationSettings {
                authorized_skill_ids: vec![
                    "evidence-search".to_owned(),
                    "evidence-search".to_owned(),
                ],
                ..ToolAuthorizationSettings::default()
            },
            ..RuntimeSettings::default()
        };
        assert!(matches!(
            duplicated.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("duplicated")
        ));

        let spaced = RuntimeSettings {
            tools: ToolAuthorizationSettings {
                authorized_skill_ids: vec!["two words".to_owned()],
                ..ToolAuthorizationSettings::default()
            },
            ..RuntimeSettings::default()
        };
        assert!(matches!(
            spaced.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("whitespace")
        ));
    }

    #[test]
    fn supervised_launch_reports_a_missing_command_clearly() {
        let settings = CodexAppServerSettings {
            command: "circuitfabric-definitely-missing-command".to_owned(),
            ..CodexAppServerSettings::default()
        };

        let error = CodexAppServerHandle::launch(&settings, &LlmProviderSettings::default())
            .expect_err("a missing command must not launch");

        assert!(matches!(error, RuntimeError::Launch { .. }));
    }

    #[test]
    fn app_server_arguments_forward_provider_references_not_key_values() {
        let settings = CodexAppServerSettings::default();
        let provider = LlmProviderSettings::default();

        let arguments = app_server_command(&settings, &provider)
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments.contains(&"app-server".to_owned()));
        assert!(arguments.contains(&"--stdio".to_owned()));
        assert!(
            arguments.contains(&format!(
                "model_providers.{}.env_key={}",
                provider.id,
                toml_string(&provider.api_key_environment_variable)
            )),
            "the API key is forwarded as an environment-variable name: {arguments:?}"
        );
        assert!(
            arguments.iter().all(|argument| !argument.contains("sk-")),
            "no key value may appear on the command line: {arguments:?}"
        );
    }
}
