//! Real Tauri lifecycle with about:blank and a temporary profile. No login,
//! model, credentials, capture, user pet or input injection is involved.
#[path = "../src/console_settings.rs"] mod console_settings;
#[path = "../src/pet_window.rs"] mod pet_window;
#[cfg(target_os = "macos")]
mod screen { pub fn invalidate_ignore_cursor_cache() {} pub fn configure_pet_window(_: &tauri::WebviewWindow) {} pub fn unregister_pet_window(_: &tauri::WebviewWindow) {} }
use tauri::Manager;
fn log_line(message: &str) { println!("{message}"); }
fn stop_pet_session(_: &tauri::AppHandle) {}

fn main() {
    let profile = tempfile::tempdir().unwrap();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.pet-lifecycle-smoke".into();
    let mut config = context.config().app.windows.iter().find(|window| window.label == "main").unwrap().clone();
    assert!(!config.create, "Production pet must not be created at launch");
    config.url = tauri::WebviewUrl::External("about:blank".parse().unwrap());
    config.data_directory = Some(profile.path().join("webview"));
    config.incognito = true;
    context.config_mut().app.windows = vec![config];
    let result = std::sync::Arc::new(std::sync::Mutex::new(None));
    let output = result.clone();
    let app = tauri::Builder::default()
        .manage(console_settings::ConsoleSettingsState::load(None))
        .setup(move |app| {
            let console = tauri::WebviewWindowBuilder::new(app, "console", tauri::WebviewUrl::External("about:blank".parse().unwrap()))
                .visible(false).incognito(true).build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let change = |active| {
                    let target = console.clone();
                    let owner = handle.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    handle.run_on_main_thread(move || {
                        let _ = tx.send(console_settings::set_console_dashboard_active(target, owner.state(), active));
                    }).map_err(|error| error.to_string())?;
                    rx.recv_timeout(std::time::Duration::from_secs(3)).map_err(|error| error.to_string())?
                };
                let wait = |wanted: bool| -> Result<(), String> {
                    for _ in 0..200 {
                        if handle.get_webview_window("main").is_some() == wanted { return Ok(()); }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err("Pet creation/destruction did not finish".into())
                };
                let verify = || -> Result<(), String> {
                    wait(false)?;
                    if pet_window::show(&handle, false).is_ok() { return Err("Pet starts before login".into()); }
                    change(false)?;
                    wait(false)?;
                    change(true)?;
                    wait(true)?;
                    let first = handle.get_webview_window("main").ok_or("Pet not created after login")?;
                    for _ in 0..100 {
                        if first.is_visible().unwrap_or(false) { break; }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    if !first.is_visible().unwrap_or(false) || first.is_decorated().unwrap() || first.is_resizable().unwrap() {
                        return Err("Login pet must be visible, transparent-layout and non-resizable".into());
                    }
                    first.hide().map_err(|error| error.to_string())?;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    change(true)?;
                    if first.is_visible().unwrap_or(true) { return Err("Dashboard refresh overwrote manual hiding".into()); }
                    change(false)?;
                    wait(false)?;
                    if pet_window::show(&handle, false).is_ok() { return Err("Pet can start after logout".into()); }
                    change(true)?;
                    wait(true)?;
                    Ok(())
                };
                let outcome = verify();
                let value = serde_json::json!({"ok":outcome.is_ok(),"error":outcome.err(),"checks":11,"profile":"temporary","account":"mock dashboard phase"});
                let success = value["ok"] == true;
                *result.lock().unwrap() = Some(value);
                handle.exit(if success { 0 } else { 1 });
            });
            let watchdog = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(15));
                watchdog.exit(1);
            });
            Ok(())
        }).build(context).unwrap();
    let status = app.run_return(|_, _| {});
    let value = output.lock().unwrap().clone().unwrap_or_else(|| serde_json::json!({"ok":false,"error":"watchdog timeout"}));
    println!("{value}");
    if status != 0 || value["ok"] != true { std::process::exit(1); }
}
