//! Native close-event verification using hidden windows and temporary settings.
//! cargo run --example console_settings_smoke --features console-settings-smoke
#[allow(dead_code)]
#[path = "../src/console_settings.rs"]
mod console_settings;
#[path = "../src/pet_window.rs"]
mod pet_window;
#[cfg(target_os = "macos")]
mod screen { pub fn invalidate_ignore_cursor_cache() {} pub fn configure_pet_window(_: &tauri::WebviewWindow) {} pub fn unregister_pet_window(_: &tauri::WebviewWindow) {} }

use console_settings::{CloseAction, ConsoleSettings, ConsoleSettingsState};
use std::sync::{mpsc, Arc, Mutex};
use tauri::Manager;

fn log_line(message: &str) {
    println!("[console-settings-smoke] {message}");
}
fn stop_pet_session(_: &tauri::AppHandle) {}

#[cfg(target_os = "macos")]
fn verify_native_chrome(window: &tauri::WebviewWindow) -> Result<(), String> {
    use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowStyleMask};
    let native = unsafe { &*window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>() };
    let style = native.styleMask();
    if !style.contains(NSWindowStyleMask::Titled | NSWindowStyleMask::FullSizeContentView) || !native.hasShadow() {
        return Err("Native titled overlay or system window shadow is missing".into());
    }
    for button_type in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        let button = native.standardWindowButton(button_type).ok_or("Native window button is missing")?;
        if button.isHidden() || !button.isEnabled() {
            return Err("Native window button is hidden or disabled".into());
        }
        let rect = button.convertRect_toView(button.bounds(), None);
        let center_y = native.frame().size.height - rect.origin.y - rect.size.height / 2.0;
        println!("[native-chrome] button {:?}: x={} centerY={}", button_type, rect.origin.x, center_y);
        if (center_y - 22.0).abs() > 1.0 || rect.origin.x < 15.0 || rect.origin.x >= 92.0 {
            return Err("Native button does not fit before the sidebar toggle in the 44px header".into());
        }
    }
    println!("PASS: macOS native traffic lights, full-size overlay, system rounded frame and shadow configured");
    Ok(())
}

fn request_close(window: &tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let target = window.clone();
        window.app_handle().run_on_main_thread(move || {
            let native = unsafe { &*target.ns_window().unwrap().cast::<objc2_app_kit::NSWindow>() };
            // Exercise the actual system close button and its native event path.
            // The button belongs to this live fixture and the callback runs on
            // AppKit's main thread; its action is NSWindow's standard close.
            unsafe { native.standardWindowButton(objc2_app_kit::NSWindowButton::CloseButton).unwrap().performClick(None) };
        }).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    window.close().map_err(|e| e.to_string())
}

fn main() {
    let login_mode = std::env::args().any(|argument| argument == "--login");
    let temporary = tempfile::tempdir().expect("temporary settings directory");
    let path = temporary.path().join("console-settings.json");
    let webview_data = temporary.path().join("webview");
    let state = ConsoleSettingsState::load(Some(path.clone()));
    state
        .save(ConsoleSettings {
            close_action: CloseAction::Tray,
            autostart: false,
            autostart_minimized: false,
        })
        .unwrap();
    state.set_dashboard_active(!login_mode);
    let result = Arc::new(Mutex::new(None::<Result<(), String>>));
    let output = result.clone();
    let (closed_tx, closed_rx) = mpsc::channel();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.console-settings-smoke".into();
    let mut console_config = context.config().app.windows.iter().find(|window| window.label == "console").unwrap().clone();
    console_config.url = tauri::WebviewUrl::External("about:blank".parse().unwrap());
    console_config.visible = cfg!(target_os = "macos");
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .manage(state)
        .on_window_event(move |window, event| {
            console_settings::handle_window_event(window, event);
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let _ = closed_tx.send(window.label().to_owned());
            }
        })
        .setup(move |app| {
            let console = tauri::WebviewWindowBuilder::from_config(app, &console_config)?
            .title("MollyCloud isolated close verification")
            .focused(false)
            .skip_taskbar(true)
            .data_directory(webview_data.clone())
            .build()?;
            // A live pet window must not keep the login-close case running.
            tauri::WebviewWindowBuilder::new(app, "main",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()))
                .title("MollyCloud isolated pet verification")
                .visible(false).skip_taskbar(true).data_directory(webview_data).build()?;
            let handle = app.handle().clone();
            let watchdog = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(15));
                watchdog.exit(1);
            });
            std::thread::spawn(move || {
                let verify = || -> Result<(), String> {
                    #[cfg(target_os = "macos")]
                    {
                        // The inset is reapplied by Wry's drawRect; inspect it
                        // after the fixture has painted instead of during setup.
                        std::thread::sleep(std::time::Duration::from_millis(400));
                        let target = console.clone();
                        let (sender, receiver) = mpsc::channel();
                        handle.run_on_main_thread(move || { let _ = sender.send(verify_native_chrome(&target)); }).map_err(|e| e.to_string())?;
                        receiver.recv_timeout(std::time::Duration::from_secs(3)).map_err(|e| e.to_string())??;
                    }
                    request_close(&console)?;
                    let label = closed_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|_| "Missing tray CloseRequested event")?;
                    if login_mode {
                        if label != "console" { return Err("Unexpected login close target".into()); }
                        return Ok(());
                    }
                    if label != "console" || handle.get_webview_window("console").is_none() {
                        return Err("Tray close destroyed the console window".into());
                    }
                    if console.is_visible().map_err(|error| error.to_string())? {
                        return Err("Tray close did not hide the console window".into());
                    }
                    handle
                        .state::<ConsoleSettingsState>()
                        .save(ConsoleSettings {
                            close_action: CloseAction::Quit,
                            autostart: false,
                            autostart_minimized: false,
                        })?;
                    request_close(&console)?;
                    closed_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|_| "Missing quit CloseRequested event")?;
                    Ok(())
                };
                let outcome = verify();
                let failed = outcome.is_err();
                *result.lock().unwrap() = Some(outcome);
                if failed {
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .build(context)
        .expect("build isolated native close host");
    let exit_events = Arc::new(Mutex::new((false, false)));
    let exit_output = exit_events.clone();
    let exit_code = app.run_return(move |_, event| match event {
        tauri::RunEvent::ExitRequested { code: Some(0), .. } => {
            exit_events.lock().unwrap().0 = true
        }
        tauri::RunEvent::Exit => exit_events.lock().unwrap().1 = true,
        _ => {}
    });
    // The close callback wakes the worker immediately before the event loop
    // exits; allow that result to finish publishing without touching other apps.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while output.lock().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    output
        .lock()
        .unwrap()
        .take()
        .expect("close verification completed")
        .unwrap();
    assert_eq!(exit_code, 0);
    let (requested_exit, completed_exit) = *exit_output.lock().unwrap();
    assert!(
        requested_exit,
        "quit did not request the normal exit lifecycle"
    );
    assert!(
        completed_exit,
        "normal exit cleanup lifecycle was not reached"
    );
    assert_eq!(
        ConsoleSettingsState::load(Some(path))
            .get()
            .unwrap()
            .close_action,
        if login_mode { CloseAction::Tray } else { CloseAction::Quit },
        "saved close policy did not survive reload"
    );
    if login_mode {
        println!("PASS: login CloseRequested exits normally with a live pet window, preserving the saved tray preference");
    } else {
        println!("PASS: native console CloseRequested hides for tray, exits normally for quit, and persists selection in temporary storage");
    }
}
