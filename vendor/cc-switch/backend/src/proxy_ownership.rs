//! Hashes of the last configuration bytes written by this embedded module.
//! The ledger contains no credentials and prevents automatic recovery over an
//! externally edited configuration, even if its proxy marker is still present.
use crate::{AppError, AppType};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

static LEDGER_LOCK: Mutex<()> = Mutex::new(());

fn paths_for(app: &AppType) -> Vec<PathBuf> {
    match app {
        AppType::Claude => vec![crate::config::get_claude_settings_path()],
        AppType::Codex => vec![
            crate::codex_config::get_codex_config_path(),
            crate::codex_config::get_codex_auth_path(),
            crate::codex_config::get_codex_model_catalog_path(),
        ],
        // Backup restore writes .env; fallback to the current provider can also
        // update settings.json, so both must remain unchanged.
        AppType::Gemini => vec![
            crate::gemini_config::get_gemini_env_path(),
            crate::gemini_config::get_gemini_settings_path(),
        ],
        AppType::GrokBuild => vec![crate::grok_config::get_grok_config_path()],
        _ => Vec::new(),
    }
}

fn path_key(path: &Path) -> Result<String, AppError> {
    crate::embedded::require_config_path(path)?;
    // A backup for another directory must never authorize this target's restore.
    let mut key = path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    key.make_ascii_lowercase();
    Ok(key)
}

fn is_managed_file(path: &Path) -> bool {
    [
        AppType::Claude,
        AppType::Codex,
        AppType::Gemini,
        AppType::GrokBuild,
    ]
    .into_iter()
    .any(|app| paths_for(&app).iter().any(|candidate| candidate == path))
}

fn file_state(path: &Path) -> Result<String, AppError> {
    crate::embedded::require_config_path(path)?;
    match std::fs::read(path) {
        Ok(bytes) => Ok(format!("{:x}", Sha256::digest(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("absent".into()),
        Err(error) => Err(AppError::io(path, error)),
    }
}

fn ledger_path() -> PathBuf {
    crate::config::get_app_config_dir().join("molly-managed-writes.json")
}

fn load_ledger() -> Result<BTreeMap<String, String>, AppError> {
    let path = ledger_path();
    crate::embedded::require_private_path(&path)?;
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Config("内置代理归属记录损坏，已停止自动恢复。".into())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(AppError::io(path, e)),
    }
}

/// Refuse to adopt or overwrite the standalone manager's active proxy.
pub(crate) fn require_no_external_proxy(app: &AppType) -> Result<(), AppError> {
    for path in paths_for(app) {
        require_no_external_proxy_file(&path)?;
    }
    Ok(())
}

pub(crate) fn backup_original_for_app(app: &AppType) -> Result<(), AppError> {
    for path in paths_for(app) {
        backup_original_file(&path)?;
    }
    Ok(())
}

pub(crate) fn require_no_external_proxy_file(path: &Path) -> Result<(), AppError> {
    if !is_managed_file(path) {
        return Ok(());
    }
    crate::embedded::require_config_path(path)?;
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            if text
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|token| token == "PROXY_MANAGED")
            {
                return Err(AppError::Config("此工具正由独立版 CC Switch 代理接管。请先在独立版关闭该工具的代理，再在 MollyCloud 启用供应商。".into()));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::io(path, e)),
    }
}

/// Save the exact original bytes once per target, before Molly first changes it.
pub(crate) fn backup_original_file(path: &Path) -> Result<(), AppError> {
    if !is_managed_file(path) {
        return Ok(());
    }
    require_no_external_proxy_file(path)?;
    let key = path_key(path)?;
    let backup = crate::config::get_app_config_dir()
        .join("system-config-originals")
        .join(format!("{:x}.json", Sha256::digest(key.as_bytes())));
    crate::embedded::require_private_path(&backup)?;
    let _lock = LEDGER_LOCK
        .lock()
        .map_err(|_| AppError::Config("配置备份锁不可用。".into()))?;
    if backup.exists() {
        return Ok(());
    }
    let original = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(AppError::io(path, e)),
    };
    let snapshot = serde_json::to_vec(&serde_json::json!({"path":key,"bytes":original}))
        .map_err(|source| AppError::JsonSerialize { source })?;
    crate::config::atomic_write_private(&backup, &snapshot)
}

pub(crate) fn record_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if !is_managed_file(path) {
        return Ok(());
    }
    update_ledger([(path_key(path)?, format!("{:x}", Sha256::digest(bytes)))])
}

pub(crate) fn record_removal(path: &Path) -> Result<(), AppError> {
    if !is_managed_file(path) {
        return Ok(());
    }
    update_ledger([(path_key(path)?, "absent".into())])
}

/// Capture files preserved by takeover too, such as an existing native login.
/// Only initial backup calls this; a later config-only write cannot adopt an
/// external auth edit as owned and thereby authorize overwriting it on restore.
pub(crate) fn record_takeover_baseline(app: &AppType) -> Result<(), AppError> {
    let states = paths_for(app)
        .into_iter()
        .map(|path| Ok((path_key(&path)?, file_state(&path)?)))
        .collect::<Result<Vec<_>, AppError>>()?;
    update_ledger(states)
}

fn update_ledger(updates: impl IntoIterator<Item = (String, String)>) -> Result<(), AppError> {
    let _lock = LEDGER_LOCK
        .lock()
        .map_err(|_| AppError::Config("内置代理归属记录不可用。".into()))?;
    let mut ledger = load_ledger()?;
    ledger.extend(updates);
    let encoded = serde_json::to_vec(&ledger)
        .map_err(|_| AppError::Config("无法保存内置代理归属记录。".into()))?;
    // This path is not a managed CLI file, so atomic_write does not recurse.
    crate::config::atomic_write(&ledger_path(), &encoded)
}

pub(crate) fn require_unchanged(app: &AppType) -> Result<(), String> {
    let paths = paths_for(app);
    if paths.is_empty() {
        return Err("此工具不支持内置代理恢复。".into());
    }
    let _lock = LEDGER_LOCK.lock().map_err(|_| "内置代理归属记录不可用。")?;
    let ledger = load_ledger().map_err(|e| e.to_string())?;
    for path in paths {
        let key = path_key(&path).map_err(|e| e.to_string())?;
        let current = file_state(&path).map_err(|e| e.to_string())?;
        let expected = ledger.get(&key).map(String::as_str).unwrap_or("absent");
        if expected != current {
            return Err(format!(
                "{} 配置已由其他程序修改，已保留现有内容；请检查后重新选择供应商。",
                app.as_str()
            ));
        }
    }
    Ok(())
}
