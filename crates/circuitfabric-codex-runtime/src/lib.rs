//! Local `Codex App Server` configuration and its JSON-RPC stdio client.
//!
//! The persisted configuration deliberately contains an environment-variable
//! *name*, never an API key value. `codex app-server` inherits that variable
//! from the desktop or bridge process.

use std::{
    env, fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

pub const DEFAULT_BRIDGE_ADDRESS: &str = "127.0.0.1:49630";
pub const DEFAULT_CODEX_COMMAND: &str = "codex";
pub const DEFAULT_API_KEY_ENV: &str = "OPENAI_API_KEY";

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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub codex: CodexAppServerSettings,
    pub bridge: BridgeSettings,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("invalid CircuitFabric runtime settings: {0}")]
    InvalidSettings(String),
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
    #[error("Codex App Server did not expose {stream}")]
    MissingStream { stream: &'static str },
    #[error("failed to communicate with Codex App Server: {0}")]
    ProtocolIo(#[from] std::io::Error),
    #[error("Codex App Server returned invalid JSON: {0}")]
    ProtocolJson(#[from] serde_json::Error),
    #[error("Codex App Server rejected `{method}`: {message}")]
    Rpc { method: String, message: String },
    #[error("Codex App Server closed its output before answering `{method}`")]
    Closed { method: String },
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
        if self.codex.api_key_environment_variable.trim().is_empty() {
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
        Ok(())
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
    /// # Errors
    ///
    /// Returns an error when an existing file cannot be read or parsed.
    pub fn load_or_default(path: &Path) -> Result<Self, RuntimeError> {
        match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|source| RuntimeError::ParseSettings { path: path.to_owned(), source }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
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
        fs::write(path, raw)
            .map_err(|source| RuntimeError::WriteSettings { path: path.to_owned(), source })
    }
}

/// A synchronous client for the documented JSONL transport of `codex app-server`.
pub struct CodexAppServerClient {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
    next_request_id: u64,
}

impl CodexAppServerClient {
    /// Starts a local child process using the configured `codex` command.
    ///
    /// # Errors
    ///
    /// Returns an error when the process or either required stdio stream cannot
    /// be created.
    pub fn launch(settings: &CodexAppServerSettings) -> Result<Self, RuntimeError> {
        let mut child = Command::new(&settings.command)
            .arg("app-server")
            .arg("--stdio")
            .current_dir(&settings.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| RuntimeError::Launch { command: settings.command.clone(), source })?;
        let input = child.stdin.take().ok_or(RuntimeError::MissingStream { stream: "stdin" })?;
        let output = child.stdout.take().ok_or(RuntimeError::MissingStream { stream: "stdout" })?;

        Ok(Self {
            child,
            input: BufWriter::new(input),
            output: BufReader::new(output),
            next_request_id: 1,
        })
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

    /// Starts a turn and forwards streamed agent-message deltas to the callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport closes or App Server rejects the turn.
    pub fn run_turn<F>(
        &mut self,
        thread_id: &str,
        prompt: &str,
        mut on_delta: F,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(&str),
    {
        self.request(
            "turn/start",
            &json!({
                "threadId": thread_id,
                "input": [{ "type": "text", "text": prompt }],
            }),
        )?;

        loop {
            let message = self.read_message("turn/completed")?;
            if message.get("method").and_then(Value::as_str) == Some("item/agentMessage/delta")
                && let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str)
            {
                on_delta(delta);
            }
            if message.get("method").and_then(Value::as_str) == Some("turn/completed") {
                return Ok(());
            }
        }
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, RuntimeError> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send(&json!({ "method": method, "id": id, "params": params }))?;
        loop {
            let message = self.read_message(method)?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(RuntimeError::Rpc {
                    method: method.to_owned(),
                    message: error.to_string(),
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

    fn read_message(&mut self, method: &str) -> Result<Value, RuntimeError> {
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            return Err(RuntimeError::Closed { method: method.to_owned() });
        }
        serde_json::from_str(&line).map_err(RuntimeError::ProtocolJson)
    }
}

impl Drop for CodexAppServerClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_loopback_only_and_do_not_hold_a_secret() {
        let settings = RuntimeSettings::default();

        settings.validate().expect("default settings are valid");
        let encoded = serde_json::to_string(&settings).expect("settings serialize");
        assert!(encoded.contains("OPENAI_API_KEY"));
        assert!(!encoded.contains("sk-"));
    }

    #[test]
    fn non_loopback_bridge_is_rejected() {
        let mut settings = RuntimeSettings::default();
        settings.bridge.listen_address = "0.0.0.0:49630".to_owned();

        assert!(matches!(settings.validate(), Err(RuntimeError::InvalidSettings(_))));
    }
}
