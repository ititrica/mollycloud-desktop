use crate::state::RuntimeState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{State, Webview};

#[derive(Clone, Serialize)]
pub struct KeyGroup {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub subscription: bool,
    pub rate: Option<f64>,
    pub custom_rate: bool,
    pub description: String,
    pub default_rate: Option<f64>,
}

#[derive(Serialize)]
pub struct KeyGroups {
    pub groups: Vec<KeyGroup>,
    pub rates_available: bool,
}

#[derive(Serialize)]
pub struct KeyGroupChanged {
    pub key_id: String,
    pub group: KeyGroup,
}

fn console_only(webview: &Webview) -> Result<(), String> {
    if webview.label() == "console" {
        Ok(())
    } else {
        Err("请在控制台管理 API 密钥".into())
    }
}

fn positive_id(value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return Err("密钥或分组 ID 无效".into());
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
        .ok_or_else(|| "密钥或分组 ID 无效".into())
}

fn id_string(value: &Value) -> Option<String> {
    let text = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    positive_id(&text).ok().map(|id| id.to_string())
}

fn rate(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn normalize_groups(groups: &Value, rates: Option<&Value>) -> Result<KeyGroups, String> {
    let groups = groups.as_array().ok_or("服务返回的分组列表格式不正确")?;
    let groups = groups
        .iter()
        .filter_map(|group| {
            if group
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|s| s != "active")
            {
                return None;
            }
            let id = id_string(group.get("id")?)?;
            let custom = rates.and_then(|rates| rates.get(&id)).and_then(rate);
            Some(KeyGroup {
                id,
                name: group.get("name")?.as_str()?.to_owned(),
                platform: group
                    .get("platform")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                subscription: group.get("subscription_type").and_then(Value::as_str)
                    == Some("subscription"),
                rate: custom.or_else(|| group.get("rate_multiplier").and_then(rate)),
                custom_rate: custom.is_some(),
                description: group
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                default_rate: group.get("rate_multiplier").and_then(rate),
            })
        })
        .collect();
    Ok(KeyGroups {
        groups,
        rates_available: rates.is_some(),
    })
}

async fn available(state: &RuntimeState, token: &str) -> Result<KeyGroups, String> {
    let (groups, rates) = tokio::join!(
        state.api.get_authenticated("/groups/available", token),
        state.api.get_authenticated("/groups/rates", token),
    );
    normalize_groups(&groups?, rates.ok().as_ref())
}

#[tauri::command]
pub async fn fetch_key_groups(
    webview: Webview,
    state: State<'_, RuntimeState>,
) -> Result<KeyGroups, String> {
    console_only(&webview)?;
    let token = state.access_token().await?;
    available(&state, &token).await
}

#[tauri::command]
pub async fn change_key_group(
    webview: Webview,
    state: State<'_, RuntimeState>,
    key_id: String,
    group_id: String,
) -> Result<KeyGroupChanged, String> {
    console_only(&webview)?;
    let key_id = positive_id(&key_id)?;
    let group_id = positive_id(&group_id)?;
    let token = state.access_token().await?;
    // Reload permissions at save time; an expired subscription must not be accepted from a stale dropdown.
    let group = available(&state, &token)
        .await?
        .groups
        .into_iter()
        .find(|group| group.id == group_id.to_string())
        .ok_or("此分组已不可用，请刷新分组列表")?;
    let updated = state.api.update_key_group(key_id, group_id, &token).await?;
    validate_updated(&updated, key_id, group_id)?;
    // The server returns a complete API key. Return only the fields needed by the UI.
    Ok(KeyGroupChanged {
        key_id: key_id.to_string(),
        group,
    })
}

fn validate_updated(value: &Value, key: u64, group: u64) -> Result<(), String> {
    if value.get("id").and_then(id_string).as_deref() != Some(&key.to_string())
        || value.get("group_id").and_then(id_string).as_deref() != Some(&group.to_string())
    {
        return Err("服务未确认目标分组，请刷新密钥列表后检查".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_api_key(
    webview: Webview,
    state: State<'_, RuntimeState>,
    key_id: String,
) -> Result<(), String> {
    console_only(&webview)?;
    let key_id = positive_id(&key_id)?;
    let token = state.access_token().await?;
    // The service checks that this key belongs to the authenticated account.
    state.api.delete_key(key_id, &token).await
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateKey {
    name: String,
    group_id: Option<String>,
    custom_key: Option<String>,
    #[serde(default)]
    ip_whitelist: Vec<String>,
    #[serde(default)]
    ip_blacklist: Vec<String>,
    quota: Option<f64>,
    expires_in_days: Option<u32>,
    rate_limit_5h: Option<f64>,
    rate_limit_1d: Option<f64>,
    rate_limit_7d: Option<f64>,
}
impl CreateKey {
    fn payload(&self) -> Result<Value, String> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err("名称须为 1–128 个字符".into());
        }
        if let Some(key) = &self.custom_key {
            if !(16..=512).contains(&key.len())
                || !key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                return Err("自定义密钥须为 16–512 位字母、数字、下划线或连字符".into());
            }
        }
        for list in [&self.ip_whitelist, &self.ip_blacklist] {
            if list.len() > 100 {
                return Err("每个 IP 列表最多 100 条".into());
            }
            for item in list {
                let (address, prefix) = item
                    .split_once('/')
                    .map_or((item.as_str(), None), |(a, p)| (a, Some(p)));
                let ip = address
                    .parse::<std::net::IpAddr>()
                    .map_err(|_| "IP 地址或 CIDR 格式无效")?;
                if let Some(prefix) = prefix {
                    let bits = prefix.parse::<u8>().map_err(|_| "CIDR 前缀无效")?;
                    if bits > if ip.is_ipv4() { 32 } else { 128 } {
                        return Err("CIDR 前缀无效".into());
                    }
                }
            }
        }
        for amount in [
            self.quota,
            self.rate_limit_5h,
            self.rate_limit_1d,
            self.rate_limit_7d,
        ]
        .into_iter()
        .flatten()
        {
            if !amount.is_finite() || amount < 0.0 {
                return Err("额度必须为非负数".into());
            }
        }
        if self.expires_in_days.is_some_and(|d| d == 0 || d > 36500) {
            return Err("有效期须为 1–36500 天".into());
        }
        let mut payload = serde_json::to_value(self).map_err(|_| "密钥参数格式错误")?;
        payload["name"] = name.into();
        payload["group_id"] = self
            .group_id
            .as_deref()
            .map(positive_id)
            .transpose()?
            .map(Value::from)
            .unwrap_or(Value::Null);
        payload.as_object_mut().unwrap().retain(|_, v| !v.is_null());
        Ok(payload)
    }
}

#[tauri::command]
pub async fn create_api_key(
    webview: Webview,
    state: State<'_, RuntimeState>,
    request: CreateKey,
) -> Result<Value, String> {
    console_only(&webview)?;
    let payload = request.payload()?;
    let token = state.access_token().await?;
    if let Some(id) = &request.group_id {
        if !available(&state, &token)
            .await?
            .groups
            .iter()
            .any(|g| &g.id == id)
        {
            return Err("此分组已不可用，请重新选择".into());
        }
    }
    let created = state
        .api
        .post_authenticated("/keys", &payload, &token)
        .await?;
    if created.get("id").and_then(id_string).is_none() {
        return Err("服务未确认创建结果，请刷新列表检查后再试".into());
    }
    // Keep the existing masked-list/copy-by-ID boundary. No full key leaves Rust.
    let mut list = serde_json::json!({"items":[created]});
    crate::commands::mask_api_keys(&mut list);
    Ok(list["items"][0].take())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn create_key_validates_ip_limits_expiry_and_custom_key() {
        let base = json!({"name":" New key ","group_id":"42","quota":0,"ip_whitelist":["10.0.0.0/8","2001:db8::/32"]});
        let payload = serde_json::from_value::<CreateKey>(base.clone())
            .unwrap()
            .payload()
            .unwrap();
        assert_eq!(payload["name"], "New key");
        assert_eq!(payload["group_id"], 42);
        assert!(payload.get("custom_key").is_none());
        for (key, value) in [
            ("name", json!(" ")),
            ("group_id", json!("1/../2")),
            ("quota", json!(-1)),
            ("expires_in_days", json!(0)),
            ("custom_key", json!("too-short")),
            ("ip_whitelist", json!(["10.1.1.1/33"])),
            ("ip_blacklist", json!(["2001:db8::/129"])),
        ] {
            let mut invalid = base.clone();
            invalid[key] = value;
            assert!(
                serde_json::from_value::<CreateKey>(invalid)
                    .unwrap()
                    .payload()
                    .is_err(),
                "{key}"
            );
        }
        let mut unknown = base;
        unknown["user_id"] = 999.into();
        assert!(serde_json::from_value::<CreateKey>(unknown).is_err());
    }
    #[test]
    fn created_key_is_masked_before_returning_to_console() {
        let mut response = json!({"items":[{"id":42,"key":"sk-sensitive-test-value-1234"}]});
        crate::commands::mask_api_keys(&mut response);
        assert_eq!(response["items"][0]["key"], "sk-••••1234");
        assert!(!response.to_string().contains("sensitive-test"));
    }
    #[test]
    fn ids_cannot_change_request_path() {
        for value in [
            "",
            "0",
            "-1",
            "1/../2",
            "1?admin=1",
            "1.2",
            "9007199254740992",
        ] {
            assert!(positive_id(value).is_err());
        }
        assert_eq!(positive_id("42").unwrap(), 42);
    }
    #[test]
    fn only_allowed_display_fields_and_effective_rates_leave_backend() {
        let groups = json!([
            {"id":1,"name":"Pro","platform":"openai","rate_multiplier":2,"secret":"must not leave backend"},
            {"id":2,"name":"Inactive","status":"disabled"},
            {"id":3,"name":"Subscription","subscription_type":"subscription","rate_multiplier":1}
        ]);
        let result = normalize_groups(&groups, Some(&json!({"1":0.5}))).unwrap();
        assert_eq!(result.groups.len(), 2);
        assert_eq!(result.groups[0].rate, Some(0.5));
        assert!(result.groups[0].custom_rate && result.groups[1].subscription);
        assert!(!serde_json::to_string(&result).unwrap().contains("secret"));
        assert!(!normalize_groups(&groups, None).unwrap().rates_available);
    }
    #[test]
    fn success_requires_server_confirmation_of_the_requested_key_and_group() {
        assert!(validate_updated(&json!({"id":8,"group_id":3}), 8, 3).is_ok());
        assert!(validate_updated(&json!({"id":8,"group_id":4}), 8, 3).is_err());
        assert!(validate_updated(&json!({"id":9,"group_id":3}), 8, 3).is_err());
    }
}
