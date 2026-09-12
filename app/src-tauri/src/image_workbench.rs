use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{ipc::Channel, State};
use tokio::sync::Notify;

#[derive(Default)]
pub struct ImageRequests(Mutex<HashMap<String, Arc<Notify>>>);

#[derive(Deserialize)]
pub struct ImageRequest {
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ImageEvent {
    Headers {
        status: u16,
        headers: HashMap<String, String>,
    },
    Chunk {
        data: String,
    },
}

fn validate_url(raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|_| "请求地址无效。")?;
    let host = url.host_str().unwrap_or_default();
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || host.ends_with(".localhost")
    {
        return Err("请求地址需要使用 HTTPS；本地服务可使用 HTTP。".into());
    }
    Ok(url)
}

#[tauri::command]
pub async fn image_request(
    window: tauri::WebviewWindow,
    state: State<'_, ImageRequests>,
    id: String,
    request: ImageRequest,
    channel: Channel<ImageEvent>,
) -> Result<(), String> {
    if window.label() != "console" {
        return Err("仅控制台可以转发生图请求。".into());
    }
    if id.len() > 100 {
        return Err("请求标识无效。".into());
    }
    let notify = Arc::new(Notify::new());
    {
        let mut requests = state.0.lock().map_err(|_| "请求状态不可用。")?;
        if requests.len() >= 12 || requests.contains_key(&id) {
            return Err("同时进行的请求过多。".into());
        }
        requests.insert(id.clone(), notify.clone());
    }
    let result = tokio::select! {
        result = transfer(request, channel) => result,
        _ = notify.notified() => Err("请求已取消。".into()),
    };
    if let Ok(mut requests) = state.0.lock() {
        requests.remove(&id);
    }
    result
}

#[tauri::command]
pub fn cancel_image_request(
    window: tauri::WebviewWindow,
    state: State<'_, ImageRequests>,
    id: String,
) -> Result<(), String> {
    if window.label() != "console" {
        return Err("仅控制台可以取消生图请求。".into());
    }
    if let Some(notify) = state.0.lock().map_err(|_| "请求状态不可用。")?.get(&id) {
        notify.notify_one();
    }
    Ok(())
}

async fn transfer(request: ImageRequest, channel: Channel<ImageEvent>) -> Result<(), String> {
    let url = validate_url(&request.url)?;
    if !matches!(
        request.method.as_str(),
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
    ) {
        return Err("不支持此请求方法。".into());
    }
    let method =
        reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| "请求方法无效。")?;
    // 每次请求独立构建，不继承账户 Cookie、令牌或上一个服务商的认证头。
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(1800))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化图片请求。")?;
    let mut outgoing = client.request(method, url);
    for (name, value) in request.headers {
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "cookie"
                | "host"
                | "origin"
                | "referer"
                | "content-length"
                | "connection"
                | "proxy-authorization"
        ) || lower.starts_with("sec-")
        {
            continue;
        }
        outgoing = outgoing.header(name, value);
    }
    if let Some(body) = request.body {
        if body.len() > 180 * 1024 * 1024 {
            return Err("上传内容超过 128 MB。".into());
        }
        let bytes = STANDARD.decode(body).map_err(|_| "上传内容无效。")?;
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("上传内容超过 128 MB。".into());
        }
        outgoing = outgoing.body(bytes);
    }
    let mut response = outgoing.send().await.map_err(|error| {
        if error.is_timeout() {
            "图片请求超时。"
        } else {
            "无法连接图片服务，请检查地址、网络或代理设置。"
        }
    })?;
    if response.status().is_redirection() {
        return Err("图片接口返回重定向，请在设置中填写最终请求地址。".into());
    }
    let headers = response
        .headers()
        .iter()
        .filter(|(name, _)| name.as_str() != "set-cookie")
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.to_owned()))
        })
        .collect();
    channel
        .send(ImageEvent::Headers {
            status: response.status().as_u16(),
            headers,
        })
        .map_err(|_| "工作台已关闭。")?;
    let mut total = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "图片响应中断，请重试。")?
    {
        total += chunk.len();
        if total > 256 * 1024 * 1024 {
            return Err("图片响应超过 256 MB，请减少单次生成数量。".into());
        }
        for part in chunk.chunks(48 * 1024) {
            channel
                .send(ImageEvent::Chunk {
                    data: STANDARD.encode(part),
                })
                .map_err(|_| "工作台已关闭。")?;
        }
    }
    Ok(())
}

pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("molly-image-assets")
        .register_uri_scheme_protocol("molly-image", |context, request| {
            let path = request.uri().path().trim_start_matches('/');
            // 此来源只分发内置静态文件，不提供文件系统、账户或通用 IPC 入口。
            if request.method() != "GET" || path.split('/').any(|part| part == ".." || part.contains('%') || part.contains('\\')) {
                return tauri::http::Response::builder().status(404).body(Vec::new()).unwrap();
            }
            let path = if path.is_empty() { "index.html" } else { path };
            #[cfg(all(debug_assertions, not(feature = "image-workbench-smoke")))]
            let asset = {
                let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/image-workbench").join(path);
                std::fs::read(file).ok().map(|bytes| {
                    let mime = match path.rsplit('.').next().unwrap_or("") { "html" => "text/html", "js" => "text/javascript", "css" => "text/css", "svg" => "image/svg+xml", "woff2" => "font/woff2", "woff" => "font/woff", "ttf" => "font/ttf", _ => "application/octet-stream" };
                    (bytes, mime.to_owned())
                })
            };
            #[cfg(any(not(debug_assertions), feature = "image-workbench-smoke"))]
            let asset = context.app_handle().asset_resolver().get(format!("image-workbench/{path}")).map(|asset| (asset.bytes, asset.mime_type));
            let _ = context;
            match asset {
                Some((bytes, mime)) => tauri::http::Response::builder()
                    .header("Content-Type", mime).header("Access-Control-Allow-Origin", "*")
                    .header("Cache-Control", "no-cache")
                    .header("Content-Security-Policy", "default-src 'none'; script-src http://molly-image.localhost; style-src http://molly-image.localhost 'unsafe-inline'; img-src https: data: blob: http://molly-image.localhost; font-src http://molly-image.localhost data:; connect-src blob: data:; worker-src blob:; base-uri 'none'")
                    .body(bytes).unwrap(),
                None => tauri::http::Response::builder().status(404).body(Vec::new()).unwrap(),
            }
        }).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_network_endpoints() {
        for url in [
            "https://mollycloud.cn/v1",
            "https://custom.example/v1",
            "http://127.0.0.1:23456/v1",
        ] {
            assert!(validate_url(url).is_ok());
        }
        for url in [
            "file:///C:/secret",
            "http://ipc.localhost",
            "https://tauri.localhost",
            "http://example.com",
            "https://key@example.com",
            "data:text/plain,a",
        ] {
            assert!(validate_url(url).is_err());
        }
    }
}
