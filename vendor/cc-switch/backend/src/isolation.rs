//! Validate providers for the user's actual CLI files, without imposing a
//! private home, credential helper or credential store policy.
use crate::{AppError, AppType};
use serde_json::Value;

pub(crate) fn validate_settings_content(app: &AppType, settings: &Value) -> Result<(), AppError> {
    if !settings.is_object() {
        return Err(AppError::Config("供应商配置必须为对象。".into()));
    }
    if matches!(app, AppType::Codex) {
        if let Some(config) = settings.get("config").and_then(Value::as_str) {
            config
                .parse::<toml::Value>()
                .map_err(|_| AppError::Config("Codex TOML 配置格式不正确。".into()))?;
        }
    }
    Ok(())
}

pub(crate) fn isolate_live_settings(app: &AppType, settings: &mut Value) -> Result<(), AppError> {
    // Existing live-write boundary: validate without changing the user's policy.
    validate_settings_content(app, settings)
}
