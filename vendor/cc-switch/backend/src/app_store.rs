use std::path::PathBuf;
use crate::error::AppError;
// Embedded data ownership is fixed by the host; never read upstream app_paths.json.
pub fn get_app_config_dir_override() -> Option<PathBuf> { Some(crate::config::get_app_config_dir()) }
pub fn refresh_app_config_dir_override(_: &tauri::AppHandle) -> Option<PathBuf> { get_app_config_dir_override() }
pub fn set_app_config_dir_to_store(_: &tauri::AppHandle, _: Option<&str>) -> Result<(), AppError> {
    Err(AppError::Config("内置 CC Switch 数据目录由 MollyCloud 管理，不能指向系统配置。".into()))
}
pub fn migrate_app_config_dir_from_settings(_: &tauri::AppHandle) -> Result<(), AppError> { Ok(()) }
