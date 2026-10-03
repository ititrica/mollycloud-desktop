//! The transparent pet is created only after the console enters its dashboard.
use tauri::{AppHandle, Emitter, Manager};

pub fn show(app: &AppHandle, focus: bool) -> Result<(), String> {
    if !app.try_state::<crate::console_settings::ConsoleSettingsState>()
        .is_some_and(|state| state.dashboard_active()) {
        return Err("请先登录 MollyCloud 控制台".into());
    }
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let config = app.config().app.windows.iter().find(|window| window.label == "main")
                .ok_or("缺少桌宠窗口配置")?;
            let window = tauri::WebviewWindowBuilder::from_config(app, config)
                .map_err(|error| error.to_string())?
                .visible(false).focused(false).build().map_err(|error| error.to_string())?;
            #[cfg(target_os = "macos")]
            {
                crate::screen::invalidate_ignore_cursor_cache();
                crate::screen::configure_pet_window(&window);
            }
            window
        }
    };
    window.show().map_err(|error| error.to_string())?;
    if focus { window.set_focus().map_err(|error| error.to_string())?; }
    let _ = app.emit_to("console", "pet-visibility-changed", true);
    Ok(())
}

pub fn end_session(app: &AppHandle) {
    // Stop transient native work; the user's saved model/audio preferences stay
    // in the pet profile and are restored by its next authenticated launch.
    crate::stop_pet_session(app);
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        crate::screen::unregister_pet_window(&window);
        let _ = window.destroy();
    }
    let _ = app.emit_to("console", "pet-visibility-changed", false);
}
