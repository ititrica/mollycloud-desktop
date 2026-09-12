use crate::api::Sub2ApiClient;
use serde_json::Value;
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const KEYRING_SERVICE: &str = "cn.mollycloud.client";
const KEYRING_ACCOUNT: &str = "mollycloud-session";

#[derive(Default)]
pub struct SessionSecrets {
    pub access_token: Option<String>,
    pub expires_at: Option<Instant>,
    pub temp_token: Option<String>,
    pub refresh_token: Option<String>,
    pub persist_refresh_token: bool,
}

pub struct RuntimeState {
    pub api: Sub2ApiClient,
    pub session: Arc<Mutex<SessionSecrets>>,
    // The console and pet share rotating refresh tokens. Only one refresh/login
    // transition may run at a time, including logout during an in-flight refresh.
    token_transition: Mutex<()>,
    pub overlay_interactive: AtomicBool,
}

impl RuntimeState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            api: Sub2ApiClient::new()?,
            session: Arc::new(Mutex::new(SessionSecrets::default())),
            token_transition: Mutex::new(()),
            overlay_interactive: AtomicBool::new(false),
        })
    }

    pub async fn accept_login(&self, data: &Value, remember_login: bool) -> Result<Value, String> {
        let _transition = self.token_transition.lock().await;
        let access_token = required_string(data, "access_token")?;
        let refresh_token = required_string(data, "refresh_token")?;
        if remember_login {
            save_refresh_token(&refresh_token)?;
        } else {
            delete_refresh_token()?;
        }

        let expires_in = data
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(3600);
        let mut session = self.session.lock().await;
        session.access_token = Some(access_token);
        session.expires_at = Some(Instant::now() + Duration::from_secs(expires_in));
        session.temp_token = None;
        session.refresh_token = Some(refresh_token);
        session.persist_refresh_token = remember_login;
        Ok(data.get("user").cloned().unwrap_or(Value::Null))
    }

    pub async fn remember_temp_token(&self, data: &Value) -> Result<(), String> {
        let token = required_string(data, "temp_token")?;
        self.session.lock().await.temp_token = Some(token);
        Ok(())
    }

    pub async fn take_temp_token(&self) -> Result<String, String> {
        self.session
            .lock()
            .await
            .temp_token
            .clone()
            .ok_or_else(|| "两步验证会话已失效，请重新登录".to_owned())
    }

    pub async fn access_token(&self) -> Result<String, String> {
        let _transition = self.token_transition.lock().await;
        let (memory_refresh_token, persist_refresh_token) = {
            let session = self.session.lock().await;
            if let (Some(token), Some(expires_at)) = (&session.access_token, session.expires_at) {
                if expires_at > Instant::now() + Duration::from_secs(60) {
                    return Ok(token.clone());
                }
            }
            (session.refresh_token.clone(), session.persist_refresh_token)
        };

        let (refresh_token, persist_refresh_token) = match memory_refresh_token {
            Some(token) => (token, persist_refresh_token),
            None => (load_refresh_token()?, true),
        };
        let data = self.api.refresh(&refresh_token).await?;
        let access_token = required_string(&data, "access_token")?;
        let next_refresh_token = data
            .get("refresh_token")
            .and_then(Value::as_str)
            .unwrap_or(&refresh_token)
            .to_owned();
        if persist_refresh_token {
            save_refresh_token(&next_refresh_token)?;
        }
        let expires_in = data
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(3600);
        let mut session = self.session.lock().await;
        session.access_token = Some(access_token.clone());
        session.expires_at = Some(Instant::now() + Duration::from_secs(expires_in));
        session.refresh_token = Some(next_refresh_token);
        session.persist_refresh_token = persist_refresh_token;
        Ok(access_token)
    }

    pub async fn refresh_token(&self) -> Result<String, String> {
        if let Some(token) = self.session.lock().await.refresh_token.clone() {
            return Ok(token);
        }
        load_refresh_token()
    }

    pub async fn clear_session(&self) {
        let _transition = self.token_transition.lock().await;
        let mut session = self.session.lock().await;
        session.access_token = None;
        session.expires_at = None;
        session.temp_token = None;
        session.refresh_token = None;
        session.persist_refresh_token = false;
        let _ = delete_refresh_token();
    }
}

pub fn load_refresh_token() -> Result<String, String> {
    credential_entry()?
        .get_password()
        .map_err(|_| "没有可恢复的登录会话".to_owned())
}

fn save_refresh_token(token: &str) -> Result<(), String> {
    credential_entry()?
        .set_password(token)
        .map_err(|_| "无法安全保存登录会话到 Windows 凭据管理器".to_owned())
}

fn delete_refresh_token() -> Result<(), String> {
    let entry = credential_entry()?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法清除 Windows 凭据管理器中的登录会话".to_owned()),
    }
}

fn credential_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|_| "Windows 凭据管理器不可用".to_owned())
}

fn required_string(data: &Value, name: &str) -> Result<String, String> {
    data.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("登录响应缺少 {name}"))
}
