use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use url::Url;

const KEYRING_SERVICE: &str = "cn.mollycloud.client";
const CONFIG_ACCOUNT: &str = "mollycloud-assistant-config-v1";
const ALLOWED_PROVIDERS: &[&str] = &[
    "mollycloud",
    "deepseek",
    "openai",
    "moonshot",
    "zhipu",
    "qwen",
    "siliconflow",
    "openrouter",
    "groq",
    "ollama",
    "custom",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub persona: String,
    pub custom_base_url: String,
    pub greet_interval: u32,
    pub molly_key_id: String,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "mollycloud".to_owned(),
            model: String::new(),
            persona: "你叫 Molly，语气自然、简洁、友好。".to_owned(),
            custom_base_url: String::new(),
            greet_interval: 20,
            molly_key_id: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AssistantConfigUpdate {
    enabled: bool,
    provider: String,
    model: String,
    persona: String,
    custom_base_url: String,
    greet_interval: u32,
    molly_key_id: String,
    api_key: Option<String>,
    clear_api_key: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AssistantConfigPublic {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub persona: String,
    pub custom_base_url: String,
    pub greet_interval: u32,
    pub molly_key_id: String,
    pub api_key_configured: bool,
}

#[tauri::command]
pub fn get_assistant_config() -> Result<AssistantConfigPublic, String> {
    let config = load_config()?;
    Ok(to_public(&config))
}

#[tauri::command]
pub fn save_assistant_config(
    config: AssistantConfigUpdate,
    app: AppHandle,
) -> Result<AssistantConfigPublic, String> {
    let provider = config.provider.trim().to_lowercase();
    if !ALLOWED_PROVIDERS.contains(&provider.as_str()) {
        return Err("不支持该 AI 服务商".to_owned());
    }
    let custom_base_url = config
        .custom_base_url
        .trim()
        .trim_end_matches('/')
        .to_owned();
    if provider == "custom" {
        validate_base_url(&custom_base_url)?;
    }
    let model = config.model.trim().chars().take(160).collect::<String>();
    let persona = config
        .persona
        .trim()
        .chars()
        .take(1_200)
        .collect::<String>();
    let next = AssistantConfig {
        enabled: config.enabled,
        provider: provider.clone(),
        model,
        persona,
        custom_base_url,
        greet_interval: config.greet_interval.clamp(5, 120),
        molly_key_id: config.molly_key_id.trim().chars().take(120).collect(),
    };

    if config.clear_api_key.unwrap_or(false) {
        delete_provider_key(&provider)?;
    } else if let Some(api_key) = config.api_key {
        let api_key = api_key.trim();
        if !api_key.is_empty() {
            if api_key.chars().count() > 4_096 {
                return Err("API Key 长度不正确".to_owned());
            }
            save_provider_key(&provider, api_key)?;
        }
    }
    save_config(&next)?;
    let public = to_public(&next);
    let _ = app.emit("assistant-config-changed", &public);
    Ok(public)
}

pub fn load_config() -> Result<AssistantConfig, String> {
    match config_entry()?.get_password() {
        Ok(value) => {
            serde_json::from_str(&value).map_err(|_| "AI 助手设置已损坏，请重新保存".to_owned())
        }
        Err(keyring::Error::NoEntry) => Ok(AssistantConfig::default()),
        Err(_) => Err("无法从 Windows 凭据管理器读取 AI 助手设置".to_owned()),
    }
}

pub fn load_provider_key(provider: &str) -> Result<Option<String>, String> {
    match provider_key_entry(provider)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("无法从 Windows 凭据管理器读取 AI API Key".to_owned()),
    }
}

pub fn provider_base(config: &AssistantConfig) -> Result<String, String> {
    provider_base_for(&config.provider, &config.custom_base_url)
}

pub fn provider_base_for(provider: &str, custom_base_url: &str) -> Result<String, String> {
    let base = match provider {
        "mollycloud" => crate::api::MOLLY_OPENAI_ROOT,
        "deepseek" => "https://api.deepseek.com",
        "openai" => "https://api.openai.com/v1",
        "moonshot" => "https://api.moonshot.cn/v1",
        "zhipu" => "https://open.bigmodel.cn/api/paas/v4",
        "qwen" => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "siliconflow" => "https://api.siliconflow.cn/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "groq" => "https://api.groq.com/openai/v1",
        "ollama" => "http://localhost:11434/v1",
        "custom" => custom_base_url,
        _ => return Err("不支持该 AI 服务商".to_owned()),
    };
    validate_base_url(base)?;
    Ok(base.trim_end_matches('/').to_owned())
}

pub fn requires_manual_key(provider: &str) -> bool {
    !matches!(provider, "mollycloud" | "ollama")
}

fn to_public(config: &AssistantConfig) -> AssistantConfigPublic {
    AssistantConfigPublic {
        enabled: config.enabled,
        provider: config.provider.clone(),
        model: config.model.clone(),
        persona: config.persona.clone(),
        custom_base_url: config.custom_base_url.clone(),
        greet_interval: config.greet_interval,
        molly_key_id: config.molly_key_id.clone(),
        api_key_configured: load_provider_key(&config.provider).ok().flatten().is_some(),
    }
}

fn save_config(config: &AssistantConfig) -> Result<(), String> {
    let value = serde_json::to_string(config).map_err(|_| "无法保存 AI 助手设置".to_owned())?;
    config_entry()?
        .set_password(&value)
        .map_err(|_| "无法把 AI 助手设置保存到 Windows 凭据管理器".to_owned())
}

fn save_provider_key(provider: &str, api_key: &str) -> Result<(), String> {
    provider_key_entry(provider)?
        .set_password(api_key)
        .map_err(|_| "无法把 AI API Key 保存到 Windows 凭据管理器".to_owned())
}

fn delete_provider_key(provider: &str) -> Result<(), String> {
    match provider_key_entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法删除已保存的 AI API Key".to_owned()),
    }
}

fn config_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, CONFIG_ACCOUNT)
        .map_err(|_| "Windows 凭据管理器不可用".to_owned())
}

fn provider_key_entry(provider: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(
        KEYRING_SERVICE,
        &format!("mollycloud-assistant-key-{provider}"),
    )
    .map_err(|_| "Windows 凭据管理器不可用".to_owned())
}

fn validate_base_url(value: &str) -> Result<(), String> {
    let parsed = Url::parse(value).map_err(|_| "请输入有效的 AI API 地址".to_owned())?;
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("AI API 地址不能包含账号、查询参数或片段".to_owned());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "AI API 地址缺少主机名".to_owned())?;
    let local = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if parsed.scheme() != "https" && !(local && parsed.scheme() == "http") {
        return Err("AI API 必须使用 HTTPS；只有本机 Ollama 可以使用 HTTP".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_endpoint_rejects_insecure_remote_http() {
        assert!(validate_base_url("http://example.com/v1").is_err());
        assert!(validate_base_url("http://localhost:11434/v1").is_ok());
        assert!(validate_base_url("https://example.com/v1").is_ok());
    }
}
