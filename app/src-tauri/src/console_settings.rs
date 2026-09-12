use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}};
use tauri::{AppHandle, Manager, State, WebviewWindow, Window, WindowEvent};

const SETTINGS_FILE: &str = "console-settings.json";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseAction {
    #[default]
    Tray,
    Quit,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleSettings {
    pub close_action: CloseAction,
}

pub struct ConsoleSettingsState {
    path: Option<PathBuf>,
    saved: Mutex<ConsoleSettings>,
    dashboard_active: AtomicBool,
}

impl ConsoleSettingsState {
    pub fn load(path: Option<PathBuf>) -> Self {
        let settings = path.as_ref().and_then(|path| match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(settings) => Some(settings),
                Err(_) => {
                    crate::log_line("console settings unreadable; using tray close action");
                    None
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => {
                crate::log_line("console settings unavailable; using tray close action");
                None
            }
        });
        Self {
            path,
            saved: Mutex::new(settings.unwrap_or_default()),
            dashboard_active: AtomicBool::new(false),
        }
    }

    // Window-close behavior follows the visible console phase, not background
    // token refreshes initiated by the pet. This state is never persisted.
    pub fn set_dashboard_active(&self, active: bool) {
        self.dashboard_active.store(active, Ordering::Release);
    }

    pub fn get(&self) -> Result<ConsoleSettings, String> {
        self.saved
            .lock()
            .map(|settings| *settings)
            .map_err(|_| "暂时无法读取控制台设置，请重启后重试".to_owned())
    }

    pub fn save(&self, settings: ConsoleSettings) -> Result<ConsoleSettings, String> {
        // Serialize concurrent saves and publish the new close action only after
        // the complete file has replaced the previous version successfully.
        let mut saved = self
            .saved
            .lock()
            .map_err(|_| "暂时无法保存控制台设置，请重启后重试".to_owned())?;
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| "无法获取控制台设置目录，请重启后重试".to_owned())?;
        let directory = path
            .parent()
            .ok_or_else(|| "控制台设置目录无效".to_owned())?;
        let persist = || -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(directory)?;
            let mut temporary = tempfile::Builder::new()
                .prefix(".console-settings-")
                .suffix(".tmp")
                .tempfile_in(directory)?;
            serde_json::to_writer_pretty(&mut temporary, &settings)?;
            temporary.write_all(b"\n")?;
            temporary.as_file().sync_all()?;
            // NamedTempFile::persist replaces the destination atomically on
            // Windows and retains the old file if replacement fails.
            temporary.persist(path)?;
            Ok(())
        };
        persist().map_err(|_| "无法保存控制台设置，请检查目录权限后重试".to_owned())?;
        *saved = settings;
        Ok(settings)
    }
}

pub fn initialize(app: &AppHandle) {
    let path = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(SETTINGS_FILE));
    if path.is_none() {
        crate::log_line("console settings directory unavailable; using tray close action");
    }
    app.manage(ConsoleSettingsState::load(path));
}

#[tauri::command]
pub fn get_console_settings(
    state: State<'_, ConsoleSettingsState>,
) -> Result<ConsoleSettings, String> {
    state.get()
}

#[tauri::command]
pub fn save_console_settings(
    window: WebviewWindow,
    state: State<'_, ConsoleSettingsState>,
    settings: ConsoleSettings,
) -> Result<ConsoleSettings, String> {
    if window.label() != "console" {
        return Err("只能在控制台中修改关闭窗口设置".to_owned());
    }
    state.save(settings)
}

#[tauri::command]
pub fn set_console_dashboard_active(
    window: WebviewWindow,
    state: State<'_, ConsoleSettingsState>,
    active: bool,
) -> Result<(), String> {
    if window.label() != "console" {
        return Err("只能由控制台更新窗口状态".to_owned());
    }
    state.set_dashboard_active(active);
    Ok(())
}

fn close_action_for_window(
    label: &str,
    settings: Option<ConsoleSettings>,
    dashboard_active: bool,
) -> Option<CloseAction> {
    (label == "console").then(|| {
        if dashboard_active { settings.unwrap_or_default().close_action }
        else { CloseAction::Quit }
    })
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let state = window
        .app_handle()
        .try_state::<ConsoleSettingsState>();
    let settings = state.as_ref().and_then(|state| state.get().ok());
    let dashboard_active = state.as_ref()
        .is_some_and(|state| state.dashboard_active.load(Ordering::Acquire));
    let Some(action) = close_action_for_window(window.label(), settings, dashboard_active) else {
        return;
    };
    api.prevent_close();
    match action {
        CloseAction::Tray => {
            if window.hide().is_err() {
                crate::log_line("console window could not be hidden");
            }
        }
        // This follows Tauri's normal exit lifecycle, including CC Switch's
        // private proxy cleanup. Explicit tray-menu exit uses the same path.
        CloseAction::Quit => window.app_handle().exit(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(action: CloseAction) -> ConsoleSettings {
        ConsoleSettings {
            close_action: action,
        }
    }

    #[test]
    fn missing_or_invalid_settings_default_to_tray_without_rewriting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        let state = ConsoleSettingsState::load(Some(path.clone()));
        assert_eq!(state.get().unwrap(), settings(CloseAction::Tray));
        assert!(!path.exists());
        std::fs::write(&path, br#"{"closeAction":"invalid"}"#).unwrap();
        let state = ConsoleSettingsState::load(Some(path.clone()));
        assert_eq!(state.get().unwrap(), settings(CloseAction::Tray));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            br#"{"closeAction":"invalid"}"#
        );
    }

    #[test]
    fn persisted_selection_survives_reload_and_atomic_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        let state = ConsoleSettingsState::load(Some(path.clone()));
        for action in [CloseAction::Quit, CloseAction::Tray, CloseAction::Quit] {
            assert_eq!(state.save(settings(action)).unwrap(), settings(action));
            assert_eq!(state.get().unwrap(), settings(action));
            let reloaded = ConsoleSettingsState::load(Some(path.clone()));
            assert_eq!(reloaded.get().unwrap(), settings(action));
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn only_explicit_supported_close_actions_deserialize() {
        for invalid in [
            r#"{"closeAction":"exit"}"#,
            r#"{"closeAction":"TRAY"}"#,
            r#"{"closeAction":null}"#,
            r#"{}"#,
            r#"{"closeAction":"quit","path":"elsewhere"}"#,
        ] {
            assert!(serde_json::from_str::<ConsoleSettings>(invalid).is_err());
        }
        assert_eq!(
            serde_json::to_value(settings(CloseAction::Quit)).unwrap(),
            serde_json::json!({ "closeAction": "quit" }),
        );
    }

    #[test]
    fn failed_save_does_not_change_live_close_action() {
        let directory = tempfile::tempdir().unwrap();
        let blocked_parent = directory.path().join("not-a-directory");
        std::fs::write(&blocked_parent, b"keep this file").unwrap();
        let state = ConsoleSettingsState::load(Some(blocked_parent.join(SETTINGS_FILE)));
        assert!(state.save(settings(CloseAction::Quit)).is_err());
        assert_eq!(state.get().unwrap(), settings(CloseAction::Tray));
        assert_eq!(std::fs::read(blocked_parent).unwrap(), b"keep this file");
        assert!(ConsoleSettingsState::load(None)
            .save(settings(CloseAction::Quit))
            .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn replacement_failure_preserves_saved_file_and_live_value() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        let state = ConsoleSettingsState::load(Some(path.clone()));
        state.save(settings(CloseAction::Quit)).unwrap();
        let previous = std::fs::read(&path).unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(state.save(settings(CloseAction::Tray)).is_err());
        assert_eq!(state.get().unwrap(), settings(CloseAction::Quit));
        drop(locked);
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn dashboard_close_policy_only_applies_to_console_and_defaults_to_tray() {
        assert_eq!(
            close_action_for_window("console", None, true),
            Some(CloseAction::Tray)
        );
        for action in [CloseAction::Quit, CloseAction::Tray] {
            assert_eq!(
                close_action_for_window("console", Some(settings(action)), true),
                Some(action)
            );
            assert_eq!(
                close_action_for_window("main", Some(settings(action)), true),
                None
            );
            assert_eq!(
                close_action_for_window("overlay", Some(settings(action)), true),
                None
            );
        }
    }

    #[test]
    fn login_startup_and_two_factor_close_always_quit() {
        for saved in [None, Some(settings(CloseAction::Tray)), Some(settings(CloseAction::Quit))] {
            assert_eq!(close_action_for_window("console", saved, false), Some(CloseAction::Quit));
            assert_eq!(close_action_for_window("main", saved, false), None);
        }
    }

    #[test]
    fn signing_out_restores_quit_without_changing_the_saved_preference() {
        let state = ConsoleSettingsState::load(None);
        let action = || close_action_for_window("console", state.get().ok(),
            state.dashboard_active.load(Ordering::Acquire));
        assert_eq!(action(), Some(CloseAction::Quit));
        state.set_dashboard_active(true);
        assert_eq!(action(), Some(CloseAction::Tray));
        state.set_dashboard_active(false);
        assert_eq!(action(), Some(CloseAction::Quit));
        assert_eq!(state.get().unwrap().close_action, CloseAction::Tray);
    }
}
