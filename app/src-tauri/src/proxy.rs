//! updater 代理发现：只读，不修改系统代理，不记录凭据。
//!
//! 优先级（从高到低）：
//!   1. 环境变量 HTTPS_PROXY / HTTP_PROXY / ALL_PROXY（进程级，TUN/手动模式）
//!   2. Windows WinINET 系统代理（Clash Verge / V2rayN 等 GUI 代理）
//!   3. 无代理（返回 None，直连）
//!
//! 返回 updater（reqwest::Proxy::all）可接受的完整 URL（默认补 http://）。
//! 检测到 PAC（AutoConfigURL）时不实现解释器，返回 None 走安全 fallback。

use std::env;

/// 读取 updater 可用代理 URL（http://host:port）。失败/无代理返回 None。
pub fn get_system_proxy() -> Option<String> {
    // 1) 进程环境变量（最高优先级，用户或 TUN 模式设置）
    for key in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
        if let Some(v) = env::var(key)
            .or_else(|_| env::var(key.to_lowercase()))
            .ok()
            .and_then(|s| normalize_proxy_url(&s))
        {
            return Some(v);
        }
    }
    #[cfg(windows)]
    {
        wininet_proxy()
    }
    #[cfg(target_os = "macos")]
    {
        macos_proxy()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

/// 规范成 updater 可接受 URL：reqwest::Proxy::all 要求带 scheme。
fn normalize_proxy_url(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("socks5") {
        Some(t.to_string())
    } else {
        Some(format!("http://{t}"))
    }
}

/// 读 WinINET registry：ProxyEnable + ProxyServer（含 per-protocol 格式）。
#[cfg(windows)]
fn wininet_proxy() -> Option<String> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        REG_VALUE_TYPE,
    };

    const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let key_wide: Vec<u16> = SUBKEY.encode_utf16().chain(Some(0)).collect();
    let mut hkey: HKEY = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(key_wide.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    // 保证关闭句柄
    struct CloseOnDrop(HKEY);
    impl Drop for CloseOnDrop {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
    let _guard = CloseOnDrop(hkey);

    fn query_value(hkey: HKEY, name: &str) -> Option<Vec<u8>> {
        let name_wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut typ: REG_VALUE_TYPE = REG_VALUE_TYPE(0);
        let mut size: u32 = 0;
        let status = unsafe {
            RegQueryValueExW(
                hkey,
                windows::core::PCWSTR(name_wide.as_ptr()),
                None,
                Some(&mut typ as *mut _),
                None,
                Some(&mut size),
            )
        };
        if status == ERROR_SUCCESS && size > 0 {
            let mut buf = vec![0u8; size as usize];
            let status2 = unsafe {
                RegQueryValueExW(
                    hkey,
                    windows::core::PCWSTR(name_wide.as_ptr()),
                    None,
                    Some(&mut typ as *mut _),
                    Some(buf.as_mut_ptr() as *mut u8),
                    Some(&mut size),
                )
            };
            if status2 == ERROR_SUCCESS {
                buf.truncate(size as usize);
                return Some(buf);
            }
        }
        None
    }

    // ProxyEnable
    let enable_raw = query_value(hkey, "ProxyEnable");
    let enable = enable_raw.and_then(|b| {
        if b.len() >= 4 {
            Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        } else {
            None
        }
    });
    if enable != Some(1) {
        return None;
    }

    // PAC 不能直接交给 reqwest 解析，但不少客户端会同时写入 PAC 和
    // ProxyServer。此前只要检测到 PAC 就直接走直连，导致 GitHub 更新在
    // Clash/V2RayN 等 PAC 模式下必然失败。优先使用可用的静态代理；仅 PAC
    // 且没有 ProxyServer 时才退回系统直连。
    let server = query_value(hkey, "ProxyServer")
        .and_then(|raw| decode_registry_string(&raw))
        .unwrap_or_default();
    if !server.is_empty() {
        return parse_proxy_server(&server);
    }
    None
}

/// scutil reads the active SystemConfiguration proxy dictionary, including the
/// network service currently in use. PAC-only configurations remain direct;
/// a PAC script is never executed inside the updater.
#[cfg(target_os = "macos")]
fn macos_proxy() -> Option<String> {
    let output = std::process::Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_macos_proxy_dictionary(&String::from_utf8(output.stdout).ok()?)
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_proxy_dictionary(dictionary: &str) -> Option<String> {
    let mut values = std::collections::HashMap::new();
    let mut depth = 0usize;
    for line in dictionary.lines() {
        let line = line.trim();
        if line.ends_with('{') {
            depth += 1;
            continue;
        }
        if line == "}" {
            depth = depth.saturating_sub(1);
            continue;
        }
        // Interface-specific inactive proxies must not replace the active
        // top-level service. Ignore Scoped/Supplemental dictionaries.
        if depth > 1 {
            continue;
        }
        if let Some((key, value)) = line.split_once(" : ") {
            values.insert(key.trim(), value.trim());
        }
    }
    for (prefix, scheme) in [("HTTPS", "http"), ("HTTP", "http"), ("SOCKS", "socks5h")] {
        if values.get(format!("{prefix}Enable").as_str()).copied() != Some("1") {
            continue;
        }
        let Some(host) = values
            .get(format!("{prefix}Proxy").as_str())
            .copied()
            .filter(|host| !host.is_empty())
        else {
            continue;
        };
        let Some(port) = values
            .get(format!("{prefix}Port").as_str())
            .and_then(|port| port.parse::<u16>().ok())
            .filter(|port| *port > 0)
        else {
            continue;
        };
        if host.chars().any(char::is_whitespace) || host.contains(['/', '@', '?', '#']) {
            continue;
        }
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        let candidate = format!("{scheme}://{host}:{port}");
        if url::Url::parse(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

// RegQueryValueExW returns UTF-16LE, including a terminating wide NUL.
// Decoding those bytes as UTF-8 inserts NULs between every ASCII character.
#[cfg(any(windows, test))]
fn decode_registry_string(raw: &[u8]) -> Option<String> {
    if raw.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16(&units)
        .ok()
        .map(|value| value.trim().to_owned())
}

/// 解析 ProxyServer：`host:port` 或 `http=host:port;https=host:port`。
#[cfg(any(windows, test))]
fn parse_proxy_server(server: &str) -> Option<String> {
    if server.contains('=') {
        // per-protocol 格式：优先 https=，否则 http=
        let mut https: Option<&str> = None;
        let mut http: Option<&str> = None;
        for part in server.split(';') {
            let p = part.trim();
            if let Some(v) = p.strip_prefix("https=") {
                https = Some(v.trim());
            } else if let Some(v) = p.strip_prefix("http=") {
                http = Some(v.trim());
            }
        }
        let picked = https.or(http)?;
        normalize_proxy_url(picked)
    } else {
        normalize_proxy_url(server)
    }
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_proxy_string_is_decoded_as_utf16() {
        for input in ["127.0.0.1:7892", "https=127.0.0.1:7892;http=127.0.0.1:7892"] {
            let raw: Vec<u8> = input
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            let decoded = decode_registry_string(&raw).unwrap();
            assert_eq!(decoded, input);
            assert!(reqwest::Proxy::all(parse_proxy_server(&decoded).unwrap()).is_ok());
        }
        assert!(decode_registry_string(&[1]).is_none());
    }

    #[test]
    fn normalize_adds_scheme() {
        assert_eq!(
            normalize_proxy_url("127.0.0.1:7897").as_deref(),
            Some("http://127.0.0.1:7897")
        );
        assert_eq!(
            normalize_proxy_url(" http://x:1 ").as_deref(),
            Some("http://x:1")
        );
        assert_eq!(normalize_proxy_url("  "), None);
    }

    #[test]
    fn parse_proxy_server_simple() {
        assert_eq!(
            parse_proxy_server("127.0.0.1:7897").as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }

    #[test]
    fn parse_proxy_server_per_protocol() {
        assert_eq!(
            parse_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891").as_deref(),
            Some("http://127.0.0.1:7891")
        );
        assert_eq!(
            parse_proxy_server("http=127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn macos_proxy_uses_active_enabled_static_proxy() {
        assert_eq!(parse_macos_proxy_dictionary("<dictionary> {\nHTTPEnable : 1\nHTTPProxy : active.example\nHTTPPort : 7890\nScoped : <dictionary> {\nen1 : <dictionary> {\nHTTPEnable : 1\nHTTPProxy : inactive.example\nHTTPPort : 8888\n}\n}\n}"), Some("http://active.example:7890".into()));
        assert_eq!(parse_macos_proxy_dictionary("<dictionary> {\n HTTPEnable : 1\n HTTPProxy : 127.0.0.1\n HTTPPort : 7890\n HTTPSEnable : 1\n HTTPSProxy : ::1\n HTTPSPort : 7891\n}"), Some("http://[::1]:7891".into()));
        assert_eq!(
            parse_macos_proxy_dictionary(
                "SOCKSEnable : 1\nSOCKSProxy : 127.0.0.1\nSOCKSPort : 1080"
            ),
            Some("socks5h://127.0.0.1:1080".into())
        );
        assert_eq!(parse_macos_proxy_dictionary("ProxyAutoConfigEnable : 1\nProxyAutoConfigURLString : https://example.test/proxy.pac"), None);
        assert_eq!(
            parse_macos_proxy_dictionary("HTTPEnable : 0\nHTTPProxy : 127.0.0.1\nHTTPPort : 7890"),
            None
        );
        assert_eq!(
            parse_macos_proxy_dictionary(
                "HTTPEnable : 1\nHTTPProxy : example.test\nHTTPPort : 999999"
            ),
            None
        );
    }
}
