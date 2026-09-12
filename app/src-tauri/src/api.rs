use reqwest::{redirect::Policy, Client, StatusCode};
use serde_json::{json, Value};
use std::time::Duration;

pub const SERVICE_ORIGIN: &str = "https://mollycloud.cn";
const API_ROOT: &str = "https://mollycloud.cn/api/v1";
pub const MOLLY_OPENAI_ROOT: &str = "https://mollycloud.cn/v1";

#[derive(Clone)]
pub struct Sub2ApiClient {
    http: Client,
}

impl Sub2ApiClient {
    pub fn new() -> Result<Self, String> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .redirect(Policy::none())
            .user_agent(concat!("MollyCloud/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("无法初始化网络连接：{error}"))?;
        Ok(Self { http })
    }

    pub async fn health(&self) -> Result<Value, String> {
        let response = self
            .http
            .get(format!("{SERVICE_ORIGIN}/health"))
            .send()
            .await
            .map_err(network_error)?;
        parse_plain_response(response).await
    }

    pub async fn public_settings(&self) -> Result<Value, String> {
        self.get_public("/settings/public").await
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Value, String> {
        let response = self
            .http
            .post(format!("{API_ROOT}/auth/login"))
            .json(&json!({ "email": email, "password": password }))
            .send()
            .await
            .map_err(network_error)?;
        parse_api_response(response).await
    }

    pub async fn complete_two_factor(
        &self,
        temp_token: &str,
        totp_code: &str,
    ) -> Result<Value, String> {
        let response = self
            .http
            .post(format!("{API_ROOT}/auth/login/2fa"))
            .json(&json!({ "temp_token": temp_token, "totp_code": totp_code }))
            .send()
            .await
            .map_err(network_error)?;
        parse_api_response(response).await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<Value, String> {
        let response = self
            .http
            .post(format!("{API_ROOT}/auth/refresh"))
            .json(&json!({ "refresh_token": refresh_token }))
            .send()
            .await
            .map_err(network_error)?;
        parse_api_response(response).await
    }

    pub async fn logout(&self, refresh_token: &str) -> Result<(), String> {
        let response = self
            .http
            .post(format!("{API_ROOT}/auth/logout"))
            .json(&json!({ "refresh_token": refresh_token }))
            .send()
            .await
            .map_err(network_error)?;
        parse_api_response(response).await.map(|_| ())
    }

    pub async fn get_authenticated(&self, path: &str, access_token: &str) -> Result<Value, String> {
        let url = format!("{API_ROOT}{path}");
        for attempt in 0..3 {
            match self
                .http
                .get(&url)
                .bearer_auth(access_token)
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let retryable_status = status == StatusCode::TOO_MANY_REQUESTS
                        || status == StatusCode::BAD_GATEWAY
                        || status == StatusCode::SERVICE_UNAVAILABLE
                        || status == StatusCode::GATEWAY_TIMEOUT;
                    if retryable_status && attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(250 * (attempt + 1))).await;
                        continue;
                    }
                    return parse_api_response(response).await;
                }
                Err(error) => {
                    let message = network_error(error);
                    if attempt == 2 {
                        return Err(message);
                    }
                    tokio::time::sleep(Duration::from_millis(250 * (attempt + 1))).await;
                }
            }
        }
        unreachable!("authenticated GET retry loop always returns")
    }

    pub async fn list_models_at(
        &self,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let mut request = self.http.get(format!("{base_url}/models"));
        if let Some(api_key) = api_key.filter(|key| !key.is_empty()) {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await.map_err(network_error)?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .map_err(|_| "模型列表返回了无法识别的数据".to_owned())?;
        if !status.is_success() {
            return Err(openai_error(status, &value));
        }
        let mut models = value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        models.sort_unstable();
        models.dedup();
        if models.is_empty() {
            return Err("AI 服务没有返回可用模型".to_owned());
        }
        Ok(models)
    }

    pub async fn chat_completion_at(
        &self,
        base_url: &str,
        api_key: Option<&str>,
        body: &Value,
    ) -> Result<Value, String> {
        let mut request = self
            .http
            .post(format!("{base_url}/chat/completions"))
            .json(body);
        if let Some(api_key) = api_key.filter(|key| !key.is_empty()) {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await.map_err(network_error)?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .map_err(|_| "AI 服务返回了无法识别的数据".to_owned())?;
        if !status.is_success() {
            return Err(openai_error(status, &value));
        }
        Ok(value)
    }

    async fn get_public(&self, path: &str) -> Result<Value, String> {
        let response = self
            .http
            .get(format!("{API_ROOT}{path}"))
            .send()
            .await
            .map_err(network_error)?;
        parse_api_response(response).await
    }
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "连接 MollyCloud 超时，请检查网络后重试".to_owned()
    } else if error.is_connect() {
        "无法连接 MollyCloud，请检查网络或服务状态".to_owned()
    } else {
        "与 MollyCloud 的连接暂时中断，请稍后重试".to_owned()
    }
}

async fn parse_plain_response(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|_| "服务返回了无法识别的数据".to_owned())?;
    if !status.is_success() {
        return Err(http_error(status, &value));
    }
    Ok(value)
}

async fn parse_api_response(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|_| "服务返回了无法识别的数据".to_owned())?;

    if !status.is_success() {
        return Err(http_error(status, &value));
    }

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 0 {
        return Err(value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("请求未成功")
            .to_owned());
    }

    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

fn http_error(status: StatusCode, value: &Value) -> String {
    if status == StatusCode::UNAUTHORIZED {
        return "登录已过期，请重新登录".to_owned();
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return "请求过于频繁，请稍后重试".to_owned();
    }
    value
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("服务请求失败（HTTP {}）", status.as_u16()))
}

fn openai_error(status: StatusCode, value: &Value) -> String {
    let message = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or("AI 请求未成功");
    let safe_message = message.chars().take(240).collect::<String>();
    format!(
        "AI 服务请求失败（HTTP {}）：{safe_message}",
        status.as_u16()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_origin_is_https_and_fixed() {
        let parsed = url::Url::parse(SERVICE_ORIGIN).expect("valid service origin");
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("mollycloud.cn"));
        assert_eq!(parsed.path(), "/");
    }
}
