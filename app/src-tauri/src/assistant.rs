use crate::{
    assistant_config::{
        load_config, load_provider_key, provider_base, provider_base_for, requires_manual_key,
        AssistantConfig,
    },
    state::RuntimeState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::State;

const MAX_HISTORY_MESSAGES: usize = 20;
const MAX_MESSAGE_CHARS: usize = 4_000;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantMessage {
    role: String,
    content: String,
    #[serde(default)]
    local_account: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantKeyOption {
    id: String,
    name: String,
    masked_key: String,
}

#[derive(Debug, Serialize)]
pub struct AssistantStatus {
    ready: bool,
    keys: Vec<AssistantKeyOption>,
    selected_key_id: Option<String>,
    models: Vec<String>,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssistantReply {
    content: String,
    tools_used: Vec<String>,
}

#[tauri::command]
pub async fn petra_assistant_models(
    provider: String,
    api_key: String,
    custom_base_url: String,
    state: State<'_, RuntimeState>,
) -> Result<Vec<String>, String> {
    let provider = provider.trim().to_lowercase();
    let base_url = provider_base_for(&provider, custom_base_url.trim())?;
    let api_key = validate_petra_api_key(&provider, &api_key)?;
    state.api.list_models_at(&base_url, api_key).await
}

#[tauri::command]
pub async fn petra_assistant_chat(
    provider: String,
    api_key: String,
    custom_base_url: String,
    mut body: Value,
    state: State<'_, RuntimeState>,
) -> Result<Value, String> {
    let provider = provider.trim().to_lowercase();
    let base_url = provider_base_for(&provider, custom_base_url.trim())?;
    let api_key = validate_petra_api_key(&provider, &api_key)?;
    let object = body
        .as_object_mut()
        .ok_or_else(|| "AI 请求格式不正确".to_owned())?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty() && model.chars().count() <= 160)
        .ok_or_else(|| "请先选择模型".to_owned())?;
    let model = model.to_owned();
    object.insert("model".to_owned(), json!(model));
    object.insert("stream".to_owned(), json!(false));
    state
        .api
        .chat_completion_at(&base_url, api_key, &body)
        .await
}

fn validate_petra_api_key<'a>(provider: &str, api_key: &'a str) -> Result<Option<&'a str>, String> {
    let api_key = api_key.trim();
    if api_key.chars().count() > 4_096 {
        return Err("API Key 长度不正确".to_owned());
    }
    if provider != "ollama" && api_key.is_empty() {
        return Err("请先填写 API Key".to_owned());
    }
    Ok((!api_key.is_empty()).then_some(api_key))
}

struct SelectedKey {
    option: AssistantKeyOption,
    secret: String,
}

struct AssistantConnection {
    base_url: String,
    api_key: Option<String>,
    keys: Vec<AssistantKeyOption>,
    selected_key_id: Option<String>,
}

#[tauri::command]
pub async fn assistant_status(state: State<'_, RuntimeState>) -> Result<AssistantStatus, String> {
    let config = load_config()?;
    let connection = match resolve_connection(&state, &config).await {
        Ok(connection) => connection,
        Err(message) => {
            return Ok(AssistantStatus {
                ready: false,
                keys: vec![],
                selected_key_id: None,
                models: vec![],
                message: Some(message),
            })
        }
    };
    match state
        .api
        .list_models_at(&connection.base_url, connection.api_key.as_deref())
        .await
    {
        Ok(models) => Ok(AssistantStatus {
            ready: true,
            keys: connection.keys,
            selected_key_id: connection.selected_key_id,
            models,
            message: None,
        }),
        Err(message) => Ok(AssistantStatus {
            ready: false,
            keys: connection.keys,
            selected_key_id: connection.selected_key_id,
            models: vec![],
            message: Some(message),
        }),
    }
}

#[tauri::command]
pub async fn assistant_chat(
    history: Vec<AssistantMessage>,
    state: State<'_, RuntimeState>,
) -> Result<AssistantReply, String> {
    if let Some(message) = history.iter().rev().find(|m| m.role == "user") {
        let reply = crate::account::local_reply(&state, &message.content).await;
        if reply.handled {
            return Ok(AssistantReply {
                content: reply.content.unwrap_or_default(),
                tools_used: vec![],
            });
        }
    }
    let config = load_config()?;
    if !config.enabled {
        return Err("Molly 助手当前已关闭，请先在设置中开启".to_owned());
    }
    let model = config.model.trim();
    if model.is_empty() || model.chars().count() > 160 {
        return Err("请先在助手设置中选择模型".to_owned());
    }
    let connection = resolve_connection(&state, &config).await?;
    let models = state
        .api
        .list_models_at(&connection.base_url, connection.api_key.as_deref())
        .await?;
    if !models.iter().any(|candidate| candidate == model) {
        return Err("所选模型已不在当前服务商的可用列表中".to_owned());
    }

    let mut messages = vec![
        json!({"role":"system","content":format!("{}\n你是 Molly，回复简洁友好。账户查询由本机接口直接处理；此对话不调用账户查询工具，也不能猜测余额、额度和到期时间。",config.persona)}),
    ];
    for item in history
        .iter()
        .rev()
        .take(MAX_HISTORY_MESSAGES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if !item.local_account && matches!(item.role.as_str(), "user" | "assistant") {
            let content = item
                .content
                .chars()
                .take(MAX_MESSAGE_CHARS)
                .collect::<String>();
            if !content.trim().is_empty() {
                messages.push(json!({"role":item.role,"content":content}));
            }
        }
    }
    let response = state
        .api
        .chat_completion_at(
            &connection.base_url,
            connection.api_key.as_deref(),
            &json!({"model":model,"messages":messages,"temperature":0.55}),
        )
        .await?;
    let content = response["choices"][0]["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("AI 服务没有返回可显示的内容")?;
    Ok(AssistantReply {
        content: content.to_string(),
        tools_used: vec![],
    })
}

async fn resolve_connection(
    state: &RuntimeState,
    config: &AssistantConfig,
) -> Result<AssistantConnection, String> {
    let base_url = provider_base(config)?;
    if config.provider == "mollycloud" {
        let access_token = state.access_token().await?;
        let raw_keys = state
            .api
            .get_authenticated("/keys?page=1&page_size=100", &access_token)
            .await?;
        let available = available_keys(&raw_keys);
        let public_keys = available
            .iter()
            .map(|key| key.option.clone())
            .collect::<Vec<_>>();
        let selected = choose_key(&available, Some(&config.molly_key_id)).ok_or_else(|| {
            "还没有可用的普通 API 密钥，请先在 MollyCloud 网页控制台创建".to_owned()
        })?;
        Ok(AssistantConnection {
            base_url,
            api_key: Some(selected.secret.clone()),
            keys: public_keys,
            selected_key_id: Some(selected.option.id.clone()),
        })
    } else {
        let api_key = load_provider_key(&config.provider)?;
        if requires_manual_key(&config.provider) && api_key.is_none() {
            return Err("请先填写并保存该服务商的 API Key".to_owned());
        }
        Ok(AssistantConnection {
            base_url,
            api_key,
            keys: vec![],
            selected_key_id: None,
        })
    }
}

fn pick(value: &Value, names: &[&str]) -> Value {
    let mut output = Map::new();
    for name in names {
        if let Some(field) = value.get(*name) {
            if field.is_string() || field.is_number() || field.is_boolean() || field.is_null() {
                output.insert((*name).to_owned(), field.clone());
            }
        }
    }
    Value::Object(output)
}

fn sanitize_key_status(value: &Value) -> Value {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(50)
        .map(|item| {
            pick(
                item,
                &[
                    "name",
                    "status",
                    "quota_used",
                    "quota_limit",
                    "last_used_at",
                    "expires_at",
                ],
            )
        })
        .collect::<Vec<_>>();
    json!({
        "total": value.get("total").cloned().unwrap_or_else(|| json!(items.len())),
        "items": items,
    })
}

fn available_keys(value: &Value) -> Vec<SelectedKey> {
    value
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let secret = item.get("key").and_then(Value::as_str)?.trim();
            if secret.is_empty() || item.get("status").and_then(Value::as_str) == Some("disabled") {
                return None;
            }
            let id = value_id(item.get("id")?)?;
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("未命名密钥")
                .to_owned();
            Some(SelectedKey {
                option: AssistantKeyOption {
                    id,
                    name,
                    masked_key: mask_key(secret),
                },
                secret: secret.to_owned(),
            })
        })
        .collect()
}

fn choose_key<'a>(keys: &'a [SelectedKey], selected: Option<&str>) -> Option<&'a SelectedKey> {
    selected
        .filter(|id| !id.is_empty())
        .and_then(|id| keys.iter().find(|key| key.option.id == id))
        .or_else(|| keys.first())
}

fn value_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

fn mask_key(secret: &str) -> String {
    let tail = secret
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("••••{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_status_never_contains_secret() {
        let value = json!({
            "items": [{ "id": 1, "name": "main", "key": "sk-super-secret", "status": "active" }]
        });
        let sanitized = sanitize_key_status(&value).to_string();
        assert!(!sanitized.contains("super-secret"));
        assert!(!sanitized.contains("\"key\""));
    }

    #[test]
    fn only_available_keys_are_selected() {
        let value = json!({
            "items": [
                { "id": 1, "name": "off", "key": "sk-disabled", "status": "disabled" },
                { "id": 2, "name": "on", "key": "sk-available", "status": "active" }
            ]
        });
        let keys = available_keys(&value);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].option.id, "2");
        assert_eq!(keys[0].option.masked_key, "••••able");
    }
}
