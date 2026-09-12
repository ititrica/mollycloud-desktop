//! Native close-event verification using hidden windows and temporary settings.
//! cargo run --example console_settings_smoke --features console-settings-smoke
#[allow(dead_code)]
#[path = "../src/console_settings.rs"]
mod console_settings;

use console_settings::{CloseAction, ConsoleSettings, ConsoleSettingsState};
use std::sync::{mpsc, Arc, Mutex};
use tauri::Manager;

fn log_line(message: &str) {
    println!("[console-settings-smoke] {message}");
}

fn main() {
    let login_mode = std::env::args().any(|argument| argument == "--login");
    let temporary = tempfile::tempdir().expect("temporary settings directory");
    let path = temporary.path().join("console-settings.json");
    let webview_data = temporary.path().join("webview");
    let state = ConsoleSettingsState::load(Some(path.clone()));
    state.save(ConsoleSettings { close_action: CloseAction::Tray, autostart: false }).unwrap();
    state.set_dashboard_active(!login_mode);
    let result = Arc::new(Mutex::new(None::<Result<(), String>>));
    let output = result.clone();
    let (closed_tx, closed_rx) = mpsc::channel();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.console-settings-smoke".into();
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
            let console = tauri::WebviewWindowBuilder::new(
                app,
                "console",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .title("MollyCloud isolated close verification")
            .visible(false)
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
                    console.close().map_err(|error| error.to_string())?;
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
                        })?;
                    console.close().map_err(|error| error.to_string())?;
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
