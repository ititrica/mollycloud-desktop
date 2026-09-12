//! Isolated native host: real production UI, real image bridge, mock account only.
#[path = "../src/image_workbench.rs"]
mod image_workbench;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static OTHER_ACCOUNT: AtomicBool = AtomicBool::new(false);
static BOOTSTRAP_CALLS: AtomicUsize = AtomicUsize::new(0);

#[tauri::command]
fn set_mock_account() {
    OTHER_ACCOUNT.store(true, Ordering::SeqCst);
}

fn mock_user() -> serde_json::Value {
    serde_json::json!({"id": if OTHER_ACCOUNT.load(Ordering::SeqCst) { "another-image-test-user" } else { "native-smoke-user" }, "email":"native-image@example.test", "username":"原生验证", "balance":12.35})
}

#[tauri::command]
fn bootstrap_public() -> serde_json::Value {
    BOOTSTRAP_CALLS.fetch_add(1, Ordering::SeqCst);
    serde_json::json!({"service_origin":"https://mollycloud.cn","health":{"status":"ok"},"settings":{}})
}
#[tauri::command]
fn bootstrap_calls() -> usize {
    BOOTSTRAP_CALLS.load(Ordering::SeqCst)
}
#[tauri::command]
fn restore_session() -> serde_json::Value {
    mock_user()
}
#[tauri::command]
fn fetch_dashboard() -> serde_json::Value {
    serde_json::json!({"user":mock_user(),"subscriptions":{},"subscription_progress":[],"usage":{},"keys":{"items":[{"id":"account-sentinel","key":"NEVER-AUTO-IMPORT-THIS-KEY"}]}})
}
#[tauri::command]
fn get_assistant_config() -> serde_json::Value {
    serde_json::json!({"enabled":false,"provider":"molly","model":"","persona":"","custom_base_url":"","greet_interval":0,"molly_key_id":"","api_key_configured":false})
}
#[tauri::command]
fn is_pet_visible() -> bool {
    false
}
#[tauri::command]
fn set_console_dashboard_active() {}

fn main() {
    let profile = std::path::PathBuf::from(
        std::env::var("MOLLY_IMAGE_SMOKE_PROFILE").expect("isolated profile is required"),
    );
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.image-smoke".into();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .plugin(image_workbench::plugin())
        .manage(image_workbench::ImageRequests::default())
        .invoke_handler(tauri::generate_handler![
            image_workbench::image_request,
            image_workbench::cancel_image_request,
            set_mock_account,
            bootstrap_public,
            bootstrap_calls,
            restore_session,
            fetch_dashboard,
            get_assistant_config,
            is_pet_visible,
            set_console_dashboard_active
        ])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "console",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("MollyCloud isolated image verification")
            .inner_size(1180.0, 760.0)
            .disable_drag_drop_handler()
            .visible(false)
            .skip_taskbar(true)
            .data_directory(profile)
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(180));
                handle.exit(1);
            });
            Ok(())
        })
        .run(context)
        .expect("native image verification host");
}
