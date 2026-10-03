//! Inbound-only browser imports. Never execute or persist an unconfirmed link.
use molly_ccswitch::{parse_deeplink_url, DeepLinkImportRequest};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct PendingImports(Mutex<Vec<DeepLinkImportRequest>>);

fn validate_external_link(raw: &str) -> Result<DeepLinkImportRequest, String> {
    if raw.len() > 32 * 1024 || !raw.starts_with("ccswitch://v1/import?") {
        return Err("不支持的 CC Switch 导入链接".into());
    }
    let request = parse_deeplink_url(raw).map_err(|error| error.to_string())?;
    // The upstream configUrl merger fetches arbitrary URLs before the user
    // confirms import. An external link must carry its data, not a remote URL.
    if request.config_url.is_some() {
        return Err("外部链接不支持远程配置地址，请使用内嵌配置导入。".into());
    }
    Ok(request)
}

pub fn accept(app: &AppHandle, raw: &str) -> Result<(), String> {
    let request = validate_external_link(raw)?;
    let queue = app.state::<PendingImports>();
    let mut queue = queue.0.lock().map_err(|_| "导入队列暂不可用")?;
    if queue.len() >= 8 { return Err("待确认导入过多，请先处理已有链接。".into()); }
    queue.push(request);
    drop(queue);
    let _ = app.emit("ccswitch-external-import", ());
    Ok(())
}

pub fn accept_arguments(app: &AppHandle, arguments: impl IntoIterator<Item = String>) {
    for argument in arguments {
        if let Some(url) = extract_ccswitch_url(&argument) {
            if let Err(error) = accept(app, url) {
                let _ = app.emit("ccswitch-external-import-error", error);
            }
        }
    }
}

fn extract_ccswitch_url(argument: &str) -> Option<&str> {
    let trimmed = argument.trim().trim_matches(['"', '\'']);
    let start = trimmed.find("ccswitch://")?;
    let candidate = &trimmed[start..];
    Some(candidate.split_whitespace().next().unwrap_or(candidate).trim_matches(['"', '\'']))
}

#[tauri::command]
pub fn has_ccswitch_external_import(state: tauri::State<'_, PendingImports>) -> bool {
    state.0.lock().is_ok_and(|queue| !queue.is_empty())
}

#[tauri::command]
pub fn take_ccswitch_external_imports(state: tauri::State<'_, PendingImports>) -> Result<Vec<DeepLinkImportRequest>, String> {
    let mut queue = state.0.lock().map_err(|_| "导入队列暂不可用")?;
    Ok(std::mem::take(&mut *queue))
}

#[cfg(windows)]
fn handler_command() -> Result<Option<String>, String> {
    use winreg::{enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER}, RegKey};
    let path = r"ccswitch\shell\open\command";
    let user = RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!(r"Software\Classes\{path}"));
    let key = match user { Ok(key) => Some(key), Err(_) => RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(path).ok() };
    key.map(|key| key.get_value::<String, _>("").map_err(|e| e.to_string())).transpose()
}

#[cfg(windows)]
fn own_handler_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_str().ok_or("安装路径无法注册网页导入")?;
    Ok(format!(r#""{exe}" "%1""#))
}

#[tauri::command]
pub fn ccswitch_web_import_handler() -> Result<Option<String>, String> {
    #[cfg(windows)]
    { handler_command() }
    #[cfg(not(windows))]
    { Ok(None) }
}

#[tauri::command]
pub fn register_ccswitch_web_import() -> Result<(), String> {
    register_ccswitch_web_import_inner(false)
}

fn register_ccswitch_web_import_inner(automatic: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        if let Some(existing) = handler_command()? {
            if existing == own_handler_command()? { return Ok(()); }
            return Err("ccswitch:// 已被其他应用关联。为避免覆盖独立版，请在系统默认应用中调整关联。".into());
        }
        if cfg!(debug_assertions) && !automatic { return Err("请在正式安装的 MollyCloud 中启用网页导入。".into()); }
        let classes = RegKey::predef(HKEY_CURRENT_USER);
        let (scheme, _) = classes.create_subkey(r"Software\Classes\ccswitch").map_err(|e| e.to_string())?;
        scheme.set_value("", &"URL:CC Switch Import").map_err(|e| e.to_string())?;
        scheme.set_value("URL Protocol", &"").map_err(|e| e.to_string())?;
        let (handler, _) = scheme.create_subkey(r"shell\open\command").map_err(|e| e.to_string())?;
        handler.set_value("", &own_handler_command()?).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    { Err("此版本仅支持在 Windows 注册网页导入。".into()) }
}

/// Register the protocol only when no other application already owns it.
pub fn ensure_ccswitch_web_import() {
    #[cfg(windows)]
    if !cfg!(debug_assertions) && handler_command().ok().flatten().is_none() {
        let _ = register_ccswitch_web_import_inner(true);
    }
}

#[tauri::command]
pub fn unregister_ccswitch_web_import() -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        if handler_command()?.as_deref() != Some(own_handler_command()?.as_str()) {
            return Err("当前关联不属于 MollyCloud，未作修改。".into());
        }
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(r"Software\Classes", winreg::enums::KEY_WRITE)
            .and_then(|classes| classes.delete_subkey_all("ccswitch"))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    { Err("此版本仅支持在 Windows 管理网页关联。".into()) }
}

#[cfg(test)]
mod tests {
    use super::{extract_ccswitch_url, validate_external_link};

    #[test]
    fn extracts_browser_arguments_with_quotes_or_prefixes() {
        assert_eq!(extract_ccswitch_url("ccswitch://v1/import?resource=provider"), Some("ccswitch://v1/import?resource=provider"));
        assert_eq!(extract_ccswitch_url("\"ccswitch://v1/import?resource=provider\""), Some("ccswitch://v1/import?resource=provider"));
        assert_eq!(extract_ccswitch_url("--url ccswitch://v1/import?resource=provider"), Some("ccswitch://v1/import?resource=provider"));
        assert_eq!(extract_ccswitch_url("https://example.test"), None);
    }

    #[test]
    fn external_web_import_is_parsed_but_never_applied() {
        let request = validate_external_link("ccswitch://v1/import?resource=provider&app=codex&name=Mock&endpoint=https%3A%2F%2Fexample.test%2Fv1&apiKey=mock-not-real").unwrap();
        assert_eq!(request.resource, "provider");
        assert_eq!(request.app.as_deref(), Some("codex"));
        assert_eq!(request.api_key.as_deref(), Some("mock-not-real"));
        assert!(validate_external_link("ccswitch://v1/import?resource=provider&app=codex&name=Mock&configUrl=http%3A%2F%2F127.0.0.1%2Fsecret").is_err());
        assert!(validate_external_link("ccswitch://v2/import?resource=provider").is_err());
        assert!(validate_external_link("https://example.test").is_err());
    }
}
