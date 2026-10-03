//! Isolated macOS native Petra-window regression. No real pet or input injection.
//! cargo run --example screen_smoke --features screen-smoke
#[cfg(target_os = "macos")]
#[path = "../src/screen.rs"]
mod screen;

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
pub struct CursorPos {
    pub x: i32,
    pub y: i32,
    pub rx: i32,
    pub ry: i32,
    pub left: i32,
    pub top: i32,
}
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[cfg(target_os = "macos")]
fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    action: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(action());
    })
    .map_err(|e| e.to_string())?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn native_state(
    app: &tauri::AppHandle,
    win: &tauri::WebviewWindow,
) -> Result<(bool, isize, f64, bool), String> {
    let win = win.clone();
    on_main(app, move || {
        let pointer = win.ns_window().map_err(|e| e.to_string())?;
        // This closure runs on AppKit's main thread and the window stays alive.
        let native = unsafe { &*pointer.cast::<objc2_app_kit::NSWindow>() };
        Ok((
            native.ignoresMouseEvents(),
            native.level(),
            native.backingScaleFactor(),
            native.isOpaque(),
        ))
    })?
}

#[cfg(target_os = "macos")]
fn wait_native_state(
    app: &tauri::AppHandle,
    win: &tauri::WebviewWindow,
    test: impl Fn((bool, isize, f64, bool)) -> bool,
) -> Result<bool, String> {
    // Tauri dispatches a window request and AppKit then applies it on its main
    // queue. A separate run_on_main_thread callback is not a flush barrier.
    for _ in 0..100 {
        if test(native_state(app, win)?) {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn verify(app: &tauri::AppHandle, win: &tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let mut checks = Vec::new();
    let mut check = |valid: bool, name: &str| -> Result<(), String> {
        if !valid {
            return Err(name.to_owned());
        }
        checks.push(name.to_owned());
        Ok(())
    };
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    check(
        !native_state(app, win)?.3,
        "Transparent fixture uses a non-opaque NSWindow",
    )?;
    let snapshot_app = app.clone();
    let snapshot_window = win.clone();
    let (area, expected, cursor, cursor_expected) = on_main(app, move || {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSEvent, NSScreen};
        let screens = NSScreen::screens(MainThreadMarker::new().unwrap());
        let primary = screens.firstObject().unwrap();
        let primary_height = primary.frame().size.height;
        let visible = primary.visibleFrame();
        let frame = primary.frame();
        let center = (
            frame.origin.x + frame.size.width / 2.0,
            primary_height - frame.origin.y - frame.size.height / 2.0,
        );
        let area = screen::work_area_at(
            (center.0 * scale).round() as i32,
            (center.1 * scale).round() as i32,
        );
        let expected = [
            (visible.origin.x * scale).round() as i32,
            ((primary_height - visible.origin.y - visible.size.height) * scale).round() as i32,
            (visible.size.width * scale).round() as i32,
            (visible.size.height * scale).round() as i32,
        ];
        let cursor = screen::cursor_pos(&snapshot_app);
        let pointer = NSEvent::mouseLocation();
        let expected_cursor = (
            (pointer.x * scale).round() as i32,
            ((primary_height - pointer.y) * scale).round() as i32,
        );
        let position = snapshot_window.outer_position().unwrap();
        let size = snapshot_window.outer_size().unwrap();
        let cursor_expected = (expected_cursor, position, size);
        (area, expected, cursor, cursor_expected)
    })?;
    check(
        [area.left, area.top, area.width, area.height] == expected,
        "Work area matches NSScreen visibleFrame including Dock and menu bar",
    )?;
    check(
        area.width > 0 && area.height > 0,
        "Work area dimensions are positive",
    )?;
    check(
        (cursor.x - cursor_expected.0 .0).abs() <= 8
            && (cursor.y - cursor_expected.0 .1).abs() <= 8,
        "Cursor uses window backing scale and top-left desktop origin",
    )?;
    check(
        cursor.left == cursor_expected.1.x
            && cursor.top == cursor_expected.1.y
            && cursor.rx == cursor.x - cursor.left - cursor_expected.2.width as i32 / 2
            && cursor.ry == cursor.y - cursor.top - cursor_expected.2.height as i32 / 2,
        "Cursor offsets match the actual window center",
    )?;
    check(
        !win.is_resizable().map_err(|e| e.to_string())?,
        "Petra fixture remains non-resizable",
    )?;

    let target_size = (420, 360);
    screen::set_window_size(win, target_size.0, target_size.1);
    let mut size_matched = false;
    for _ in 0..100 {
        let size = win.inner_size().map_err(|e| e.to_string())?;
        if size.width == target_size.0 as u32 && size.height == target_size.1 as u32 {
            size_matched = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    check(
        { if !size_matched { eprintln!("resize actual={:?}",win.inner_size()); } size_matched },
        "Programmatic physical resize works with resizable false",
    )?;
    check(
        !win.is_resizable().map_err(|e| e.to_string())?,
        "Programmatic resize preserves non-resizable policy",
    )?;

    for ignore in [true, false, true, false] {
        screen::set_ignore_cursor(win, ignore);
        check(
            wait_native_state(app, win, |native| native.0 == ignore)?,
            "Click-through updates the real NSWindow property",
        )?;
        screen::set_ignore_cursor(win, ignore);
        check(
            native_state(app, win)?.0 == ignore,
            "Repeated click-through requests retain the applied state",
        )?;
    }
    for topmost in [true, false] {
        screen::set_topmost(win, topmost);
        check(
            wait_native_state(
                app,
                win,
                |native| if topmost { native.1 > 0 } else { native.1 == 0 },
            )?,
            "Always-on-top asynchronous update completes",
        )?;
        let level = native_state(app, win)?.1;
        check(
            if topmost { level > 0 } else { level == 0 },
            "Always-on-top changes the real NSWindow level",
        )?;
        check(
            screen::is_topmost(win) == topmost,
            "Always-on-top getter agrees with native state",
        )?;
    }

    let start = win.outer_position().map_err(|e| e.to_string())?;
    let target = (start.x + 100, start.y + 80);
    let mut reached = false;
    for _ in 0..200 {
        if screen::move_window_toward(win, target.0 as f64, target.1 as f64, 800.0, 0.016, true, None) {
            reached = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    let position = win.outer_position().map_err(|e| e.to_string())?;
    check(
        reached && (position.x - target.0).abs() <= 1 && (position.y - target.1).abs() <= 1,
        "Asynchronous smooth motion reaches the requested physical position",
    )?;
    check(
        screen::drag_offset(win).is_some() && screen::cursor_client_pos(win).is_some(),
        "Native drag and hit-test cursor coordinates are available",
    )?;
    // Drive the native drag follower with synthetic offsets, without moving the
    // user's pointer. Transparent padding must be allowed beyond every edge.
    let bounds = (100, 80, 320, 300);
    for (x, y) in [
        (area.left - bounds.0, area.top - bounds.1),
        (area.left + area.width - bounds.2, area.top - bounds.1),
        (area.left - bounds.0, area.top + area.height - bounds.3),
        (area.left + area.width - bounds.2, area.top + area.height - bounds.3),
    ] {
        let cursor = screen::cursor_pos(app);
        screen::drag_follow(win, cursor.x - x, cursor.y - y, None, Some(bounds), scale);
        let mut matched = false;
        for _ in 0..100 {
            let position = win.outer_position().map_err(|error| error.to_string())?;
            if (position.x - x).abs() <= 1 && (position.y - y).abs() <= 1 {
                matched = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !matched { eprintln!("corner requested=({x},{y}) actual={:?} scale={scale}", win.outer_position()); }
        check(matched, "Native drag reaches a work-area corner while transparent padding is offscreen")?;
    }
    let native_scale = native_state(app, win)?.2;
    let control_owner = app.clone();
    let standard_constraint = on_main(app, move || {
        let control = tauri::WebviewWindowBuilder::new(&control_owner, "isolated-console", tauri::WebviewUrl::External("about:blank".parse().unwrap()))
            .visible(false).incognito(true).inner_size(200.0, 180.0).build().unwrap();
        let native = unsafe { &*control.ns_window().unwrap().cast::<objc2_app_kit::NSWindow>() };
        let mut requested = native.frame();
        let primary = objc2_app_kit::NSScreen::screens(objc2::MainThreadMarker::new().unwrap()).firstObject().unwrap();
        requested.origin.y = primary.frame().size.height + 200.0;
        let constrained = native.constrainFrameRect_toScreen(requested, Some(&primary));
        control.destroy().unwrap();
        constrained.origin.y < requested.origin.y
    })?;
    check(standard_constraint, "Unregistered console windows retain AppKit's standard screen constraint")?;
    check(
        (native_scale - scale).abs() < 0.001,
        "Tauri physical scale agrees with the NSWindow backing scale",
    )?;
    Ok(
        json!({"ok":true,"checks":checks,"scaleFactor":scale,"workArea":{"left":area.left,"top":area.top,"width":area.width,"height":area.height},"size":{"width":target_size.0,"height":target_size.1},"motionTarget":{"x":target.0,"y":target.1},"retinaDisplayPresent":scale>1.0}),
    )
}

#[cfg(target_os = "macos")]
fn main() {
    use std::sync::{Arc, Mutex};
    let temporary = tempfile::tempdir().unwrap();
    let profile = temporary.path().join("webview");
    let result = Arc::new(Mutex::new(None));
    let output = result.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.screen-smoke".into();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .setup(move |app| {
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .title("MollyCloud isolated Petra-window verification")
            .position(160.0, 160.0)
            .inner_size(300.0, 300.0)
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .visible(true)
            .focused(false)
            .incognito(true)
            .data_directory(profile)
            .build()?;
            screen::initialize(app.handle());
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let outcome = verify(&handle, &window)
                    .unwrap_or_else(|error| serde_json::json!({"ok":false,"error":error}));
                let success = outcome["ok"] == true;
                *result.lock().unwrap() = Some(outcome);
                handle.exit(if success { 0 } else { 1 });
            });
            let watchdog = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(15));
                watchdog.exit(1);
            });
            Ok(())
        })
        .build(context)
        .unwrap();
    let status = app.run_return(|_, _| {});
    let outcome = output.lock().unwrap().clone().unwrap_or_else(
        || serde_json::json!({"ok":false,"error":"native screen watchdog timeout"}),
    );
    println!("{outcome}");
    if status != 0 || outcome["ok"] != true {
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This isolated native screen regression runs on macOS.");
}
