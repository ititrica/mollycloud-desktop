//! Isolated WebView2 integration regression for Skill Manager. No real Agent data.
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct ResultState(Arc<Mutex<Option<Value>>>);
#[tauri::command]
fn smoke_report(app: tauri::AppHandle, state: tauri::State<'_, ResultState>, result: Value) {
    *state.0.lock().unwrap() = Some(result);
    app.exit(0);
}

#[tauri::command]
fn bootstrap_public() -> Value {
    json!({"service_origin":"https://example.test","health":{"status":"ok"},"settings":{}})
}
#[tauri::command]
fn restore_session() -> Value {
    json!({"id":"smoke","username":"Isolated MCP test","balance":0})
}
#[tauri::command]
fn fetch_dashboard() -> Value {
    json!({"user":restore_session(),"keys":{"items":[]},"usage":{},"subscriptions":{},"subscription_progress":[]})
}
#[tauri::command]
fn get_assistant_config() -> Value {
    json!({"enabled":false,"provider":"mollycloud","model":"test","persona":"","custom_base_url":"","greet_interval":20,"molly_key_id":"","api_key_configured":false})
}
#[tauri::command]
fn is_pet_visible() -> bool {
    false
}
#[tauri::command]
fn set_console_dashboard_active() {}
#[tauri::command]
fn check_desktop_update() -> Option<Value> {
    None
}
#[tauri::command]
fn fetch_account_balance() -> Value {
    json!({"balance":0})
}


const SCRIPT: &str = include_str!("skills_smoke.js");
struct FixtureRoot(std::path::PathBuf);
#[tauri::command]
fn smoke_change_source(root: tauri::State<'_, FixtureRoot>) {
    std::fs::write(root.0.join("source/SKILL.md"), "---\nname: example-skill\ndescription: Updated local test\n---\nUpdated test body\n").unwrap();
}
#[tauri::command]
async fn smoke_prepare_remote(root: tauri::State<'_, FixtureRoot>) -> Result<(), String> {
    // Local fixtures bypass only the public URL parser, never weaken production validation.
    let remote = root.0.join("remote.git");
    tauri::async_runtime::spawn_blocking(move || molly_skills::core::git_backup::set_remote(
        &molly_skills::core::central_repo::skills_dir(), &remote.to_string_lossy()
    ).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}
fn main() {
    if molly_skills::is_cli_request() { molly_skills::cli::main(); return; }
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().to_path_buf();
    let home = root.join("agent-home");
    let private = root.join("molly-private");
    for dir in [home.join(".codex/skills/unmanaged"),home.join(".claude/skills"),home.join(".agent-skills"),home.join(".skills-manager"),root.join("source"),root.join("project/.codex/skills"),root.join("linked"),root.join("linked-disabled")] {std::fs::create_dir_all(dir).unwrap();}
    std::fs::write(home.join(".codex/skills/unmanaged/SKILL.md"), "---\nname: unmanaged\ndescription: Keep this file\n---\nUntouched external skill").unwrap();
    std::fs::write(home.join(".agent-skills/sentinel"), "legacy untouched").unwrap();
    std::fs::write(home.join(".skills-manager/sentinel"), "standalone untouched").unwrap();
    std::fs::write(root.join("source/SKILL.md"), "---\nname: example-skill\ndescription: Isolated test skill\n---\nA local test document").unwrap();
    let status = std::process::Command::new("git").args(["init","--bare"]).arg(root.join("remote.git")).output().unwrap();assert!(status.status.success());
    let paths = json!({"root":root,"home":home,"source":root.join("source"),"project":root.join("project"),"linked":root.join("linked"),"disabled":root.join("linked-disabled"),"remote":root.join("remote.git"),"relocated":root.join("moved-library")});
    let result = ResultState(Arc::new(Mutex::new(None)));
    let output = result.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.skills-smoke".into();
    context.config_mut().app.windows.clear();
    let script = format!("window.__skillPaths={paths};\n{SCRIPT}");
    let saved_root = root.clone();
    let app = tauri::Builder::default().manage(result).manage(FixtureRoot(root.clone()))
        .plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_opener::init())
        .plugin(molly_skills::embedded::init_for_test(private.clone(), home.clone()))
        .invoke_handler(tauri::generate_handler![smoke_report, smoke_change_source, smoke_prepare_remote, bootstrap_public,restore_session,fetch_dashboard,get_assistant_config,is_pet_visible,set_console_dashboard_active,check_desktop_update,fetch_account_balance])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(app,"console",tauri::WebviewUrl::App("index.html".into()))
                .visible(false).inner_size(1180.0,760.0).data_directory(root.join("webview"))
                .initialization_script(&script).build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {std::thread::sleep(std::time::Duration::from_secs(180));handle.exit(1);});
            Ok(())
        }).build(context).expect("skills smoke app");
    app.run_return(|_,_|{});
    let result = output.0.lock().unwrap().clone().unwrap_or(json!({"ok":false,"error":"watchdog timeout"}));
    println!("{result}");
    assert_eq!(result["ok"],true);
    assert_eq!(std::fs::read_to_string(home.join(".skills-manager/sentinel")).unwrap(),"standalone untouched");
    assert_eq!(std::fs::read_to_string(home.join(".agent-skills/sentinel")).unwrap(),"legacy untouched");
    assert!(std::fs::read_to_string(home.join(".codex/skills/unmanaged/SKILL.md")).unwrap().contains("Untouched external"));
    assert!(!home.join(".codex/skills/example-skill").exists());
    assert!(private.join("library/skills-manager.db").exists());
    assert!(!saved_root.join("moved-library").exists());
    assert!(!private.join("bin").exists(), "Agent setup must not write executable copies");
    println!("Skill Manager native UI / IPC / deployment / backup / isolation checks passed");
}
