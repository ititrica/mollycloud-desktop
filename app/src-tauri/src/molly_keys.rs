//! Authenticated account keys -> private CC Switch library. Credentials stay native.
use crate::{
    api::{Sub2ApiClient, MOLLY_CCSWITCH_IMPORT_ROOT, MOLLY_OPENAI_ROOT},
    state::RuntimeState,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, State, Webview};

pub(crate) fn supported_agent(agent: &str) -> bool {
    matches!(
        agent,
        "claude"
            | "claude-desktop"
            | "codex"
            | "gemini"
            | "grokbuild"
            | "opencode"
            | "openclaw"
            | "hermes"
            | "pi"
            | "mcode"
    )
}
fn automatic(agent: &str) -> bool {
    matches!(
        agent,
        "claude" | "claude-desktop" | "codex" | "gemini" | "grokbuild"
    )
}
fn compatible(agent: &str, platform: &str) -> bool {
    if !automatic(agent) {
        return true;
    }
    platform == "composite"
        || platform == "all"
        || platform
            == match agent {
                "claude" | "claude-desktop" => "anthropic",
                "codex" => "openai",
                "gemini" => "gemini",
                "grokbuild" => "grok",
                _ => "",
            }
}
fn choose_model<'a>(agent: &str, models: &'a [String]) -> Option<&'a str> {
    let family = match agent {
        "claude" | "claude-desktop" => "claude",
        "codex" => "gpt",
        "gemini" => "gemini",
        "grokbuild" => "grok",
        _ => "",
    };
    let eligible = |m: &&String| {
        !m.to_ascii_lowercase().contains("embedding")
            && !m.to_ascii_lowercase().contains("image")
            && !m.to_ascii_lowercase().contains("audio")
            && m.len() <= 160
            && !m.chars().any(char::is_control)
    };
    models
        .iter()
        .filter(eligible)
        .find(|m| m.to_ascii_lowercase().contains(family))
        .or_else(|| models.iter().find(eligible))
        .map(String::as_str)
}
fn id(item: &Value) -> Option<String> {
    let value = item.get("id")?;
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    raw.parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
        .map(|n| n.to_string())
}

/// Only a verified complete snapshot is eligible for private-library pruning.
#[derive(Default)]
struct KeyPages {
    items: Vec<Value>,
    seen: HashSet<String>,
    total: Option<u64>,
}
impl KeyPages {
    fn append(&mut self, value: &Value) -> Result<bool, String> {
        let batch = value["items"].as_array().ok_or("密钥列表格式不正确")?;
        for item in batch {
            let key_id = id(item).ok_or("服务返回无效的密钥 ID")?;
            if !self.seen.insert(key_id) {
                return Err("服务返回了重复的密钥分页，请刷新后重试".into());
            }
            self.items.push(item.clone());
        }
        if let Some(raw) = value.get("total") {
            let total = raw
                .as_u64()
                .or_else(|| raw.as_str()?.parse().ok())
                .ok_or("密钥总数格式不正确，已保留现有供应商配置")?;
            if self.total.is_some_and(|previous| previous != total)
                || self.items.len() as u64 > total
            {
                return Err("密钥列表在分页期间发生变化，请重新刷新；已保留现有供应商配置".into());
            }
            self.total = Some(total);
            if self.items.len() as u64 == total {
                return Ok(true);
            }
            if batch.is_empty() {
                return Err("密钥分页不完整，已保留现有供应商配置".into());
            }
            return Ok(false);
        }
        Ok(batch.len() < 100)
    }
}
pub(crate) async fn fetch_all_keys(api: &Sub2ApiClient, token: &str) -> Result<Value, String> {
    let mut pages = KeyPages::default();
    for page in 1..=100 {
        let value = api
            .get_authenticated(&format!("/keys?page={page}&page_size=100"), token)
            .await?;
        if pages.append(&value)? {
            return Ok(json!({"total":pages.items.len(),"items":pages.items}));
        }
    }
    Err("账户密钥数量超过本地同步上限，已保留现有供应商配置".into())
}

fn public_key(item: &Value) -> Value {
    let tail = item["key"]
        .as_str()
        .unwrap_or("")
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let mut group = serde_json::Map::new();
    for field in [
        "id",
        "name",
        "platform",
        "rate_multiplier",
        "description",
        "subscription_type",
    ] {
        if let Some(value) = item["group"].get(field) {
            group.insert(field.into(), value.clone());
        }
    }
    json!({"id":id(item).unwrap_or_default(),"name":item["name"],"key":format!("sk-••••{tail}"),"status":item["status"],
        "group":group,"group_id":item["group_id"],"quota":item["quota"],"quota_used":item["quota_used"],"usage":item["usage"],"expires_at":item["expires_at"]})
}

#[tauri::command]
pub async fn sync_molly_key_providers(
    webview: Webview,
    app: AppHandle,
    state: State<'_, RuntimeState>,
    agent: String,
) -> Result<Value, String> {
    if webview.label() != "console" {
        return Err("请在控制台管理账户密钥".into());
    }
    if !supported_agent(&agent) {
        return Err("工具类型无效".into());
    }
    crate::console_plugins::require(&app, "ccswitch")?;
    let _serial = state.key_provider_sync.lock().await;
    let token = state.access_token().await?;
    {
        let session = state.session.lock().await;
        if session.access_token.as_deref() != Some(&token) {
            return Err("登录状态已改变，请重新刷新密钥".into());
        }
        // Empty local Official choices remain usable when account networking fails.
        molly_ccswitch::ensure_molly_official(&app, &agent)?;
    }
    let (user, mut page) = tokio::try_join!(
        state.api.get_authenticated("/auth/me", &token),
        fetch_all_keys(&state.api, &token)
    )?;
    let account = user["id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| user["id"].as_u64().map(|n| n.to_string()))
        .ok_or("账户缺少 ID")?;
    let ids = crate::account::key_ids(&page);
    if !ids.is_empty() {
        let stats = state
            .api
            .post_authenticated(
                "/usage/dashboard/api-keys-usage",
                &json!({"api_key_ids":ids}),
                &token,
            )
            .await
            .unwrap_or(Value::Null);
        crate::account::apply_key_usage(&mut page, &stats);
    }
    let items = page["items"].as_array().ok_or("密钥列表格式不正确")?;
    let mut discoveries = HashMap::new();
    let mut requests = tokio::task::JoinSet::new();
    let mut missing = Vec::new();
    for item in items {
        let key_id = id(item).ok_or("密钥 ID 无效")?;
        if automatic(&agent)
            && compatible(&agent, item["group"]["platform"].as_str().unwrap_or(""))
            && item["status"] == "active"
            && molly_ccswitch::get_molly_key_provider(&app, &account, &key_id, &agent)?.is_none_or(
                |p| {
                    agent == "codex"
                        && p.meta.as_ref().and_then(|m| m.molly_codex_mode.as_deref())
                            == Some("mapped")
                },
            )
        {
            if let Some(secret) = item["key"].as_str().filter(|s| !s.is_empty()) {
                missing.push((key_id, secret.to_owned()))
            }
        }
    }
    for batch in missing.chunks(4) {
        for (key_id, secret) in batch {
            let api = state.api.clone();
            let key_id = key_id.clone();
            let secret = secret.clone();
            requests.spawn(async move {
                (
                    key_id,
                    api.list_models_at(MOLLY_OPENAI_ROOT, Some(&secret)).await,
                )
            });
        }
        while let Some(result) = requests.join_next().await {
            let (key_id, models) = result.map_err(|_| "模型发现任务未完成")?;
            discoveries.insert(key_id, models);
        }
    }
    // Hold the session lock across private-library writes. Logout or account
    // switching cannot publish a snapshot belonging to the previous session.
    let session = state.session.lock().await;
    if session.access_token.as_deref() != Some(&token) {
        return Err("登录状态已改变，请重新刷新密钥".into());
    }
    let mut keys = Vec::new();
    let mut key_ids = Vec::new();
    for item in items {
        let key_id = id(item).ok_or("密钥 ID 无效")?;
        key_ids.push(key_id.clone());
        let can_use = compatible(&agent, item["group"]["platform"].as_str().unwrap_or(""));
        let mut saved = molly_ccswitch::find_molly_key_provider(&app, &account, &key_id, &agent)?;
        let mut error: Option<String> = None;
        if saved.is_none() && automatic(&agent) && can_use && item["status"] == "active" {
            match discoveries
                .remove(&key_id)
                .unwrap_or_else(|| Err("密钥内容为空".into()))
            {
                Ok(models) => {
                    if let Some(model) = choose_model(&agent, &models) {
                        let input = molly_ccswitch::MollyProviderImport {
                            account_id: account.clone(),
                            key_id: key_id.clone(),
                            name: item["name"]
                                .as_str()
                                .unwrap_or("未命名密钥")
                                .chars()
                                .take(80)
                                .collect(),
                            app: agent.clone(),
                            api_key: item["key"].as_str().unwrap_or("").into(),
                            base_url: MOLLY_CCSWITCH_IMPORT_ROOT.into(),
                            model: model.into(),
                            usage_script: Some(
                                "/* MollyCloud account balance is supplied by the host. */".into(),
                            ),
                        };
                        let provider_id = molly_ccswitch::import_molly_provider(&app, input)?;
                        saved = Some((provider_id, model.into()));
                    } else {
                        error = Some("服务没有返回可用的对话模型，请手动配置".into())
                    }
                }
                Err(_) => error = Some("模型列表暂不可用，可手动选择模型并配置".into()),
            }
        }
        if agent == "codex" && saved.is_some() {
            let mapped = molly_ccswitch::get_molly_key_provider(&app, &account, &key_id, &agent)?
                .is_some_and(|p| {
                    p.meta.as_ref().and_then(|m| m.molly_codex_mode.as_deref()) == Some("mapped")
                });
            let models = if mapped {
                discoveries.remove(&key_id)
            } else {
                None
            };
            if mapped {
                match models {
                    Some(Ok(models)) => {
                        if let Err(e) = molly_ccswitch::update_molly_codex_provider(
                            &app,
                            &account,
                            &key_id,
                            None,
                            Some(&models),
                        ) {
                            error = Some(e);
                        }
                    }
                    _ => error = Some("分组模型暂不可用，保留已有映射；请刷新后再启用。".into()),
                }
            } else {
                if let Err(e) =
                    molly_ccswitch::update_molly_codex_provider(&app, &account, &key_id, None, None)
                {
                    error = Some(e);
                }
            }
            saved = molly_ccswitch::find_molly_key_provider(&app, &account, &key_id, &agent)?;
        }
        let mut value = public_key(item);
        value["provider_id"] = saved
            .as_ref()
            .map(|(id, _)| json!(id))
            .unwrap_or(Value::Null);
        value["model"] = saved
            .as_ref()
            .map(|(_, model)| json!(model))
            .unwrap_or(json!(""));
        value["compatible"] = json!(can_use);
        value["error"] = json!(error);
        keys.push(value);
    }
    let current_removed =
        molly_ccswitch::reconcile_molly_key_providers(&app, &account, &agent, &key_ids)?;
    drop(session);
    Ok(json!({"agent":agent,"keys":keys,"current_removed":current_removed}))
}

/// Preparing an editor is a user action. Additive tools stay empty until this action.
#[tauri::command]
pub async fn prepare_molly_key_provider(
    webview: Webview,
    app: AppHandle,
    state: State<'_, RuntimeState>,
    agent: String,
    key_id: String,
) -> Result<String, String> {
    if webview.label() != "console" || !supported_agent(&agent) {
        return Err("请在控制台编辑密钥配置".into());
    }
    crate::console_plugins::require(&app, "ccswitch")?;
    let _serial = state.key_provider_sync.lock().await;
    let token = state.access_token().await?;
    let (user, keys) = tokio::try_join!(
        state.api.get_authenticated("/auth/me", &token),
        fetch_all_keys(&state.api, &token)
    )?;
    let account = user["id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| user["id"].as_u64().map(|n| n.to_string()))
        .ok_or("账户缺少 ID")?;
    let item = keys["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| id(item).as_deref() == Some(&key_id))
        })
        .ok_or("未找到此账户密钥")?;
    if !compatible(&agent, item["group"]["platform"].as_str().unwrap_or("")) {
        return Err("当前分组不适用于此工具，请先修改分组".into());
    }
    let secret = item["key"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("密钥内容为空")?;
    let session = state.session.lock().await;
    if session.access_token.as_deref() != Some(&token) {
        return Err("登录状态已改变，请重新刷新密钥".into());
    }
    molly_ccswitch::prepare_molly_provider(
        &app,
        molly_ccswitch::MollyProviderImport {
            account_id: account,
            key_id,
            name: item["name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("未命名密钥")
                .chars()
                .take(80)
                .collect(),
            app: agent,
            api_key: secret.into(),
            base_url: MOLLY_CCSWITCH_IMPORT_ROOT.into(),
            model: String::new(),
            usage_script: None,
        },
    )
}

#[tauri::command]
pub async fn set_molly_key_mode(
    webview: Webview,
    app: AppHandle,
    state: State<'_, RuntimeState>,
    agent: String,
    key_id: String,
    mode: String,
) -> Result<(), String> {
    if webview.label() != "console"
        || agent != "codex"
        || !matches!(mode.as_str(), "native" | "mapped")
    {
        return Err("Codex 配置模式无效".into());
    }
    crate::console_plugins::require(&app, "ccswitch")?;
    let _serial = state.key_provider_sync.lock().await;
    let token = state.access_token().await?;
    let (user, keys) = tokio::try_join!(
        state.api.get_authenticated("/auth/me", &token),
        fetch_all_keys(&state.api, &token)
    )?;
    let account = user["id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| user["id"].as_u64().map(|n| n.to_string()))
        .ok_or("账户缺少 ID")?;
    let item = keys["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| id(item).as_deref() == Some(&key_id))
        })
        .ok_or("未找到此账户密钥")?;
    if item["status"] != "active"
        || !compatible(&agent, item["group"]["platform"].as_str().unwrap_or(""))
    {
        return Err("请使用正常且适用于 Codex 的密钥".into());
    }
    let models = if mode == "mapped" {
        let secret = item["key"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("密钥内容为空")?;
        Some(
            state
                .api
                .list_models_at(MOLLY_OPENAI_ROOT, Some(secret))
                .await
                .map_err(|_| "分组模型拉取失败，未修改配置。")?,
        )
    } else {
        None
    };
    let session = state.session.lock().await;
    if session.access_token.as_deref() != Some(&token) {
        return Err("登录状态已改变，请重新刷新密钥".into());
    }
    molly_ccswitch::update_molly_codex_provider(
        &app,
        &account,
        &key_id,
        Some(&mode),
        models.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_requires_complete_distinct_pages_before_pruning() {
        let mut pages = KeyPages::default();
        assert!(!pages.append(&json!({"total":"101","items":(1..=100).map(|id|json!({"id":id})).collect::<Vec<_>>()})).unwrap());
        assert!(pages
            .append(&json!({"total":101,"items":[{"id":"101"}]}))
            .unwrap());
        assert_eq!(pages.items.len(), 101);
        let mut truncated = KeyPages::default();
        assert!(!truncated
            .append(&json!({"total":2,"items":[{"id":1}]}))
            .unwrap());
        assert!(truncated.append(&json!({"total":2,"items":[]})).is_err());
        assert!(truncated
            .append(&json!({"total":2,"items":[{"id":1}]}))
            .is_err());
        let mut changed = KeyPages::default();
        assert!(!changed
            .append(&json!({"total":2,"items":[{"id":1}]}))
            .unwrap());
        assert!(changed.append(&json!({"total":1,"items":[]})).is_err());
        assert!(KeyPages::default()
            .append(&json!({"total":"invalid","items":[]}))
            .is_err());
        assert!(KeyPages::default()
            .append(&json!({"total":0,"items":[]}))
            .unwrap());
    }
    #[test]
    fn platform_matching_and_additive_tools_do_not_auto_import() {
        assert!(compatible("codex", "openai"));
        assert!(!compatible("codex", "anthropic"));
        assert!(compatible("claude", "anthropic"));
        assert!(compatible("gemini", "composite"));
        for app in ["opencode", "openclaw", "hermes", "pi", "mcode"] {
            assert!(!automatic(app));
            assert!(compatible(app, "anything"));
        }
    }
    #[test]
    fn public_snapshot_masks_credentials_and_selects_an_available_model() {
        let result = public_key(
            &json!({"id":42,"key":"mock-secret-1234","name":"Key","status":"active","group":{"id":1,"name":"OpenAI","platform":"openai","private_secret":"do-not-copy"},"private_token":"hidden"}),
        );
        assert_eq!(result["key"], "sk-••••1234");
        assert!(!result.to_string().contains("mock-secret"));
        assert!(!result.to_string().contains("private_secret"));
        let models = vec![
            "embedding-small".into(),
            "claude-test".into(),
            "gpt-test".into(),
        ];
        assert_eq!(choose_model("codex", &models), Some("gpt-test"));
        assert_eq!(choose_model("claude", &models), Some("claude-test"));
    }
}
