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

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const RELEASE_MANIFEST_URL: &str = "https://desktop.veriolink.com/latest-macos-aarch64.json";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
pub const RELEASE_MANIFEST_URL: &str = "https://desktop.veriolink.com/latest-macos-x86_64.json";
#[cfg(not(target_os = "macos"))]
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
    sha256: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub current_version: String,
    pub download_url: String,
    pub notes: Option<String>,
    pub sha256: String,
    pub size_bytes: u64,
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
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(RELEASE_HOST))
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && release_url_version(url.path()).is_some()
}

fn release_url_version(path: &str) -> Option<Version> {
    release_url_version_for_platform(path, std::env::consts::OS, std::env::consts::ARCH)
}

fn release_url_version_for_platform(
    path: &str,
    platform: &str,
    architecture: &str,
) -> Option<Version> {
    let name = path.strip_prefix("/MollyCloud_")?;
    let version = match (platform, architecture) {
        ("windows", "x86_64") => name.strip_suffix("_x64-setup.exe")?,
        ("macos", "aarch64") => name
            .strip_suffix("_aarch64.dmg")
            .or_else(|| name.strip_suffix("_universal.dmg"))?,
        ("macos", "x86_64") => name
            .strip_suffix("_x64.dmg")
            .or_else(|| name.strip_suffix("_universal.dmg"))?,
        _ => return None,
    };
    parse_version(version).ok()
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
    if !is_release_download_url(&manifest.download_url)
        || Url::parse(&manifest.download_url).map_or(true, |url| {
            release_url_version(url.path()).as_ref() != Some(&latest)
        })
    {
        return Err("更新清单中的下载地址未获信任".to_owned());
    }
    let sha256 = manifest.sha256.ok_or("更新清单中的安装包校验信息无效")?;
    let size_bytes = manifest
        .size_bytes
        .ok_or("更新清单中的安装包校验信息无效")?;
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) || size_bytes == 0
    {
        return Err("更新清单中的安装包校验信息无效".to_owned());
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
        current_version: current.to_string(),
        download_url: manifest.download_url,
        notes,
        sha256: sha256.to_ascii_lowercase(),
        size_bytes,
    }))
}

fn manifest_request_url(current_version: &str) -> String {
    // 清单很小且发布时设置为 no-store；再按分钟变化查询串，避免中间缓存延迟首启检查。
    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 60;
    format!("{RELEASE_MANIFEST_URL}?current_version={current_version}&platform={}&arch={}&checked_at={checked_at}", std::env::consts::OS, std::env::consts::ARCH)
}

fn build_client(proxy_url: Option<String>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none());
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

    fn download_url(version: &str) -> String {
        let suffix = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64.dmg",
            ("macos", _) => "x64.dmg",
            _ => "x64-setup.exe",
        };
        format!("https://desktop.veriolink.com/MollyCloud_{version}_{suffix}")
    }

    fn manifest(version: &str, url: String) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "version": version,
            "downloadUrl": url,
            "notes": "Startup check fixed",
            "sha256": "a".repeat(64),
            "sizeBytes": 12345,
        }))
        .unwrap()
    }

    #[test]
    fn returns_only_a_newer_trusted_release() {
        let url = download_url("0.1.2");
        let manifest = manifest("v0.1.2", url.clone());
        assert_eq!(
            parse_available_update("0.1.1", &manifest).unwrap(),
            Some(AvailableUpdate {
                version: "0.1.2".to_owned(),
                current_version: "0.1.1".to_owned(),
                download_url: url,
                notes: Some("Startup check fixed".to_owned()),
                sha256: "a".repeat(64),
                size_bytes: 12345,
            })
        );
    }

    #[test]
    fn hides_the_current_or_an_older_release() {
        let manifest = manifest("0.1.1", download_url("0.1.1"));
        assert_eq!(parse_available_update("0.1.1", &manifest).unwrap(), None);
    }

    #[test]
    fn rejects_a_download_outside_the_release_domain() {
        let manifest = manifest(
            "0.1.2",
            download_url("0.1.2").replace(RELEASE_HOST, "example.com"),
        );
        assert!(parse_available_update("0.1.1", &manifest).is_err());
    }

    #[test]
    fn rejects_missing_checksum_or_mismatched_versioned_file() {
        let absent = serde_json::to_vec(
            &serde_json::json!({"version":"0.1.2","downloadUrl":download_url("0.1.2")}),
        )
        .unwrap();
        assert!(parse_available_update("0.1.1", &absent).is_err());
        let mismatched = manifest("0.1.2", download_url("0.1.3"));
        assert!(parse_available_update("0.1.1", &mismatched).is_err());
        assert!(!is_release_download_url(&format!(
            "{}?src=other",
            download_url("0.1.2")
        )));
        assert!(!is_release_download_url(&download_url("fake")));
    }

    #[test]
    fn release_files_match_the_platform_and_architecture() {
        for (path, platform, architecture) in [
            ("/MollyCloud_0.2.1_aarch64.dmg", "macos", "aarch64"),
            ("/MollyCloud_0.2.1_x64.dmg", "macos", "x86_64"),
            ("/MollyCloud_0.2.1_universal.dmg", "macos", "aarch64"),
            ("/MollyCloud_0.2.1_universal.dmg", "macos", "x86_64"),
            ("/MollyCloud_0.2.1_x64-setup.exe", "windows", "x86_64"),
        ] {
            assert_eq!(
                release_url_version_for_platform(path, platform, architecture),
                Some(Version::new(0, 2, 1))
            );
        }
        for (path, platform, architecture) in [
            ("/MollyCloud_0.2.1_x64-setup.exe", "macos", "aarch64"),
            ("/MollyCloud_0.2.1_x64.dmg", "macos", "aarch64"),
            ("/MollyCloud_0.2.1_aarch64.dmg", "macos", "x86_64"),
            ("/MollyCloud_0.2.1_universal.dmg", "windows", "x86_64"),
        ] {
            assert!(release_url_version_for_platform(path, platform, architecture).is_none());
        }
    }
}
