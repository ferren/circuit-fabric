//! Model catalog queries for the provider editor.
//!
//! The desktop model field stays free text; these helpers enumerate the models
//! a source actually offers so the editor can also present them as clickable
//! choices. Three sources exist, one per provider connection:
//!
//! - a Codex account provider asks its App Server via the `model/list`
//!   JSON-RPC method (the account's own catalog, default included);
//! - a Kimi account provider asks the local `kimi` CLI via
//!   `kimi provider list --json` (aliases as the `--model` flag accepts);
//! - an API-key provider queries the OpenAI-compatible `GET {base_url}/models`
//!   endpoint with the referenced key — endpoints that do not implement it
//!   simply fail, and manual model entry still works.

use std::{io::Read, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CodexAppServerSettings, LlmProviderSettings, RuntimeError, secrets, tools};

/// How long a `/models` HTTP request may take before giving up.
const ENDPOINT_TIMEOUT: Duration = Duration::from_secs(20);
/// Safety bound on one `/models` response body, mirroring the Jev adapter.
const ENDPOINT_MAX_BODY: u64 = 4 * 1024 * 1024;
/// How long an assistant material download may take before giving up.
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
/// Upper bound for one downloaded document; datasheets sit far below this.
const FETCH_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Downloads one `http(s)` document for the assistant's material collection.
///
/// The CLIs' web tools return processed text, not binaries, so PDFs and other
/// documents are fetched by the application itself after the user confirms the
/// proposed URL.
///
/// # Errors
///
/// Rejects non-http(s) URLs, network and status failures, interrupted bodies
/// and files above the size cap.
pub fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, RuntimeError> {
    let url = url.trim();
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(catalog_error("仅支持 http/https 链接"));
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|_| catalog_error("无法初始化 HTTP 客户端"))?;
    let response = client.get(url).send().map_err(|error| {
        catalog_error(if error.is_timeout() {
            "下载超时".to_owned()
        } else {
            format!("下载失败：{error}")
        })
    })?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(catalog_error(format!("服务器返回 HTTP {status}")));
    }
    let bytes =
        response.bytes().map_err(|error| catalog_error(format!("下载中断：{error}")))?.to_vec();
    if bytes.len() > FETCH_MAX_BYTES {
        return Err(catalog_error(format!(
            "文件过大（{} MB），超过 {} MB 上限",
            bytes.len() / 1_048_576,
            FETCH_MAX_BYTES / 1_048_576
        )));
    }
    Ok(bytes)
}
/// The largest endpoint catalog kept for the picker; longer lists are still
/// valid — the dropdown searches this locally and renders only the top rows,
/// and the model field itself accepts anything. Sized so `OpenRouter`'s full
/// catalog (~460 models today) fits without truncation.
pub const ENDPOINT_CHOICE_LIMIT: usize = 1000;

/// One selectable model in a provider editor's fetched catalog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelChoice {
    /// The exact value to place into the provider's model field.
    pub id: String,
    /// Human-friendly label from the source (e.g. `GPT-6.1-Sol`); `None` when
    /// the source only reports the id, or when both are identical.
    pub display_name: Option<String>,
    /// Whether the source marks this model as its default.
    pub is_default: bool,
    /// Whether the source reports the model as accepting image input;
    /// `None` when the source does not say.
    pub supports_vision: Option<bool>,
}

fn catalog_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::ModelCatalog(message.into())
}

fn str_field<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str).filter(|value| !value.is_empty())
}

fn bool_field(entry: &Value, key: &str) -> bool {
    entry.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn string_array_field(entry: &Value, key: &str) -> Option<Vec<String>> {
    entry
        .get(key)?
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_owned).collect())
}

/// Lists the models a Codex App Server offers for the provider's configuration.
///
/// The server is launched from the saved Codex settings, queried through the
/// `model/list` request, and stopped; nothing stays resident. A `codex_account`
/// provider therefore lists the local sign-in's own model catalog.
///
/// # Errors
///
/// Returns an error when the server cannot launch, rejects `model/list`
/// (older Codex versions predate it), or returns a malformed response.
pub fn list_codex_models(
    settings: &CodexAppServerSettings,
    provider: &LlmProviderSettings,
    secrets: Option<&secrets::SecretValues>,
) -> Result<Vec<ModelChoice>, RuntimeError> {
    let mut handle = crate::CodexAppServerHandle::launch_with_secrets(settings, provider, secrets)?;
    let result = handle.list_models();
    let _ = handle.stop();
    result
}

/// Lists the model aliases configured in the local Kimi Code CLI via
/// `kimi provider list --json`. The aliases are the values the CLI's
/// `--model` flag accepts, so they map one-to-one onto the model field.
///
/// # Errors
///
/// Returns an error when the CLI cannot run or prints invalid JSON.
pub fn list_kimi_models(command: &str) -> Result<Vec<ModelChoice>, RuntimeError> {
    let output = tools::executable(command)
        .args(["provider", "list", "--json"])
        .output()
        .map_err(|source| RuntimeError::Launch { command: command.to_owned(), source })?;
    if !output.status.success() {
        let stderr: String = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(catalog_error(format!(
            "`{command} provider list --json` 失败（退出码 {}）：{stderr}",
            output.status.code().unwrap_or(-1)
        )));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| catalog_error("`kimi provider list --json` 未返回有效 JSON"))?;
    parse_kimi_provider_list(&parsed)
}

/// Lists the models of an OpenAI-compatible endpoint through
/// `GET {base_url}/models`, authenticated with the provider's referenced
/// key variable (unlocked vault first, process environment second).
///
/// # Errors
///
/// Returns an error when the base URL is empty, the key variable has no
/// value, the endpoint answers a non-success status, or the body is not a
/// model list.
pub fn list_endpoint_models(
    provider: &LlmProviderSettings,
    secrets: Option<&secrets::SecretValues>,
) -> Result<Vec<ModelChoice>, RuntimeError> {
    let base = provider.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(catalog_error("Base URL 为空；请先填写再获取模型列表"));
    }
    let key =
        secrets::resolve(&provider.api_key_environment_variable, secrets).ok_or_else(|| {
            catalog_error(secrets::missing_variable_message(&provider.api_key_environment_variable))
        })?;
    let client = reqwest::blocking::Client::builder()
        .timeout(ENDPOINT_TIMEOUT)
        .build()
        .map_err(|_| catalog_error("无法初始化 HTTP 客户端"))?;
    let response =
        client.get(format!("{base}/models")).bearer_auth(key.as_str()).send().map_err(|error| {
            catalog_error(if error.is_timeout() {
                "请求 /models 超时".to_owned()
            } else {
                "连接 /models 失败，请检查 Base URL".to_owned()
            })
        })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Err(catalog_error(if status == 401 || status == 403 {
            format!(
                "GET {base}/models 返回 HTTP {status}：已携带所配置的密钥变量，服务端拒绝了它；请确认密钥对该端点有效且未过期"
            )
        } else {
            format!(
                "GET {base}/models 返回 HTTP {status}；部分端点不提供模型列表，可直接手动输入模型名"
            )
        }));
    }
    let mut bytes = Vec::new();
    response
        .take(ENDPOINT_MAX_BODY + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| catalog_error("读取 /models 响应失败"))?;
    if bytes.len() as u64 > ENDPOINT_MAX_BODY {
        return Err(catalog_error("/models 响应超过 4 MiB"));
    }
    let parsed: Value = serde_json::from_slice(&bytes)
        .map_err(|_| catalog_error("/models 未返回 JSON；该端点可能不兼容 OpenAI 模型列表"))?;
    parse_endpoint_model_list(&parsed).map_err(|fallback| {
        // Some gateways answer HTTP 200 with an error body — z.ai's
        // undocumented `/api/v1` path returns
        // `{"code":401,"msg":"token expired or incorrect","success":false}`
        // whatever key is sent. Surface that message instead of the generic
        // missing-array error, so a wrong Base URL is diagnosable — and when
        // the envelope is an auth rejection, say plainly that the configured
        // key was sent and refused.
        endpoint_error_detail(&parsed).map_or(fallback, |detail| {
            let auth_note = if endpoint_error_is_auth_rejection(&parsed) {
                "；已携带所配置的密钥变量，服务端拒绝了它"
            } else {
                ""
            };
            catalog_error(format!(
                "{detail}{auth_note}（HTTP 200 却返回错误报文；请确认 Base URL 为该服务的 OpenAI 兼容地址）"
            ))
        })
    })
}

/// Whether an HTTP-200 error envelope reports an authentication rejection:
/// z.ai-style `{"code":401,…}` or OpenAI-style `{"error":{"code":"401",…}}`.
fn endpoint_error_is_auth_rejection(parsed: &Value) -> bool {
    parsed.get("code").and_then(Value::as_i64) == Some(401)
        || parsed.pointer("/error/code").and_then(Value::as_str) == Some("401")
        || parsed.pointer("/error/code").and_then(Value::as_i64) == Some(401)
}

/// A one-line structural preview of an unrecognized `/models` body — keys and
/// value kinds only, never values, so the error message cannot leak content.
fn body_structure_preview(parsed: &Value) -> String {
    fn kind(value: &Value) -> String {
        match value {
            Value::Null => "null".to_owned(),
            Value::Bool(_) => "布尔".to_owned(),
            Value::Number(_) => "数字".to_owned(),
            Value::String(_) => "字符串".to_owned(),
            Value::Array(items) => format!("数组({})", items.len()),
            Value::Object(map) => {
                let keys: Vec<&str> = map.keys().map(String::as_str).collect();
                format!("对象{{{}}}", keys.join(","))
            }
        }
    }
    match parsed.as_object() {
        Some(map) => map
            .iter()
            .map(|(key, value)| format!("{key}:{}", kind(value)))
            .collect::<Vec<_>>()
            .join(", "),
        None => "非对象响应".to_owned(),
    }
}

/// Extracts the human-readable error from bodies that carry one despite a
/// success status: the OpenAI-style `{"error":{"message":"…"}}` envelope and
/// z.ai-style `{"code":…,"msg":"…","success":false}` object.
fn endpoint_error_detail(parsed: &Value) -> Option<String> {
    if let Some(message) = parsed.pointer("/error/message").and_then(Value::as_str) {
        return Some(message.to_owned());
    }
    if parsed.get("success").and_then(Value::as_bool) == Some(false) {
        let message = str_field(parsed, "msg")
            .or_else(|| str_field(parsed, "message"))
            .unwrap_or("unknown error");
        return Some(match parsed.get("code") {
            Some(code) => format!("{message}（code {code}）"),
            None => message.to_owned(),
        });
    }
    None
}

/// Parses one `model/list` App Server response page into choices, skipping
/// entries the server hides from its own picker.
pub(crate) fn parse_codex_model_list(response: &Value) -> Result<Vec<ModelChoice>, RuntimeError> {
    let entries = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| catalog_error("`model/list` 响应缺少 data 数组"))?;
    let choices = entries
        .iter()
        .filter(|entry| !bool_field(entry, "hidden"))
        .filter_map(|entry| str_field(entry, "id").map(|id| (entry, id)))
        .map(|(entry, id)| ModelChoice {
            id: id.to_owned(),
            display_name: str_field(entry, "displayName")
                .filter(|name| *name != id)
                .map(str::to_owned),
            is_default: bool_field(entry, "isDefault"),
            // `inputModalities` is `["text","image"]` on vision-capable models.
            supports_vision: string_array_field(entry, "inputModalities")
                .map(|modalities| modalities.iter().any(|modality| modality == "image")),
        })
        .collect();
    Ok(choices)
}

/// Parses the `kimi provider list --json` document into choices; the map keys
/// are the aliases `kimi --model` accepts.
pub(crate) fn parse_kimi_provider_list(parsed: &Value) -> Result<Vec<ModelChoice>, RuntimeError> {
    let models = parsed
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| catalog_error("`kimi provider list --json` 输出缺少 models 字段"))?;
    let mut choices = models
        .iter()
        .filter(|(_, entry)| str_field(entry, "model").is_some())
        .map(|(alias, entry)| ModelChoice {
            id: alias.clone(),
            display_name: str_field(entry, "displayName")
                .filter(|name| *name != alias)
                .map(str::to_owned),
            is_default: false,
            // Kimi marks image-capable models with the `image_in` capability.
            supports_vision: string_array_field(entry, "capabilities")
                .map(|capabilities| capabilities.iter().any(|capability| capability == "image_in")),
        })
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return Err(catalog_error("Kimi 未配置任何模型；请先运行 `kimi login`"));
    }
    // The JSON object's iteration order is not guaranteed; sort by alias so the
    // picker is deterministic regardless of map implementation.
    choices.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(choices)
}

/// Parses an OpenAI-compatible `/models` body: the standard
/// `{"object":"list","data":[...]}` envelope, a bare model array, or the
/// z.ai/zhipu-style wrapper that nests the list one level deeper
/// (`{"code":0,"data":{"list":[...]}}`). Sorted by id and truncated to
/// [`ENDPOINT_CHOICE_LIMIT`]. When nothing matches, the error carries a
/// structural preview of the body so an unusual gateway shape is diagnosable
/// from the UI alone.
pub(crate) fn parse_endpoint_model_list(parsed: &Value) -> Result<Vec<ModelChoice>, RuntimeError> {
    let entries = parsed
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| parsed.as_array())
        .or_else(|| {
            ["/data/list", "/data/models", "/data/data", "/models", "/list"]
                .iter()
                .find_map(|pointer| parsed.pointer(pointer).and_then(Value::as_array))
        })
        .ok_or_else(|| {
            catalog_error(format!(
                "/models 响应缺少模型数组；实际结构：{}",
                body_structure_preview(parsed)
            ))
        })?;
    let mut choices: Vec<ModelChoice> = entries
        .iter()
        .filter_map(|entry| str_field(entry, "id").map(|id| (entry, id)))
        .map(|(entry, id)| ModelChoice {
            id: id.to_owned(),
            display_name: None,
            is_default: false,
            // OpenRouter-style entries carry `architecture.input_modalities`;
            // plain OpenAI-compatible endpoints do not say.
            supports_vision: entry
                .pointer("/architecture/input_modalities")
                .and_then(Value::as_array)
                .map(|modalities| {
                    modalities.iter().any(|modality| modality.as_str() == Some("image"))
                }),
        })
        .collect();
    choices.sort_by(|a, b| a.id.cmp(&b.id));
    choices.dedup_by(|a, b| a.id == b.id);
    choices.truncate(ENDPOINT_CHOICE_LIMIT);
    if choices.is_empty() {
        return Err(catalog_error("/models 返回了空模型列表"));
    }
    Ok(choices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_model_list_keeps_ids_and_skips_hidden() {
        let response = json!({
            "data": [
                {
                    "id": "gpt-6.1-sol",
                    "displayName": "GPT-6.1-Sol",
                    "isDefault": true,
                    "hidden": false,
                    "inputModalities": ["text", "image"]
                },
                {"id": "gpt-6-astra", "displayName": "GPT-6-Astra", "hidden": false},
                {"id": "secret-internal", "displayName": "Internal", "hidden": true}
            ],
            "nextCursor": null
        });
        let choices = parse_codex_model_list(&response).expect("parse");
        assert_eq!(
            choices,
            vec![
                ModelChoice {
                    id: "gpt-6.1-sol".to_owned(),
                    display_name: Some("GPT-6.1-Sol".to_owned()),
                    is_default: true,
                    supports_vision: Some(true)
                },
                ModelChoice {
                    id: "gpt-6-astra".to_owned(),
                    display_name: Some("GPT-6-Astra".to_owned()),
                    is_default: false,
                    supports_vision: None
                },
            ]
        );
    }

    #[test]
    fn codex_model_list_rejects_responses_without_data() {
        let error = parse_codex_model_list(&json!({"nextCursor": null})).expect_err("no data");
        assert!(error.to_string().contains("model/list"), "{error}");
    }

    #[test]
    fn kimi_provider_list_parses_aliases_with_display_names() {
        let parsed = json!({
            "providers": {
                "managed:kimi-code": {"baseUrl": "https://api.kimi.com/coding/v1", "type": "kimi"}
            },
            "models": {
                "kimi-code/kimi-for-coding": {
                    "provider": "managed:kimi-code",
                    "model": "kimi-for-coding",
                    "maxContextSize": 1_048_576,
                    "displayName": "K2.8 Preview",
                    "capabilities": ["thinking", "image_in", "video_in", "tool_use"]
                },
                "kimi-code/k3": {"provider": "managed:kimi-code", "model": "k3", "displayName": "K3"},
                "kimi-code/no-model": {"provider": "managed:kimi-code"}
            }
        });
        let choices = parse_kimi_provider_list(&parsed).expect("parse");
        // Map keys iterate sorted, so the aliases come back deterministically.
        assert_eq!(
            choices,
            vec![
                ModelChoice {
                    id: "kimi-code/k3".to_owned(),
                    display_name: Some("K3".to_owned()),
                    is_default: false,
                    supports_vision: None
                },
                ModelChoice {
                    id: "kimi-code/kimi-for-coding".to_owned(),
                    display_name: Some("K2.8 Preview".to_owned()),
                    is_default: false,
                    supports_vision: Some(true)
                },
            ]
        );
    }

    #[test]
    fn kimi_provider_list_without_models_is_an_error() {
        let error = parse_kimi_provider_list(&json!({"providers": {}})).expect_err("no models key");
        assert!(error.to_string().contains("models"), "{error}");
        let error = parse_kimi_provider_list(&json!({"models": {}})).expect_err("empty models");
        assert!(error.to_string().contains("kimi login"), "{error}");
    }

    #[test]
    fn endpoint_model_list_sorts_dedupes_and_truncates() {
        let envelope = json!({
            "object": "list",
            "data": [
                {"id": "glm-4.7"},
                {"id": "glm-5.3-flash", "architecture": {"input_modalities": ["text", "image"]}},
                {"id": "glm-4.7"}
            ]
        });
        let choices = parse_endpoint_model_list(&envelope).expect("parse");
        assert_eq!(
            choices.iter().map(|choice| choice.id.as_str()).collect::<Vec<_>>(),
            vec!["glm-4.7", "glm-5.3-flash"]
        );
        assert_eq!(choices[0].supports_vision, None, "plain endpoints do not say");
        assert_eq!(choices[1].supports_vision, Some(true), "OpenRouter-style modality parsed");

        let many = json!(
            (1..=1100).map(|index| json!({"id": format!("m{index:04}")})).collect::<Vec<_>>()
        );
        let truncated = parse_endpoint_model_list(&many).expect("parse");
        assert_eq!(truncated.len(), ENDPOINT_CHOICE_LIMIT);
        assert_eq!(truncated.first().expect("first").id, "m0001");

        let bare = json!([{"id": "solo"}]);
        assert_eq!(parse_endpoint_model_list(&bare).expect("parse")[0].id, "solo");

        let error = parse_endpoint_model_list(&json!({"error": "nope"})).expect_err("not a list");
        assert!(error.to_string().contains("/models"), "{error}");
    }

    #[test]
    fn endpoint_error_envelopes_answered_with_200_surface_their_message() {
        // z.ai's undocumented `/api/v1` path answers HTTP 200 with this body
        // for any key; the fetch must report the endpoint's own message.
        let zai = json!({"code": 401, "msg": "token expired or incorrect", "success": false});
        assert_eq!(
            endpoint_error_detail(&zai).as_deref(),
            Some("token expired or incorrect（code 401）")
        );
        let openai = json!({"error": {"code": "401", "message": "Invalid API key"}});
        assert_eq!(endpoint_error_detail(&openai).as_deref(), Some("Invalid API key"));
        assert_eq!(
            endpoint_error_detail(&json!({"success": false, "msg": "no access"})).as_deref(),
            Some("no access")
        );
        // Real list envelopes are never mistaken for errors.
        assert_eq!(endpoint_error_detail(&json!({"data": [{"id": "m"}]})), None);
        assert_eq!(endpoint_error_detail(&json!([{"id": "m"}])), None);
    }

    #[test]
    fn endpoint_model_list_accepts_nested_wrappers() {
        // z.ai/zhipu-style success wrapper: the list sits one level deeper.
        let nested = json!({"code": 0, "success": true, "data": {"list": [{"id": "glm-5.3"}]}});
        let choices = parse_endpoint_model_list(&nested).expect("nested list parses");
        assert_eq!(choices[0].id, "glm-5.3");
        // An unrecognized shape reports its structure, not just "missing".
        let error = parse_endpoint_model_list(&json!({"code": 0, "data": {"unexpected": 1}}))
            .expect_err("not a list");
        assert!(error.to_string().contains("data:对象{unexpected}"), "{error}");
    }

    #[test]
    fn endpoint_error_auth_rejection_is_detected() {
        assert!(endpoint_error_is_auth_rejection(&json!({
            "code": 401,
            "msg": "token expired or incorrect",
            "success": false
        })));
        assert!(endpoint_error_is_auth_rejection(
            &json!({"error": {"code": "401", "message": "Invalid API key"}})
        ));
        assert!(endpoint_error_is_auth_rejection(&json!({"error": {"code": 401}})));
        // Missing-header and non-auth errors are not key rejections.
        assert!(!endpoint_error_is_auth_rejection(&json!({"code": 1001, "msg": "no header"})));
        assert!(!endpoint_error_is_auth_rejection(&json!({"data": []})));
    }
}
