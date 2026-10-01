# Provider settings

The desktop control plane manages multiple OpenAI-compatible LLM providers. The first provider is a Z.ai-compatible example based on the legacy JLCircuit-Agent settings; its API key is not included.

Runtime settings are saved outside the repository at the platform application-data path. Each provider has:

- `id`, `name`, `base_url`, and `model`;
- `api_key_environment_variable`, which is a variable name rather than a secret value;
- `enabled` and `supports_vision`;
- optional vision `base_url`, `model`, and API-key environment-variable name.

The selected default provider is used when the JLCircuit bridge starts Codex App Server. Multiple providers can be added, edited, enabled/disabled, and switched as the default in the desktop UI.

Example shape, with no credentials:

```json
{
  "default_provider_id": "zai",
  "providers": [
    {
      "id": "zai",
      "name": "Z.ai",
      "base_url": "https://api.z.ai/api/coding/paas/v4",
      "model": "glm-5.3-flash",
      "api_key_environment_variable": "JLCIRCUIT_LLM_API_KEY",
      "enabled": true,
      "supports_vision": true,
      "vision_base_url": "https://openrouter.ai/api/v1",
      "vision_model": "z-ai/glm-5.3-flash",
      "vision_api_key_environment_variable": "JLCIRCUIT_VISION_LLM_API_KEY"
    }
  ]
}
```

The bridge uses the Codex adapter's selected provider. The desktop image input routes through the selected provider's separate Vision URL, model and environment-variable reference. Bridge messages currently accept text only.

## Runtime binding and protocols

`adapters.codex_provider_id`, `adapters.claude_provider_id` and `adapters.dsh_provider_id` independently select providers; an empty binding uses the default. Disabled and missing bindings fail validation. Codex requires Responses, Claude Code requires Anthropic Messages, and DSH uses Chat Completions through its `llm-deepseek` adapter. A service must support that protocol and tool calling. Claude normalizes a trailing `/v1` to avoid duplicate path segments.

Each task creates an isolated process/session from saved configuration. Results show the provider, model and service URL used by that task. Save form changes before running the next task. The Codex Start button performs a separate JSON-RPC connection check; restart that process to change its configuration. Claude and DSH use per-task processes rather than a persistent idle server.

## 保存操作的范围

「智能体与工具」取消右上角的总保存按钮，各配置在对应详情页保存：

| 操作 | 保存内容 | 生效时间 |
| --- | --- | --- |
| 保存 Provider 列表 | 所有 Provider 的新增、编辑、删除、启停、Vision 配置及默认项 | 下次任务；运行中的服务重启后 |
| 保存 Codex 配置 | Codex 命令、工作目录和 Provider 关联 | 下次任务；连接检查进程重启后 |
| 保存 Claude Code / DSH 配置 | 对应运行时的命令和 Provider 关联 | 下次任务 |
| EDA 服务：保存 Bridge 地址 | 此服务的监听地址 | 下次启动；运行中需重启 |
| 技能 / MCP：导入、保存定义、启停、删除 | 技能与 MCP 定义 | 即时保存；取消当前任务，新任务使用新配置 |
| 授权 / 撤销 | 选定的全局或项目作用域授权 | 即时保存，现有任务请求取消 |
| Jev 弹窗：保存后端 / 密钥 | 判断后端配置 / 加密保险库中的对应变量 | 后端用于下次调用；密钥由保险库独立保存 |
| 全局设置：保存全局设置 | 外观、语言、数据目录、密钥来源和日志级别 | 独立保存；主题、语言、密钥来源和日志级别的切换即时保存 |

Provider 和各运行时详情展示是否存在未保存修改。切换页面、Provider 或运行时保留草稿，退出应用不保留未保存草稿。启动、连接测试、发起任务和文档判断不会代替保存；执行读取已保存配置（首次未配置时采用默认配置）。新增 Provider 必须先保存，才能保存引用它的运行时配置。Codex 显示当前进程使用的 Provider 快照和下次启动使用的配置；Bridge 监测和测试使用当前服务地址，未运行时使用已保存地址。

每次局部保存先读取最新配置，只替换对应部分，再校验并原子写入。其他页面的草稿不会被顺带提交；读写或校验失败不会更新已保存快照，Jev 保存失败会保留弹窗及草稿供修正或重试。

2026-10-01 本地验证：`cargo test -p circuitfabric-desktop --bin circuitfabric-desktop settings_persistence` 的 5 项测试通过，覆盖跨页面局部保存、连续保存后重新加载、其他页面无效草稿隔离、无效 Provider 引用及读写失败保护；`cargo check -p circuitfabric-desktop --features native-ui` 和 `cargo build -p circuitfabric-desktop --features native-ui` 通过。本次尚未进行原生界面逐项点击验收或外部运行时真实调用，不作为整项运行时功能验收完成的证据。

## Skills and MCP definitions

Import a directory containing SKILL.md or its full file path. The directory name becomes the skill ID. The catalog supports preview, enable/disable, deletion and direct authorization into the selected scope. Only authorized, enabled files are loaded into task instructions; files are limited to 256 KiB. Referenced scripts and resources are not recursively loaded or executed.

MCP currently supports local stdio servers. Configure an executable, a JSON array of arguments and names of inherited environment variables. Save and authorize the definition before testing the connection. The test performs real initialize and tools/list requests; runtime tasks use native MCP clients to call tools.

One server ships with the application: `typesafe-jev`, the TypeSafe Jev `evaluate` server built from `vendor/typesafe-mcp` (see `native/README.md`). Its catalog entry is added automatically when the bundled binary is present; it follows the same rules as any other server — explicit authorization and a `TYPESAFE_API_KEY` value from the vault or launch environment are still required before agents can call it.

The Jev settings page can also switch this same server to an embedded Rust LLM adapter. Configure a Chat Completions-compatible base URL, model and key variable (DeepSeek/z.ai presets provided), then save the backend and its key. It supports probabilities/discrete answers, JSON Object/JSON Schema/prompted JSON, normalization, timeouts and malformed-output retries. The adapter retains the `evaluate` contract used by agents and datasheet review, and labels its estimates as uncalibrated. See [configuration and protocol details](llm-judgments.md).

```json
{
  "catalog": {
    "skills": [{"id":"review","path":"C:/skills/review/SKILL.md","enabled":true}],
    "mcp_servers": [{
      "id":"parts", "command":"python",
      "args":["C:/tools/parts_server.py"],
      "environment_variables":["PARTS_API_TOKEN"], "enabled":true
    }]
  },
  "tools": {
    "authorized_skill_ids":["review"],
    "authorized_mcp_server_ids":["parts"]
  }
}
```

Never put credentials in command arguments or URLs. Supply the named variables either from the desktop application's launch environment, or from the built-in secrets vault (see below). Authorizing a local MCP executable permits it to run local code; model tool restrictions do not sandbox a malicious server.

## Secrets vault

The desktop app keeps every API key value in one encrypted store, `secrets.vault.json` next to `runtime.json`. The file holds only AES-256-GCM ciphertext under a key derived with PBKDF2-HMAC-SHA256 (600k iterations) from a vault password the user chooses at creation; a plaintext index of variable *names* (never values) lets the locked screen show what is stored. On startup the app prompts for the password when a vault exists; unlocking holds only the derived key in zeroized memory until relock or exit.

At run time a whitelisted variable name resolves to the unlocked vault value first and to the process environment second. Task, MCP-test, supervised Codex and desktop-spawned bridge processes all receive values strictly through child-process environment injection; nothing is logged or passed on command lines. A missing value fails the task with a message naming both remedies. A manually started bridge never sees the vault: give it real environment variables, or start it from the desktop.

Existing deployments that already export `JLCIRCUIT_LLM_API_KEY` etc. keep working unchanged — the vault is additive, and an OS-level variable only applies when the unlocked vault does not define the same name.

## Scope, persistence and cleanup

Provider, runtime and catalog definitions are global. Projects persist their own grants and instructions. Effective grants are the deduplicated union of global and current-project grants; another project's grants never participate. A disabled or missing definition fails closed even if its ID remains authorized. Revoking a project grant does not override a global grant: revoke globally or disable the definition to prohibit it everywhere.

Desktop revocation, catalog changes and project switches cancel the active task. Bridge tasks monitor configuration files and cancel when they change. New tasks recalculate authorization. Configuration writes validate first, then replace the destination through a temporary file. Failed catalog/grant writes restore prior in-memory configuration and report failure.

Task processes use isolated temporary working/configuration directories and explicit project instructions; Codex does not read parent-directory project instructions. Engineering access is through authorized MCP servers. The configured working directory is currently used by the Codex connection-check process; task processes use isolated directories. Windows Job Objects with KILL_ON_JOB_CLOSE own child process trees, including application-exit cleanup. Temporary configurations are removed at task completion; live processes and sessions are not persisted.

## Verification boundary

See [runtime integration validation](runtime-integration-validation.md). Real remote inference, complete native UI acceptance, referenced skill assets, project-level definition overrides and per-tool permissions remain unverified or incomplete. Passing local protocol tests does not complete all acceptance criteria for issue 398.
