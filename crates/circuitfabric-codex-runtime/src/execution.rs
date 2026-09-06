//! Concrete one-task adapters. A new process owns each task and is always reaped.
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
    #[serde(default)]
    pub codex_provider_id: String,
    #[serde(default)]
    pub claude_provider_id: String,
    #[serde(default)]
    pub dsh_provider_id: String,
}
impl Default for AdapterSettings {
    fn default() -> Self {
        Self {
            claude_command: "claude".into(),
            dsh_command: "dsh".into(),
            codex_provider_id: String::new(),
            claude_provider_id: String::new(),
            dsh_provider_id: String::new(),
        }
    }
}
impl AdapterSettings {
    pub(crate) fn validate(&self, settings: &RuntimeSettings) -> Result<(), RuntimeError> {
        for id in [&self.codex_provider_id, &self.claude_provider_id, &self.dsh_provider_id] {
            if !id.is_empty() && !settings.providers.iter().any(|p| &p.id == id && p.enabled) {
                return Err(invalid("运行时引用了不存在或已停用的 Provider"));
            }
        }
        if self.claude_command.trim().is_empty() || self.dsh_command.trim().is_empty() {
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
    if let Some(directory) = working_directory
        && !directory.is_dir()
    {
        return Err(invalid("工作目录不存在或不是文件夹"));
    }
    run_task_in(settings, kind, grants, prompt, image, working_directory, cancel)
}

fn run_task_in(
    settings: &RuntimeSettings,
    kind: AgentKind,
    grants: &ToolAuthorizationSettings,
    prompt: &str,
    image: Option<&std::path::Path>,
    working_directory: Option<&std::path::Path>,
    cancel: &Cancellation,
) -> Result<String, RuntimeError> {
    settings.validate()?;
    if cancel.0.load(Ordering::SeqCst) {
        return Err(invalid("任务已取消"));
    }
    if prompt.trim().is_empty() {
        return Err(invalid("任务不能为空"));
    }
    let mut provider =
        selected_provider(settings, kind).ok_or_else(|| invalid("Provider 不可用"))?.clone();
    if image.is_some() {
        if kind != AgentKind::Codex || !provider.supports_vision {
            return Err(invalid("图片任务需要 Codex 运行时及已启用的 Vision 配置"));
        }
        provider.base_url =
            provider.vision_base_url.clone().ok_or_else(|| invalid("缺少 Vision 服务地址"))?;
        provider.model =
            provider.vision_model.clone().ok_or_else(|| invalid("缺少 Vision 模型"))?;
        provider.api_key_environment_variable = provider
            .vision_api_key_environment_variable
            .clone()
            .ok_or_else(|| invalid("缺少 Vision 环境变量名"))?;
    }
    let key = env::var(&provider.api_key_environment_variable)
        .map_err(|_| invalid(format!("缺少环境变量 {}", provider.api_key_environment_variable)))?;
    if key.is_empty() {
        return Err(invalid("API Key 环境变量为空"));
    }
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
            if env::var_os(name).is_none() {
                return Err(invalid(format!("缺少环境变量 {name}")));
            }
        }
    }
    let environment = RunEnvironment::new(working_directory)?;
    let input = format!("{instructions}\n\n{prompt}");
    if kind == AgentKind::Codex {
        let mut inputs = vec![serde_json::json!({"type":"text","text":input})];
        if let Some(path) = image {
            inputs.push(serde_json::json!({"type":"localImage","path":fs::canonicalize(path)?}));
        }
        return run_codex(settings, &provider, &servers, &environment, grants, &inputs, cancel)
            .map(|output| redact(&output, &key, &servers));
    }
    let command = if kind == AgentKind::Claude {
        claude_command(settings, &provider, &servers, &environment.home, &key)?
    } else {
        dsh_command(settings, &provider, &servers, &environment.home, &input)?
    };
    execute_cli(command, &environment, input, kind, cancel, &key, &servers)
}

fn run_codex(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    environment: &RunEnvironment,
    grants: &ToolAuthorizationSettings,
    input: &[serde_json::Value],
    cancel: &Cancellation,
) -> Result<String, RuntimeError> {
    let mut command = crate::app_server_command(&settings.codex, provider);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers);
    command
        .current_dir(&environment.current)
        .env("CODEX_HOME", &environment.home)
        .args(["-c", "features.multi_agent=false"])
        .args([
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
        let discovered = settings.catalog.list_tools(&server.id, grants)?;
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
    client.initialize()?;
    let thread = client.start_thread(Some(&provider.model))?;
    let mut output = String::new();
    client.run_turn_with_input(&thread, input, |delta| output.push_str(delta))?;
    Ok(output)
}

fn claude_command(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    home: &std::path::Path,
    key: &str,
) -> Result<std::process::Command, RuntimeError> {
    let mut command = executable(&settings.adapters.claude_command);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers);
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
        .env(
            "ANTHROPIC_BASE_URL",
            provider
                .base_url
                .trim_end_matches('/')
                .strip_suffix("/v1")
                .unwrap_or(provider.base_url.trim_end_matches('/')),
        )
        .env("ANTHROPIC_API_KEY", key);
    if !servers.is_empty() {
        command
            .arg("--allowedTools")
            .arg(servers.iter().map(|s| format!("mcp__{}__*", s.id)).collect::<Vec<_>>().join(","));
    }
    Ok(command)
}

fn dsh_command(
    settings: &RuntimeSettings,
    provider: &crate::LlmProviderSettings,
    servers: &[&crate::tools::McpServerDefinition],
    home: &std::path::Path,
    input: &str,
) -> Result<std::process::Command, RuntimeError> {
    let mut command = executable(&settings.adapters.dsh_command);
    restrict_environment(&mut command, &provider.api_key_environment_variable, servers);
    let patch = home.join("runtime.patch.yml");
    let mut patches = vec![
        serde_json::json!({"id":"agent-default-model","config":{"provider":"deepseek-official","model":provider.model}}),
        serde_json::json!({"id":"llm-deepseek","config":{"baseURL":provider.base_url,"apiKeyEnv":provider.api_key_environment_variable,"thinking":"disabled","maxTokens":8192,"models":[{"id":provider.model}]}}),
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

fn execute_cli(
    mut command: std::process::Command,
    environment: &RunEnvironment,
    input: String,
    kind: AgentKind,
    cancel: &Cancellation,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
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
                    Err(invalid("运行时任务失败，请检查服务地址、协议及模型"))
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
    if let Err(error) = outcome {
        let mut detail = error_output.replace(key, "[REDACTED]");
        for server in servers {
            for name in &server.environment_variables {
                if let Ok(value) = env::var(name)
                    && !value.is_empty()
                {
                    detail = detail.replace(&value, "[REDACTED]");
                }
            }
        }
        return Err(invalid(format!("{error}: {}", detail.chars().take(8000).collect::<String>())));
    }
    if kind == AgentKind::Dsh {
        return Ok(redact(&output, key, servers));
    }
    let response: serde_json::Value = serde_json::from_str(&output)?;
    if response["is_error"] == true {
        return Err(invalid("Claude Code 返回任务错误"));
    }
    Ok(redact(
        response["result"].as_str().ok_or_else(|| invalid("Claude Code 未返回文本结果"))?,
        key,
        servers,
    ))
}

pub(crate) fn redact(
    text: &str,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
) -> String {
    let mut output = if key.is_empty() { text.to_owned() } else { text.replace(key, "[REDACTED]") };
    for name in servers.iter().flat_map(|s| &s.environment_variables) {
        if let Ok(value) = env::var(name)
            && !value.is_empty()
        {
            output = output.replace(&value, "[REDACTED]");
        }
    }
    output
}

fn restrict_environment(
    command: &mut std::process::Command,
    key: &str,
    servers: &[&crate::tools::McpServerDefinition],
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
    {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
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
        Ok(Self { home, current })
    }
}
impl Drop for RunEnvironment {
    fn drop(&mut self) {
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
    use super::*;

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
