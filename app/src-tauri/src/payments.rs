//! Fixed account payment operations. Gateway credentials stay in native memory.
use crate::{
    recharge::{self, RechargeState},
    state::RuntimeState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use tauri::{AppHandle, State, Webview};

#[derive(Default)]
pub struct PaymentState(pub tokio::sync::Mutex<HashMap<u64, PaymentSession>>);
pub struct PaymentSession {
    owner: Value,
    result: Value,
    request: CreateOrder,
}

fn console_only(label: &str) -> Result<(), String> {
    if label == "console" {
        Ok(())
    } else {
        Err("请在控制台管理充值订单".into())
    }
}
fn id(value: u64) -> Result<u64, String> {
    if value > 0 && value <= 9_007_199_254_740_991 {
        Ok(value)
    } else {
        Err("订单 ID 无效".into())
    }
}
fn fields(value: &Value, keys: &[&str]) -> Value {
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(v) = value.get(key) {
            out.insert((*key).into(), v.clone());
        }
    }
    Value::Object(out)
}
fn order_display(value: &Value) -> Result<Value, String> {
    id(value["id"].as_u64().ok_or("订单数据无效")?)?;
    Ok(fields(
        value,
        &[
            "id",
            "amount",
            "pay_amount",
            "currency",
            "fee_rate",
            "bonus_amount",
            "payment_type",
            "out_trade_no",
            "status",
            "order_type",
            "created_at",
            "expires_at",
            "paid_at",
            "completed_at",
            "plan_id",
            "refund_amount",
        ],
    ))
}
fn normalize_checkout(info: &mut Value, config: &Value) -> Result<(), String> {
    let enabled = config
        .get("enabled")
        .or_else(|| config.get("payment_enabled"))
        .and_then(Value::as_bool)
        .ok_or("支付配置格式无效")?;
    info["payment_enabled"] = enabled.into();
    // Current upstream DTOs omit these flags: both endpoints return only
    // available methods / plans for sale. Preserve explicit older-server flags.
    let methods = info["methods"].as_object_mut().ok_or("付款方式格式无效")?;
    for method in methods.values_mut() {
        method
            .as_object_mut()
            .ok_or("付款方式格式无效")?
            .entry("available")
            .or_insert(Value::Bool(true));
    }
    let plans = info["plans"].as_array_mut().ok_or("套餐列表格式无效")?;
    for plan in plans {
        let plan = plan.as_object_mut().ok_or("套餐格式无效")?;
        plan.entry("for_sale").or_insert(Value::Bool(true));
        // Molly's site separates display visibility from the ability to purchase.
        plan.entry("user_visible").or_insert(Value::Bool(true));
    }
    Ok(())
}

async fn checkout(state: &RuntimeState, token: &str) -> Result<Value, String> {
    let config = state
        .api
        .get_authenticated("/payment/config", token)
        .await?;
    let mut info = match state
        .api
        .get_authenticated_optional("/payment/checkout-info", token)
        .await?
    {
        Some(info) => info,
        None => {
            // Older servers expose these fixed endpoints separately. Only 404 permits fallback.
            let (limits, plans) = tokio::join!(
                state.api.get_authenticated("/payment/limits", token),
                state.api.get_authenticated("/payment/plans", token)
            );
            let mut result = config.clone();
            let limits = limits?;
            for key in ["methods", "global_min", "global_max"] {
                result[key] = limits[key].clone();
            }
            result["plans"] = plans?;
            result
        }
    };
    normalize_checkout(&mut info, &config)?;
    Ok(fields(
        &info,
        &[
            "payment_enabled",
            "methods",
            "global_min",
            "global_max",
            "plans",
            "balance_disabled",
            "balance_recharge_multiplier",
            "subscription_usd_to_cny_rate",
            "recharge_fee_rate",
            "recharge_bonus_tiers",
            "recharge_bonus_mode",
            "recharge_bonus_notice",
            "help_text",
        ],
    ))
}

#[tauri::command]
pub async fn payment_checkout(
    webview: Webview,
    state: State<'_, RuntimeState>,
) -> Result<Value, String> {
    console_only(webview.label())?;
    checkout(&state, &state.access_token().await?).await
}
#[tauri::command]
pub async fn payment_orders(
    webview: Webview,
    state: State<'_, RuntimeState>,
    page: u32,
) -> Result<Value, String> {
    console_only(webview.label())?;
    if page == 0 || page > 100_000 {
        return Err("页码无效".into());
    }
    let token = state.access_token().await?;
    let result = state
        .api
        .get_authenticated(
            &format!("/payment/orders/my?page={page}&page_size=10"),
            &token,
        )
        .await?;
    let items = result["items"]
        .as_array()
        .ok_or("订单列表格式无效")?
        .iter()
        .map(order_display)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items":items,"total":result["total"],"page":page}))
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateOrder {
    amount: f64,
    payment_type: String,
    order_type: String,
    plan_id: Option<u64>,
}
impl CreateOrder {
    fn payload(&self) -> Result<Value, String> {
        if !self.amount.is_finite() || self.amount <= 0.0 || self.amount > 1e9 {
            return Err("金额必须大于 0".into());
        }
        if !["balance", "subscription"].contains(&self.order_type.as_str()) {
            return Err("订单类型无效".into());
        }
        if self.payment_type.is_empty()
            || self.payment_type.len() > 64
            || !self
                .payment_type
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err("付款方式无效".into());
        }
        if self.order_type == "subscription" {
            id(self.plan_id.ok_or("请选择套餐")?)?;
        }
        if self.order_type == "balance" && self.plan_id.is_some() {
            return Err("余额充值不应包含套餐".into());
        }
        let mut body = json!({"amount":self.amount,"payment_type":self.payment_type,"order_type":self.order_type,"is_mobile":false,"payment_source":"hosted_redirect","return_url":"https://mollycloud.cn/payment/result"});
        if let Some(plan) = self.plan_id {
            body["plan_id"] = plan.into();
        }
        Ok(body)
    }
    fn validate_available(&self, info: &Value) -> Result<(), String> {
        if info["payment_enabled"] != true {
            return Err("暂未开放在线支付".into());
        }
        if self.order_type == "balance" && info["balance_disabled"] == true {
            return Err("暂未开放余额充值".into());
        }
        let alias = match self.payment_type.as_str() {
            "alipay" => "alipay_direct",
            "wxpay" => "wxpay_direct",
            other => other,
        };
        let method = info["methods"]
            .get(&self.payment_type)
            .or_else(|| info["methods"].get(alias))
            .ok_or("付款方式已不可用，请刷新页面")?;
        if method["available"] != true {
            return Err("付款方式暂不可用".into());
        }
        if let Some(plan) = self.plan_id {
            let plan = info["plans"]
                .as_array()
                .and_then(|plans| plans.iter().find(|p| p["id"] == plan))
                .ok_or("套餐已不可用，请刷新页面")?;
            if plan["for_sale"] != true || plan["user_visible"] == false {
                return Err("此套餐暂不可购买，请刷新后选择其他套餐".into());
            }
            if plan["price"]
                .as_f64()
                .is_none_or(|price| (price - self.amount).abs() > 0.000001)
            {
                return Err("套餐价格已变化，请刷新后重新选择".into());
            }
        }
        // The server remains authoritative for exchange rates, discounts and channel limits.
        Ok(())
    }
}
fn creation_display(result: &Value, request: &CreateOrder) -> Result<Value, String> {
    id(result["order_id"]
        .as_u64()
        .ok_or("未能确认订单，请先刷新订单记录")?)?;
    let mut output = fields(
        result,
        &[
            "order_id",
            "amount",
            "pay_amount",
            "currency",
            "fee_rate",
            "bonus_amount",
            "expires_at",
            "qr_code",
        ],
    );
    output["can_open"] = payment_target(result, request).is_ok().into();
    Ok(output)
}
#[tauri::command]
pub async fn payment_create_order(
    webview: Webview,
    state: State<'_, RuntimeState>,
    payments: State<'_, PaymentState>,
    gate: State<'_, RechargeState>,
    request: CreateOrder,
) -> Result<Value, String> {
    console_only(webview.label())?;
    let body = request.payload()?;
    let _guard = gate.0.try_lock().map_err(|_| "正在处理支付操作，请稍候")?;
    let token = state.access_token().await?;
    let owner = state.api.get_authenticated("/auth/me", &token).await?["id"].clone();
    if owner.is_null() {
        return Err("无法确认当前账户".into());
    }
    request.validate_available(&checkout(&state, &token).await?)?;
    let result = state
        .api
        .post_authenticated("/payment/orders", &body, &token)
        .await
        .map_err(|e| format!("{e}；请先刷新订单记录确认是否已创建，避免重复下单。"))?;
    let display = creation_display(&result, &request)?;
    let mut sessions = payments.0.lock().await;
    sessions.retain(|_, s| s.owner == owner);
    if sessions.len() >= 30 {
        if let Some(old) = sessions.keys().copied().min() {
            sessions.remove(&old);
        }
    }
    sessions.insert(
        result["order_id"].as_u64().unwrap(),
        PaymentSession {
            owner,
            result,
            request,
        },
    );
    Ok(display)
}
#[tauri::command]
pub async fn payment_order(
    webview: Webview,
    state: State<'_, RuntimeState>,
    order_id: u64,
    verify: bool,
) -> Result<Value, String> {
    console_only(webview.label())?;
    let order_id = id(order_id)?;
    let token = state.access_token().await?;
    let mut order = state
        .api
        .get_authenticated(&format!("/payment/orders/{order_id}"), &token)
        .await?;
    if verify && order["status"] == "PENDING" {
        let trade = order["out_trade_no"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("订单缺少交易号")?;
        order = state
            .api
            .post_authenticated(
                "/payment/orders/verify",
                &json!({"out_trade_no":trade}),
                &token,
            )
            .await?;
    }
    order_display(&order)
}
#[tauri::command]
pub async fn payment_cancel_order(
    webview: Webview,
    state: State<'_, RuntimeState>,
    payments: State<'_, PaymentState>,
    gate: State<'_, RechargeState>,
    order_id: u64,
) -> Result<(), String> {
    console_only(webview.label())?;
    let order_id = id(order_id)?;
    let _guard = gate.0.lock().await;
    let token = state.access_token().await?;
    state
        .api
        .post_authenticated(
            &format!("/payment/orders/{order_id}/cancel"),
            &json!({}),
            &token,
        )
        .await?;
    payments.0.lock().await.remove(&order_id);
    Ok(())
}

fn payment_target(result: &Value, request: &CreateOrder) -> Result<String, String> {
    let order_id = id(result["order_id"].as_u64().ok_or("订单 ID 无效")?)?;
    if let Some(secret) = result["client_secret"].as_str().filter(|s| !s.is_empty()) {
        let airwallex = request.payment_type == "airwallex"
            && result["intent_id"].as_str().is_some_and(|s| !s.is_empty());
        let mut target = url::Url::parse(if airwallex {
            "https://mollycloud.cn/payment/airwallex"
        } else {
            "https://mollycloud.cn/payment/stripe"
        })
        .unwrap();
        {
            let mut query = target.query_pairs_mut();
            query.append_pair("order_id", &order_id.to_string());
            if airwallex {
                if let Some(trade) = result["out_trade_no"].as_str() {
                    query.append_pair("out_trade_no", trade);
                }
            } else {
                query.append_pair("client_secret", secret);
                if request.payment_type != "stripe" {
                    query.append_pair(
                        "method",
                        if request.payment_type == "wxpay" {
                            "wechat_pay"
                        } else {
                            "alipay"
                        },
                    );
                }
            }
            if let Some(resume) = result["resume_token"].as_str() {
                query.append_pair("resume_token", resume);
            }
        }
        return Ok(target.into());
    }
    if let Some(qr) = result["qr_code"].as_str().filter(|s| !s.is_empty()) {
        let mut target = url::Url::parse("https://mollycloud.cn/payment/qrcode").unwrap();
        {
            let mut query = target.query_pairs_mut();
            query.append_pair("order_id", &order_id.to_string());
            query.append_pair("qr", qr);
            query.append_pair("payment_type", &request.payment_type);
            if let Some(expires) = result["expires_at"].as_str() {
                query.append_pair("expires_at", expires);
            }
        }
        return Ok(target.into());
    }
    if let Some(target) = result["pay_url"]
        .as_str()
        .filter(|s| recharge::allowed_navigation(s))
    {
        return Ok(target.into());
    }
    Err("服务端未返回可打开的支付链接或二维码".into())
}
fn recovery(result: &Value, request: &CreateOrder) -> Value {
    let mut value = json!({"orderId":result["order_id"],"amount":result["amount"],"payAmount":result["pay_amount"],"orderType":request.order_type,"createdAt":chrono::Utc::now().timestamp_millis()});
    for (target, source) in [
        ("qrCode", "qr_code"),
        ("expiresAt", "expires_at"),
        ("paymentType", "payment_type"),
        ("payUrl", "pay_url"),
        ("outTradeNo", "out_trade_no"),
        ("clientSecret", "client_secret"),
        ("intentId", "intent_id"),
        ("currency", "currency"),
        ("countryCode", "country_code"),
        ("paymentEnv", "payment_env"),
        ("paymentMode", "payment_mode"),
        ("resumeToken", "resume_token"),
    ] {
        value[target] = result[source].as_str().unwrap_or("").into();
    }
    if value["paymentType"] == "" {
        value["paymentType"] = request.payment_type.clone().into();
    }
    value
}
#[tauri::command]
pub async fn open_payment_order(
    app: AppHandle,
    webview: Webview,
    state: State<'_, RuntimeState>,
    payments: State<'_, PaymentState>,
    gate: State<'_, RechargeState>,
    order_id: u64,
    dark: bool,
    view_id: String,
) -> Result<(), String> {
    console_only(webview.label())?;
    let order_id = id(order_id)?;
    if view_id.len() != 36 || !view_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("支付会话无效".into());
    }
    let _guard = gate.0.lock().await;
    let token = state.access_token().await?;
    let user = state.api.get_authenticated("/auth/me", &token).await?;
    let order = state
        .api
        .get_authenticated(&format!("/payment/orders/{order_id}"), &token)
        .await?;
    if order["status"] != "PENDING" {
        return Err("订单已结束，请刷新订单状态".into());
    }
    let sessions = payments.0.lock().await;
    let session = sessions
        .get(&order_id)
        .filter(|s| s.owner == user["id"])
        .ok_or("支付会话已结束，请查看订单状态；如仍未支付，可取消订单后重新下单。")?;
    let target = payment_target(&session.result, &session.request)?;
    let snapshot = recovery(&session.result, &session.request);
    drop(sessions);
    recharge::open_payment_target(&app, &token, &user, dark, target, snapshot, view_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> CreateOrder {
        CreateOrder {
            amount: 100.0,
            payment_type: "alipay".into(),
            order_type: "balance".into(),
            plan_id: None,
        }
    }
    #[test]
    fn accepts_actual_upstream_dtos_and_retains_explicit_disabled_flags() {
        let mut info = json!({"methods":{"alipay":{"payment_type":"alipay","daily_limit":1000,"single_min":1,"single_max":500,"fee_rate":2}},"plans":[{"id":1,"price":10}]});
        normalize_checkout(&mut info, &json!({"enabled":true})).unwrap();
        assert_eq!(info["payment_enabled"], true);
        assert_eq!(info["methods"]["alipay"]["available"], true);
        assert_eq!(info["plans"][0]["for_sale"], true);
        assert!(info["methods"]["alipay"].get("daily_remaining").is_none());
        info["methods"]["alipay"]["available"] = false.into();
        info["plans"][0]["for_sale"] = false.into();
        normalize_checkout(&mut info, &json!({"payment_enabled":false})).unwrap();
        assert_eq!(info["payment_enabled"], false);
        assert_eq!(info["methods"]["alipay"]["available"], false);
        assert_eq!(info["plans"][0]["for_sale"], false);
        assert!(normalize_checkout(
            &mut json!({"methods":{"alipay":5},"plans":[]}),
            &json!({"enabled":true})
        )
        .is_err());
    }
    #[test]
    fn visible_disabled_and_hidden_plans_cannot_create_orders() {
        let mut info = json!({"methods":{"alipay":{"available":true}},"plans":[{"id":7,"price":100,"for_sale":false,"user_visible":true}]});
        normalize_checkout(&mut info, &json!({"enabled":true})).unwrap();
        assert_eq!(info["plans"][0]["user_visible"], true);
        assert_eq!(info["plans"][0]["for_sale"], false);
        let mut r = request();
        r.order_type = "subscription".into();
        r.plan_id = Some(7);
        assert!(r
            .validate_available(&info)
            .unwrap_err()
            .contains("暂不可购买"));
        info["plans"][0]["for_sale"] = true.into();
        info["plans"][0]["user_visible"] = false.into();
        assert!(r
            .validate_available(&info)
            .unwrap_err()
            .contains("暂不可购买"));
        info["plans"][0]["user_visible"] = true.into();
        assert!(r.validate_available(&info).is_ok());
        // Revocation after the directory was read must also fail the fresh preflight.
        info["plans"][0]["for_sale"] = false.into();
        assert!(r.validate_available(&info).is_err());
    }
    #[test]
    fn qr_only_orders_have_a_fixed_official_payment_window_route() {
        let value = json!({"order_id":42,"qr_code":"weixin://wxpay/bizpayurl?pr=a&x=b","expires_at":"2026-10-02T10:00:00+08:00"});
        let target = url::Url::parse(&payment_target(&value, &request()).unwrap()).unwrap();
        assert_eq!(
            target.origin().ascii_serialization(),
            "https://mollycloud.cn"
        );
        assert_eq!(target.path(), "/payment/qrcode");
        let query = target
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query.get("qr").unwrap(),
            "weixin://wxpay/bizpayurl?pr=a&x=b"
        );
        assert_eq!(
            creation_display(&value, &request()).unwrap()["can_open"],
            true
        );
    }
    #[test]
    fn only_console_can_manage_payments() {
        assert!(console_only("console").is_ok());
        for label in ["main", "assistant", "ccswitch", "payment"] {
            assert!(console_only(label).is_err());
        }
    }
    #[test]
    fn validates_writes_and_fixes_return_destination() {
        let mut r = request();
        assert_eq!(
            r.payload().unwrap()["return_url"],
            "https://mollycloud.cn/payment/result"
        );
        for n in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            r.amount = n;
            assert!(r.payload().is_err());
        }
        assert!(serde_json::from_value::<CreateOrder>(json!({"amount":1,"payment_type":"alipay","order_type":"balance","return_url":"https://evil.example"})).is_err());
    }
    #[test]
    fn payment_secrets_never_leave_creation_response() {
        let r = request();
        let value = json!({"order_id":42,"amount":100,"pay_amount":100,"client_secret":"secret","resume_token":"resume","pay_url":"https://pay.example/a"});
        let display = creation_display(&value, &r).unwrap().to_string();
        for secret in ["secret", "resume", "pay.example"] {
            assert!(!display.contains(secret));
        }
    }
    #[test]
    fn payment_target_rejects_local_urls_and_encodes_secrets() {
        let r = request();
        assert!(payment_target(&json!({"order_id":1,"pay_url":"http://localhost/a"}), &r).is_err());
        let target = payment_target(
            &json!({"order_id":1,"client_secret":"a&return_url=bad"}),
            &r,
        )
        .unwrap();
        let parsed = url::Url::parse(&target).unwrap();
        assert_eq!(parsed.host_str(), Some("mollycloud.cn"));
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(k, _)| k == "client_secret")
                .unwrap()
                .1,
            "a&return_url=bad"
        );
        assert!(!parsed.query_pairs().any(|(k, _)| k == "return_url"));
        let mut stripe = request();
        stripe.payment_type = "stripe".into();
        let response = json!({"order_id":1,"client_secret":"secret","intent_id":"intent"});
        assert!(payment_target(&response, &stripe)
            .unwrap()
            .contains("/payment/stripe?"));
        stripe.payment_type = "airwallex".into();
        assert!(payment_target(&response, &stripe)
            .unwrap()
            .contains("/payment/airwallex?"));
    }
    #[test]
    fn stale_or_disabled_plan_is_rejected() {
        let mut r = request();
        r.order_type = "subscription".into();
        r.plan_id = Some(2);
        let mut info = json!({"payment_enabled":true,"methods":{"alipay":{"available":true}},"plans":[{"id":2,"for_sale":true,"price":100}]});
        assert!(r.validate_available(&info).is_ok());
        info["plans"][0]["price"] = 101.into();
        assert!(r.validate_available(&info).is_err());
    }
}
