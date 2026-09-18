use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub required: bool,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub choices: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub kind: String,
    #[serde(default)]
    pub package: String,
    pub version: String,
    #[serde(default)]
    pub bin: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub oauth: bool,
    #[serde(default)]
    pub optional_args: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub homepage: String,
    pub source: String,
    pub fields: Vec<Field>,
    pub recipe: Option<Recipe>,
}
pub fn templates() -> Vec<Entry> {
    serde_json::from_str(include_str!("../../../src/mcp/catalog.json"))
        .expect("bundled MCP catalog")
}
pub fn template(id: &str) -> Result<Entry, String> {
    templates()
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or("未找到安装模板，请刷新目录".into())
}
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err("名称仅支持字母、数字、连字符和下划线（最多 80 字符）".into());
    }
    Ok(())
}
pub fn validate_url(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|_| "请输入有效的 MCP 地址")?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("请将凭据填写到请求头，不要放在地址中".into());
    }
    if url.scheme() != "https"
        && !(url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    {
        return Err("远程服务须使用 HTTPS，本机服务可使用 HTTP".into());
    }
    Ok(())
}
pub fn validate_spec(spec: &Value) -> Result<(), String> {
    let obj = spec.as_object().ok_or("MCP 配置必须是 JSON 对象")?;
    if serde_json::to_vec(spec).map_err(|_| "配置格式错误")?.len() > 32_768 {
        return Err("MCP 配置过长".into());
    }
    if obj
        .keys()
        .any(|key| !["type", "url", "command", "args", "env", "headers"].contains(&key.as_str()))
    {
        return Err(
            "支持 type、url、command、args、env、headers 字段，请粘贴单个 MCP 的连接配置".into(),
        );
    }
    if obj.get("type").is_some_and(|value| !value.is_string()) {
        return Err("type 必须是字符串".into());
    }
    match spec["type"].as_str().unwrap_or("stdio") {
        "stdio" => {
            let command = spec["command"].as_str().ok_or("本地 MCP 缺少 command")?;
            if command.trim().is_empty() || command.chars().any(char::is_control) {
                return Err("启动命令无效".into());
            }
            if let Some(args) = obj.get("args") {
                if !args.as_array().is_some_and(|a| {
                    a.len() <= 100
                        && a.iter()
                            .all(|v| v.as_str().is_some_and(|s| !s.contains('\0')))
                }) {
                    return Err("args 必须是字符串数组".into());
                }
            }
            if obj.contains_key("url") || obj.contains_key("headers") {
                return Err("本地 MCP 不使用 url 或 headers".into());
            }
        }
        "http" | "sse" => {
            validate_url(spec["url"].as_str().ok_or("远程 MCP 缺少 url")?)?;
            if obj.contains_key("command") || obj.contains_key("env") || obj.contains_key("args") {
                return Err("远程 MCP 不使用 command、args 或 env".into());
            }
        }
        _ => return Err("支持 stdio、http 和 sse 连接".into()),
    }
    for key in ["env", "headers"] {
        if let Some(value) = obj.get(key) {
            if !value.as_object().is_some_and(|v| {
                v.iter().all(|(k, v)| {
                    !k.is_empty()
                        && !k.chars().any(char::is_control)
                        && v.as_str().is_some_and(|s| !s.chars().any(char::is_control))
                })
            }) {
                return Err(format!("{key} 必须是字符串键值对"));
            }
        }
    }
    if let Some(env) = spec["env"].as_object() {
        if env.keys().any(|key| key.contains('=')) {
            return Err("环境变量名称不能包含等号".into());
        }
    }
    if let Some(headers) = spec["headers"].as_object() {
        for (key, value) in headers {
            reqwest::header::HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| "请求头名称无效")?;
            reqwest::header::HeaderValue::from_str(value.as_str().unwrap_or(""))
                .map_err(|_| "请求头内容无效")?;
        }
    }
    Ok(())
}

pub async fn registry_search(query: &str) -> Result<Vec<Entry>, String> {
    let client = super::process::network_client("https://registry.modelcontextprotocol.io")?;
    let mut response = client
        .get("https://registry.modelcontextprotocol.io/v0.1/servers")
        .query(&[("search", query), ("version", "latest"), ("limit", "40")])
        .send()
        .await
        .map_err(|_| "官方目录暂时无法连接，可继续使用本地目录")?;
    if !response.status().is_success() {
        return Err("官方目录暂时不可用，可稍后重试".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "目录读取失败")? {
        if bytes.len() + chunk.len() > 2_000_000 {
            return Err("目录响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let data: Value =
        serde_json::from_slice(&bytes).map_err(|_| "目录格式有变化，请使用本地目录")?;
    Ok(data["servers"]
        .as_array()
        .ok_or("目录格式有变化")?
        .iter()
        .filter_map(|row| {
            let s = &row["server"];
            let name = s["name"].as_str()?;
            let homepage = s["repository"]["url"]
                .as_str()
                .or(s["websiteUrl"].as_str())
                .unwrap_or("https://registry.modelcontextprotocol.io");
            if validate_url(homepage).is_err() {
                return None;
            }
            // Registry entries are discovery data, not automatically trusted install commands.
            Some(Entry {
                id: name.into(),
                name: s["title"].as_str().unwrap_or(name).into(),
                description: s["description"].as_str().unwrap_or("").into(),
                category: "官方目录".into(),
                homepage: homepage.into(),
                source: "registry".into(),
                fields: vec![],
                recipe: None,
            })
        })
        .collect())
}
