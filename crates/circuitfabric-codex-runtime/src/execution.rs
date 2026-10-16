//! Concrete turn adapters. Each process is reaped; project session history can survive it.
use crate::{
    CodexAppServerClient, RuntimeError, RuntimeSettings, ToolAuthorizationSettings,
    tools::{executable, invalid},
};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::{
    env, fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterSettings {
    pub claude_command: String,
    pub dsh_command: String,
    // Configs written before the Kimi runtime lack this field.
    #[serde(default = "default_kimi_command")]
    pub kimi_command: String,
    #[serde(default)]
    pub codex_provider_id: String,
    #[serde(default)]
    pub claude_provider_id: String,
    #[serde(default)]
    pub dsh_provider_id: String,
    #[serde(default)]
    pub kimi_provider_id: String,
}
fn default_kimi_command() -> String {
    "kimi".into()
}
impl Default for AdapterSettings {
    fn default() -> Self {
        Self {
            claude_command: "claude".into(),
            dsh_command: "dsh".into(),
            kimi_command: "kimi".into(),
            codex_provider_id: String::new(),
            claude_provider_id: String::new(),
            dsh_provider_id: String::new(),
            kimi_provider_id: String::new(),
        }
    }
}
impl AdapterSettings {
    pub(crate) fn validate(&self, settings: &RuntimeSettings) -> Result<(), RuntimeError> {
        for id in [
            &self.codex_provider_id,
            &self.claude_provider_id,
            &self.dsh_provider_id,
            &self.kimi_provider_id,
        ] {
            if !id.is_empty() && !settings.providers.iter().any(|p| &p.id == id && p.enabled) {
                return Err(invalid("运行时引用了不存在或已停用的 Provider"));
            }
        }
        for id in [&self.claude_provider_id, &self.dsh_provider_id, &self.kimi_provider_id] {
            if !id.is_empty()
                && settings.providers.iter().any(|p| p.id == *id && p.uses_codex_account())
            {
                return Err(invalid("Codex 账号直连 Provider 仅支持 Codex 运行时"));
            }
        }
        for id in [&self.codex_provider_id, &self.claude_provider_id, &self.dsh_provider_id] {
            if !id.is_empty()
                && settings.providers.iter().any(|p| p.id == *id && p.uses_kimi_account())
            {
                return Err(invalid("Kimi 账号直连 Provider 仅支持 Kimi 运行时"));
            }
        }
        if self.claude_command.trim().is_empty()
            || self.dsh_command.trim().is_empty()
            || self.kimi_command.trim().is_empty()
        {
            return Err(invalid("运行时命令不能为空"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentKind {
    Codex,
    Claude,
    Dsh,
    Kimi,
}

/// Reported runtime counters. Absence is represented by `None`, never an estimated zero.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskTokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl TaskTokenUsage {
    pub(crate) fn from_codex_notification(value: &serde_json::Value) -> Option<Self> {
        // Totals are cumulative per thread. Repeated notifications replace the previous
        // snapshot; persistent sessions subtract the saved baseline before reporting a turn.
        let total = value.pointer("/params/tokenUsage/total")?;
        Some(Self {
            input_tokens: total["inputTokens"].as_u64()?,
            output_tokens: total["outputTokens"].as_u64()?,
        })
    }

    fn from_claude_result(value: &serde_json::Value) -> Option<Self> {
        let usage = value.get("usage")?;
        Some(Self {
            input_tokens: usage["input_tokens"]
                .as_u64()?
                .saturating_add(usage["cache_read_input_tokens"].as_u64().unwrap_or(0))
                .saturating_add(usage["cache_creation_input_tokens"].as_u64().unwrap_or(0)),
            output_tokens: usage["output_tokens"].as_u64()?,
        })
    }
}

/// Resolve an explicit runtime binding, falling back to the global default.
#[must_use]
pub fn selected_provider(
    settings: &RuntimeSettings,
    kind: AgentKind,
) -> Option<&crate::LlmProviderSettings> {
    let binding = match kind {
        AgentKind::Codex => &settings.adapters.codex_provider_id,
        AgentKind::Claude => &settings.adapters.claude_provider_id,
        AgentKind::Dsh => &settings.adapters.dsh_provider_id,
        AgentKind::Kimi => &settings.adapters.kimi_provider_id,
    };
    let id = if binding.is_empty() { &settings.default_provider_id } else { binding };
    settings.providers.iter().find(|p| &p.id == id && p.enabled)
}

/// Cancellation checked while waiting for a CLI result.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(pub Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Execute one task with the selected provider and the effective authorization snapshot.
/// # Errors
/// Rejects invalid settings, unavailable keys, runtime errors and cancelled tasks.
pub fn run_task(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    cancel: &Cancellation,
) -> Result<String, RuntimeError> {
    run_task_with_image(settings, kind, grants, prompt, None, cancel)
}

/// Route an optional image through the provider's explicit Vision configuration.
/// # Errors
/// Rejects unsupported adapters, missing files, disabled Vision and runtime failures.
pub fn run_task_with_image(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    cancel: &Cancellation,
) -> Result<String, RuntimeError> {
    run_task_with_options(settings, kind, grants, prompt, image, None, cancel)
}

/// Run one task with an explicit working directory for the agent process.
///
/// The isolated temporary home (`CODEX_HOME` / `CLAUDE_CONFIG_DIR` / `DSH_HOME` and generated
/// MCP configuration) is kept regardless; `working_directory` only becomes the process's
/// current directory, which is how a project-scoped run points an agent at the project root.
/// `None` runs in the isolated temporary directory.
/// # Errors
/// Rejects invalid settings, a missing working directory, unavailable keys, runtime errors and
/// cancelled tasks.
pub fn run_task_with_options(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: Option<&std::path::Path>,
    cancel: &Cancellation,
) -> Result<String, RuntimeError> {
    run_task_with_secrets(settings, kind, grants, prompt, image, working_directory, None, cancel)
}

/// Run one task with an explicit working directory plus the unlocked secrets
/// vault as the fallback source for whitelisted variables whose values are not
/// in this process's environment.
/// # Errors
/// Rejects invalid settings, a missing working directory, unavailable keys, runtime errors and
/// cancelled tasks.
#[allow(clippy::too_many_arguments)]
pub fn run_task_with_secrets(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: Option<&std::path::Path>,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
) -> Result<String, RuntimeError> {
    run_task_streaming(
        settings,
        kind,
        grants,
        prompt,
        image,
        working_directory,
        secrets,
        cancel,
        &mut |_, _| {},
    )
}

/// Like [`run_task_with_secrets`], additionally forwarding the agent's reasoning summary
/// and reply as they stream.
///
/// Codex forwards each redacted delta tagged with its stream; the CLI adapters produce no
/// incremental output, so `on_delta` is not called for them. The returned string is the
/// complete reply (answer stream only) either way.
/// # Errors
/// Same as [`run_task_with_secrets`].
#[allow(clippy::too_many_arguments)]
pub fn run_task_streaming(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: Option<&std::path::Path>,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
) -> Result<String, RuntimeError> {
    run_task_observed(
        settings,
        kind,
        grants,
        prompt,
        image,
        working_directory,
        secrets,
        cancel,
        on_delta,
        &mut |_| {},
    )
}

/// Streams a task and reports authoritative usage snapshots when the adapter supplies them.
/// # Errors
/// Same errors as `run_task_streaming`; already reported usage remains observable on failure.
#[allow(clippy::too_many_arguments)]
pub fn run_task_observed(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: Option<&std::path::Path>,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
    on_usage: &mut dyn FnMut(TaskTokenUsage),
) -> Result<String, RuntimeError> {
    if let Some(directory) = working_directory
        && !directory.is_dir()
    {
        return Err(invalid("工作目录不存在或不是文件夹"));
    }
    run_task_in(
        settings,
        kind,
        grants,
        prompt,
        image,
        working_directory,
        secrets,
        cancel,
        on_delta,
        on_usage,
        None,
        crate::TURN_MAX_DURATION,
        false,
    )
}

/// Uses an application-owned persistent Codex home and resumes its saved thread.
/// Credentials and MCP configuration are supplied afresh for each turn.
/// # Errors
/// Fails explicitly on corrupt/missing history or a changed provider/tool scope.
#[allow(clippy::too_many_arguments)]
pub fn run_session_observed(
    settings: &RuntimeSettings,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: &std::path::Path,
    history_directory: &std::path::Path,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
    on_usage: &mut dyn FnMut(TaskTokenUsage),
) -> Result<String, RuntimeError> {
    run_session_observed_within(
        settings,
        grants,
        prompt,
        image,
        working_directory,
        history_directory,
        secrets,
        cancel,
        on_delta,
        on_usage,
        crate::TURN_MAX_DURATION,
    )
}

/// [`run_session_observed`] with a caller-chosen upper bound for the turn, for short
/// structured steps where a model that keeps reasoning without answering should fail fast
/// with [`RuntimeError::TooLong`] instead of after [`crate::TURN_MAX_DURATION`].
/// # Errors
/// Same errors as `run_session_observed`.
#[allow(clippy::too_many_arguments)]
pub fn run_session_observed_within(
    settings: &RuntimeSettings,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: &std::path::Path,
    history_directory: &std::path::Path,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
    on_usage: &mut dyn FnMut(TaskTokenUsage),
    max_turn: std::time::Duration,
) -> Result<String, RuntimeError> {
    if !working_directory.is_dir() {
        return Err(invalid("项目工作目录不存在"));
    }
    run_task_in(
        settings,
        AgentKind::Codex,
        grants,
        prompt,
        image,
        Some(working_directory),
        secrets,
        cancel,
        on_delta,
        on_usage,
        Some(history_directory),
        max_turn,
        false,
    )
}

/// Continue an existing native thread; never create a replacement conversation.
/// # Errors
/// Returns an error if the native binding/history is missing or incompatible.
#[allow(clippy::too_many_arguments)]
pub fn resume_session_observed(
    settings: &RuntimeSettings,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: &std::path::Path,
    history_directory: &std::path::Path,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_usage: &mut dyn FnMut(TaskTokenUsage),
) -> Result<String, RuntimeError> {
    resume_session_streaming(
        settings,
        grants,
        prompt,
        image,
        working_directory,
        history_directory,
        secrets,
        cancel,
        &mut |_, _| {},
        on_usage,
    )
}

/// Resume the saved thread and forward redacted answer/reasoning deltas as they arrive.
/// # Errors
/// Same as `resume_session_observed`; a missing binding never starts a new thread.
#[allow(clippy::too_many_arguments)]
pub fn resume_session_streaming(
    settings: &RuntimeSettings,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: &std::path::Path,
    history_directory: &std::path::Path,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
    on_usage: &mut dyn FnMut(TaskTokenUsage),
) -> Result<String, RuntimeError> {
    if !working_directory.is_dir() {
        return Err(invalid("项目工作目录不存在"));
    }
    if !history_directory.join("circuitfabric-thread.json").is_file() {
        return Err(invalid("此记录缺少 Codex 原生会话历史，无法恢复完整上下文"));
    }
    run_task_in(
        settings,
        AgentKind::Codex,
        grants,
        prompt,
        image,
        Some(working_directory),
        secrets,
        cancel,
        on_delta,
        on_usage,
        Some(history_directory),
        crate::TURN_MAX_DURATION,
        true,
    )
}

#[derive(Serialize, Deserialize)]
struct StoredThread {
    schema_version: u32,
    thread_id: String,
    /// Only identifiers/settings, never resolved key values.
    scope: serde_json::Value,
    usage: TaskTokenUsage,
}

fn save_thread(path: &std::path::Path, state: &StoredThread) -> Result<(), RuntimeError> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(state)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

/// Route an image task through the provider's separate vision model, used when
/// the LLM itself cannot see images. Each missing field names itself so the
/// user knows exactly what to configure.
fn apply_vision_provider(provider: &mut crate::LlmProviderSettings) -> Result<(), RuntimeError> {
    provider.base_url = provider.vision_base_url.clone().ok_or_else(|| {
        invalid(
            "该 LLM 无原生视觉能力且未配置 Vision 服务地址；请在 Provider 设置中填写\
             独立 Vision 模型，或开启「LLM 本身支持视觉」",
        )
    })?;
    provider.model = provider.vision_model.clone().ok_or_else(|| invalid("缺少 Vision 模型"))?;
    provider.api_key_environment_variable = provider
        .vision_api_key_environment_variable
        .clone()
        .ok_or_else(|| invalid("缺少 Vision API Key 环境变量名"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_task_in(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: Option<&std::path::Path>,
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
    on_usage: &mut dyn FnMut(TaskTokenUsage),
    history_directory: Option<&std::path::Path>,
    max_turn: std::time::Duration,
    resume_required: bool,
) -> Result<String, RuntimeError> {
    settings.validate()?;
    settings.catalog.validate_secret_references(secrets)?;
    let effective = settings.catalog.effective_grants(&settings.tools, Some(grants));
    let grants = &effective;
    if cancel.0.load(Ordering::SeqCst) {
        return Err(invalid("任务已取消"));
    }
    if prompt.trim().is_empty() {
        return Err(invalid("任务不能为空"));
    }
    let mut provider =
        selected_provider(settings, kind).ok_or_else(|| invalid("Provider 不可用"))?.clone();
    if provider.uses_codex_account() && kind != AgentKind::Codex {
        return Err(invalid("Codex 账号直连 Provider 仅支持 Codex 运行时"));
    }
    if provider.uses_kimi_account() && kind != AgentKind::Kimi {
        return Err(invalid("Kimi 账号直连 Provider 仅支持 Kimi 运行时"));
    }
    if kind == AgentKind::Kimi && !provider.uses_kimi_account() {
        return Err(invalid("Kimi 运行时仅支持 Kimi 账号直连 Provider（本机 kimi 登录态）"));
    }
    if image.is_some() {
        if kind != AgentKind::Codex {
            return Err(invalid("图片任务需要 Codex 运行时"));
        }
        if provider.uses_codex_account() && !provider.native_vision {
            // A direct provider has no separate vision endpoint: its ChatGPT
            // account signs in once and the main model must see images itself.
            return Err(invalid(
                "Codex 账号直连不支持独立 Vision 配置；请开启「LLM 本身支持视觉」",
            ));
        }
        if !provider.native_vision {
            // The LLM cannot see images itself; route the task through the
            // separate vision model.
            apply_vision_provider(&mut provider)?;
        }
        // With `native_vision`, the image goes straight to the main model with
        // the provider's regular base URL, model, and key configuration.
    }
    // An image test may have opened its thread on the separate vision endpoint.
    // Text-only follow-ups must stay on that model rather than switch to the
    // provider's main text model. The saved scope is still checked in run_codex.
    if resume_required
        && image.is_none()
        && !provider.native_vision
        && !provider.uses_codex_account()
    {
        let binding = history_directory
            .expect("resuming a persistent session")
            .join("circuitfabric-thread.json");
        let saved: StoredThread = serde_json::from_slice(&fs::read(binding)?)?;
        if (saved.scope["model"] != provider.model || saved.scope["baseUrl"] != provider.base_url)
            && provider.vision_model.as_deref().is_some_and(|model| saved.scope["model"] == model)
            && provider.vision_base_url.as_deref().is_some_and(|url| saved.scope["baseUrl"] == url)
        {
            apply_vision_provider(&mut provider)?;
        }
    }
    let key = if provider.uses_codex_account() || provider.uses_kimi_account() {
        // Direct sign-in carries its own credentials in the real CLI home;
        // there is no API key to resolve or redact.
        zeroize::Zeroizing::new(String::new())
    } else {
        crate::secrets::resolve(&provider.api_key_environment_variable, secrets).ok_or_else(
            || {
                invalid(crate::secrets::missing_variable_message(
                    &provider.api_key_environment_variable,
                ))
            },
        )?
    };
    let instructions = settings.catalog.skill_instructions(grants)?;
    let servers = grants
        .authorized_mcp_server_ids
        .iter()
        .map(|id| {
            settings
                .catalog
                .mcp_servers
                .iter()
                .find(|s| &s.id == id && s.enabled)
                .ok_or_else(|| invalid(format!("MCP {id} 未定义或已停用")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for server in &servers {
        for name in &server.environment_variables {
            if crate::secrets::resolve(name, secrets).is_none() {
                return Err(invalid(crate::secrets::missing_variable_message(name)));
            }
        }
    }
    let environment = if let Some(home) = history_directory {
        fs::create_dir_all(home)?;
        RunEnvironment {
            home: home.to_owned(),
            current: working_directory.expect("session project root").to_owned(),
            temporary: false,
        }
    } else {
        RunEnvironment::new(working_directory)?
    };
    let input = format!("{instructions}\n\n{prompt}");
    if kind == AgentKind::Codex {
        let mut inputs = vec![serde_json::json!({"type":"text","text":input})];
        if let Some(path) = image {
            inputs.push(serde_json::json!({"type":"localImage","path":fs::canonicalize(path)?}));
        }
        return run_codex(
            settings,
            &provider,
            &servers,
            &environment,
            grants,
            &inputs,
            secrets,
            cancel,
            &mut |kind, delta| on_delta(kind, &redact(delta, &key, &servers, secrets)),
            on_usage,
            max_turn,
            resume_required,
        )
        .map(|output| redact(&output, &key, &servers, secrets));
    }
    let command = match kind {
        AgentKind::Claude => {
            claude_command(settings, &provider, &servers, &environment.home, &key, secrets)?
        }
        AgentKind::Dsh => {
            dsh_command(settings, &provider, &servers, &environment.home, &input, secrets)?
        }
        AgentKind::Kimi => kimi_command(settings, &provider, &servers, &input, secrets),
        AgentKind::Codex => unreachable!("Codex is dispatched before the CLI adapters"),
    };
    execute_cli(command, &environment, input, kind, cancel, &key, &servers, secrets, on_usage)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_codex(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    environment: &RunEnvironment,
    grants: &ToolAuthorizationSettings,
    input: &[serde_json::Value],
    secrets: Option<&crate::secrets::SecretValues>,
    cancel: &Cancellation,
    on_delta: &mut dyn FnMut(crate::TurnDelta, &str),
    on_usage: &mut dyn FnMut(TaskTokenUsage),
    max_turn: std::time::Duration,
    resume_required: bool,
) -> Result<String, RuntimeError> {
    let mut command = crate::app_server_command(&settings.codex, provider);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers, secrets);
    command.current_dir(&environment.current);
    if !provider.uses_codex_account() {
        // API-key providers run against an isolated home so overrides and
        // thread state never touch the user's real `~/.codex`; a direct
        // provider must inherit the real one, or its ChatGPT sign-in would
        // be invisible. `environment.home` still binds `circuitfabric-thread.json`.
        command.env("CODEX_HOME", &environment.home);
    }
    command.args(["-c", "features.multi_agent=false"]).args([
        "-c",
        "features.skip_host_skill_discovery=true",
        "-c",
        "project_doc_max_bytes=0",
        "-c",
        "features.shell_tool=false",
        "-c",
        "features.apps=false",
        "-c",
        "features.skill_mcp_dependency_install=false",
        "-c",
        "web_search=\"disabled\"",
        "-c",
        "sandbox_mode=\"read-only\"",
        "-c",
        "approval_policy=\"never\"",
    ]);
    for server in servers {
        let discovered = settings.catalog.list_tools_with_secrets(&server.id, grants, secrets)?;
        let names = discovered["tools"]
            .as_array()
            .ok_or_else(|| invalid("MCP 未返回工具清单"))?
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect::<Vec<_>>();
        command.arg("-c").arg(format!(
            "mcp_servers.{}.enabled_tools={}",
            server.id,
            serde_json::to_string(&names)?
        ));
        let approved = names
            .iter()
            .map(|name| format!("{}={{approval_mode=\"approve\"}}", crate::toml_string(name)))
            .collect::<Vec<_>>()
            .join(",");
        command.arg("-c").arg(format!("mcp_servers.{}.tools={{{approved}}}", server.id));
        for (field, value) in [
            ("command", serde_json::to_string(&server.command)?),
            ("args", serde_json::to_string(&server.args)?),
            ("env_vars", serde_json::to_string(&server.environment_variables)?),
            ("required", "true".to_owned()),
        ] {
            command.arg("-c").arg(format!("mcp_servers.{}.{field}={value}", server.id));
        }
    }
    let mut client =
        CodexAppServerClient::launch_command(command, &settings.codex.command, cancel.clone())?;
    client.max_duration = max_turn;
    client.initialize()?;
    let binding_path = environment.home.join("circuitfabric-thread.json");
    let scope = serde_json::json!({"provider": provider.id, "model": provider.model,
        "baseUrl": provider.base_url, "grants": grants, "servers": servers});
    let mut stored = if !environment.temporary && binding_path.exists() {
        let saved: StoredThread = serde_json::from_slice(&fs::read(&binding_path)?)?;
        if saved.schema_version != 1 || saved.scope != scope {
            return Err(invalid("会话的 Provider 或工具授权已变化，请新建会话；旧历史保留"));
        }
        client.resume_thread(&saved.thread_id, &provider.model, &environment.current)?;
        saved
    } else {
        if resume_required {
            return Err(invalid("Codex 原生会话绑定已丢失，未创建替代会话"));
        }
        StoredThread {
            schema_version: 1,
            thread_id: client.start_thread(Some(&provider.model))?,
            scope,
            usage: TaskTokenUsage::default(),
        }
    };
    if !environment.temporary {
        save_thread(&binding_path, &stored)?;
    }
    let baseline = stored.usage;
    let mut output = String::new();
    let thread_id = stored.thread_id.clone();
    let mut persistence_error = None;
    let outcome = client.run_turn_observed(
        &thread_id,
        input,
        |kind, delta| {
            if kind == crate::TurnDelta::Answer {
                output.push_str(delta);
            }
            on_delta(kind, delta);
        },
        |reported| {
            on_usage(TaskTokenUsage {
                input_tokens: reported.input_tokens.saturating_sub(baseline.input_tokens),
                output_tokens: reported.output_tokens.saturating_sub(baseline.output_tokens),
            });
            stored.usage = reported;
            if !environment.temporary
                && let Err(error) = save_thread(&binding_path, &stored)
            {
                persistence_error = Some(error);
            }
        },
    );
    if let Some(error) = persistence_error {
        return Err(error);
    }
    outcome?;
    Ok(output)
}

fn claude_command(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    home: &std::path::Path,
    key: &str,
    secrets: Option<&crate::secrets::SecretValues>,
) -> Result<std::process::Command, RuntimeError> {
    let mut command = executable(&settings.adapters.claude_command);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers, secrets);
    let (base_url, credential_variable) = claude_endpoint(&provider.base_url);
    let mut mcp = serde_json::Map::new();
    for server in servers {
        // Claude expands ${NAME} in MCP env entries at runtime.
        let refs = server
            .environment_variables
            .iter()
            .map(|name| (name.clone(), serde_json::Value::String(format!("${{{name}}}"))))
            .collect::<serde_json::Map<_, _>>();
        mcp.insert(
            server.id.clone(),
            serde_json::json!({"command":server.command,"args":server.args,"env":refs}),
        );
    }
    let path = home.join("mcp.json");
    fs::write(&path, serde_json::to_vec(&serde_json::json!({"mcpServers":mcp}))?)?;
    command
        .args([
            "--print",
            "--output-format",
            "json",
            "--bare",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--tools",
        ])
        .arg(servers.iter().map(|s| format!("mcp__{}__*", s.id)).collect::<Vec<_>>().join(","))
        .args(["--model", &provider.model])
        .arg("--mcp-config")
        .arg(path)
        .env("CLAUDE_CONFIG_DIR", home)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env("ANTHROPIC_BASE_URL", base_url)
        // Recent CLIs retry even authentication failures up to ten times. Their
        // backoff can outlast our task deadline, hiding the final JSON error.
        .env("CLAUDE_CODE_MAX_RETRIES", "2")
        .env(credential_variable, key);
    if !servers.is_empty() {
        command
            .arg("--allowedTools")
            .arg(servers.iter().map(|s| format!("mcp__{}__*", s.id)).collect::<Vec<_>>().join(","));
    }
    Ok(command)
}

/// Claude always speaks Anthropic Messages, regardless of Codex's `wire_api`.
/// Z.ai's bundled Responses URL and Coding Plan chat URL share a separate
/// Anthropic endpoint; changing the saved provider would break its Codex binding.
fn claude_endpoint(base_url: &str) -> (String, &'static str) {
    let base_url = base_url.trim_end_matches('/');
    if let Ok(mut url) = reqwest::Url::parse(base_url)
        && url.host_str() == Some("api.z.ai")
        && matches!(
            url.path(),
            "/api/v1" | "/api/coding/paas/v4" | "/api/anthropic" | "/api/anthropic/v1"
        )
    {
        url.set_path("/api/anthropic");
        return (url.to_string(), "ANTHROPIC_AUTH_TOKEN");
    }
    (base_url.strip_suffix("/v1").unwrap_or(base_url).to_owned(), "ANTHROPIC_API_KEY")
}

/// DSH always speaks Chat Completions through its `llm-deepseek` adapter
/// (`{baseURL}/chat/completions`). Z.ai's Responses and Anthropic URLs do not
/// serve that path — the gateway answers HTTP 500 — and the two documented
/// chat bases differ by key billing, so there is no safe rewrite: reject with
/// the endpoints to use instead of letting the task fail opaquely.
fn dsh_endpoint(base_url: &str) -> Result<String, RuntimeError> {
    let trimmed = base_url.trim_end_matches('/');
    if let Ok(url) = reqwest::Url::parse(trimmed)
        && url.host_str() == Some("api.z.ai")
        && !matches!(url.path(), "/api/coding/paas/v4" | "/api/paas/v4")
    {
        return Err(invalid(format!(
            "z.ai 的 {trimmed} 不提供 Chat Completions，DSH 任务中止：请把 Provider 的 Base URL 改为 \
             https://api.z.ai/api/coding/paas/v4（编程包密钥）或 https://api.z.ai/api/paas/v4（按量付费密钥）"
        )));
    }
    Ok(trimmed.to_owned())
}

fn dsh_command(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    home: &std::path::Path,
    input: &str,
    secrets: Option<&crate::secrets::SecretValues>,
) -> Result<std::process::Command, RuntimeError> {
    let mut command = executable(&settings.adapters.dsh_command);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers, secrets);
    let base_url = dsh_endpoint(&provider.base_url)?;
    let patch = home.join("runtime.patch.yml");
    let mut patches = vec![
        serde_json::json!({"id":"agent-default-model","config":{"provider":"deepseek-official","model":provider.model}}),
        serde_json::json!({"id":"llm-deepseek","config":{"baseURL":base_url,"apiKeyEnv":provider.api_key_environment_variable,"thinking":"disabled","maxTokens":8192,"models":[{"id":provider.model}]}}),
    ];
    // All ambient discovery, arbitrary execution and external settings are disabled.
    for id in [
        "settings",
        "skill-filesystem",
        "tool-skill",
        "skill-badge",
        "tool-bash",
        "tool-pwsh",
        "tool-jobs",
        "tool-fs",
        "tool-fs-search",
        "tool-subagent-control",
        "tool-subagent-list-agents",
        "tool-subagent",
        "tool-subagent-fork",
        "tool-subagent-report",
        "tool-workflow",
        "tool-str-replace-editor",
        "tool-web",
        "tool-ralph",
        "tool-goal",
        "tool-todo",
    ] {
        patches.push(serde_json::json!({"id":id,"disabled":true}));
    }
    let mut yaml = String::new();
    for patch in patches {
        let _ = writeln!(yaml, "- {}", serde_json::to_string(&patch)?);
    }
    for server in servers {
        let _ = write!(
            yaml,
            "- insert:\n    - id: cf-mcp-{}\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        transport: stdio\n        serverName: {}\n        command: {}\n        args: {}\n        failOnStartupError: true\n        env:\n",
            server.id,
            serde_json::to_string(&server.id)?,
            serde_json::to_string(&server.command)?,
            serde_json::to_string(&server.args)?
        );
        if server.environment_variables.is_empty() {
            yaml.push_str("          {}\n");
        }
        for name in &server.environment_variables {
            let _ = writeln!(
                yaml,
                "          {name}: !!js process.env[{}]",
                serde_json::to_string(name)?
            );
        }
    }
    fs::write(&patch, yaml)?;
    command
        .args(["--profile", "headless", "--patch"])
        .arg(&patch)
        .arg(input)
        .env("DSH_HOME", home.join("dsh"))
        .env("DSH_TELEMETRY_DISABLED", "1");
    Ok(command)
}

/// One-shot `kimi` CLI invocation: the prompt travels as an argument (its
/// `-p` flag requires one and stdin piping hangs), and the reply is parsed
/// from the `stream-json` JSONL events. The provider must be a
/// `kimi_account` direct provider, so the child reads the local CLI's own
/// sign-in from the real home directory and no API key exists to inject.
fn kimi_command(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    input: &str,
    secrets: Option<&crate::secrets::SecretValues>,
) -> std::process::Command {
    let mut command = executable(&settings.adapters.kimi_command);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers, secrets);
    command.args(["--prompt", input, "--output-format", "stream-json"]);
    if !provider.model.trim().is_empty() {
        command.args(["--model", &provider.model]);
    }
    command
}

#[allow(clippy::too_many_arguments)]
fn execute_cli(
    mut command: std::process::Command,
    environment: &RunEnvironment,
    input: String,
    kind: AgentKind,
    cancel: &Cancellation,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
    secrets: Option<&crate::secrets::SecretValues>,
    on_usage: &mut dyn FnMut(TaskTokenUsage),
) -> Result<String, RuntimeError> {
    command
        .current_dir(&environment.current)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let ownership = ProcessOwnership::attach(&mut child)?;
    let mut stdin = child.stdin.take().ok_or_else(|| invalid("运行时缺少 stdin"))?;
    let mut stdout = child.stdout.take().ok_or_else(|| invalid("运行时缺少 stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| invalid("运行时缺少 stderr"))?;
    let error_reader = std::thread::spawn(move || {
        let mut output = String::new();
        stderr.take(1024 * 1024).read_to_string(&mut output).map(|_| output)
    });
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        stdout.read_to_string(&mut output).map(|_| output)
    });
    let writer = std::thread::spawn(move || {
        if kind == AgentKind::Claude { stdin.write_all(input.as_bytes()) } else { Ok(()) }
    });
    let deadline = Instant::now() + Duration::from_secs(180);
    let outcome = loop {
        if cancel.0.load(Ordering::SeqCst) || Instant::now() >= deadline {
            stop_child(&mut child);
            break Err(invalid("任务已取消或超时"));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(invalid(format!("运行时任务失败（{status}），请检查服务地址、协议及模型")))
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                stop_child(&mut child);
                break Err(error.into());
            }
        }
    };
    drop(ownership);
    let _ = writer.join();
    let output = reader.join().map_err(|_| invalid("读取任务结果失败"))??;
    let error_output = error_reader.join().map_err(|_| invalid("读取运行时错误失败"))??;
    if kind == AgentKind::Claude
        && let Ok(response) = serde_json::from_str::<serde_json::Value>(&output)
        && let Some(usage) = TaskTokenUsage::from_claude_result(&response)
    {
        on_usage(usage);
    }
    if let Err(error) = outcome {
        let detail = failure_detail(&error_output, &output, key, servers, secrets);
        return Err(with_failure_detail(error, &detail));
    }
    if kind == AgentKind::Dsh {
        return Ok(redact(&output, key, servers, secrets));
    }
    if kind == AgentKind::Kimi {
        // `stream-json` emits one JSON event per line; the answer is the
        // concatenation of every assistant message's string content.
        let mut text = String::new();
        for line in output.lines() {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if event.get("role").and_then(serde_json::Value::as_str) == Some("assistant")
                && let Some(content) = event.get("content").and_then(serde_json::Value::as_str)
            {
                text.push_str(content);
            }
        }
        if text.trim().is_empty() {
            return Err(invalid("Kimi 未返回文本结果"));
        }
        return Ok(redact(&text, key, servers, secrets));
    }
    let response: serde_json::Value = serde_json::from_str(&output)?;
    if response["is_error"] == true {
        let detail = failure_detail(&error_output, &output, key, servers, secrets);
        return Err(with_failure_detail(invalid("Claude Code 返回任务错误"), &detail));
    }
    Ok(redact(
        response["result"].as_str().ok_or_else(|| invalid("Claude Code 未返回文本结果"))?,
        key,
        servers,
        secrets,
    ))
}

fn with_failure_detail(error: RuntimeError, detail: &str) -> RuntimeError {
    match error {
        RuntimeError::InvalidSettings(message) if !detail.trim().is_empty() => {
            invalid(format!("{message}: {detail}"))
        }
        other => other,
    }
}

/// Claude emits structured failures on stdout, even alongside stderr warnings.
/// Other adapters may report plain-text errors on stdout instead of stderr.
fn failure_detail(
    error_output: &str,
    output: &str,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
    secrets: Option<&crate::secrets::SecretValues>,
) -> String {
    let mut parts = Vec::new();
    if let Ok(response) = serde_json::from_str::<serde_json::Value>(output)
        && response["is_error"] == true
    {
        if let Some(result) = response["result"].as_str().filter(|s| !s.trim().is_empty()) {
            parts.push(result);
        }
        if let Some(errors) = response["errors"].as_array() {
            parts.extend(errors.iter().filter_map(serde_json::Value::as_str));
        }
        parts.push(error_output.trim());
        return redact(&parts.join("\n"), key, servers, secrets).chars().take(8000).collect();
    }
    let raw = if error_output.trim().is_empty() { output } else { error_output };
    redact(raw, key, servers, secrets).chars().take(8000).collect()
}

pub(crate) fn redact(
    text: &str,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
    secrets: Option<&crate::secrets::SecretValues>,
) -> String {
    let mut output = if key.is_empty() { text.to_owned() } else { text.replace(key, "[REDACTED]") };
    for name in servers.iter().flat_map(|s| &s.environment_variables) {
        if let Some(value) = crate::secrets::resolve(name, secrets)
            && !value.is_empty()
        {
            output = output.replace(value.as_str(), "[REDACTED]");
        }
    }
    output
}

fn restrict_environment(
    command: &mut std::process::Command,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
    secrets: Option<&crate::secrets::SecretValues>,
) {
    command.env_clear();
    for name in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "COMSPEC",
        "PATHEXT",
        key,
    ]
    .into_iter()
    .chain(servers.iter().flat_map(|s| s.environment_variables.iter().map(String::as_str)))
    .filter(|name| !name.is_empty())
    {
        if let Some(value) = crate::secrets::resolve(name, secrets) {
            command.env(name, value.as_str());
        }
    }
}

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Per-run filesystem scope: an isolated temporary configuration home plus the agent
/// process's working directory (a scoped project root, or the home itself when unscoped).
///
/// Only the isolated home is ever removed on drop; a scoped current directory — for example a
/// project root — always belongs to the user.
struct RunEnvironment {
    home: PathBuf,
    current: PathBuf,
    temporary: bool,
}
impl RunEnvironment {
    fn new(working_directory: Option<&std::path::Path>) -> Result<Self, RuntimeError> {
        let home = env::temp_dir().join(format!(
            "circuitfabric-run-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&home)?;
        let current = working_directory.map_or_else(|| home.clone(), std::path::Path::to_path_buf);
        Ok(Self { home, current, temporary: true })
    }
}
impl Drop for RunEnvironment {
    fn drop(&mut self) {
        if !self.temporary {
            return;
        }
        for _ in 0..20 {
            if fs::remove_dir_all(&self.home).is_ok() || !self.home.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Terminates a supervised child together with any process tree it created.
///
/// This is the sanctioned kill path for supervised local processes: it never
/// turns an already-exited process into an error.
pub fn stop_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(windows)]
    {
        let _ = executable("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The OS closes this job even if the application exits without running destructors.
#[derive(Debug)]
pub struct ProcessOwnership {
    #[cfg(windows)]
    _job: win32job::Job,
}

impl ProcessOwnership {
    /// Places the child in a kill-on-close job so it cannot outlive the supervisor.
    ///
    /// # Errors
    ///
    /// Returns an error (after stopping the child) when the OS job boundary
    /// cannot be established.
    pub fn attach(child: &mut Child) -> Result<Self, RuntimeError> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let create = || {
                let mut limits = win32job::ExtendedLimitInfo::new();
                limits.limit_kill_on_job_close();
                let job = win32job::Job::create_with_limit_info(&limits)?;
                job.assign_process(child.as_raw_handle() as isize)?;
                Ok::<_, win32job::JobError>(job)
            };
            if let Ok(job) = create() {
                Ok(Self { _job: job })
            } else {
                stop_child(child);
                Err(invalid("无法建立运行时进程清理边界，已停止启动"))
            }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Ok(Self {})
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn persistent_threads_resume_after_process_exit_and_quota_failure_without_double_usage() {
        let test_scope = RunEnvironment::new(None).unwrap();
        let launcher = test_scope.home.join("mock.cmd");
        fs::write(&launcher, "@powershell -NoProfile -NonInteractive -File \"%~dp0mock.ps1\"\r\n")
            .unwrap();
        fs::write(test_scope.home.join("mock.ps1"), r#"
$ErrorActionPreference = 'Stop'
$out = [Console]::Out
$history = Join-Path $env:CODEX_HOME 'mock-history.json'
$trace = Join-Path $env:CODEX_HOME 'mock-rpc.txt'
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $m = $line | ConvertFrom-Json
  Add-Content -LiteralPath $trace -Value $m.method
  if ($m.method -eq 'initialize') { $out.WriteLine('{"id":' + $m.id + ',"result":{}}') }
  elseif ($m.method -eq 'thread/start') {
    '{"turns":0,"first":""}' | Set-Content -LiteralPath $history
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
    if ($state.turns -eq 1) { $state.first = $m.params.input[0].text }
    $state | ConvertTo-Json | Set-Content -LiteralPath $history
    $out.WriteLine('{"id":' + $m.id + ',"result":{"turn":{"id":"turn"}}}')
    $out.WriteLine('{"method":"thread/tokenUsage/updated","params":{"threadId":"persistent-thread","turnId":"turn","tokenUsage":{"total":{"inputTokens":' + ($state.turns * 10) + ',"outputTokens":' + ($state.turns * 2) + '}}}}')
    if ($m.params.input[0].text -match 'quota') {
      $out.WriteLine('{"method":"turn/completed","params":{"threadId":"persistent-thread","turn":{"id":"turn","status":"failed","error":{"message":"quota exhausted"}}}}')
    } else {
      $delta = @{method='item/agentMessage/delta';params=@{delta=$state.first}} | ConvertTo-Json -Compress -Depth 6
      $out.WriteLine($delta)
      $out.WriteLine('{"method":"turn/completed","params":{"threadId":"persistent-thread","turn":{"id":"turn","status":"completed"}}}')
    }
  }
  $out.Flush()
}
"#).unwrap();
        let mut settings = RuntimeSettings::default();
        settings.codex.command = launcher.to_string_lossy().into_owned();
        settings.providers[0].native_vision = false;
        settings.providers[0].vision_model = Some("original-vision-model".into());
        settings.providers[0].vision_base_url = Some("https://vision.invalid/v1".into());
        settings.providers[0].vision_api_key_environment_variable =
            Some(settings.providers[0].api_key_environment_variable.clone());
        let image = test_scope.home.join("mock-image.png");
        fs::write(&image, b"mock image not read by the fixture").unwrap();
        let secrets = crate::secrets::SecretValues::single(
            &settings.providers[0].api_key_environment_variable,
            "mock-key-value",
        );
        let home = test_scope.home.join("project/sessions/eda/s1/codex");
        fs::create_dir_all(home.parent().unwrap()).unwrap();
        let deltas = std::cell::RefCell::new(Vec::new());
        let run = |prompt: &str, latest: &mut Option<TaskTokenUsage>| {
            if prompt != "remember first message" {
                return resume_session_streaming(
                    &settings,
                    &ToolAuthorizationSettings::default(),
                    prompt,
                    None,
                    &test_scope.home,
                    &home,
                    Some(&secrets),
                    &Cancellation::default(),
                    &mut |kind, delta| {
                        if kind == crate::TurnDelta::Answer {
                            deltas.borrow_mut().push(delta.to_owned());
                        }
                    },
                    &mut |value| *latest = Some(value),
                );
            }
            run_session_observed(
                &settings,
                &ToolAuthorizationSettings::default(),
                prompt,
                Some(&image),
                &test_scope.home,
                &home,
                Some(&secrets),
                &Cancellation::default(),
                &mut |_, _| {},
                &mut |value| *latest = Some(value),
            )
        };
        let mut usage = None;
        assert!(
            run("remember first message", &mut usage).unwrap().contains("remember first message")
        );
        assert_eq!(usage, Some(TaskTokenUsage { input_tokens: 10, output_tokens: 2 }));
        assert!(home.join("circuitfabric-thread.json").is_file());
        assert!(run("quota", &mut usage).unwrap_err().to_string().contains("quota exhausted"));
        assert_eq!(usage, Some(TaskTokenUsage { input_tokens: 10, output_tokens: 2 }));
        assert!(run("continue", &mut usage).unwrap().contains("remember first message"));
        assert!(deltas.borrow().concat().contains("remember first message"));
        assert_eq!(usage, Some(TaskTokenUsage { input_tokens: 10, output_tokens: 2 }));
        let trace = fs::read_to_string(home.join("mock-rpc.txt")).unwrap();
        assert_eq!(trace.matches("thread/start").count(), 1);
        assert_eq!(trace.matches("thread/resume").count(), 2);
        assert!(
            !fs::read_to_string(home.join("circuitfabric-thread.json"))
                .unwrap()
                .contains("mock-key-value")
        );
        fs::write(home.join("circuitfabric-thread.json"), "{").unwrap();
        assert!(
            run("continue", &mut usage).is_err(),
            "corruption must not start a fresh conversation"
        );
        let trace_after = fs::read_to_string(home.join("mock-rpc.txt")).unwrap();
        assert_eq!(trace_after.matches("thread/start").count(), 1);
        fs::remove_file(home.join("circuitfabric-thread.json")).unwrap();
        assert!(run("continue", &mut usage).unwrap_err().to_string().contains("原生会话历史"));
        assert_eq!(fs::read_to_string(home.join("mock-rpc.txt")).unwrap(), trace_after);
    }

    #[test]
    fn runtime_usage_uses_cumulative_snapshots_and_distinguishes_zero_from_missing() {
        use super::TaskTokenUsage;
        use serde_json::json;
        let snapshot = |input, output| json!({"params":{"tokenUsage":{"total":{"inputTokens":input,"outputTokens":output},"last":{"inputTokens":1,"outputTokens":1}}}});
        let reports = [snapshot(10, 2), snapshot(20, 4), snapshot(20, 4)];
        let mut latest = None;
        for report in reports {
            latest = TaskTokenUsage::from_codex_notification(&report);
        }
        assert_eq!(latest, Some(TaskTokenUsage { input_tokens: 20, output_tokens: 4 }));
        assert_eq!(
            TaskTokenUsage::from_codex_notification(&snapshot(0, 0)),
            Some(TaskTokenUsage::default())
        );
        assert_eq!(TaskTokenUsage::from_codex_notification(&json!({})), None);
        assert_eq!(TaskTokenUsage::from_codex_notification(&snapshot(-1, 2)), None);
        assert_eq!(
            TaskTokenUsage::from_claude_result(
                &json!({"usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":20,"cache_creation_input_tokens":30}})
            ),
            Some(TaskTokenUsage { input_tokens: 60, output_tokens: 4 })
        );
        assert_eq!(
            TaskTokenUsage::from_claude_result(
                &json!({"usage":{"input_tokens":0,"output_tokens":0}})
            ),
            Some(TaskTokenUsage::default())
        );
        assert_eq!(TaskTokenUsage::from_claude_result(&json!({"usage":{"input_tokens":10}})), None);
    }
    use super::*;
    use crate::LlmProviderSettings;

    #[test]
    fn adapters_written_before_kimi_still_parse() {
        let restored: AdapterSettings = serde_json::from_str(
            r#"{"claude_command":"claude","dsh_command":"dsh","codex_provider_id":"","claude_provider_id":"","dsh_provider_id":""}"#,
        )
        .expect("legacy adapter settings parse");
        assert_eq!(restored, AdapterSettings::default());
    }

    #[test]
    fn cli_failure_detail_stays_readable_with_an_empty_key() {
        // kimi-account providers have no key: replacing "" would insert
        // "[REDACTED]" between every character of the message.
        assert_eq!(
            failure_detail("boom: model `k3` is not configured", "", "", &[], None),
            "boom: model `k3` is not configured"
        );
        assert_eq!(failure_detail("", "stdout says why", "", &[], None), "stdout says why");
        assert_eq!(failure_detail("key is sekrit", "", "sekrit", &[], None), "key is [REDACTED]");
    }

    #[test]
    fn claude_uses_zai_messages_endpoint_without_changing_the_shared_provider() {
        let settings = RuntimeSettings::default();
        let scope = RunEnvironment::new(None).unwrap();
        let mut provider = settings.providers[0].clone();
        for base_url in [
            "https://api.z.ai/api/v1",
            "https://api.z.ai/api/v1/",
            "https://api.z.ai/api/coding/paas/v4",
            "https://api.z.ai/api/anthropic",
            "https://api.z.ai/api/anthropic/v1/",
        ] {
            provider.base_url = base_url.into();
            let command =
                claude_command(&settings, &provider, &[], &scope.home, "fixture-key", None)
                    .unwrap();
            let environment = command.get_envs().collect::<std::collections::BTreeMap<_, _>>();
            let value = |name: &str| environment.get(std::ffi::OsStr::new(name)).copied().flatten();
            assert_eq!(
                value("ANTHROPIC_BASE_URL"),
                Some(std::ffi::OsStr::new("https://api.z.ai/api/anthropic"))
            );
            assert_eq!(value("ANTHROPIC_AUTH_TOKEN"), Some(std::ffi::OsStr::new("fixture-key")));
            assert_eq!(value("ANTHROPIC_API_KEY"), None);
            assert_eq!(provider.base_url, base_url);
        }
        // A provider's display name/id never redirects credentials to Z.ai.
        assert_eq!(
            claude_endpoint("https://gateway.example/api/v1/"),
            ("https://gateway.example/api".into(), "ANTHROPIC_API_KEY")
        );
        assert_eq!(
            claude_endpoint("https://api.z.ai.example/api/v1"),
            ("https://api.z.ai.example/api".into(), "ANTHROPIC_API_KEY")
        );
        assert_eq!(
            claude_endpoint("http://127.0.0.1:1234/api/anthropic"),
            ("http://127.0.0.1:1234/api/anthropic".into(), "ANTHROPIC_API_KEY")
        );
    }

    #[test]
    fn dsh_rejects_zai_non_chat_endpoints_and_passes_documented_ones_through() {
        let settings = RuntimeSettings::default();
        let scope = RunEnvironment::new(None).unwrap();
        let mut provider = settings.providers[0].clone();
        // The bundled Z.ai provider ships the Responses URL; DSH would post to
        // `/chat/completions` on it and the gateway answers HTTP 500.
        for base_url in [
            "https://api.z.ai/api/v1",
            "https://api.z.ai/api/v1/",
            "https://api.z.ai/api/anthropic",
        ] {
            provider.base_url = base_url.into();
            let error =
                dsh_command(&settings, &provider, &[], &scope.home, "task", None).unwrap_err();
            let message = error.to_string();
            assert!(message.contains("Chat Completions"), "{message}");
            assert!(message.contains("/api/coding/paas/v4"), "{message}");
            assert!(message.contains("/api/paas/v4"), "{message}");
        }
        for base_url in [
            "https://api.z.ai/api/coding/paas/v4",
            "https://api.z.ai/api/paas/v4/",
            "https://api.deepseek.com/v1",
            "https://gateway.example/api/v1",
        ] {
            provider.base_url = base_url.into();
            dsh_command(&settings, &provider, &[], &scope.home, "task", None).unwrap();
            let patch = std::fs::read_to_string(scope.home.join("runtime.patch.yml")).unwrap();
            let expected = format!("\"baseURL\":\"{}\"", base_url.trim_end_matches('/'));
            assert!(patch.contains(&expected), "{patch} lacks {expected}");
        }
        // Only the exact api.z.ai host redirects; look-alikes pass through.
        assert_eq!(
            dsh_endpoint("https://api.z.ai.example/api/v1").unwrap(),
            "https://api.z.ai.example/api/v1"
        );
    }

    #[test]
    fn claude_structured_errors_keep_provider_detail_and_redact_all_keys() {
        let server = crate::tools::McpServerDefinition {
            id: "fixture".into(),
            display_name: String::new(),
            command: "unused".into(),
            args: Vec::new(),
            environment_variables: vec!["CF_TEST_MCP_SECRET".into()],
            enabled: true,
        };
        let secrets = crate::secrets::SecretValues::single("CF_TEST_MCP_SECRET", "mcp-secret");
        let output = serde_json::json!({
            "is_error": true,
            "result": "API Error: 401 invalid fixture-key",
            "errors": ["Provider rejected mcp-secret"],
        })
        .to_string();
        let detail =
            failure_detail("CLI warning", &output, "fixture-key", &[&server], Some(&secrets));
        assert!(detail.contains("API Error: 401"));
        assert!(detail.contains("Provider rejected [REDACTED]"));
        assert!(detail.contains("CLI warning"));
        assert!(!detail.contains("fixture-key"));
        assert!(!detail.contains("mcp-secret"));
        let error = with_failure_detail(invalid("运行时任务失败"), &detail).to_string();
        assert_eq!(error.matches("invalid CircuitFabric runtime settings").count(), 1);
        assert!(error.contains("API Error: 401"));
        let empty = with_failure_detail(invalid("运行时任务失败"), "").to_string();
        assert!(empty.ends_with("运行时任务失败"));
    }

    #[test]
    fn binding_overrides_default_and_rejects_disabled_provider() {
        let mut settings = RuntimeSettings::default();
        let mut second = settings.providers[0].clone();
        second.id = "second".into();
        second.model = "second-model".into();
        settings.providers.push(second);
        settings.adapters.claude_provider_id = "second".into();
        assert_eq!(selected_provider(&settings, AgentKind::Claude).unwrap().model, "second-model");
        assert_eq!(
            selected_provider(&settings, AgentKind::Codex).unwrap().id,
            settings.default_provider_id
        );
        settings.providers[1].enabled = false;
        assert!(settings.validate().is_err());
        assert!(selected_provider(&settings, AgentKind::Claude).is_none());
    }

    #[test]
    fn cancellation_fails_before_credentials_or_process_launch() {
        let cancel = Cancellation::default();
        cancel.cancel();
        for kind in [AgentKind::Codex, AgentKind::Claude, AgentKind::Dsh] {
            let error = run_task(
                &RuntimeSettings::default(),
                kind,
                &ToolAuthorizationSettings::default(),
                "test",
                &cancel,
            )
            .unwrap_err();
            assert!(error.to_string().contains("任务已取消"));
        }
    }

    #[test]
    fn codex_account_provider_rejects_non_codex_runtimes() {
        use crate::ProviderConnection;
        let mut settings = RuntimeSettings::default();
        settings.providers[0] = LlmProviderSettings {
            id: "codex-account".into(),
            name: "Codex 账号直连".into(),
            connection: ProviderConnection::CodexAccount,
            base_url: String::new(),
            model: String::new(),
            api_key_environment_variable: String::new(),
            enabled: true,
            native_vision: true,
            vision_base_url: None,
            vision_model: None,
            vision_api_key_environment_variable: None,
            ..LlmProviderSettings::default()
        };
        settings.default_provider_id = "codex-account".into();
        settings.validate().expect("a direct provider stays valid as the default");

        let error = run_task(
            &settings,
            AgentKind::Claude,
            &ToolAuthorizationSettings::default(),
            "test",
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("仅支持 Codex 运行时"));
    }

    #[test]
    fn adapters_reject_codex_account_bindings_for_cli_runtimes() {
        use crate::ProviderConnection;
        let mut settings = RuntimeSettings::default();
        let direct = LlmProviderSettings {
            id: "codex-account".into(),
            name: "Codex 账号直连".into(),
            connection: ProviderConnection::CodexAccount,
            ..LlmProviderSettings::default()
        };
        settings.providers.push(direct);
        settings.adapters.claude_provider_id = "codex-account".into();
        assert!(matches!(
            settings.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("仅支持 Codex 运行时")
        ));
        settings.adapters.claude_provider_id = String::new();
        settings.adapters.dsh_provider_id = "codex-account".into();
        assert!(matches!(
            settings.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("仅支持 Codex 运行时")
        ));
    }

    #[test]
    fn kimi_account_provider_needs_only_a_name_and_binds_only_to_kimi() {
        use crate::ProviderConnection;
        let mut settings = RuntimeSettings::default();
        let kimi = LlmProviderSettings {
            id: "kimi-account".into(),
            name: "Kimi 账号直连".into(),
            connection: ProviderConnection::KimiAccount,
            base_url: String::new(),
            model: String::new(),
            api_key_environment_variable: String::new(),
            enabled: true,
            native_vision: true,
            vision_base_url: None,
            vision_model: None,
            vision_api_key_environment_variable: None,
            ..LlmProviderSettings::default()
        };
        settings.providers.push(kimi);
        settings.default_provider_id = "kimi-account".into();
        settings.adapters.kimi_provider_id = "kimi-account".into();
        settings.validate().expect("a kimi direct provider stays valid");

        // The Kimi runtime refuses any provider that is not a kimi-account one
        // (here the default Z.ai API-key provider), before any process starts.
        let error = run_task(
            &RuntimeSettings::default(),
            AgentKind::Kimi,
            &ToolAuthorizationSettings::default(),
            "test",
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("仅支持 Kimi 账号直连 Provider"));

        // Other runtimes refuse a kimi-account provider.
        for kind in [AgentKind::Codex, AgentKind::Claude, AgentKind::Dsh] {
            let error = run_task(
                &settings,
                kind,
                &ToolAuthorizationSettings::default(),
                "test",
                &Cancellation::default(),
            )
            .unwrap_err();
            assert!(error.to_string().contains("Kimi 账号直连 Provider 仅支持 Kimi 运行时"));
        }

        // A kimi-account binding on a non-Kimi adapter fails validation.
        settings.adapters.codex_provider_id = "kimi-account".into();
        assert!(matches!(
            settings.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("仅支持 Kimi 运行时")
        ));
        settings.adapters.codex_provider_id = String::new();
        settings.adapters.claude_provider_id = "kimi-account".into();
        assert!(matches!(
            settings.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("仅支持 Kimi 运行时")
        ));
        settings.adapters.claude_provider_id = String::new();
        settings.adapters.dsh_provider_id = "kimi-account".into();
        assert!(matches!(
            settings.validate(),
            Err(RuntimeError::InvalidSettings(message)) if message.contains("仅支持 Kimi 运行时")
        ));
    }

    #[test]
    fn a_scoped_run_keeps_the_scoped_directory_and_cleans_only_its_home() {
        let scope = RunEnvironment::new(None).unwrap();
        let project_root = scope.home.clone();
        let environment = RunEnvironment::new(Some(&project_root)).unwrap();

        assert_eq!(environment.current, project_root);
        assert_ne!(environment.home, project_root);
        drop(environment);
        assert!(project_root.exists(), "a scoped working directory is never cleaned up");
    }

    #[test]
    fn a_missing_working_directory_is_rejected_before_launch() {
        let cancel = Cancellation::default();
        let missing = PathBuf::from("circuitfabric-no-such-working-directory");
        for kind in [AgentKind::Codex, AgentKind::Claude, AgentKind::Dsh] {
            let error = run_task_with_options(
                &RuntimeSettings::default(),
                kind,
                &ToolAuthorizationSettings::default(),
                "test",
                None,
                Some(&missing),
                &cancel,
            )
            .unwrap_err();
            assert!(error.to_string().contains("工作目录不存在"));
        }
        // A real directory reaches the next guard (empty prompt) instead.
        let existing = env::temp_dir();
        let error = run_task_with_options(
            &RuntimeSettings::default(),
            AgentKind::Codex,
            &ToolAuthorizationSettings::default(),
            "   ",
            None,
            Some(&existing),
            &cancel,
        )
        .unwrap_err();
        assert!(error.to_string().contains("任务不能为空"));
    }

    #[test]
    fn failed_save_preserves_last_valid_configuration() {
        let environment = RunEnvironment::new(None).unwrap();
        let path = environment.home.join("runtime.json");
        let mut settings = RuntimeSettings::default();
        settings.save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        settings.providers[0].api_key_environment_variable = "sk-not-an-environment-name".into();
        assert!(settings.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let loaded = RuntimeSettings::load_or_default(&path).unwrap();
        assert_eq!(
            loaded.providers[0].api_key_environment_variable,
            RuntimeSettings::default().providers[0].api_key_environment_variable
        );
        assert!(loaded.save(&environment.home).is_err());
        assert!(!environment.home.with_extension(format!("{}.tmp", std::process::id())).exists());
    }
}
