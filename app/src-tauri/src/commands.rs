use crate::{
    api::{MOLLY_OPENAI_ROOT, SERVICE_ORIGIN},
    state::RuntimeState,
};
use arboard::Clipboard;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct ServiceBootstrap {
    service_origin: &'static str,
    health: Value,
    settings: Value,
}

#[derive(Serialize)]
pub struct LoginOutcome {
    requires_two_factor: bool,
    user_email_masked: Option<String>,
    user: Value,
}

#[derive(Serialize)]
pub struct DashboardPayload {
    user: Value,
    subscriptions: Value,
    subscription_progress: Value,
    usage: Value,
    keys: Value,
}

#[derive(Serialize)]
pub struct AccountBalance {
    balance: Value,
    today_tokens: Value,
}

#[tauri::command]
pub async fn bootstrap_public(state: State<'_, RuntimeState>) -> Result<ServiceBootstrap, String> {
    let (health, settings) = tokio::try_join!(state.api.health(), state.api.public_settings())?;
    Ok(ServiceBootstrap {
        service_origin: SERVICE_ORIGIN,
        health,
        settings,
    })
}

#[tauri::command]
pub async fn login(
    email: String,
    password: String,
    remember_login: bool,
    state: State<'_, RuntimeState>,
    app: AppHandle,
) -> Result<LoginOutcome, String> {
    let email = email.trim();
    if email.is_empty() || password.is_empty() {
        return Err("请输入邮箱和密码".to_owned());
    }
    let data = state.api.login(email, &password).await?;
    if data
        .get("requires_2fa")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        state.remember_temp_token(&data).await?;
        return Ok(LoginOutcome {
            requires_two_factor: true,
            user_email_masked: data
                .get("user_email_masked")
                .and_then(Value::as_str)
                .map(str::to_owned),
            user: Value::Null,
        });
    }

    let user = state.accept_login(&data, remember_login).await?;
    let _ = app.emit_to("main", "live2d-motion", "Nod");
    let _ = app.emit_to("main", "account-session-changed", ());
    Ok(LoginOutcome {
        requires_two_factor: false,
        user_email_masked: None,
        user,
    })
}

#[tauri::command]
pub async fn complete_two_factor(
    totp_code: String,
    remember_login: bool,
    state: State<'_, RuntimeState>,
    app: AppHandle,
) -> Result<LoginOutcome, String> {
    if totp_code.trim().len() != 6 {
        return Err("请输入 6 位验证码".to_owned());
    }
    let temp_token = state.take_temp_token().await?;
    let data = state
        .api
        .complete_two_factor(&temp_token, totp_code.trim())
        .await?;
    let user = state.accept_login(&data, remember_login).await?;
    let _ = app.emit_to("main", "live2d-motion", "Nod");
    let _ = app.emit_to("main", "account-session-changed", ());
    Ok(LoginOutcome {
        requires_two_factor: false,
        user_email_masked: None,
        user,
    })
}

#[tauri::command]
pub async fn restore_session(state: State<'_, RuntimeState>) -> Result<Option<Value>, String> {
    if state.refresh_token().await.is_err() {
        return Ok(None);
    }
    let token = state.access_token().await?;
    let user = state.api.get_authenticated("/auth/me", &token).await?;
    Ok(Some(user))
}

#[tauri::command]
pub async fn fetch_account_balance(
    state: State<'_, RuntimeState>,
) -> Result<AccountBalance, String> {
    let token = state.access_token().await?;
    let (user, usage) = tokio::join!(
        state.api.get_authenticated("/auth/me", &token),
        state
            .api
            .get_authenticated("/usage/dashboard/stats", &token),
    );
    let user = user?;
    Ok(AccountBalance {
        balance: user.get("balance").cloned().unwrap_or(Value::Null),
        today_tokens: usage
            .ok()
            .and_then(|usage| usage.get("today_tokens").cloned())
            .unwrap_or(Value::Null),
    })
}

#[tauri::command]
pub async fn fetch_dashboard(state: State<'_, RuntimeState>) -> Result<DashboardPayload, String> {
    let token = state.access_token().await?;
    let (user, subscriptions, subscription_progress, usage, mut keys) = tokio::try_join!(
        state.api.get_authenticated("/auth/me", &token),
        state
            .api
            .get_authenticated("/subscriptions/summary", &token),
        state
            .api
            .get_authenticated("/subscriptions/progress", &token),
        state
            .api
            .get_authenticated("/usage/dashboard/stats", &token),
        state
            .api
            .get_authenticated("/keys?page=1&page_size=100", &token),
    )?;
    mask_api_keys(&mut keys);
    Ok(DashboardPayload {
        user,
        subscriptions,
        subscription_progress,
        usage,
        keys,
    })
}

fn mask_api_keys(value: &mut Value) {
    let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(key_value) = item.get_mut("key") else {
            continue;
        };
        let Some(key) = key_value.as_str() else {
            continue;
        };
        let tail: String = key
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        *key_value = Value::String(format!("sk-••••{tail}"));
    }
}

fn copy_to_clipboard(value: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|_| "无法访问系统剪贴板".to_owned())?;
    clipboard
        .set_text(value.to_owned())
        .map_err(|_| "无法写入系统剪贴板".to_owned())
}

fn id_matches(value: Option<&Value>, expected: &str) -> bool {
    match value {
        Some(Value::String(value)) => value == expected,
        Some(Value::Number(value)) => value.to_string() == expected,
        _ => false,
    }
}

#[tauri::command]
pub fn copy_api_endpoint() -> Result<(), String> {
    copy_to_clipboard(MOLLY_OPENAI_ROOT)
}

#[tauri::command]
pub async fn copy_api_key(key_id: String, state: State<'_, RuntimeState>) -> Result<(), String> {
    if key_id.trim().is_empty() {
        return Err("未找到要复制的 API 密钥".to_owned());
    }
    let token = state.access_token().await?;
    let keys = state
        .api
        .get_authenticated("/keys?page=1&page_size=100", &token)
        .await?;
    let key = keys
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| id_matches(item.get("id"), key_id.trim()))
        })
        .and_then(|item| item.get("key"))
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "未找到要复制的 API 密钥".to_owned())?;
    copy_to_clipboard(key)
}

#[tauri::command]
pub async fn fetch_ccswitch_import_models(
    key_id: String,
    state: State<'_, RuntimeState>,
) -> Result<Vec<String>, String> {
    if key_id.trim().is_empty() {
        return Err("未找到要导入的 API 密钥".to_owned());
    }
    let token = state.access_token().await?;
    let keys = state
        .api
        .get_authenticated("/keys?page=1&page_size=100", &token)
        .await?;
    let api_key = keys
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| id_matches(item.get("id"), key_id.trim()))
        })
        .and_then(|item| item.get("key"))
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "未找到要拉取模型的 API 密钥".to_owned())?;
    state
        .api
        .list_models_at(MOLLY_OPENAI_ROOT, Some(api_key))
        .await
}

#[tauri::command]
pub async fn import_api_key_to_ccswitch(
    key_id: String,
    name: String,
    agent: String,
    model: String,
    state: State<'_, RuntimeState>,
    app: AppHandle,
) -> Result<CcSwitchImportOutcome, String> {
    if key_id.trim().is_empty() {
        return Err("未找到要导入的 API 密钥".to_owned());
    }

    let name = name.trim();
    let model = model.trim();
    let agent = agent.trim().to_ascii_lowercase();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("供应商名称应为 1–80 个可见字符".to_owned());
    }
    if model.is_empty() || model.chars().count() > 160 || model.chars().any(char::is_control) {
        return Err("默认模型应为 1–160 个可见字符".to_owned());
    }
    if !matches!(
        agent.as_str(),
        "claude"
            | "claude-desktop"
            | "codex"
            | "gemini"
            | "grokbuild"
            | "opencode"
            | "openclaw"
            | "hermes"
            | "pi"
    ) {
        return Err("请选择 CC Switch 支持的 Agent".to_owned());
    }

    let token = state.access_token().await?;
    let (user, keys) = tokio::try_join!(
        state.api.get_authenticated("/auth/me", &token),
        state
            .api
            .get_authenticated("/keys?page=1&page_size=100", &token),
    )?;
    let account_id = account_identity(&user)?;
    let item = keys
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| id_matches(item.get("id"), key_id.trim()))
        })
        .ok_or_else(|| "未找到要导入的 API 密钥".to_owned())?;
    let api_key = item
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "API 密钥内容为空".to_owned())?;
    let usage_script = r#"({
      request: {
        url: "{{baseUrl}}/usage",
        method: "GET",
        headers: { "Authorization": "Bearer {{apiKey}}" }
      },
      extractor: function(response) {
        const remaining = response?.remaining ?? response?.quota?.remaining ?? response?.balance;
        const unit = response?.unit ?? response?.quota?.unit ?? "USD";
        return {
          isValid: response?.is_active ?? response?.isValid ?? true,
          remaining,
          unit
        };
      }
    })"#;

    let input = molly_ccswitch::MollyProviderImport {
        account_id,
        key_id: key_id.trim().to_owned(),
        name: name.to_owned(),
        app: agent.clone(),
        api_key: api_key.to_owned(),
        base_url: MOLLY_OPENAI_ROOT.to_owned(),
        model: model.to_owned(),
        usage_script: Some(usage_script.to_owned()),
    };
    let provider_id = tauri::async_runtime::spawn_blocking(move || {
        molly_ccswitch::import_molly_provider(&app, input)
    })
    .await
    .map_err(|_| "内置 CC Switch 导入任务未完成".to_owned())??;
    Ok(CcSwitchImportOutcome {
        provider_id,
        app: agent,
    })
}

#[derive(Serialize)]
pub struct CcSwitchImportOutcome {
    provider_id: String,
    app: String,
}

fn account_identity(user: &Value) -> Result<String, String> {
    match user.get("id") {
        Some(Value::String(id)) if !id.trim().is_empty() => Ok(id.clone()),
        Some(Value::Number(id)) => Ok(id.to_string()),
        _ => Err("账户响应缺少 ID，无法确定内置供应商归属".to_owned()),
    }
}

#[tauri::command]
pub async fn logout(state: State<'_, RuntimeState>, app: AppHandle) -> Result<(), String> {
    let refresh_token = state.refresh_token().await.ok();
    if let Some(refresh_token) = refresh_token {
        let _ = state.api.logout(&refresh_token).await;
    }
    state.clear_session().await;
    let _ = app.emit_to("main", "account-session-changed", ());
    Ok(())
}

#[tauri::command]
pub fn set_overlay_interactive(
    interactive: bool,
    app: AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<(), String> {
    let overlay = app
        .get_webview_window("main")
        .ok_or_else(|| "桌宠窗口尚未启动".to_owned())?;
    overlay
        .set_ignore_cursor_events(!interactive)
        .map_err(|error| format!("无法切换桌宠交互模式：{error}"))?;
    state
        .overlay_interactive
        .store(interactive, Ordering::Relaxed);
    let _ = app.emit_to("main", "overlay-interactive-changed", interactive);
    Ok(())
}

#[tauri::command]
pub fn play_overlay_motion(group: String, app: AppHandle) -> Result<(), String> {
    match group.as_str() {
        "Idle" | "Blink" | "Nod" | "Shake" => app
            .emit_to("main", "live2d-motion", &group)
            .map_err(|error| format!("无法播放动作：{error}")),
        _ => Err("未知的模型动作".to_owned()),
    }
}
