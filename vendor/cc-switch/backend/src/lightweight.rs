pub fn is_lightweight_mode() -> bool { false }
pub fn enter_lightweight_mode(_: &tauri::AppHandle) -> Result<(), String> { Err("内置 CC Switch 窗口由 MollyCloud 管理".into()) }
pub fn exit_lightweight_mode(_: &tauri::AppHandle) -> Result<(), String> { Err("内置 CC Switch 窗口由 MollyCloud 管理".into()) }
