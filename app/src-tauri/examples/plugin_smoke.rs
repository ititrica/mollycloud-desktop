//! Hidden WebView2, temporary data; tests installed asset routing and lifecycle.
#[allow(dead_code)]
#[path = "../src/console_plugins.rs"]
mod console_plugins;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
struct Outcome(Arc<AtomicBool>);
#[tauri::command]
fn finish(app: tauri::AppHandle, state: tauri::State<'_, Outcome>, error: Option<String>) {
    if let Some(error) = error {
        eprintln!("Plugin native smoke failed: {error}");
    } else {
        state.0.store(true, Ordering::SeqCst);
        println!("Plugin native smoke: installed HTML/JS, same-origin bridge, rollback, uninstall and isolated data passed");
    }
    app.exit(0);
}
fn main() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("plugins");
    let profile = temporary.path().join("webview");
    let okay = Arc::new(AtomicBool::new(false));
    let result = okay.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.plugin-smoke".into();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .manage(Outcome(result))
        .invoke_handler(tauri::generate_handler![
            finish,
            console_plugins::list_console_plugins,
            console_plugins::change_console_plugin
        ])
        .setup(move |app| {
            console_plugins::initialize_smoke(app.handle(), root)?;
            let handle = app.handle().clone();
            tauri::WebviewWindowBuilder::new(
                app,
                "console",
                tauri::WebviewUrl::App("plugin-smoke.html".into()),
            )
            .visible(false)
            .skip_taskbar(true)
            .data_directory(profile)
            .on_web_resource_request(move |request, response| {
                if request.uri().path() == "/plugin-smoke.html" {
                    *response.body_mut() = std::borrow::Cow::Owned(
                        b"<!doctype html><html><body></body></html>".to_vec(),
                    );
                    response
                        .headers_mut()
                        .insert("Content-Type", "text/html".parse().unwrap());
                } else {
                    console_plugins::intercept(&handle, request, response);
                }
            })
            .initialization_script(include_str!("plugin_smoke.js"))
            .build()?;
            let app = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(30));
                app.exit(1);
            });
            Ok(())
        })
        .run(context)
        .unwrap();
    assert!(
        okay.load(Ordering::SeqCst),
        "Native plugin smoke did not pass"
    );
}
