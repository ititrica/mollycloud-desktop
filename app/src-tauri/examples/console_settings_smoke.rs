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

/// Trace a Quit AppleEvent before AppKit clears it. This optional probe belongs
/// only to the isolated fixture, and preserves AppKit's default NSTerminateNow.
#[cfg(target_os = "macos")]
fn install_termination_probe() {
    use objc2::{runtime::{AnyObject, Imp, Sel}, sel, MainThreadMarker};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager};
    extern "C-unwind" fn should_terminate(_: &AnyObject, _: Sel, _: &AnyObject) -> usize {
        let event = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent();
        let sender: Option<objc2::rc::Retained<NSAppleEventDescriptor>> = event.as_ref().and_then(|event| unsafe {
            objc2::msg_send![event, attributeDescriptorForKeyword: u32::from_be_bytes(*b"spid")]
        });
        println!("[native-termination] external quit sender_pid={:?}", sender.map(|pid| pid.int32Value()));
        1
    }
    let native = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
    let delegate = native.delegate().unwrap();
    let object: &AnyObject = delegate.as_ref();
    unsafe {
        assert!(objc2::ffi::class_addMethod(object.class() as *const _ as *mut _, sel!(applicationShouldTerminate:),
            std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize, Imp>(should_terminate),
            c"Q@:@".as_ptr()).as_bool(), "Quit probe must not replace an existing delegate method");
    }
}

#[cfg(target_os = "macos")]
fn verify_native_chrome(window: &tauri::WebviewWindow) -> Result<(), String> {
    use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowStyleMask};
    println!("[native-host] bundle_id={}", objc2_foundation::NSBundle::mainBundle().bundleIdentifier().map(|id| id.to_string()).unwrap_or_default());
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

#[cfg(target_os = "macos")]
fn verify_native_role(app: &tauri::AppHandle, foreground: bool, pet_visible: Option<bool>) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    let native = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
    let expected = if foreground { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
    if native.activationPolicy() != expected {
        return Err(format!("Unexpected native activation policy: {:?}; expected {expected:?}", native.activationPolicy()));
    }
    if let Some(visible) = pet_visible {
        let pet = app.get_webview_window("main").ok_or("Close destroyed the pet window")?;
        if pet.is_visible().map_err(|error| error.to_string())? != visible {
            return Err("Changing the console role changed pet visibility".into());
        }
    }
    println!("[native-role] foreground={foreground}; pet_visible={pet_visible:?}");
    Ok(())
}

#[cfg(target_os = "macos")]
fn verify_role_on_main_thread(app: &tauri::AppHandle, foreground: bool, pet_visible: Option<bool>) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || { let _ = sender.send(verify_native_role(&handle, foreground, pet_visible)); }).map_err(|error| error.to_string())?;
    receiver.recv_timeout(std::time::Duration::from_secs(3)).map_err(|_| "Background main loop stopped")?
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
    let no_pet = std::env::args().any(|argument| argument == "--no-pet");
    let visible_pet = std::env::args().any(|argument| argument == "--visible-pet");
    let menu_quit = std::env::args().any(|argument| argument == "--menu-quit");
    #[cfg(target_os = "macos")]
    let native_quit = std::env::args().any(|argument| argument == "--native-quit");
    #[cfg(target_os = "macos")]
    let trace_native_quit = std::env::args().any(|argument| argument == "--trace-native-quit");
    let idle_seconds = std::env::args().find_map(|argument| argument.strip_prefix("--idle-seconds=").and_then(|v| v.parse::<u64>().ok())).unwrap_or(3);
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
            if matches!(event, tauri::WindowEvent::Destroyed) {
                println!("[close-event] destroyed {}", window.label());
            }
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let _ = closed_tx.send(window.label().to_owned());
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            if trace_native_quit { install_termination_probe(); }
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);
            tauri::tray::TrayIconBuilder::with_id("pet-tray")
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?)
                .tooltip("MollyCloud close verification")
                .build(app)?;
            let console = tauri::WebviewWindowBuilder::from_config(app, &console_config)?
            .title("MollyCloud isolated close verification")
            .focused(false)
            .skip_taskbar(true)
            .data_directory(webview_data.clone())
            .build()?;
            // A live pet window must not keep the login-close case running.
            if !no_pet { tauri::WebviewWindowBuilder::new(app, "main",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()))
                .title("MollyCloud isolated pet verification")
                .inner_size(160.0, 120.0).decorations(false).transparent(true)
                .visible(visible_pet).skip_taskbar(true).data_directory(webview_data).build()?; }
            let handle = app.handle().clone();
            let watchdog = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(45));
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
                        verify_role_on_main_thread(&handle, true, (!no_pet).then_some(visible_pet))?;
                    }
                    request_close(&console)?;
                    let label = closed_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|_| "Missing tray CloseRequested event")?;
                    if login_mode && !cfg!(target_os = "macos") {
                        if label != "console" { return Err("Unexpected login close target".into()); }
                        return Ok(());
                    }
                    if label != "console" || handle.get_webview_window("console").is_none() {
                        return Err("Tray close destroyed the console window".into());
                    }
                    if console.is_visible().map_err(|error| error.to_string())? {
                        return Err("Tray close did not hide the console window".into());
                    }
                    // Keep all fixture windows hidden long enough to catch an
                    // implicit exit, then reopen and exercise the close again.
                    std::thread::sleep(std::time::Duration::from_secs(idle_seconds));
                    if handle.tray_by_id("pet-tray").is_none() {
                        return Err("Tray close removed the menu bar entry".into());
                    }
                    let (sender, receiver) = mpsc::channel();
                    handle.run_on_main_thread(move || { let _ = sender.send(()); }).map_err(|e| e.to_string())?;
                    receiver.recv_timeout(std::time::Duration::from_secs(3)).map_err(|_| "Background main loop stopped")?;
                    #[cfg(target_os = "macos")]
                    verify_role_on_main_thread(&handle, false, (!no_pet).then_some(visible_pet))?;
                    println!("[close-step] first hidden window remains alive");
                    console_settings::show_console(&handle)?;
                    #[cfg(target_os = "macos")]
                    verify_role_on_main_thread(&handle, true, (!no_pet).then_some(visible_pet))?;
                    request_close(&console)?;
                    closed_rx.recv_timeout(std::time::Duration::from_secs(3)).map_err(|error| error.to_string())?;
                    std::thread::sleep(std::time::Duration::from_secs(idle_seconds));
                    println!("[close-step] second hidden window remains alive");
                    if handle.get_webview_window("console").is_none() || console.is_visible().unwrap_or(true) {
                        return Err("Repeated tray close stopped the background console".into());
                    }
                    if handle.tray_by_id("pet-tray").is_none() {
                        return Err("Repeated tray close removed the menu bar entry".into());
                    }
                    #[cfg(target_os = "macos")]
                    verify_role_on_main_thread(&handle, false, (!no_pet).then_some(visible_pet))?;
                    if menu_quit {
                        println!("[close-step] requesting explicit menu bar quit while tray policy is saved");
                        handle.exit(0);
                        return Ok(());
                    }
                    #[cfg(target_os = "macos")]
                    if native_quit {
                        println!("PASS: ready for native explicit quit after both background checks");
                        handle.run_on_main_thread(|| {
                            use objc2_foundation::NSObjectNSThreadPerformAdditions;
                            let native = objc2_app_kit::NSApplication::sharedApplication(objc2::MainThreadMarker::new().unwrap());
                            let object: &objc2_foundation::NSObject = native.as_ref();
                            // AppKit must quit on the next outer run-loop turn,
                            // after Tauri releases this user-callback borrow.
                            unsafe { object.performSelectorOnMainThread_withObject_waitUntilDone(objc2::sel!(terminate:), None, false); }
                        }).map_err(|error| error.to_string())?;
                        return Ok(());
                    }
                    handle
                        .state::<ConsoleSettingsState>()
                        .save(ConsoleSettings {
                            close_action: CloseAction::Quit,
                            autostart: false,
                            autostart_minimized: false,
                        })?;
                    println!("[close-step] requesting explicitly configured quit");
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
    let exit_code = app.run_return(move |app, event| { console_settings::handle_run_event(app, &event); match event {
        tauri::RunEvent::ExitRequested { code, .. } => {
            println!("[close-event] ExitRequested code={code:?}");
            if code == Some(0) { exit_events.lock().unwrap().0 = true; }
        }
        tauri::RunEvent::Exit => {
            println!("[close-event] Exit");
            exit_events.lock().unwrap().1 = true;
        }
        _ => {}
    }});
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
        if menu_quit || (login_mode && !cfg!(target_os = "macos")) { CloseAction::Tray } else { CloseAction::Quit },
        "saved close policy did not survive reload"
    );
    if menu_quit {
        println!("PASS: explicit menu bar quit exits from background mode and preserves tray selection");
    } else if login_mode && !cfg!(target_os = "macos") {
        println!("PASS: login CloseRequested exits normally with a live pet window, preserving the saved tray preference");
    } else {
        println!("PASS: native console CloseRequested hides for tray, exits normally for quit, and persists selection in temporary storage");
    }
}
