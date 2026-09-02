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

The current bridge sends the selected provider's language-model settings to Codex App Server. Vision fields are persisted for the provider contract and UI; image routing will be connected when the bridge accepts image observations.
