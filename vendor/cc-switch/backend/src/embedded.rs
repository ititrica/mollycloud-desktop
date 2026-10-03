//! MollyCloud host boundary. The upstream application launcher is deliberately absent.
use crate::{AppState, Database, Provider};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::{Component, Path, PathBuf},
    sync::{Arc, OnceLock},
};
use tauri::{Emitter, Manager};

static PRIVATE_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[cfg(any(test, feature = "test-hooks"))]
static TEST_HOME: OnceLock<PathBuf> = OnceLock::new();

#[cfg(any(test, feature = "test-hooks"))]
pub(crate) fn test_home() -> Option<PathBuf> {
    TEST_HOME
        .get()
        .cloned()
        .or_else(|| std::env::var_os("CC_SWITCH_TEST_HOME").map(PathBuf::from))
}

/// Tool files may live outside Molly. The standalone manager's database is
/// never a tool configuration target, including through directory aliases.
pub(crate) fn require_config_path(path: &Path) -> Result<(), crate::AppError> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(crate::AppError::Config(
            "配置路径必须为不包含上级跳转的绝对路径。".into(),
        ));
    }
    if let Some(root) = PRIVATE_ROOT.get() {
        if crate::config::path_is_within(root, path) {
            return require_private_path(path);
        }
    }
    let external_data = crate::config::get_home_dir().join(".cc-switch");
    let mut existing = path;
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| crate::AppError::Config("配置路径无效。".into()))?;
    }
    let resolved = dunce::canonicalize(existing).map_err(|e| crate::AppError::io(existing, e))?;
    let external_resolved = dunce::canonicalize(&external_data).unwrap_or(external_data.clone());
    if crate::config::path_is_within(&external_data, path)
        || crate::config::path_is_within(&external_resolved, &resolved)
    {
        return Err(crate::AppError::Config(
            "请使用工具的配置目录，不能覆盖独立版 CC Switch 的数据目录。".into(),
        ));
    }
    Ok(())
}

pub(crate) fn private_root() -> &'static Path {
    PRIVATE_ROOT
        .get()
        .expect("embedded CC Switch must be initialized by MollyCloud")
        .as_path()
}

/// Validate existing ancestors too, so junctions and symbolic links cannot escape ownership.
pub(crate) fn require_private_path(path: &Path) -> Result<(), crate::AppError> {
    let root = PRIVATE_ROOT
        .get()
        .ok_or_else(|| crate::AppError::Config("内置 CC Switch 尚未初始化。".into()))?;
    require_contained_path(root, path)
}

fn require_contained_path(root: &Path, path: &Path) -> Result<(), crate::AppError> {
    let error =
        || crate::AppError::Config("内置 CC Switch 仅允许使用 MollyCloud 私有配置目录。".into());
    if !path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
        || !path.starts_with(root)
    {
        return Err(error());
    }
    let mut existing = path;
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => {
                let resolved = dunce::canonicalize(existing).map_err(|_| error())?;
                if !resolved.starts_with(root) {
                    return Err(error());
                }
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                existing = existing.parent().ok_or_else(error)?;
            }
            Err(_) => return Err(error()),
        }
    }
    Ok(())
}

fn reject_reparse_point(path: &Path, metadata: &std::fs::Metadata) -> Result<(), crate::AppError> {
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = metadata.file_type().is_symlink();
    if reparse {
        return Err(crate::AppError::Config(format!(
            "内置配置目录不能包含链接或重解析点：{}",
            path.display()
        )));
    }
    Ok(())
}

/// Resolve the actual storage location before defining the containment boundary.
/// MSIX can virtualize an AppData child even when its existing parent resolves to
/// the normal Roaming directory. This is not a junction or symbolic link.
fn resolve_private_directory(path: &Path) -> Result<PathBuf, crate::AppError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => reject_reparse_point(path, &metadata)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(crate::AppError::io(path, error)),
    }
    std::fs::create_dir_all(path).map_err(|error| crate::AppError::io(path, error))?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| crate::AppError::io(path, error))?;
    // Check the requested entry before canonicalization can erase link evidence.
    reject_reparse_point(path, &metadata)?;
    dunce::canonicalize(path).map_err(|error| crate::AppError::io(path, error))
}

fn initialize_paths(app_data: &Path) -> Result<(), crate::AppError> {
    std::fs::create_dir_all(app_data).map_err(|error| crate::AppError::io(app_data, error))?;
    let owner =
        dunce::canonicalize(app_data).map_err(|error| crate::AppError::io(app_data, error))?;
    // The host supplies AppData; the only child we create is the fixed ccswitch
    // directory. All subsequent paths must remain under its resolved location.
    let root = resolve_private_directory(&owner.join("ccswitch"))?;
    if let Some(previous) = PRIVATE_ROOT.get() {
        if *previous != root {
            return Err(crate::AppError::Config(
                "内置 CC Switch 私有数据目录不能在运行中更改。".into(),
            ));
        }
    }
    for child in ["home", "data", "cache", "logs"] {
        let path = root.join(child);
        require_contained_path(&root, &path)?;
        resolve_private_directory(&path)?;
        require_contained_path(&root, &path)?;
    }
    validate_private_tree(&root)?;
    // Failed initialization must not publish a partially prepared private root.
    if PRIVATE_ROOT.get().is_none() {
        PRIVATE_ROOT.set(root).map_err(|_| {
            crate::AppError::Config("内置 CC Switch 私有数据目录初始化冲突。".into())
        })?;
    }
    Ok(())
}

/// Scope is explicit and fail-closed. New upstream commands require a fresh audit.
pub(crate) use crate::allowed_commands::ALLOWED_COMMANDS;

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    build_plugin(None)
}

#[cfg(feature = "test-hooks")]
pub fn init_for_test(
    app_data_root: PathBuf,
    system_home: PathBuf,
) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    assert!(system_home.is_absolute());
    TEST_HOME
        .set(system_home)
        .expect("test home may only be initialized once");
    build_plugin(Some(app_data_root))
}

pub fn init_guarded(gate: impl Fn() -> bool + Send + Sync + 'static) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    build_guarded(None, Arc::new(gate))
}
fn build_plugin(app_data_override: Option<PathBuf>) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    build_guarded(app_data_override, Arc::new(|| true))
}
fn build_guarded(app_data_override: Option<PathBuf>, gate: Arc<dyn Fn() -> bool + Send + Sync>) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let setup_gate=gate.clone();
    let upstream = crate::upstream_handler();
    tauri::plugin::Builder::new("molly-ccswitch")
        .setup(move |app, _| {
            if !setup_gate() { return Ok(()); }
            let initialization = (|| -> Result<(), Box<dyn std::error::Error>> {
            let app_data = match &app_data_override { Some(path) => path.clone(), None => app.path().app_data_dir()? };
            initialize_paths(&app_data)?;
            validate_private_tree(private_root())?;
            crate::settings::reload_settings()?;
            validate_settings(&crate::settings::get_settings())?;
            let state = AppState::new(Arc::new(Database::init()?));
            migrate_system_targets(&state.db)?;
            state.proxy_service.set_app_handle(app.clone());
            crate::usage_events::init(app.clone());
            crate::services::webdav_auto_sync::start_worker(state.db.clone(), app.clone());
            crate::services::s3_auto_sync::start_worker(state.db.clone(), app.clone());
            if state.proxy_service.detect_takeover_in_live_configs() {
                if let Err(error) = tauri::async_runtime::block_on(state.proxy_service.stop_with_restore()) {
                    log::warn!("内置代理私有配置恢复停止: {error}");
                    crate::init_status::set_init_error(crate::init_status::InitErrorPayload {
                        path: crate::config::get_app_config_dir().display().to_string(), error,
                        kind: Some("private_proxy_recovery".into()), db_version: None, supported_version: None,
                    });
                }
            }
            app.manage(crate::commands::CodexOAuthState(state.codex_oauth_manager.clone()));
            app.manage(crate::commands::CopilotAuthState(Arc::new(tokio::sync::RwLock::new(
                crate::proxy::providers::copilot_auth::CopilotAuthManager::new(crate::config::get_app_config_dir())
            ))));
            app.manage(crate::commands::XaiOAuthState(Arc::new(tokio::sync::RwLock::new(
                crate::proxy::providers::xai_oauth_auth::XaiOAuthManager::new(crate::config::get_app_config_dir())
            ))));
            app.manage(state);
            // No external-manager import, independent tray, updater, protocol or
            // single-instance hooks. User-enabled sync concerns Molly's database.
            Ok(())
            })();
            if let Err(error) = initialization {
                log::warn!("内置 CC Switch 初始化失败，MollyCloud 继续运行: {error}");
                crate::init_status::set_init_error(crate::init_status::InitErrorPayload {
                    path: PRIVATE_ROOT.get().map(|path| path.display().to_string())
                        .unwrap_or_else(|| "MollyCloud 私有数据目录".into()),
                    error: format!("内置 CC Switch 暂不可用：{error}"),
                    kind: Some("private_module_init".into()), db_version: None, supported_version: None,
                });
            }
            // A damaged optional module must not prevent the pet or Molly console from starting.
            Ok(())
        })
        .invoke_handler(move |invoke| {
            if !gate() { invoke.resolver.reject("插件未安装或需要重启后启用"); return true; }
            if invoke.message.webview().window().label() != "console" {
                invoke.resolver.reject("内置 CC Switch 仅供 MollyCloud 控制台访问");
                return true;
            }
            if !ALLOWED_COMMANDS.contains(&invoke.message.command()) {
                invoke.resolver.reject("此应用生命周期或数据目录操作由 MollyCloud 管理。请使用控制台设置。");
                return true;
            }
            if invoke.message.command() == "get_init_error" {
                return upstream(invoke);
            }
            if invoke.message.webview().try_state::<AppState>().is_none() {
                invoke.resolver.reject("内置 CC Switch 初始化未完成，请查看模块错误信息；MollyCloud 的其他功能可继续使用。");
                return true;
            }
            if let Err(error) = validate_settings(&crate::settings::get_settings()) {
                invoke.resolver.reject(error.to_string());
                return true;
            }
            upstream(invoke)
        })
        .on_event(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Err(error) = validate_settings(&crate::settings::get_settings()) {
                        log::warn!("内置代理退出时保留配置: {error}");
                        return;
                    }
                    let result = tauri::async_runtime::block_on(async {
                        let cleanup = async {
                            if state.proxy_service.detect_takeover_in_live_configs() {
                                state.proxy_service.stop_with_restore().await
                            } else if state.proxy_service.is_running().await {
                                state.proxy_service.stop().await
                            } else { Ok(()) }
                        };
                        tokio::time::timeout(std::time::Duration::from_secs(6), cleanup)
                            .await.unwrap_or_else(|_| Err("内置代理退出清理超时；私有恢复记录将在下次启动重试。".into()))
                    });
                    if let Err(error) = result { log::warn!("内置代理退出恢复停止: {error}"); }
                }
            }
        })
        .build()
}

pub(crate) fn validate_settings(
    settings: &crate::settings::AppSettings,
) -> Result<(), crate::AppError> {
    let root = PRIVATE_ROOT
        .get()
        .ok_or_else(|| crate::AppError::Config("内置 CC Switch 尚未初始化。".into()))?;
    validate_private_tree(root)?;
    for override_path in [
        &settings.claude_config_dir,
        &settings.codex_config_dir,
        &settings.gemini_config_dir,
        &settings.grok_config_dir,
        &settings.opencode_config_dir,
        &settings.openclaw_config_dir,
        &settings.hermes_config_dir,
        &settings.pi_config_dir,
    ] {
        if let Some(path) = override_path {
            let path = crate::settings::resolve_override_path(path);
            require_config_path(&path)?;
        }
    }
    // Known default targets must also be checked when no override exists.
    for path in [
        crate::config::get_home_dir(),
        crate::config::get_app_config_dir(),
        crate::config::get_claude_config_dir(),
        crate::config::get_claude_mcp_path(),
        crate::codex_config::get_codex_config_dir(),
        crate::gemini_config::get_gemini_dir(),
        crate::grok_config::get_grok_config_dir(),
        crate::opencode_config::get_opencode_dir(),
        crate::openclaw_config::get_openclaw_dir(),
        crate::hermes_config::get_hermes_dir(),
        crate::mcode_config::data_dir(),
        crate::mcode_config::config_path(),
    ] {
        require_config_path(&path)?;
    }
    require_config_path(&crate::pi_config::get_pi_agent_dir()?)?;
    Ok(())
}

/// A private-profile selection is not an activation in the user's real CLI.
/// Preserve the old manager database before resetting only its target bindings.
fn migrate_system_targets(db: &Database) -> Result<(), crate::AppError> {
    const MARKER: &str = "molly_system_targets_v1";
    if db.get_setting(MARKER)?.as_deref() == Some("true") {
        return Ok(());
    }
    let original = crate::settings::get_settings();
    crate::config::write_json_file(
        &private_root().join("data/private-target-settings-backup.json"),
        &original,
    )?;
    db.backup_database_file()?;
    let mut settings = original;
    for path in [
        &mut settings.claude_config_dir,
        &mut settings.codex_config_dir,
        &mut settings.gemini_config_dir,
        &mut settings.grok_config_dir,
        &mut settings.opencode_config_dir,
        &mut settings.openclaw_config_dir,
        &mut settings.hermes_config_dir,
        &mut settings.pi_config_dir,
    ] {
        if path.as_ref().is_some_and(|value| {
            crate::config::path_is_within(
                &private_root().join("home"),
                &crate::settings::resolve_override_path(value),
            )
        }) {
            *path = None;
        }
    }
    settings.current_provider_claude = None;
    settings.current_provider_claude_desktop = None;
    settings.current_provider_codex = None;
    settings.current_provider_gemini = None;
    settings.current_provider_grokbuild = None;
    settings.current_provider_opencode = None;
    settings.current_provider_openclaw = None;
    settings.current_provider_hermes = None;
    crate::settings::update_settings(settings)?;
    for app in [
        "claude",
        "claude-desktop",
        "codex",
        "gemini",
        "grokbuild",
        "opencode",
        "openclaw",
        "hermes",
        "pi",
    ] {
        db.set_current_provider(app, "")?;
    }
    tauri::async_runtime::block_on(async {
        for app in ["claude", "codex", "gemini", "grokbuild"] {
            db.delete_live_backup(app).await?;
            let mut config = db.get_proxy_config_for_app(app).await?;
            config.enabled = false;
            db.update_proxy_config_for_app(config).await?;
        }
        db.set_live_takeover_active(false).await
    })?;
    db.set_setting(MARKER, "true")
}

/// Inspect managed configuration, not CLI project workspaces or session histories.
/// File operations still independently validate their exact target before access.
fn is_managed_tree_entry(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let names: Vec<_> = relative
        .components()
        .filter_map(|part| {
            if let Component::Normal(name) = part {
                Some(name.to_string_lossy().to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect();
    let parts: Vec<_> = names.iter().map(String::as_str).collect();
    match parts.as_slice() {
        [] => true,
        ["data" | "cache" | "logs", ..] => true,
        ["home"] => true,
        ["home", ".claude", "projects" | "todos" | "tasks" | "debug" | "session-env", ..] => false,
        ["home", ".codex", "sessions" | "archived_sessions" | "log", ..] => false,
        ["home", ".gemini", "tmp" | "history", ..] => false,
        ["home", ".cc-switch" | ".claude" | ".codex" | ".gemini" | ".grok" | ".config" | ".openclaw"
        | ".hermes" | ".pi" | ".minimax" | "appdata", ..] => true,
        _ => false,
    }
}

/// Refuse reparse points in managed configuration and backup directories.
fn validate_private_tree(root: &Path) -> Result<(), crate::AppError> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(crate::AppError::io(&path, e)),
        };
        reject_reparse_point(&path, &metadata)?;
        if metadata.is_dir() {
            let entries = match std::fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(crate::AppError::io(&path, e)),
            };
            for entry in entries {
                let entry = entry.map_err(|e| crate::AppError::io(&path, e))?;
                if is_managed_tree_entry(root, &entry.path()) {
                    pending.push(entry.path());
                }
            }
        }
    }
    Ok(())
}

/// Inspect the user's CLI home without activating a provider or creating files.
pub fn cli_system_home() -> Result<PathBuf, String> {
    PRIVATE_ROOT.get().ok_or("内置 CC Switch 尚未初始化。")?;
    let home = crate::config::get_home_dir();
    require_config_path(&home).map_err(|e| e.to_string())?;
    Ok(home)
}

pub fn cli_config_dir(app_type: &str) -> Result<PathBuf, String> {
    let path = match app_type {
        "claude" => crate::config::get_claude_config_dir(),
        "codex" => crate::codex_config::get_codex_config_dir(),
        "gemini" => crate::gemini_config::get_gemini_dir(),
        _ => return Err("不支持的 CLI 类型。".into()),
    };
    require_config_path(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn cli_launch_environment(app_type: &str) -> Result<Vec<(String, Option<PathBuf>)>, String> {
    let dir = cli_config_dir(app_type)?;
    let entry = match app_type {
        "codex" => ("CODEX_HOME", Some(dir)),
        "claude" => (
            "CLAUDE_CONFIG_DIR",
            if crate::config::get_claude_mcp_path()
                == crate::config::get_home_dir().join(".claude.json")
            {
                None
            } else {
                Some(dir)
            },
        ),
        "gemini" => {
            if dir.file_name().and_then(|s| s.to_str()) != Some(".gemini") {
                return Err("Gemini CLI 的配置目录须以 .gemini 结尾；请在设置中选择实际使用的 .gemini 目录。".into());
            }
            ("GEMINI_CLI_HOME", dir.parent().map(Path::to_path_buf))
        }
        _ => return Err("不支持的 CLI 类型。".into()),
    };
    Ok(vec![(entry.0.into(), entry.1)])
}

pub fn prepare_cli_launch(
    app: &tauri::AppHandle,
    app_type: &str,
    provider_id: &str,
) -> Result<PathBuf, String> {
    let kind = match app_type {
        "claude" => crate::AppType::Claude,
        "codex" => crate::AppType::Codex,
        "gemini" => crate::AppType::Gemini,
        _ => return Err("支持启动 Claude、Codex 和 Gemini。".into()),
    };
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化。")?;
    validate_settings(&crate::settings::get_settings()).map_err(|e| e.to_string())?;
    if state
        .db
        .get_provider_by_id(provider_id, app_type)
        .map_err(|_| "读取供应商失败。")?
        .is_none()
    {
        return Err("所选供应商不存在，请先导入。".into());
    }
    crate::ProviderService::switch(&state, kind.clone(), provider_id).map_err(|e| e.to_string())?;
    let live = match kind {
        crate::AppType::Claude => {
            crate::config::read_json_file(&crate::config::get_claude_settings_path())
        }
        crate::AppType::Codex => crate::codex_config::read_codex_live_settings(),
        crate::AppType::Gemini => crate::gemini_config::read_gemini_env()
            .map(|env| crate::gemini_config::env_to_json(&env)),
        _ => unreachable!(),
    }
    .map_err(|e| e.to_string())?;
    crate::isolation::validate_settings_content(&kind, &live).map_err(|e| e.to_string())?;
    cli_config_dir(app_type)
}

/// Secret-bearing input stays inside Rust; never derive an external URL or process argument.
pub struct MollyProviderImport {
    pub account_id: String,
    pub key_id: String,
    pub name: String,
    pub app: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub usage_script: Option<String>,
}

fn molly_provider_config(
    input: &MollyProviderImport,
    provider_id: &str,
) -> Result<serde_json::Value, String> {
    let config = match input.app.as_str() {
        "claude" | "claude-desktop" => json!({
            "env": {
                "ANTHROPIC_BASE_URL": input.base_url,
                "ANTHROPIC_AUTH_TOKEN": input.api_key,
                "ANTHROPIC_MODEL": input.model,
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": input.model,
                "ANTHROPIC_DEFAULT_SONNET_MODEL": input.model,
                "ANTHROPIC_DEFAULT_OPUS_MODEL": input.model,
            }
        }),
        "codex" => {
            let config = toml::to_string(&json!({
                "model_provider": "mollycloud", "model": input.model,
                "model_providers": { "mollycloud": { "name": "MollyCloud", "base_url": input.base_url,
                    "wire_api": "responses", "requires_openai_auth": true } }
            }))
            .map_err(|_| "无法生成 Codex 供应商配置。")?;
            json!({"auth": {"OPENAI_API_KEY": input.api_key}, "config": config})
        }
        "gemini" => json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": input.base_url,
                "GEMINI_API_KEY": input.api_key,
                "GEMINI_MODEL": input.model,
            }
        }),
        "grokbuild" => {
            let mut profiles = serde_json::Map::new();
            profiles.insert(
                input.model.clone(),
                json!({
                    "model": input.model,
                    "base_url": input.base_url,
                    "name": input.name,
                    "api_key": input.api_key,
                    "api_backend": "responses",
                    "context_window": 128000,
                }),
            );
            let config = toml::to_string(&json!({
                "models": {"default": input.model},
                "model": profiles,
            }))
            .map_err(|_| "无法生成 Grok Build 供应商配置。")?;
            json!({"config": config})
        }
        "opencode" => {
            let mut models = serde_json::Map::new();
            models.insert(input.model.clone(), json!({"name": input.model}));
            json!({
                "npm": "@ai-sdk/openai-compatible",
                "name": input.name,
                "options": {"baseURL": input.base_url, "apiKey": input.api_key, "setCacheKey": true},
                "models": models,
            })
        }
        "openclaw" => json!({
            "baseUrl": input.base_url,
            "apiKey": input.api_key,
            "api": "openai-responses",
            "models": [{"id": input.model, "name": input.model, "contextWindow": 128000, "maxTokens": 32768}],
        }),
        "hermes" => json!({
            "name": provider_id,
            "base_url": input.base_url,
            "api_key": input.api_key,
            "api_mode": "codex_responses",
            "models": [{"id": input.model, "name": input.model, "context_length": 128000}],
        }),
        "pi" => json!({
            "name": input.name,
            "baseUrl": input.base_url,
            "apiKey": input.api_key,
            "api": "openai-responses",
            "models": [{"id": input.model, "name": input.model, "contextWindow": 128000, "maxTokens": 32768}],
        }),
        "mcode" => {
            let mut models = serde_json::Map::new();
            models.insert(input.model.clone(), json!({"name": input.model}));
            json!({
                "name": input.name,
                "kind": "custom",
                "api": "openai-responses",
                "options": {"baseURL": input.base_url, "apiKey": input.api_key},
                "models": models,
            })
        }
        _ => return Err("请选择 CC Switch 支持的 Agent。".into()),
    };
    Ok(config)
}

fn save_molly_provider(db: &Database, input: MollyProviderImport) -> Result<String, String> {
    if input.account_id.trim().is_empty()
        || input.key_id.trim().is_empty()
        || input.api_key.trim().is_empty()
    {
        return Err("账户或密钥信息不完整，无法导入。".into());
    }
    let url = url::Url::parse(&input.base_url).map_err(|_| "MollyCloud API 地址无效。")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("MollyCloud API 地址无效。".into());
    }
    if input.name.trim().is_empty() || input.model.trim().is_empty() {
        return Err("供应商名称和默认模型不能为空。".into());
    }
    let mut hash = Sha256::new();
    hash.update(input.account_id.as_bytes());
    hash.update([0]);
    hash.update(input.key_id.as_bytes());
    if input.app != "codex" {
        hash.update([0]);
        hash.update(input.app.as_bytes());
    }
    let id = format!("molly-{:x}", hash.finalize());
    let settings_config = molly_provider_config(&input, &id)?;
    let existing = db
        .get_provider_by_id(&id, &input.app)
        .map_err(|_| "读取内置供应商库失败。")?;
    let mut provider = existing.unwrap_or_else(|| {
        Provider::with_id(
            id.clone(),
            input.name.clone(),
            json!({}),
            Some(input.base_url.clone()),
        )
    });
    provider.name = input.name;
    provider.settings_config = settings_config;
    provider.website_url = url::Url::parse(&input.base_url).ok().map(|url| url.origin().ascii_serialization());
    provider
        .created_at
        .get_or_insert_with(|| chrono::Utc::now().timestamp());
    provider.category = Some("custom".into());
    provider.notes = Some(format!(
        "由 MollyCloud API 密钥页导入；点击启用后写入本机 {} 配置。",
        input.app
    ));
    let mut meta = provider.meta.take().unwrap_or_default();
    if let Some(code) = input.usage_script {
        meta.usage_script = Some(
            serde_json::from_value(json!({
                "enabled":true, "language":"javascript", "code":code, "autoQueryInterval":30
            }))
            .map_err(|_| "无法保存用量脚本配置。")?,
        );
    }
    if input.app == "claude" || input.app == "claude-desktop" {
        meta.api_format = Some("openai_responses".into());
        meta.api_key_field = Some("ANTHROPIC_AUTH_TOKEN".into());
    }
    if input.app == "claude-desktop" {
        meta.claude_desktop_mode = Some(crate::provider::ClaudeDesktopMode::Proxy);
        meta.claude_desktop_model_routes = [
            "claude-sonnet-5",
            "claude-opus-5",
            "claude-fable-5",
            "claude-haiku-4-5",
        ]
        .into_iter()
        .map(|route| {
            (
                route.to_owned(),
                crate::provider::ClaudeDesktopModelRoute {
                    model: input.model.clone(),
                    label_override: Some(input.model.clone()),
                    supports_1m: None,
                },
            )
        })
        .collect();
    }
    provider.meta = Some(meta);
    // Database::save_provider preserves current flag only for existing rows; new rows are false.
    // Never call ProviderService::add (which activates the first provider) or update (which rewrites live).
    db.save_provider(&input.app, &provider)
        .map_err(|_| "写入内置供应商库失败。")?;
    Ok(id)
}

pub fn import_molly_provider(
    app: &tauri::AppHandle,
    input: MollyProviderImport,
) -> Result<String, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化。")?;
    validate_settings(&crate::settings::get_settings()).map_err(|e| e.to_string())?;
    let app_type = input.app.clone();
    let id = save_molly_provider(&state.db, input)?;
    let _ = app.emit(
        "molly-ccswitch:provider-imported",
        json!({"app":app_type, "providerId":id}),
    );
    Ok(id)
}

/// Account balance remains a Molly host capability, never a vendored JS script.
pub fn is_molly_imported_provider(app: &tauri::AppHandle, agent: &str, id: &str) -> Result<bool, String> {
    if !id.starts_with("molly-") { return Ok(false); }
    let state = app.try_state::<AppState>().ok_or("内置 CC Switch 尚未初始化。")?;
    let provider = state.db.get_provider_by_id(id, agent).map_err(|e| e.to_string())?;
    Ok(provider.is_some_and(|provider| {
        provider.notes.as_deref().is_some_and(|note| note.starts_with("由 MollyCloud API 密钥页导入；"))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_test_directory(target: &Path, link: &Path) {
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd.exe")
                .args(["/c", "mklink", "/j"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(target.to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "temporary test junction could not be created: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    fn unlink_test_directory(link: &Path) {
        #[cfg(windows)]
        std::fs::remove_dir(link).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(link).unwrap();
    }

    #[test]
    fn private_root_rejects_directory_links_before_canonicalization() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("external-profile");
        std::fs::create_dir_all(&outside).unwrap();
        let sentinel = outside.join("sentinel");
        std::fs::write(&sentinel, b"external configuration").unwrap();
        let link = temp.path().join("molly/ccswitch");
        link_test_directory(&outside, &link);
        let rejected = resolve_private_directory(&link).is_err();
        unlink_test_directory(&link);
        assert!(
            rejected,
            "root links must not be accepted as virtualized AppData"
        );
        assert_eq!(std::fs::read(sentinel).unwrap(), b"external configuration");
    }

    #[test]
    fn private_directory_io_error_identifies_the_failed_path() {
        let temp = tempfile::tempdir().unwrap();
        let occupied = temp.path().join("ccswitch");
        std::fs::write(&occupied, b"occupied").unwrap();
        let error = resolve_private_directory(&occupied).unwrap_err();
        assert!(matches!(error, crate::AppError::Io { .. }));
        assert!(error.to_string().contains("ccswitch"));
        assert_eq!(std::fs::read(occupied).unwrap(), b"occupied");
    }

    #[test]
    #[cfg(windows)]
    fn roaming_child_uses_its_actual_storage_location() {
        // Use a unique disposable Roaming child: under a packaged launcher its
        // parent and child may resolve to different MSIX filesystem views.
        let roaming = dirs::data_dir().unwrap();
        let temp = tempfile::Builder::new()
            .prefix("molly-ccswitch-path-test-")
            .tempdir_in(&roaming)
            .unwrap();
        let owner = dunce::canonicalize(&roaming).unwrap();
        let requested = owner.join(temp.path().file_name().unwrap());
        let root = resolve_private_directory(&requested).unwrap();
        assert_eq!(root, dunce::canonicalize(temp.path()).unwrap());
        let child = root.join("home/.codex");
        require_contained_path(&root, &child).unwrap();
        std::fs::create_dir_all(&child).unwrap();
        require_contained_path(&root, &child).unwrap();
        assert!(require_contained_path(&root, &owner).is_err());
    }

    #[test]
    fn molly_import_builds_a_saved_provider_for_every_supported_agent() {
        let db = Database::memory().unwrap();
        let agents = [
            "claude",
            "claude-desktop",
            "codex",
            "gemini",
            "grokbuild",
            "opencode",
            "openclaw",
            "hermes",
            "pi",
            "mcode",
        ];
        let mut ids = std::collections::HashSet::new();
        for agent in agents {
            let id = save_molly_provider(
                &db,
                MollyProviderImport {
                    account_id: "account-all-agents".into(),
                    key_id: "key-all-agents".into(),
                    name: format!("MollyCloud {agent}"),
                    app: agent.into(),
                    api_key: "secret-not-real".into(),
                    base_url: "https://example.test/v1".into(),
                    model: "model-selected-by-user".into(),
                    usage_script: None,
                },
            )
            .unwrap();
            assert!(
                ids.insert(id.clone()),
                "provider IDs must be unique per agent"
            );
            let provider = db.get_provider_by_id(&id, agent).unwrap().unwrap();
            assert_eq!(provider.name, format!("MollyCloud {agent}"));
            assert!(provider.settings_config.is_object());
            let serialized = provider.settings_config.to_string();
            assert!(serialized.contains("secret-not-real"));
            assert!(serialized.contains("model-selected-by-user"));
            assert!(db.get_current_provider(agent).unwrap().is_none());
            if agent == "mcode" {
                crate::mcode_config::validate_provider(&id, &provider.settings_config).unwrap();
            }
            if agent == "claude-desktop" {
                crate::claude_desktop_config::validate_provider(&provider).unwrap();
                assert_eq!(
                    crate::claude_desktop_config::provider_mode(&provider),
                    crate::provider::ClaudeDesktopMode::Proxy
                );
            }
        }

        assert!(save_molly_provider(
            &db,
            MollyProviderImport {
                account_id: "account-all-agents".into(),
                key_id: "key-all-agents".into(),
                name: "Unsupported".into(),
                app: "unsupported".into(),
                api_key: "secret-not-real".into(),
                base_url: "https://example.test/v1".into(),
                model: "model".into(),
                usage_script: None,
            },
        )
        .is_err());
    }

    #[test]
    fn molly_import_separates_website_from_model_api() {
        let db = Database::memory().unwrap();
        let id = save_molly_provider(&db, MollyProviderImport {
            account_id: "mock-account".into(), key_id: "mock-key".into(),
            name: "MollyCloud".into(), app: "codex".into(),
            api_key: "mock-not-real".into(),
            base_url: "https://mollycloud.cn/v1".into(),
            model: "mock-model".into(), usage_script: None,
        }).unwrap();
        let card = db.get_provider_by_id(&id, "codex").unwrap().unwrap();
        assert_eq!(card.website_url.as_deref(), Some("https://mollycloud.cn"));
        assert_eq!(card.resolve_usage_credentials(&crate::AppType::Codex).0, "https://mollycloud.cn/v1");
        assert!(db.get_current_provider("codex").unwrap().is_none());
    }

    #[test]
    fn system_targets_import_switch_and_proxy_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let system_home = temp.path().join("system-user");
        std::fs::create_dir_all(&system_home).unwrap();
        TEST_HOME.set(system_home.clone()).unwrap();
        assert!(cli_system_home().is_err());
        let blocked_data = temp.path().join("blocked-data");
        std::fs::create_dir_all(&blocked_data).unwrap();
        std::fs::write(blocked_data.join("ccswitch"), b"occupied by a file").unwrap();
        assert!(initialize_paths(&blocked_data).is_err());
        assert!(cli_system_home().is_err());
        initialize_paths(&temp.path().join("molly")).unwrap();
        let external = temp.path().join("external");
        std::fs::create_dir_all(external.join(".codex")).unwrap();
        let sentinel = external.join(".codex/auth.json");
        std::fs::write(&sentinel, b"PROXY_MANAGED external sentinel").unwrap();
        let db = Database::memory().unwrap();
        let input = |key: &str| MollyProviderImport {
            account_id: "account-1".into(),
            key_id: "key-1".into(),
            name: "MollyCloud".into(),
            app: "codex".into(),
            api_key: key.into(),
            base_url: "https://example.test/v1".into(),
            model: "gpt-5.5".into(),
            usage_script: None,
        };
        let first = save_molly_provider(&db, input("secret-one")).unwrap();
        let second = save_molly_provider(&db, input("secret-two")).unwrap();
        assert_eq!(first, second);
        assert_eq!(db.get_all_providers("codex").unwrap().len(), 1);
        assert!(db.get_current_provider("codex").unwrap().is_none());
        assert_eq!(
            db.get_provider_by_id(&first, "codex")
                .unwrap()
                .unwrap()
                .settings_config["auth"]["OPENAI_API_KEY"],
            "secret-two"
        );
        let provider = db.get_provider_by_id(&first, "codex").unwrap().unwrap();
        let config: toml::Value = provider.settings_config["config"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            config["model_providers"]["mollycloud"]["base_url"].as_str(),
            Some("https://example.test/v1")
        );
        assert!(!crate::codex_config::get_codex_auth_path().exists());
        assert!(!crate::codex_config::get_codex_config_path().exists());
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"PROXY_MANAGED external sentinel"
        );
        assert!(require_private_path(&sentinel).is_err());
        assert!(require_private_path(&private_root().join("../outside")).is_err());
        // Upgrade retains providers but never mistakes private activation for
        // an already-applied system configuration; running it twice is harmless.
        db.set_current_provider("codex", &first).unwrap();
        let mut legacy_settings = crate::settings::AppSettings::default();
        legacy_settings.current_provider_codex = Some(first.clone());
        legacy_settings.codex_config_dir =
            Some(private_root().join("home/.codex").display().to_string());
        crate::settings::update_settings(legacy_settings).unwrap();
        migrate_system_targets(&db).unwrap();
        assert!(crate::settings::get_settings().codex_config_dir.is_none());
        assert!(db.get_current_provider("codex").unwrap().is_none());
        assert_eq!(db.get_all_providers("codex").unwrap().len(), 1);
        assert!(!crate::codex_config::get_codex_config_path().exists());
        db.set_current_provider("codex", &first).unwrap();
        migrate_system_targets(&db).unwrap();
        assert_eq!(
            db.get_current_provider("codex").unwrap().as_deref(),
            Some(first.as_str())
        );
        db.set_current_provider("codex", "").unwrap();
        let mut settings = crate::settings::AppSettings::default();
        settings.codex_config_dir = Some(external.to_string_lossy().into());
        assert!(validate_settings(&settings).is_ok());
        for (app, value) in [
            (
                crate::AppType::Claude,
                json!({"env":{"CLAUDE_CONFIG_DIR": "C:/outside"},"apiKeyHelper":"user-helper"}),
            ),
            (
                crate::AppType::Gemini,
                json!({"env":{"HOME": "C:/outside"}}),
            ),
            (
                crate::AppType::Codex,
                json!({"config":"sqlite_home = 'C:/outside'\ncli_auth_credentials_store = 'keyring'"}),
            ),
        ] {
            crate::isolation::validate_settings_content(&app, &value).unwrap();
            let mut live = value.clone();
            crate::isolation::isolate_live_settings(&app, &mut live).unwrap();
            assert_eq!(live, value, "preserve native credential and path settings");
        }
        let mut migration_settings = crate::settings::AppSettings::default();
        migration_settings.unify_codex_session_history = true;
        assert!(validate_settings(&migration_settings).is_ok());
        assert_eq!(crate::config::get_home_dir(), system_home);
        assert_eq!(
            crate::config::get_claude_mcp_path(),
            system_home.join(".claude.json")
        );
        assert_eq!(
            cli_launch_environment("claude").unwrap(),
            vec![("CLAUDE_CONFIG_DIR".into(), None)]
        );
        let external_manager = system_home.join(".cc-switch");
        std::fs::create_dir_all(&external_manager).unwrap();
        std::fs::write(
            external_manager.join("settings.json"),
            b"standalone settings sentinel",
        )
        .unwrap();
        assert!(require_config_path(&external_manager.join("cc-switch.db")).is_err());
        verify_mcode_system_configuration(&db, &system_home, &external_manager);
        let mut override_settings = crate::settings::AppSettings::default();
        override_settings.codex_config_dir =
            Some(temp.path().join("custom-codex").display().to_string());
        crate::settings::update_settings(override_settings).unwrap();
        assert_eq!(
            cli_launch_environment("codex").unwrap()[0].1,
            Some(temp.path().join("custom-codex"))
        );
        crate::settings::update_settings(crate::settings::AppSettings::default()).unwrap();
        // Workspace dependencies and CLI history links must not disable all IPC;
        // an attempt to access their external target is still independently denied.
        for suffix in [
            "home/workspace/node_modules/link",
            "home/another-project/link",
            "home/.codex/sessions/2026/09/12",
            "home/.claude/projects/project-1",
            "home/.gemini/tmp/session-1",
        ] {
            let link = private_root().join(suffix);
            link_test_directory(&external, &link);
            validate_settings(&crate::settings::AppSettings::default()).unwrap();
            assert!(require_private_path(&link.join(".codex/auth.json")).is_err());
            unlink_test_directory(&link);
        }
        let managed_link = private_root().join("data/backups/external");
        link_test_directory(&external, &managed_link);
        assert!(validate_settings(&crate::settings::AppSettings::default()).is_err());
        unlink_test_directory(&managed_link);
        let settings_link = private_root().join("home/.cc-switch");
        let saved_settings = private_root().join("home/settings-test-backup");
        std::fs::rename(&settings_link, &saved_settings).unwrap();
        link_test_directory(&external, &settings_link);
        assert!(crate::settings::reload_settings().is_err());
        assert!(crate::settings::update_settings(crate::settings::AppSettings::default()).is_err());
        unlink_test_directory(&settings_link);
        std::fs::rename(&saved_settings, &settings_link).unwrap();
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"PROXY_MANAGED external sentinel"
        );
        let claude_path = crate::config::get_claude_settings_path();
        crate::config::atomic_write(&claude_path, b"owned bytes").unwrap();
        assert!(crate::proxy_ownership::require_unchanged(&crate::AppType::Claude).is_ok());
        std::fs::write(&claude_path, b"external change").unwrap();
        assert!(crate::proxy_ownership::require_unchanged(&crate::AppType::Claude).is_err());
        for command in [
            "set_auto_launch",
            "restart_app",
            "open_provider_terminal",
            "set_app_config_dir_override",
        ] {
            assert!(!ALLOWED_COMMANDS.contains(&command));
        }
        for command in [
            "delete_env_vars",
            "restore_db_backup",
            "install_skill_unified",
            "list_sessions",
            "pick_directory",
        ] {
            assert!(ALLOWED_COMMANDS.contains(&command));
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let mut config = crate::proxy::types::ProxyConfig::default();
            assert_eq!(config.listen_port, 24327);
            config.listen_port = occupied.local_addr().unwrap().port();
            let db = Arc::new(db);
            let state = AppState::new(db.clone());
            crate::ProviderService::switch(&state, crate::AppType::Codex, &first).unwrap();
            let real_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path()).unwrap();
            assert_eq!(crate::codex_config::extract_codex_experimental_bearer_token(&real_config).as_deref(), Some("secret-two"));
            assert!(!private_root().join("home/.codex/auth.json").exists());
            assert_eq!(std::fs::read(external_manager.join("settings.json")).unwrap(), b"standalone settings sentinel");
            let service = crate::ProxyService::new(db.clone());
            service.update_config(&config).await.unwrap();
            assert!(service.start().await.is_err());
            assert!(!service.is_running().await);
            assert!(std::net::TcpStream::connect(occupied.local_addr().unwrap()).is_ok());
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"PROXY_MANAGED external sentinel");
            let original = json!({"env":{"ANTHROPIC_AUTH_TOKEN":"restored-test-secret"}});
            let owned = json!({"env":{"ANTHROPIC_AUTH_TOKEN":"MOLLY_CCSWITCH_PROXY_MANAGED"}});
            db.save_live_backup("claude", &original.to_string()).await.unwrap();
            crate::config::write_json_file(&claude_path, &owned).unwrap();
            service.stop_with_restore().await.unwrap();
            assert_eq!(crate::config::read_json_file::<serde_json::Value>(&claude_path).unwrap(), original);

            db.save_live_backup("claude", &original.to_string()).await.unwrap();
            crate::config::write_json_file(&claude_path, &owned).unwrap();
            let edited = json!({"env":{"ANTHROPIC_AUTH_TOKEN":"MOLLY_CCSWITCH_PROXY_MANAGED"},"externalEdit":true}).to_string();
            std::fs::write(&claude_path, &edited).unwrap();
            assert!(service.stop_with_restore().await.is_err());
            assert_eq!(std::fs::read_to_string(&claude_path).unwrap(), edited);

            let external_marker = json!({"env":{"ANTHROPIC_AUTH_TOKEN":"PROXY_MANAGED"}}).to_string();
            std::fs::write(&claude_path, &external_marker).unwrap();
            assert!(!service.detect_takeover_in_live_config_for_app(&crate::AppType::Claude));
            assert!(crate::proxy_ownership::require_no_external_proxy(&crate::AppType::Claude).is_err());
            assert!(crate::config::atomic_write(&claude_path, b"must not overwrite external takeover").is_err());
            service.stop_with_restore().await.unwrap();
            assert_eq!(std::fs::read_to_string(&claude_path).unwrap(), external_marker);

            let codex_auth = crate::codex_config::get_codex_auth_path();
            let codex_config = crate::codex_config::get_codex_config_path();
            let codex_original_config = "model_provider = 'molly'\n[model_providers.molly]\nname = 'MollyCloud'\nbase_url = 'https://example.test/v1'\nwire_api = 'responses'\n";
            let codex_proxy_config = format!("{codex_original_config}experimental_bearer_token = 'MOLLY_CCSWITCH_PROXY_MANAGED'\n");
            let codex_original_auth = json!({"OPENAI_API_KEY":"original-test-key"});
            let codex_owned_auth = json!({"OPENAI_API_KEY":"owned-test-key"});
            let codex_backup = json!({"auth":codex_original_auth,"config":codex_original_config});
            db.save_live_backup("codex", &codex_backup.to_string()).await.unwrap();
            crate::codex_config::write_codex_live_atomic(&codex_owned_auth, Some(&codex_proxy_config)).unwrap();
            assert!(service.detect_takeover_in_live_config_for_app(&crate::AppType::Codex));
            service.stop_with_restore().await.unwrap();
            assert_eq!(crate::config::read_json_file::<serde_json::Value>(&codex_auth).unwrap(), codex_original_auth);

            db.save_live_backup("codex", &codex_backup.to_string()).await.unwrap();
            crate::codex_config::write_codex_live_atomic(&codex_owned_auth, Some(&codex_proxy_config)).unwrap();
            let edited_auth = json!({"OPENAI_API_KEY":"externally-edited-test-key"}).to_string();
            std::fs::write(&codex_auth, &edited_auth).unwrap();
            // A later config-only module write must not adopt the external auth edit.
            crate::config::write_text_file(&codex_config, &codex_proxy_config).unwrap();
            assert!(service.stop_with_restore().await.is_err());
            assert_eq!(std::fs::read_to_string(&codex_auth).unwrap(), edited_auth);
            assert_eq!(std::fs::read_to_string(&codex_config).unwrap(), codex_proxy_config);

            let gemini_env = crate::gemini_config::get_gemini_env_path();
            let gemini_settings = crate::gemini_config::get_gemini_settings_path();
            let gemini_proxy_env = "GEMINI_API_KEY=MOLLY_CCSWITCH_PROXY_MANAGED\n";
            crate::config::atomic_write(&gemini_env, gemini_proxy_env.as_bytes()).unwrap();
            crate::config::write_json_file(&gemini_settings, &json!({"security":{"auth":{"selectedType":"gemini-api-key"}}})).unwrap();
            let gemini_edited_settings = json!({"externalEdit":true}).to_string();
            std::fs::write(&gemini_settings, &gemini_edited_settings).unwrap();
            assert!(service.restore_live_config_for_app_with_fallback_inner(&crate::AppType::Gemini).await.is_err());
            assert_eq!(std::fs::read_to_string(&gemini_settings).unwrap(), gemini_edited_settings);
            assert_eq!(std::fs::read_to_string(&gemini_env).unwrap(), gemini_proxy_env);
        });
    }

    fn verify_mcode_system_configuration(db: &Database, home: &Path, external_manager: &Path) {
        use crate::mcode_config;
        assert_eq!(mcode_config::data_dir(), home.join(".minimax"));
        let path = mcode_config::config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = "defaultModel: minimax/native\ncustom_provider:\n  native:\n    kind: account\n    name: Keep\nminimax_api:\n  apiKey: mock-native-key\nunknown: [a, b]\n";
        std::fs::write(&path, original).unwrap();
        let id = save_molly_provider(db, MollyProviderImport {
            account_id: "minimax-test".into(), key_id: "key".into(),
            name: "Molly MiniMax test".into(), app: "mcode".into(),
            api_key: "mock-not-real".into(), base_url: "https://example.test/v1".into(),
            model: "test-model".into(), usage_script: None,
        }).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "import is DB-only");
        let config = db.get_provider_by_id(&id, "mcode").unwrap().unwrap().settings_config;
        let lock = path.with_extension("yaml.lock");
        std::fs::create_dir(&lock).unwrap();
        assert!(mcode_config::set_provider(&id, config.clone()).is_err());
        assert!(lock.exists(), "do not remove the native tool's active lock");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        std::fs::remove_dir(&lock).unwrap();
        mcode_config::set_provider(&id, config.clone()).unwrap();
        let before: serde_yaml::Value = serde_yaml::from_str(original).unwrap();
        let mut after: serde_yaml::Value = serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for field in ["defaultModel", "minimax_api", "unknown"] {
            assert_eq!(before[field], after[field]);
        }
        assert_eq!(before["custom_provider"]["native"], after["custom_provider"]["native"]);
        assert!(mcode_config::set_provider("native", config.clone()).is_err());
        after["defaultModel"] = format!("custom_provider:{id}/test-model").into();
        std::fs::write(&path, serde_yaml::to_string(&after).unwrap()).unwrap();
        assert!(mcode_config::remove_provider(&id).is_err());
        let mut replacement = config.clone();
        replacement["models"] = json!({"different-model":{}});
        assert!(mcode_config::set_provider(&id, replacement).is_err());
        after["defaultModel"] = before["defaultModel"].clone();
        std::fs::write(&path, serde_yaml::to_string(&after).unwrap()).unwrap();
        mcode_config::remove_provider(&id).unwrap();
        assert_eq!(serde_yaml::from_str::<serde_yaml::Value>(&std::fs::read_to_string(&path).unwrap()).unwrap(), before);

        let mcp_path = mcode_config::data_dir().join("mcp.json");
        let original_mcp = json!({"extra":42,"mcpServers":{"keep":{"url":"https://example.test/mcp","enabled":false}}});
        std::fs::write(&mcp_path, original_mcp.to_string()).unwrap();
        crate::mcp::mcode::sync("test", Some(&json!({"command":"mock-server","args":[]}))).unwrap();
        let mcp: serde_json::Value = crate::config::read_json_file(&mcp_path).unwrap();
        assert_eq!(mcp["mcpServers"]["keep"], original_mcp["mcpServers"]["keep"]);
        assert_eq!(mcp["extra"], 42);
        assert_eq!(mcp["mcpServers"]["test"]["enabled"], true);
        crate::mcp::mcode::sync("test", None).unwrap();
        assert_eq!(crate::config::read_json_file::<serde_json::Value>(&mcp_path).unwrap(), original_mcp);

        // The shared path guard must reject a junction into standalone CC Switch
        // before creating a lock, directory, or touching the target configuration.
        let preserved = home.join("minimax-preserved");
        std::fs::rename(mcode_config::data_dir(), &preserved).unwrap();
        link_test_directory(external_manager, &mcode_config::data_dir());
        assert!(validate_settings(&crate::settings::get_settings()).is_err());
        assert!(mcode_config::set_provider(&id, config).is_err());
        assert!(!external_manager.join("config.yaml").exists());
        assert!(!external_manager.join("config.yaml.lock").exists());
        unlink_test_directory(&mcode_config::data_dir());
        std::fs::rename(preserved, mcode_config::data_dir()).unwrap();
    }
}
