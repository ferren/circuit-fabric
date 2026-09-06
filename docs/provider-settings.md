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

Each task creates an isolated process/session from saved configuration. Results show the provider, model and service URL used by that task. Form changes affect the next task. The Codex Start button performs a separate JSON-RPC connection check; restart that process to change its configuration. Claude and DSH use per-task processes rather than a persistent idle server.

## Skills and MCP definitions

Import a directory containing SKILL.md or its full file path. The directory name becomes the skill ID. The catalog supports preview, enable/disable, deletion and direct authorization into the selected scope. Only authorized, enabled files are loaded into task instructions; files are limited to 256 KiB. Referenced scripts and resources are not recursively loaded or executed.

MCP currently supports local stdio servers. Configure an executable, a JSON array of arguments and names of inherited environment variables. Save and authorize the definition before testing the connection. The test performs real initialize and tools/list requests; runtime tasks use native MCP clients to call tools.

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

Never put credentials in command arguments or URLs. Configure the named variables in the desktop application's launch environment and restart it. Authorizing a local MCP executable permits it to run local code; model tool restrictions do not sandbox a malicious server.

## Scope, persistence and cleanup

Provider, runtime and catalog definitions are global. Projects persist their own grants and instructions. Effective grants are the deduplicated union of global and current-project grants; another project's grants never participate. A disabled or missing definition fails closed even if its ID remains authorized. Revoking a project grant does not override a global grant: revoke globally or disable the definition to prohibit it everywhere.

Desktop revocation, catalog changes and project switches cancel the active task. Bridge tasks monitor configuration files and cancel when they change. New tasks recalculate authorization. Configuration writes validate first, then replace the destination through a temporary file. Failed catalog/grant writes restore prior in-memory configuration and report failure.

Task processes use isolated temporary working/configuration directories and explicit project instructions; Codex does not read parent-directory project instructions. Engineering access is through authorized MCP servers. The configured working directory is currently used by the Codex connection-check process; task processes use isolated directories. Windows Job Objects with KILL_ON_JOB_CLOSE own child process trees, including application-exit cleanup. Temporary configurations are removed at task completion; live processes and sessions are not persisted.

## Verification boundary

See [runtime integration validation](runtime-integration-validation.md). Real remote inference, complete native UI acceptance, referenced skill assets, project-level definition overrides and per-tool permissions remain unverified or incomplete. Passing local protocol tests does not complete all acceptance criteria for issue 398.
