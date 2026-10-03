//! Inbound-only browser imports. Never execute or persist an unconfirmed link.
use molly_ccswitch::{parse_deeplink_url, DeepLinkImportRequest};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct PendingImports(Mutex<Vec<DeepLinkImportRequest>>);

fn validate_external_link(raw: &str) -> Result<DeepLinkImportRequest, String> {
    let normalized;
    let raw = if let Some(query) = raw.strip_prefix("mollycloud://ccswitch/import?") {
        normalized = format!("ccswitch://v1/import?{query}");
        normalized.as_str()
    } else {
        raw
    };
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
    #[cfg(target_os = "macos")]
    if raw.starts_with("mollycloud://") && !macos_import_enabled(app)? {
        return Err("MollyCloud 网页导入已停用，请在 CC Switch 设置中重新启用。".into());
    }
    let request = validate_external_link(raw)?;
    let queue = app.state::<PendingImports>();
    let mut queue = queue.0.lock().map_err(|_| "导入队列暂不可用")?;
    if queue.len() >= 8 {
        return Err("待确认导入过多，请先处理已有链接。".into());
    }
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
    let start = trimmed
        .find("mollycloud://ccswitch/import?")
        .or_else(|| trimmed.find("ccswitch://"))?;
    let candidate = &trimmed[start..];
    Some(
        candidate
            .split_whitespace()
            .next()
            .unwrap_or(candidate)
            .trim_matches(['"', '\'']),
    )
}

#[tauri::command]
pub fn has_ccswitch_external_import(state: tauri::State<'_, PendingImports>) -> bool {
    state.0.lock().is_ok_and(|queue| !queue.is_empty())
}

#[tauri::command]
pub fn take_ccswitch_external_imports(
    state: tauri::State<'_, PendingImports>,
) -> Result<Vec<DeepLinkImportRequest>, String> {
    let mut queue = state.0.lock().map_err(|_| "导入队列暂不可用")?;
    Ok(std::mem::take(&mut *queue))
}

#[cfg(windows)]
fn handler_command() -> Result<Option<String>, String> {
    use winreg::{
        enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER},
        RegKey,
    };
    let path = r"ccswitch\shell\open\command";
    let user = RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!(r"Software\Classes\{path}"));
    let key = match user {
        Ok(key) => Some(key),
        Err(_) => RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(path).ok(),
    };
    key.map(|key| key.get_value::<String, _>("").map_err(|e| e.to_string()))
        .transpose()
}

#[cfg(windows)]
fn own_handler_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_str().ok_or("安装路径无法注册网页导入")?;
    Ok(format!(r#""{exe}" "%1""#))
}

#[tauri::command]
pub fn ccswitch_web_import_handler(app_handle: AppHandle) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        handler_command()
    }
    #[cfg(target_os = "macos")]
    {
        if macos_import_enabled(&app_handle)? {
            macos_handler()
        } else {
            Ok(None)
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub fn register_ccswitch_web_import(app_handle: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return register_macos_import(&app_handle);
    }
    #[cfg(not(target_os = "macos"))]
    register_ccswitch_web_import_inner(false)
}

#[cfg(not(target_os = "macos"))]
fn register_ccswitch_web_import_inner(automatic: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        if let Some(existing) = handler_command()? {
            if existing == own_handler_command()? {
                return Ok(());
            }
            return Err(
                "ccswitch:// 已被其他应用关联。为避免覆盖独立版，请在系统默认应用中调整关联。"
                    .into(),
            );
        }
        if cfg!(debug_assertions) && !automatic {
            return Err("请在正式安装的 MollyCloud 中启用网页导入。".into());
        }
        let classes = RegKey::predef(HKEY_CURRENT_USER);
        let (scheme, _) = classes
            .create_subkey(r"Software\Classes\ccswitch")
            .map_err(|e| e.to_string())?;
        scheme
            .set_value("", &"URL:CC Switch Import")
            .map_err(|e| e.to_string())?;
        scheme
            .set_value("URL Protocol", &"")
            .map_err(|e| e.to_string())?;
        let (handler, _) = scheme
            .create_subkey(r"shell\open\command")
            .map_err(|e| e.to_string())?;
        handler
            .set_value("", &own_handler_command()?)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("此版本仅支持在 Windows 注册网页导入。".into())
    }
}

/// Register the protocol only when no other application already owns it.
pub fn ensure_ccswitch_web_import() {
    #[cfg(windows)]
    if !cfg!(debug_assertions) && handler_command().ok().flatten().is_none() {
        let _ = register_ccswitch_web_import_inner(true);
    }
    // macOS discovers the dedicated mollycloud:// scheme from the app bundle.
    // Never claim the independent CC Switch application's ccswitch:// scheme.
}

#[tauri::command]
pub fn unregister_ccswitch_web_import(app_handle: AppHandle) -> Result<(), String> {
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
    #[cfg(target_os = "macos")]
    {
        set_macos_import_enabled(&app_handle, false)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Err("此版本仅支持在 Windows 管理网页关联。".into())
    }
}

#[cfg(target_os = "macos")]
fn import_preference_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ccswitch-web-import.json"))
}

#[cfg(target_os = "macos")]
fn macos_import_enabled(app: &AppHandle) -> Result<bool, String> {
    let path = import_preference_path(app)?;
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "网页导入偏好已损坏，请重新启用。".into())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(target_os = "macos")]
fn set_macos_import_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use std::io::Write;
    let path = import_preference_path(app)?;
    let parent = path.parent().ok_or("网页导入偏好目录无效")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary
        .write_all(if enabled { b"true" } else { b"false" })
        .map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSCopyDefaultHandlerForURLScheme(scheme: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn LSRegisterURL(url: *const std::ffi::c_void, update: bool) -> i32;
    fn LSSetDefaultHandlerForURLScheme(
        scheme: *const std::ffi::c_void,
        bundle: *const std::ffi::c_void,
    ) -> i32;
}

#[cfg(target_os = "macos")]
fn macos_handler() -> Result<Option<String>, String> {
    use objc2::{rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;
    let scheme = NSString::from_str("mollycloud");
    // CFString and NSString are toll-free bridged. The Copy result is +1;
    // Retained releases it exactly once after copying the bundle identifier.
    let pointer = unsafe { LSCopyDefaultHandlerForURLScheme(Retained::as_ptr(&scheme).cast()) };
    let value = unsafe { Retained::<NSString>::from_raw(pointer.cast::<AnyObject>().cast()) };
    Ok(value.map(|value| value.to_string()))
}

#[cfg(target_os = "macos")]
fn register_macos_import(app: &AppHandle) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2_foundation::{NSString, NSURL};
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle_path = current_exe
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .ok_or("请在正式安装的 MollyCloud.app 中启用网页导入。")?;
    let own_id = &app.config().identifier;
    if macos_handler()?.is_some_and(|existing| existing != *own_id) {
        return Err("mollycloud:// 已由其他应用处理，MollyCloud 没有覆盖其关联。".into());
    }
    let bundle_url = NSURL::fileURLWithPath(&NSString::from_str(
        bundle_path.to_str().ok_or("应用路径无效")?,
    ));
    let scheme = NSString::from_str("mollycloud");
    let bundle = NSString::from_str(own_id);
    let result = unsafe { LSRegisterURL(Retained::as_ptr(&bundle_url).cast(), true) };
    if result != 0 {
        return Err(format!(
            "无法注册 MollyCloud 网页导入（macOS 错误 {result}）"
        ));
    }
    let result = unsafe {
        LSSetDefaultHandlerForURLScheme(
            Retained::as_ptr(&scheme).cast(),
            Retained::as_ptr(&bundle).cast(),
        )
    };
    if result != 0 {
        return Err(format!(
            "无法启用 MollyCloud 网页导入（macOS 错误 {result}）"
        ));
    }
    set_macos_import_enabled(app, true)
}

#[cfg(test)]
mod tests {
    use super::{extract_ccswitch_url, validate_external_link};

    #[test]
    fn extracts_browser_arguments_with_quotes_or_prefixes() {
        assert_eq!(
            extract_ccswitch_url("ccswitch://v1/import?resource=provider"),
            Some("ccswitch://v1/import?resource=provider")
        );
        assert_eq!(
            extract_ccswitch_url("\"ccswitch://v1/import?resource=provider\""),
            Some("ccswitch://v1/import?resource=provider")
        );
        assert_eq!(
            extract_ccswitch_url("--url ccswitch://v1/import?resource=provider"),
            Some("ccswitch://v1/import?resource=provider")
        );
        assert_eq!(extract_ccswitch_url("https://example.test"), None);
        assert_eq!(
            extract_ccswitch_url("mollycloud://ccswitch/import?resource=provider"),
            Some("mollycloud://ccswitch/import?resource=provider")
        );
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
        let molly = validate_external_link("mollycloud://ccswitch/import?resource=provider&app=codex&name=Mock&endpoint=https%3A%2F%2Fexample.test%2Fv1&apiKey=mock-not-real").unwrap();
        assert_eq!(molly.app.as_deref(), Some("codex"));
        assert_eq!(molly.api_key.as_deref(), Some("mock-not-real"));
        assert!(validate_external_link("mollycloud://ccswitch/import?resource=provider&configUrl=http%3A%2F%2F127.0.0.1%2Fsecret").is_err());
    }
}
