//! MollyCloud 桌面版更新检查。
//!
//! 更新清单只用于告诉用户有可下载的新版本；安装仍由用户在系统浏览器中明确发起。
//! 不从清单执行代码，也不接受非 MollyCloud 发布域名的下载地址。

use reqwest::header::{ACCEPT, CACHE_CONTROL, USER_AGENT};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use url::Url;

pub const RELEASE_MANIFEST_URL: &str = "https://desktop.veriolink.com/latest.json";
const RELEASE_HOST: &str = "desktop.veriolink.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_MANIFEST_BYTES: usize = 32 * 1024;
const MAX_NOTES_CHARS: usize = 2_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseManifest {
    version: String,
    download_url: String,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub download_url: String,
    pub notes: Option<String>,
}

fn parse_version(raw: &str) -> Result<Version, String> {
    let version = raw.trim().trim_start_matches(['v', 'V']);
    if version.is_empty() || version.len() > 64 {
        return Err("更新清单中的版本号无效".to_owned());
    }
    Version::parse(version).map_err(|_| "更新清单中的版本号无效".to_owned())
}

fn is_release_download_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(RELEASE_HOST))
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().to_ascii_lowercase().ends_with(".exe")
}

fn parse_available_update(
    current_version: &str,
    body: &[u8],
) -> Result<Option<AvailableUpdate>, String> {
    let current = parse_version(current_version)?;
    let manifest: ReleaseManifest =
        serde_json::from_slice(body).map_err(|_| "更新清单格式无效".to_owned())?;
    let latest = parse_version(&manifest.version)?;

    if latest <= current {
        return Ok(None);
    }
    if !is_release_download_url(&manifest.download_url) {
        return Err("更新清单中的下载地址未获信任".to_owned());
    }

    let notes = manifest.notes.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.chars().take(MAX_NOTES_CHARS).collect())
        }
    });
    Ok(Some(AvailableUpdate {
        version: latest.to_string(),
        download_url: manifest.download_url,
        notes,
    }))
}

fn manifest_request_url(current_version: &str) -> String {
    // 清单很小且发布时设置为 no-store；再按分钟变化查询串，避免中间缓存延迟首启检查。
    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 60;
    format!("{RELEASE_MANIFEST_URL}?current_version={current_version}&checked_at={checked_at}")
}

fn build_client(proxy_url: Option<String>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .https_only(true);
    if let Some(proxy_url) = proxy_url {
        let proxy = reqwest::Proxy::all(proxy_url).map_err(|_| "系统代理配置无效".to_owned())?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|_| "无法创建更新检查请求".to_owned())
}

async fn fetch_manifest(
    client: &reqwest::Client,
    url: &str,
    current_version: &str,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .header(ACCEPT, "application/json")
        .header(CACHE_CONTROL, "no-cache")
        .header(USER_AGENT, format!("MollyCloud/{current_version}"))
        .send()
        .await
        .map_err(|_| "无法连接更新服务器".to_owned())?
        .error_for_status()
        .map_err(|_| "更新服务器暂不可用".to_owned())?;

    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES as u64)
    {
        return Err("更新清单过大".to_owned());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "无法读取更新清单".to_owned())?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("更新清单过大".to_owned());
    }
    Ok(bytes.to_vec())
}

#[tauri::command]
pub async fn check_for_desktop_update(app: AppHandle) -> Result<Option<AvailableUpdate>, String> {
    let current_version = app.package_info().version.to_string();
    let request_url = manifest_request_url(&current_version);

    // 代理意外失效时，直连重试一次，且更新检查永远不会妨碍应用启动。
    let proxied = crate::proxy::get_system_proxy().and_then(|url| build_client(Some(url)).ok());
    let body = if let Some(client) = proxied {
        match fetch_manifest(&client, &request_url, &current_version).await {
            Ok(body) => body,
            Err(proxy_error) => {
                let direct = build_client(None)?;
                fetch_manifest(&direct, &request_url, &current_version)
                    .await
                    .map_err(|_| proxy_error)?
            }
        }
    } else {
        let direct = build_client(None)?;
        fetch_manifest(&direct, &request_url, &current_version).await?
    };
    parse_available_update(&current_version, &body)
}

#[tauri::command]
pub fn open_desktop_update(download_url: String) -> Result<(), String> {
    if !is_release_download_url(&download_url) {
        return Err("更新下载地址未获信任".to_owned());
    }
    crate::launch::open_url(&download_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_only_a_newer_trusted_release() {
        let manifest = br#"{
          "version": "v0.1.2",
          "downloadUrl": "https://desktop.veriolink.com/MollyCloud_0.1.2_x64-setup.exe",
          "notes": "Startup check fixed"
        }"#;
        assert_eq!(
            parse_available_update("0.1.1", manifest).unwrap(),
            Some(AvailableUpdate {
                version: "0.1.2".to_owned(),
                download_url: "https://desktop.veriolink.com/MollyCloud_0.1.2_x64-setup.exe"
                    .to_owned(),
                notes: Some("Startup check fixed".to_owned()),
            })
        );
    }

    #[test]
    fn hides_the_current_or_an_older_release() {
        let manifest = br#"{"version":"0.1.1","downloadUrl":"https://desktop.veriolink.com/MollyCloud_0.1.1_x64-setup.exe"}"#;
        assert_eq!(parse_available_update("0.1.1", manifest).unwrap(), None);
    }

    #[test]
    fn rejects_a_download_outside_the_release_domain() {
        let manifest = br#"{"version":"0.1.2","downloadUrl":"https://example.com/MollyCloud_0.1.2_x64-setup.exe"}"#;
        assert!(parse_available_update("0.1.1", manifest).is_err());
    }
}
