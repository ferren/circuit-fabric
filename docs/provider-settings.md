# Provider settings

The desktop control plane manages multiple LLM providers. The first provider is a Z.ai-compatible example based on the legacy JLCircuit-Agent settings; its API key is not included.

Runtime settings are saved outside the repository at the platform application-data path. Each provider has:

- `id` and `name`;
- `connection` (defaults to `api_key`): `api_key` is an OpenAI-compatible endpoint authenticated by an API-key environment-variable name; `codex_account` is a direct provider that reuses the local Codex installation's own ChatGPT sign-in (`codex login`, `~/.codex/auth.json`) — no `base_url` or API key applies, the model may be left empty to use the Codex default, and only the Codex runtime may use it (Claude Code, DSH and Kimi Code bindings reject it at validation); `kimi_account` is a direct provider that reuses the local Kimi Code CLI's own sign-in (`kimi login`, `~/.kimi-code`) — no `base_url` or API key applies, the model may be left empty to use the kimi default, and only the Kimi runtime may use it (Codex, Claude Code and DSH bindings reject it at validation);
- for `api_key` providers: `wire_api` (defaults to `responses`) — the protocol the Codex runtime uses against the endpoint. Modern codex-cli builds launch only with `responses` (`wire_api = "chat"` is rejected at startup); `chat` stays selectable for older codex installations whose endpoints only implement chat completions;
- for `api_key` providers: `base_url`, `model`, and `api_key_environment_variable` (a variable name rather than a secret value);
- `enabled` and `native_vision` (whether the LLM itself processes images);
- optional vision `base_url`, `model`, and API-key environment-variable name.

A direct (`codex_account`) provider launches `codex app-server` without any `model_provider`/`model_providers.*` override and without the isolated `CODEX_HOME`, so the child process reads the real local sign-in. It requires the user to have completed `codex login` (chatgpt.com web authorization) beforehand. Image tasks need `native_vision`, since a direct provider has no separate vision endpoint.

The selected default provider is used when the JLCircuit bridge starts Codex App Server. Multiple providers can be added, edited, enabled/disabled, and switched as the default in the desktop UI.

Example shape, with no credentials:

```json
{
  "default_provider_id": "zai",
  "providers": [
    {
      "id": "zai",
      "name": "Z.ai",
      "connection": "api_key",
      "base_url": "https://api.z.ai/api/v1",
      "wire_api": "responses",
      "model": "glm-5.3-flash",
      "api_key_environment_variable": "JLCIRCUIT_LLM_API_KEY",
      "enabled": true,
      "native_vision": false,
      "vision_base_url": "https://openrouter.ai/api/v1",
      "vision_model": "z-ai/glm-5.3-flash",
      "vision_api_key_environment_variable": "JLCIRCUIT_VISION_LLM_API_KEY"
    }
  ]
}
```

The bridge uses the Codex adapter's selected provider. Image tasks follow the provider's vision mode: with `native_vision` the image goes straight to the main model and the separate vision fields are unused (the desktop grays them out but keeps their values); without it, the image routes through the separate Vision URL, model and environment-variable reference, which image tasks require at run time. Bridge messages currently accept text only.

## Runtime binding and protocols

`adapters.codex_provider_id`, `adapters.claude_provider_id`, `adapters.dsh_provider_id` and `adapters.kimi_provider_id` independently select providers; an empty binding uses the default. Disabled and missing bindings fail validation. Codex requires Responses, Claude Code requires Anthropic Messages, and DSH uses Chat Completions through its `llm-deepseek` adapter (`{baseURL}/chat/completions`). A service must support that protocol and tool calling. Claude normalizes a trailing `/v1` to avoid duplicate path segments.

Because DSH posts to `{baseURL}/chat/completions`, a Z.ai provider must carry a chat-completions base URL: `https://api.z.ai/api/coding/paas/v4` (Coding Plan keys) or `https://api.z.ai/api/paas/v4` (pay-as-you-go keys). Binding the bundled Z.ai provider as-is fails at task launch with an error naming both endpoints — its `https://api.z.ai/api/v1` serves only OpenAI Responses, and z.ai answers `/api/v1/chat/completions` with HTTP 500 (observed as `dsh: SERVER: DeepSeek API error (HTTP 500)`). Unlike the Claude case there is no safe per-invocation rewrite, because the two chat endpoints differ by key billing and a wrong pick returns 1113/401 from the provider; the launch-time guard covers explicit bindings and the default provider alike.

Each task creates an isolated process/session from saved configuration. Results show the provider, model and service URL used by that task. Save form changes before running the next task. The Codex endpoint offers a one-shot connection check: the saved configuration is launched once, must complete the JSON-RPC initialization handshake, and the process stops immediately — nothing stays resident. Claude, DSH and Kimi Code use per-task processes rather than a persistent idle server. Kimi Code requires a `kimi_account` provider: the `kimi` CLI runs with `--prompt <task> --output-format stream-json` and answers from the assistant events' concatenated text; the local `kimi` sign-in supplies credentials and no usage is reported.

The provider-binding chips enforce the same exclusivity at selection time: a `codex_account` provider renders as a disabled, muted chip ("Codex App Server only") on the Claude Code, DSH and Kimi Code adapters, and a `kimi_account` provider likewise only on Kimi Code. A currently saved incompatible binding stays visible through its selected border until switched; save-time validation rejects it either way.

## Model selection

The provider editor's model field is a searchable type-or-pick: free text always works, and the catalog is fetched automatically in the background — when the editor opens, when the connection mode switches, or when the field is focused after the endpoint coordinates changed; there is no fetch button. Clicking the field or its trailing caret opens a floating dropdown flush with the field's bottom-left corner (an overlay that takes no layout space and closes on a pick, on blur, or on any outside click); the field doubles as the search box — its current value filters the catalog by substring against the id and display name (a value holding an exact catalog id counts as a finished selection and shows the catalog's first rows). The panel pages through matches web-combobox style: ten rows stay visible, the open dropdown already holds fifteen, and a scrollbar appears whenever more matches exist — wheeling or dragging the scrollbar to the loaded bottom loads the next page of fifteen instantly (the catalog is cached in full, so paging never fires new requests; a trailing affordance loads more on click too). Filtering is likewise local over the fetched catalog: endpoints such as OpenRouter offer no server-side name search (their Models API filters by modality/parameter only), so typing never triggers new requests. Rows carry badges for the source's default model and, when the source reports it, image/vision support (Codex `inputModalities`, kimi `image_in` capability, OpenRouter-style `architecture.input_modalities`); endpoint catalogs are kept up to 1000 entries so OpenRouter's full list (~460 today) stays searchable. The fetch reads the current draft (base URL and key variable for endpoints), so a freshly typed endpoint works before saving; the list is keyed to the edited provider's draft signature and disappears when the provider, connection mode, or endpoint changes. On the provider detail pane, account-direct providers show the same Vision badge API-key providers get from their vision configuration once the fetched catalog reports the selected model (or the catalog default when the model field is empty) as vision-capable; direct catalogs are prefetched in the background at startup.

| Connection | Source | Verified |
| --- | --- | --- |
| `codex_account` | `codex app-server` JSON-RPC `model/list` (paginated; skips server-hidden entries, marks the account default) | 2026-10-14 live: 8 models incl. `gpt-6.1-sol (GPT-6.1-Sol)` as default; all report image input |
| `kimi_account` | `kimi provider list --json` — aliases exactly as `--model` accepts, with display names | 2026-10-14 live: 4 aliases incl. `kimi-code/k3 (K3)`; all report `image_in` |
| `api_key` | OpenAI-compatible `GET {base_url}/models` with the referenced key (vault first, environment second), sorted, capped at 400 choices | transport live-verified (timeout, HTTP 401, and z.ai `/api/v1`'s HTTP-200 error envelope — surfaced verbatim; success parsing unit-tested) |

Endpoints that do not implement `/models` surface their HTTP status with a hint to keep typing manually. Gateways that answer HTTP 200 with an error body — z.ai's gateway returns `{"code":401,"msg":"token expired or incorrect","success":false}` when auth fails or the path is unknown — have their message surfaced verbatim. Codex builds that predate `model/list` reject the request; upgrading the CLI fixes it.

z.ai endpoints differ by protocol, and the provider's `wire_api` must match the base URL: the bundled Z.ai provider uses the documented Codex combination `https://api.z.ai/api/v1` + `responses` (z.ai's dedicated OpenAI-Responses endpoint; model `glm-5.3`). `https://api.z.ai/api/coding/paas/v4` is a chat-completions endpoint — it serves `GET /models` but has no `/responses`, so Codex tasks against it fail with HTTP 404.

Claude Code always uses Anthropic Messages; the Codex `wire_api` selector does not change that protocol. When Claude is bound to a Z.ai provider using `/api/v1` or the Coding Plan `/api/coding/paas/v4` URL on `api.z.ai`, CircuitFabric selects `https://api.z.ai/api/anthropic` for that invocation and supplies the key as `ANTHROPIC_AUTH_TOKEN`, following [Z.ai's Claude Code configuration](https://docs.z.ai/devpack/tool/claude). An explicit `/api/anthropic` URL (with an optional `/v1` suffix) also works. The saved provider URL, model and key reference are retained, so a shared Codex binding keeps its own route. Custom gateway addresses are not redirected based on provider name/id and must implement Anthropic Messages themselves; an OpenAI-compatible service alone is insufficient. CLI failures include the exit status and Claude's structured error result, with provider/MCP credentials redacted, rather than hiding them behind a generic message or duplicating the settings-error prefix.

Kimi Code itself also supports third-party model API keys, managed in kimi's own configuration rather than in CircuitFabric: `kimi provider add <registry-url> --api-key <key>` imports providers from a custom `api.json` registry, `kimi provider catalog list` / `kimi provider catalog add <providerId>` import from the public models.dev catalog, and `kimi provider remove <providerId>` drops one. Imported aliases appear in `kimi provider list` and can be typed into a Kimi account provider's model field — CircuitFabric passes the value straight to `--model`. Keys imported that way live in kimi's own `~/.kimi-code` store; CircuitFabric's vault is not involved for direct providers.

## 保存操作的范围

「智能体与工具」取消右上角的总保存按钮，各配置在对应详情页保存：

| 操作 | 保存内容 | 生效时间 |
| --- | --- | --- |
| 保存 Provider 列表 | 所有 Provider 的新增、编辑、删除、启停、Vision 配置及默认项 | 下次任务；运行中的服务重启后 |
| 保存 Codex 配置 | Codex 命令和 Provider 关联 | 下次任务；连接检查进程重启后 |
| 保存 Claude Code / DSH / Kimi Code 配置 | 对应运行时的命令和 Provider 关联 | 下次任务 |
| EDA 服务：保存 Bridge 地址 | 此服务的监听地址 | 下次启动；运行中需重启 |
| 技能 / MCP：导入、保存定义、启停、删除 | 技能与 MCP 定义 | 即时保存；取消当前任务，新任务使用新配置 |
| 授权 / 撤销 | 选定的全局或项目作用域授权 | 即时保存，现有任务请求取消 |
| Jev 弹窗：保存后端 / 密钥 | 判断后端配置 / 加密保险库中的对应变量 | 后端用于下次调用；密钥由保险库独立保存 |
| 全局设置：保存全局设置 | 外观、语言、数据目录、密钥来源和日志级别 | 独立保存；主题、语言、密钥来源和日志级别的切换即时保存 |

Provider 和各运行时详情展示是否存在未保存修改。切换页面、Provider 或运行时保留草稿，退出应用不保留未保存草稿。Provider 编辑弹窗的两个取消按钮和点击遮罩关闭均丢弃本次编辑，恢复打开前的字段与模型目录，并作废尚未完成的草稿模型查询；列表和详情随之还原，不写设置，也不清除其他 Provider 的草稿。保存失败时保留弹窗及输入，继续修正或取消；成功保存后，下次编辑的取消会恢复到这次保存后的值。启动、连接测试、发起任务和文档判断不会代替保存；执行读取已保存配置（首次未配置时采用默认配置）。新增 Provider 必须先保存，才能保存引用它的运行时配置。Codex 显示当前进程使用的 Provider 快照和下次启动使用的配置；Bridge 监测和测试使用当前服务地址，未运行时使用已保存地址。

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

At run time a whitelisted variable name resolves to the unlocked vault value first and to the process environment second. Task, MCP-test, connection-check Codex and desktop-spawned bridge processes all receive values strictly through child-process environment injection; nothing is logged or passed on command lines. A missing value fails the task with a message naming both remedies. A manually started bridge never sees the vault: give it real environment variables, or start it from the desktop.

Existing deployments that already export `JLCIRCUIT_LLM_API_KEY` etc. keep working unchanged — the vault is additive, and an OS-level variable only applies when the unlocked vault does not define the same name.

## Scope, persistence and cleanup

Provider, runtime and catalog definitions are global. Projects persist their own grants and instructions. Without a selected project, effective grants come from enabled global definitions. With a project, effective grants are the intersection of global and current-project grants, restricted to existing, enabled definitions; another project's grants never participate. A project cannot expand global permissions. Revoking either the global or current-project grant prevents subsequent project-scoped use. See [skills and MCP management](skills-mcp-management.md) for the management flow and validation evidence.

Desktop revocation, catalog changes and project switches cancel the active task. Bridge tasks monitor configuration files and cancel when they change. New tasks recalculate authorization. Configuration writes validate first, then replace the destination through a temporary file. Failed catalog/grant writes restore prior in-memory configuration and report failure.

桌面不驻留 Codex 进程：连接检查用已保存配置临时启动一次、握手成功后立即结束。任务与数据手册提取的工作目录由当前项目根目录逐次确定，不属于全局运行时设置。旧配置文件中的 `codex.working_directory` 字段保留以兼容已有配置，任务不再读取它作为工作目录；保存命令或 Provider 不会写入项目路径。工作目录表示进程的起始目录，访问权限仍由运行时沙箱及授权规则决定。

Task processes use the selected project root as their working directory and an isolated temporary configuration/home directory. Without a project, the task test panel uses an isolated temporary working directory. Codex receives explicit project instructions rather than inheriting parent-directory project instructions. Engineering access is through authorized MCP servers. Windows Job Objects with KILL_ON_JOB_CLOSE own child process trees, including application-exit cleanup. Temporary configurations are removed at task completion; project directories are never removed. Live processes and sessions are not persisted.

## Verification boundary

2026-10-02：项目工作目录修改在独立工作副本验证：`cargo test -p circuitfabric-desktop --bin circuitfabric-desktop`（24 项通过），`cargo build -p circuitfabric-desktop --features native-ui` 通过。新增测试覆盖切换项目时选择对应根目录、兼容旧目录且不修改保存配置、未选择项目及无效目录报错；保存测试确认命令与 Provider 的修改保留原有目录字段。尚未执行原生界面点击或真实外部 Codex 调用，启动中切换项目的清理路径已实现但尚无人工验收记录。

See [runtime integration validation](runtime-integration-validation.md). Real remote inference, complete native UI acceptance, referenced skill assets, project-level definition overrides and per-tool permissions remain unverified or incomplete. Passing local protocol tests does not complete all acceptance criteria for issue 398.
