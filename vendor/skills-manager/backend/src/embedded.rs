//! Host boundary: optional, lazy startup and a console-only command surface.
use std::{path::{Path, PathBuf}, sync::{Arc, OnceLock}};
use tauri::Manager;
use crate::core::{self, skill_store::SkillStore};
#[path = "allowed_commands.rs"]
mod allowed;
pub use allowed::ALLOWED_COMMANDS;
static ROOT: OnceLock<PathBuf> = OnceLock::new();
static TEST_HOME: OnceLock<PathBuf> = OnceLock::new();
static INIT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn system_home() -> Option<PathBuf> { TEST_HOME.get().cloned().or_else(dirs::home_dir) }
pub fn system_config() -> Option<PathBuf> {
    TEST_HOME.get().map(|p| p.join("config")).or_else(dirs::config_dir)
}
pub fn private_root() -> PathBuf {
    ROOT.get().cloned().unwrap_or_else(|| dirs::data_dir().expect("AppData unavailable").join("cn.mollycloud.client/skills-manager"))
}
fn resolved(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() { return p; }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) { return resolved(parent).join(name); }
    path.to_path_buf()
}
pub fn validate_relocation(source: &Path, target: &Path) -> anyhow::Result<()> {
    validate_repo_path(target)?;
    let source = resolved(source);
    let target = resolved(target);
    if source != target && (source.starts_with(&target) || target.starts_with(&source)) {
        anyhow::bail!("新旧技能库不能互为父子目录");
    }
    Ok(())
}
pub fn validate_repo_path(path: &Path) -> anyhow::Result<()> {
    if !path.is_absolute() { anyhow::bail!("技能库必须使用绝对路径"); }
    let candidate = resolved(path).to_string_lossy().replace('\\', "/").to_lowercase();
    let protected = [system_home().map(|p| p.join(".skills-manager")), system_home().map(|p| p.join(".agent-skills")), system_config().map(|p| p.join("skills-manager"))];
    for old in protected.into_iter().flatten() {
        let old = resolved(&old).to_string_lossy().replace('\\', "/").to_lowercase();
        if candidate == old || candidate.starts_with(&(old.clone() + "/")) || old.starts_with(&(candidate.clone() + "/")) {
            anyhow::bail!("请选择独立目录；不能覆盖独立版 Skills Manager 的数据或其父目录");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn initialize(app: tauri::AppHandle) -> Result<(), String> {
    let _guard = INIT.lock().await;
    if app.try_state::<Arc<SkillStore>>().is_some() { return Ok(()); }
    let store = tauri::async_runtime::spawn_blocking(core::app_state::initialize_cli_store)
        .await.map_err(|e| e.to_string())?.map_err(|e| format!("技能库初始化失败：{e:#}"))?;
    app.manage(Arc::new(core::install_cancel::InstallCancelRegistry::new()));
    app.manage(store.clone());
    core::file_watcher::start_file_watcher(app.clone(), store.clone());
    core::skill_auto_updater::start(app.clone(), store.clone());
    core::auto_backup::start(app, store);
    Ok(())
}

#[tauri::command]
pub async fn prepare_agent_control() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let binary = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = private_root().join("bundled/manage-molly-skills");
        let doc = core::cli_bridge::agent_document(&binary, &core::central_repo::skills_dir())
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("SKILL.md"), doc).map_err(|e| e.to_string())?;
        Ok(dir.to_string_lossy().into_owned())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pick_path(app: tauri::AppHandle, directory: bool, multiple: bool) -> Result<serde_json::Value, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file().set_title("选择 Skill 文件或目录");
        let paths = match (directory, multiple) {
            (true, true) => dialog.blocking_pick_folders(),
            (true, false) => dialog.blocking_pick_folder().map(|p| vec![p]),
            (false, true) => dialog.add_filter("Skill", &["zip", "skill"]).blocking_pick_files(),
            (false, false) => dialog.add_filter("Skill", &["zip", "skill"]).blocking_pick_file().map(|p| vec![p]),
        };
        let Some(paths) = paths else { return Ok(serde_json::Value::Null); };
        let paths: Result<Vec<String>, String> = paths.into_iter().map(|p| p.into_path().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())).collect();
        let paths = paths?;
        Ok(if multiple { serde_json::json!(paths) } else { serde_json::json!(paths.first()) })
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn open_external(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") { return Err("仅支持 HTTP(S) 来源链接".into()); }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text)).map_err(|e| e.to_string())
}

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> { build_plugin(None) }
#[cfg(feature = "test-hooks")]
pub fn init_for_test(root: PathBuf, home: PathBuf) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    TEST_HOME.set(home).expect("test home set once");
    build_plugin(Some(root))
}
pub fn init_guarded(gate: impl Fn() -> bool + Send + Sync + 'static) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    build_guarded(None, Arc::new(gate))
}
fn build_plugin(root: Option<PathBuf>) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    build_guarded(root, Arc::new(|| true))
}
fn build_guarded(root: Option<PathBuf>, gate: Arc<dyn Fn() -> bool + Send + Sync>) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let handler = crate::handler();
    tauri::plugin::Builder::new("molly-skills")
        .setup(move |app, _| {
            ROOT.set(root.unwrap_or(app.path().app_data_dir()?.join("skills-manager"))).ok();
            Ok(())
        })
        .invoke_handler(move |invoke| {
            if !gate() { invoke.resolver.reject("插件未安装或需要重启后启用"); return true; }
            if invoke.message.webview().label() != "console" || !ALLOWED_COMMANDS.contains(&invoke.message.command()) {
                invoke.resolver.reject("此 Skill 操作只允许从 Molly 控制台执行"); return true;
            }
            if invoke.message.command() != "initialize" && invoke.message.webview().try_state::<Arc<SkillStore>>().is_none() {
                invoke.resolver.reject("请先初始化 Skill 管理器"); return true;
            }
            if let Err(e) = validate_repo_path(&core::central_repo::base_dir()) {
                invoke.resolver.reject(e.to_string()); return true;
            }
            handler(invoke)
        }).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_library_migration_and_standalone_protection() {
        let _guard = core::central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("molly");
        ROOT.set(root.clone()).unwrap();
        TEST_HOME.set(home.clone()).unwrap();
        let legacy = home.join(".agent-skills");
        let standalone = home.join(".skills-manager");
        for path in [&legacy, &standalone] {
            std::fs::create_dir_all(path).unwrap();
            std::fs::write(path.join("sentinel"), "keep").unwrap();
        }
        let store = core::app_state::initialize_cli_store().unwrap();
        store.set_setting("test_migration", "keep metadata").unwrap();
        assert_eq!(store.get_setting("sync_mode").unwrap().as_deref(), Some("copy"));
        assert_eq!(core::central_repo::base_dir(), root.join("library"));
        assert!(!root.join("library/sentinel").exists());
        assert!(validate_repo_path(&standalone).is_err());
        assert!(validate_repo_path(&legacy.join("child")).is_err());
        assert!(validate_repo_path(&home).is_err());
        assert!(validate_relocation(&root.join("library"), &root.join("library/child")).is_err());
        #[cfg(windows)] {
            let alias = temp.path().join("alias");
            junction::create(&standalone, &alias).unwrap();
            assert!(validate_repo_path(&alias.join("child")).is_err());
            junction::delete(&alias).unwrap();
        }
        let target = temp.path().join("relocated");
        core::central_repo::set_base_dir_override(Some(target.display().to_string())).unwrap();
        assert_eq!(core::central_repo::base_dir(), root.join("library"));
        assert!(!target.exists());
        assert_eq!(store.get_setting("test_migration").unwrap().as_deref(), Some("keep metadata"));
        drop(store);
        core::central_repo::set_runtime_base_dir_override(None);
        let reopened = core::app_state::initialize_cli_store().unwrap();
        assert_eq!(core::central_repo::base_dir(), target);
        assert_eq!(reopened.get_setting("test_migration").unwrap().as_deref(), Some("keep metadata"));
        assert!(root.join("repo-config.json").exists());
        assert_eq!(std::fs::read_to_string(standalone.join("sentinel")).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(legacy.join("sentinel")).unwrap(), "keep");
        for command in ["app_exit", "restart_app", "hide_to_tray", "check_app_update"] {
            assert!(!ALLOWED_COMMANDS.contains(&command));
        }
    }
}
