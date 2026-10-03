use crate::state::RuntimeState;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{State, Webview};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct SubscriptionState {
    mutation: Mutex<()>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoRenewInput {
    enabled: Option<bool>,
    #[serde(default)]
    clear_error: bool,
}

fn console_only(label: &str) -> Result<(), String> {
    if label == "console" { Ok(()) } else { Err("请在控制台管理订阅".into()) }
}

fn positive_id(value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) { return Err("订阅 ID 无效".into()); }
    value.parse::<u64>().ok().filter(|id| *id > 0 && *id <= 9_007_199_254_740_991).ok_or_else(|| "订阅 ID 无效".into())
}

fn auto_renew_body(input: AutoRenewInput) -> Result<Value, String> {
    let mut body = json!({});
    if let Some(enabled) = input.enabled { body["enabled"] = json!(enabled); }
    if input.clear_error { body["clear_error"] = json!(true); }
    if body.as_object().is_none_or(|body| body.is_empty()) { return Err("自动续杯设置无效".into()); }
    Ok(body)
}

#[tauri::command]
pub async fn fetch_subscriptions(webview: Webview, state: State<'_, RuntimeState>) -> Result<Value, String> {
    console_only(webview.label())?;
    let token = state.access_token().await?;
    // Summary omits platform, quota usage and auto-renew fields. Fetch the
    // same complete owned subscription DTOs as the website instead.
    state.api.get_authenticated("/subscriptions", &token).await
}

#[tauri::command]
pub async fn reset_subscription(webview: Webview, state: State<'_, RuntimeState>, subscriptions: State<'_, SubscriptionState>, subscription_id: String) -> Result<Value, String> {
    console_only(webview.label())?;
    let id = positive_id(&subscription_id)?;
    let _guard = subscriptions.mutation.try_lock().map_err(|_| "另一项订阅操作正在进行，请稍后重试")?;
    let token = state.access_token().await?;
    state.api.reset_subscription(id, &token).await
}

#[tauri::command]
pub async fn update_subscription_auto_renew(webview: Webview, state: State<'_, RuntimeState>, subscriptions: State<'_, SubscriptionState>, subscription_id: String, input: AutoRenewInput) -> Result<Value, String> {
    console_only(webview.label())?;
    let id = positive_id(&subscription_id)?;
    let body = auto_renew_body(input)?;
    let _guard = subscriptions.mutation.try_lock().map_err(|_| "另一项订阅操作正在进行，请稍后重试")?;
    let token = state.access_token().await?;
    state.api.update_subscription_auto_renew(id, &body, &token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_console_sources_and_invalid_subscription_ids() {
        assert!(console_only("console").is_ok());
        for label in ["main", "netspeed-widget", "skills", "payment-1"] { assert!(console_only(label).is_err()); }
        for id in ["0", "-1", "1/reset", "42?user_id=1", " 42", "9007199254740992", "18446744073709551616"] { assert!(positive_id(id).is_err()); }
        assert_eq!(positive_id("42").unwrap(), 42);
    }

    #[test]
    fn clearing_error_does_not_disable_auto_renew_and_empty_changes_are_rejected() {
        assert_eq!(auto_renew_body(AutoRenewInput { enabled: None, clear_error: true }).unwrap(), json!({"clear_error": true}));
        assert_eq!(auto_renew_body(AutoRenewInput { enabled: Some(false), clear_error: false }).unwrap(), json!({"enabled": false}));
        assert!(auto_renew_body(AutoRenewInput { enabled: None, clear_error: false }).is_err());
        assert!(serde_json::from_value::<AutoRenewInput>(json!({"enabled": true, "user_id": 2})).is_err());
    }
}
