//! Read-only, allowlisted account data for assistant tools and local reminders.
use crate::state::RuntimeState;
use serde_json::{json, Map, Value};
use tauri::{State, Webview};

pub(crate) fn numeric(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}
fn pick(value: &Value, names: &[&str]) -> Value {
    let mut out = Map::new();
    for name in names {
        if let Some(field) = value.get(*name) {
            if field.is_string() || field.is_number() || field.is_boolean() || field.is_null() {
                out.insert((*name).into(), field.clone());
            }
        }
    }
    Value::Object(out)
}
fn allowed(label: &str) -> Result<(), String> {
    if matches!(label, "main" | "console") {
        Ok(())
    } else {
        Err("此窗口不能查询账户信息".into())
    }
}
pub(crate) fn subscriptions_public(value: &Value) -> Value {
    let source = value
        .as_array()
        .or_else(|| value.get("subscriptions")?.as_array());
    let items=source.into_iter().flatten().take(100).map(|item| {
        let group=&item["group"];
        let mut result=pick(item,&["id","status","starts_at","expires_at","auto_renew_enabled"]);
        result["name"]=group.get("name").or_else(|| item.get("group_name")).cloned().unwrap_or(json!("未命名订阅"));
        let quotas=[("daily",1),("weekly",7),("monthly",30)].into_iter().filter_map(|(period,days)| {
            let limit=numeric(&group[format!("{period}_limit_usd")])?;
            if limit<=0.0 { return None; }
            let used=numeric(&item[format!("{period}_usage_usd")])?.max(0.0);
            let remaining=(limit-used).max(0.0);
            Some(json!({"period":period,"limit_usd":limit,"used_usd":used,"remaining_usd":remaining,"remaining_percent":(remaining/limit*100.0).clamp(0.0,100.0),"window_start":item.get(format!("{period}_window_start")).cloned().unwrap_or(Value::Null),"window_days":days}))
        }).collect::<Vec<_>>();
        result["quotas"]=json!(quotas);
        result
    }).collect::<Vec<_>>();
    json!({"currency":"USD","subscriptions":items})
}
#[tauri::command]
pub async fn assistant_account_health(
    webview: Webview,
    state: State<'_, RuntimeState>,
) -> Result<Value, String> {
    allowed(webview.label())?;
    let token = state.access_token().await?;
    let (user, subs) = tokio::try_join!(
        state.api.get_authenticated("/auth/me", &token),
        state.api.get_authenticated("/subscriptions", &token)
    )?;
    let user_id = user
        .get("id")
        .filter(|id| id.is_string() || id.is_u64())
        .cloned()
        .ok_or("账户信息缺少 ID")?;
    Ok(
        json!({"user_id":user_id,"currency":"USD","balance":numeric(&user["balance"]),"subscriptions":subscriptions_public(&subs)["subscriptions"]}),
    )
}
#[tauri::command]
pub async fn assistant_account_tool(
    webview: Webview,
    state: State<'_, RuntimeState>,
    tool_name: String,
) -> Result<Value, String> {
    allowed(webview.label())?;
    if !matches!(
        tool_name.as_str(),
        "get_account_overview"
            | "get_subscription_status"
            | "get_usage_summary"
            | "get_api_key_status"
    ) {
        return Err("此账户查询未获授权".into());
    }
    Ok(json!({"ok":true,"data":query_data(&state,&tool_name).await?}))
}
async fn query_data(state: &RuntimeState, tool_name: &str) -> Result<Value, String> {
    let token = state.access_token().await?;
    let data = match tool_name {
        "get_account_overview" => {
            let user = state.api.get_authenticated("/auth/me", &token).await?;
            let mut data = pick(
                &user,
                &["username", "balance", "status", "concurrency_limit"],
            );
            data["currency"] = json!("USD");
            data
        }
        "get_subscription_status" => subscriptions_public(
            &state
                .api
                .get_authenticated("/subscriptions", &token)
                .await?,
        ),
        "get_usage_summary" => pick(
            &state
                .api
                .get_authenticated("/usage/dashboard/stats", &token)
                .await?,
            &[
                "today_requests",
                "today_tokens",
                "today_actual_cost",
                "total_requests",
                "total_tokens",
                "total_actual_cost",
                "rpm",
                "tpm",
            ],
        ),
        _ => {
            let keys = state
                .api
                .get_authenticated("/keys?page=1&page_size=100", &token)
                .await?;
            let items = keys["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| {
                    pick(
                        item,
                        &[
                            "name",
                            "status",
                            "quota",
                            "quota_used",
                            "last_used_at",
                            "expires_at",
                        ],
                    )
                })
                .collect::<Vec<_>>();
            json!({"currency":"USD","items":items})
        }
    };
    Ok(data)
}

#[derive(serde::Serialize)]
pub struct LocalAccountReply {
    pub handled: bool,
    pub content: Option<String>,
}
fn query_plan(text: &str) -> Vec<&'static str> {
    let text = text.trim().to_lowercase();
    let query_intent = [
        "查",
        "余额",
        "多少",
        "剩余",
        "到期",
        "我的",
        "账户",
        "账号",
        "用量",
        "消费",
        "balance",
        "quota",
        "expiry",
        "usage",
        "my subscription",
        "subscription",
        "密钥",
        "api key",
        "token",
    ]
    .iter()
    .any(|word| text.contains(word));
    if !query_intent {
        return vec![];
    }
    let code_task = [
        "写代码",
        "翻译",
        "写文章",
        "解释代码",
        "代码示例",
        "translate",
        "write code",
    ]
    .iter()
    .any(|word| text.contains(word));
    if code_task {
        return vec![];
    }
    let all = [
        "账户概览",
        "账户情况",
        "账户信息",
        "账号情况",
        "account overview",
    ]
    .iter()
    .any(|word| text.contains(word));
    let mut queries = vec![];
    if all || ["余额", "balance"].iter().any(|word| text.contains(word)) {
        queries.push("get_account_overview");
    }
    if all
        || [
            "订阅",
            "套餐",
            "额度",
            "到期",
            "quota",
            "subscription",
            "expiry",
            "expires",
        ]
        .iter()
        .any(|word| text.contains(word))
    {
        queries.push("get_subscription_status");
    }
    if all
        || [
            "用量",
            "消费",
            "请求次数",
            "token 使用",
            "token用量",
            "usage",
            "spent",
            "token",
        ]
        .iter()
        .any(|word| text.contains(word))
    {
        queries.push("get_usage_summary");
    }
    if ["密钥状态", "api key status"]
        .iter()
        .any(|word| text.contains(word))
    {
        queries.push("get_api_key_status");
    }
    queries
}
fn money(value: &Value) -> String {
    numeric(value)
        .map(|n| format!("US${n:.4}"))
        .unwrap_or_else(|| "暂未获取".into())
}
fn format_query(kind: &str, data: &Value) -> String {
    match kind {
        "get_account_overview" => format!("当前账户余额：{}。", money(&data["balance"])),
        "get_usage_summary" => format!(
            "今日消费：{}；累计消费：{}。\n今日请求：{}；今日 Token：{}。",
            money(&data["today_actual_cost"]),
            money(&data["total_actual_cost"]),
            data["today_requests"],
            data["today_tokens"]
        ),
        "get_subscription_status" => {
            let lines = data["subscriptions"]
                .as_array()
                .into_iter()
                .flatten()
                .take(10)
                .map(|sub| {
                    let name = sub["name"].as_str().unwrap_or("未命名订阅");
                    let expiry = sub["expires_at"]
                        .as_str()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|date| {
                            date.with_timezone(&chrono::Local)
                                .format("%Y/%m/%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_else(|| "无到期时间".into());
                    let quotas = sub["quotas"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|q| {
                            format!(
                                "{}剩余 {} / {}（{:.2}%）",
                                match q["period"].as_str() {
                                    Some("daily") => "每日",
                                    Some("weekly") => "每周",
                                    _ => "每月",
                                },
                                money(&q["remaining_usd"]),
                                money(&q["limit_usd"]),
                                numeric(&q["remaining_percent"]).unwrap_or(0.0)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("；");
                    let status = match sub["status"].as_str() {
                        Some("active") => "有效",
                        Some("expired") => "已过期",
                        Some("revoked") => "已撤销",
                        _ => "其他状态",
                    };
                    format!(
                        "{name}（{status}）\n{}\n到期时间：{expiry}",
                        if quotas.is_empty() {
                            "未设置周期额度"
                        } else {
                            &quotas
                        }
                    )
                })
                .collect::<Vec<_>>();
            if lines.is_empty() {
                "当前没有订阅套餐。".into()
            } else {
                lines.join("\n\n")
            }
        }
        _ => {
            let items = data["items"]
                .as_array()
                .into_iter()
                .flatten()
                .take(20)
                .map(|key| {
                    format!(
                        "{}：{}",
                        key["name"].as_str().unwrap_or("未命名密钥"),
                        key["status"].as_str().unwrap_or("未知")
                    )
                })
                .collect::<Vec<_>>();
            if items.is_empty() {
                "当前没有 API 密钥。".into()
            } else {
                items.join("\n")
            }
        }
    }
}
pub(crate) async fn local_reply(state: &RuntimeState, text: &str) -> LocalAccountReply {
    let plan = query_plan(text);
    if plan.is_empty() {
        return LocalAccountReply {
            handled: false,
            content: None,
        };
    }
    let mut replies = vec![];
    for kind in plan {
        match query_data(state, kind).await {
            Ok(data) => replies.push(format_query(kind, &data)),
            Err(_) => replies.push("账户查询失败，请确认已登录 MollyCloud 并检查网络。".into()),
        }
    }
    LocalAccountReply {
        handled: true,
        content: Some(replies.join("\n\n").chars().take(3500).collect()),
    }
}
#[tauri::command]
pub async fn assistant_local_query(
    webview: Webview,
    state: State<'_, RuntimeState>,
    text: String,
) -> Result<LocalAccountReply, String> {
    allowed(webview.label())?;
    if text.chars().count() > 4000 {
        return Err("查询文本过长".into());
    }
    Ok(local_reply(&state, &text).await)
}

pub(crate) fn key_ids(keys: &Value) -> Vec<u64> {
    keys["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item["id"]
                .as_u64()
                .or_else(|| item["id"].as_str()?.parse().ok())
        })
        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
        .take(100)
        .collect()
}
pub(crate) fn apply_key_usage(keys: &mut Value, stats: &Value) {
    if let Some(items) = keys["items"].as_array_mut() {
        for item in items {
            let id = item["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| item["id"].to_string());
            let entry = &stats["stats"][id];
            let cost = numeric(&entry["total_actual_cost"]).filter(|n| *n >= 0.0);
            let today = numeric(&entry["today_actual_cost"]).filter(|n| *n >= 0.0);
            item["usage"] = json!({"total_actual_cost":cost,"today_actual_cost":today});
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_subscription_details_keep_all_quotas_and_no_credentials() {
        let raw = json!([{"id":1,"status":"active","expires_at":"2026-10-04T00:00:00Z","daily_usage_usd":9.7,"weekly_usage_usd":80,"group":{"name":"Pro","daily_limit_usd":10,"weekly_limit_usd":100,"secret":"hidden"},"user":{"password":"hidden"}}]);
        let result = subscriptions_public(&raw);
        assert_eq!(
            result["subscriptions"][0]["quotas"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            result["subscriptions"][0]["quotas"][0]["remaining_percent"]
                .as_f64()
                .unwrap()
                < 5.0
        );
        assert!(!result.to_string().contains("hidden"));
    }
    #[test]
    fn key_spend_comes_from_actual_cost_not_quota_count() {
        let mut keys = json!({"items":[{"id":7,"quota_used":0},{"id":8,"quota_used":99}]});
        apply_key_usage(
            &mut keys,
            &json!({"stats":{"7":{"total_actual_cost":0.0123,"today_actual_cost":0.0012,"secret":"hidden"}}}),
        );
        assert_eq!(
            keys["items"][0]["usage"]["total_actual_cost"],
            json!(0.0123)
        );
        assert!(keys["items"][1]["usage"]["total_actual_cost"].is_null());
        assert!(!keys.to_string().contains("hidden"));
    }
    #[test]
    fn validates_account_window_scope_and_key_ids() {
        assert!(allowed("main").is_ok());
        for label in ["payment-1", "netspeed-widget", "image"] {
            assert!(allowed(label).is_err());
        }
        assert_eq!(
            key_ids(&json!({"items":[{"id":1},{"id":"2"},{"id":-1},{"id":"2?token=x"}]})),
            vec![1, 2]
        );
    }
    #[test]
    fn local_account_intents_are_resolved_without_a_model() {
        assert_eq!(query_plan("查一下我的余额"), vec!["get_account_overview"]);
        assert_eq!(
            query_plan("订阅额度还剩多少，什么时候到期？"),
            vec!["get_subscription_status"]
        );
        assert_eq!(query_plan("查询账户概览").len(), 3);
        assert!(query_plan("翻译 balance 这个单词").is_empty());
        assert!(query_plan("你好，讲个笑话").is_empty());
        assert!(
            format_query("get_account_overview", &json!({"balance":3.72})).contains("US$3.7200")
        );
    }
}
