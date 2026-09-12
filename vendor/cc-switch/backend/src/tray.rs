// MollyCloud owns the only application tray. Upstream refresh hooks are inert.
pub const TRAY_ID: &str = "molly-ccswitch-disabled-tray";
pub fn schedule_tray_refresh(_: &tauri::AppHandle) {}
pub fn refresh_tray_menu(_: &tauri::AppHandle) {}
pub fn apply_tray_policy(_: &tauri::AppHandle, _: bool) {}
pub fn create_tray_menu(_: &tauri::AppHandle, _: &crate::AppState) -> Result<tauri::menu::Menu<tauri::Wry>, crate::AppError> {
    Err(crate::AppError::Config("内置模块没有独立托盘".into()))
}
