//! Petra coordinates use Tauri physical desktop pixels. AppKit screen frames are
//! converted with the *window's* scale, including mixed-DPI external displays.
use crate::{CursorPos, WorkArea};
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use std::sync::{
    atomic::{AtomicI8, AtomicU64, Ordering},
    OnceLock, RwLock,
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

#[derive(Clone, Copy)]
struct ScreenFrame {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    work: [f64; 4],
}
static SCREENS: OnceLock<RwLock<Vec<ScreenFrame>>> = OnceLock::new();
static WINDOW_SCALE: AtomicU64 = AtomicU64::new(1.0f64.to_bits());
static PRIMARY_SCALE: AtomicU64 = AtomicU64::new(1.0f64.to_bits());
static IGNORE_CURSOR: AtomicI8 = AtomicI8::new(-1);

fn remember_scale(win: &WebviewWindow) -> f64 {
    let scale = win.scale_factor().unwrap_or(1.0).max(1.0);
    WINDOW_SCALE.store(scale.to_bits(), Ordering::Relaxed);
    scale
}

fn refresh_screens() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.firstObject() else {
        return;
    };
    PRIMARY_SCALE.store(primary.backingScaleFactor().to_bits(), Ordering::Relaxed);
    let desktop_top = primary.frame().size.height;
    let frames = screens
        .iter()
        .map(|screen| {
            let frame = screen.frame();
            let work = screen.visibleFrame();
            ScreenFrame {
                left: frame.origin.x,
                top: desktop_top - frame.origin.y - frame.size.height,
                width: frame.size.width,
                height: frame.size.height,
                work: [
                    work.origin.x,
                    desktop_top - work.origin.y - work.size.height,
                    work.size.width,
                    work.size.height,
                ],
            }
        })
        .collect();
    if let Ok(mut current) = SCREENS.get_or_init(|| RwLock::new(Vec::new())).write() {
        *current = frames;
    }
}

/// Called once from Tauri setup. AppKit is accessed exclusively on the UI thread;
/// motion/hit-test workers read a small cached snapshot and never block that thread.
pub fn initialize(app: &AppHandle) {
    IGNORE_CURSOR.store(-1, Ordering::Relaxed);
    refresh_screens();
    if let Some(win) = app.get_webview_window("main") {
        remember_scale(&win);
        configure_pet_window(&win);
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if app.run_on_main_thread(refresh_screens).is_err() {
            break;
        }
    });
}

// Only registered Petra instances bypass the native origin constraint.
// Every other Tao window retains AppKit's normal placement and resize rules.
static PET_WINDOWS: OnceLock<std::sync::Mutex<std::collections::HashSet<usize>>> = OnceLock::new();

pub fn configure_pet_window(win: &WebviewWindow) {
    if win.label() != "main" { return; }
    remember_scale(win);
    let window = win.clone();
    let configure = move || {
        use objc2::{runtime::{AnyObject, Sel}, sel, msg_send, ClassType};
        use objc2_foundation::NSRect;
        use objc2_app_kit::NSWindow;
        static INSTALLED: OnceLock<()> = OnceLock::new();
        extern "C-unwind" fn constrain(this: &AnyObject, _: Sel, frame: NSRect, screen: Option<&NSScreen>) -> NSRect {
            let mut constrained: NSRect = unsafe { msg_send![super(this, NSWindow::class()), constrainFrameRect: frame, toScreen: screen] };
            let pet = PET_WINDOWS.get().is_some_and(|windows| windows.lock().unwrap().contains(&(this as *const AnyObject as usize)));
            if pet { constrained.origin = frame.origin; }
            constrained
        }
        let Ok(pointer) = window.ns_window() else { return };
        let object = unsafe { &*pointer.cast::<AnyObject>() };
        INSTALLED.get_or_init(|| {
            let class = object.class();
            let selector = sel!(constrainFrameRect:toScreen:);
            let method = class.instance_method(selector).expect("NSWindow frame constraint");
            // Exact AppKit method signature; the override is added to TaoWindow,
            // preserving NSWindow and every unregistered console/payment window.
            unsafe {
                let implementation = std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, NSRect, Option<&NSScreen>) -> NSRect, objc2::runtime::Imp>(constrain);
                assert!(objc2::ffi::class_addMethod(class as *const _ as *mut _, selector, implementation, objc2::ffi::method_getTypeEncoding(method as *const _ as *mut _)).as_bool());
            }
        });
        PET_WINDOWS.get_or_init(Default::default).lock().unwrap().insert(pointer as usize);
    };
    if MainThreadMarker::new().is_some() { configure(); }
    else { let _ = win.run_on_main_thread(configure); }
}

pub fn unregister_pet_window(win: &WebviewWindow) {
    if let (Some(windows), Ok(pointer)) = (PET_WINDOWS.get(), win.ns_window()) {
        windows.lock().unwrap().remove(&(pointer as usize));
    }
}

pub fn cursor_pos(app: &AppHandle) -> CursorPos {
    let mut result = CursorPos {
        x: 0,
        y: 0,
        rx: 0,
        ry: 0,
        left: 0,
        top: 0,
    };
    if let Some(win) = app.get_webview_window("main") {
        remember_scale(&win);
        if let Some(cursor) = physical_cursor(&win) {
            result.x = cursor.x.round() as i32;
            result.y = cursor.y.round() as i32;
        }
        if let (Ok(position), Ok(size)) = (win.outer_position(), win.outer_size()) {
            result.left = position.x;
            result.top = position.y;
            result.rx = result.x - position.x - size.width as i32 / 2;
            result.ry = result.y - position.y - size.height as i32 / 2;
        }
    }
    result
}

fn physical_cursor(win: &WebviewWindow) -> Option<PhysicalPosition<f64>> {
    let cursor = win.cursor_position().ok()?;
    // Tao returns desktop cursor pixels in the primary monitor's scale, while
    // outer_position and window dimensions use this window's scale. Convert
    // here so a 1x external screen and a 2x Retina screen do not diverge.
    let primary = f64::from_bits(PRIMARY_SCALE.load(Ordering::Relaxed));
    let ratio = remember_scale(win) / primary.max(1.0);
    Some(PhysicalPosition::new(cursor.x * ratio, cursor.y * ratio))
}

pub fn invalidate_ignore_cursor_cache() {
    IGNORE_CURSOR.store(-1, Ordering::Relaxed);
}

fn distance_to_frame(screen: &ScreenFrame, x: f64, y: f64) -> f64 {
    let dx = (screen.left - x)
        .max(0.0)
        .max(x - screen.left - screen.width);
    let dy = (screen.top - y)
        .max(0.0)
        .max(y - screen.top - screen.height);
    dx * dx + dy * dy
}

pub fn work_area_at(x: i32, y: i32) -> WorkArea {
    let scale = f64::from_bits(WINDOW_SCALE.load(Ordering::Relaxed));
    let screens = SCREENS.get_or_init(|| RwLock::new(Vec::new()));
    if let Ok(screens) = screens.read() {
        if let Some(screen) =
            screens.iter().min_by(|a, b| {
                distance_to_frame(a, x as f64 / scale, y as f64 / scale)
                    .total_cmp(&distance_to_frame(b, x as f64 / scale, y as f64 / scale))
            })
        {
            return WorkArea {
                left: (screen.work[0] * scale).round() as i32,
                top: (screen.work[1] * scale).round() as i32,
                width: (screen.work[2] * scale).round() as i32,
                height: (screen.work[3] * scale).round() as i32,
            };
        }
    }
    WorkArea {
        left: 0,
        top: 0,
        width: 1440,
        height: 900,
    }
}

pub fn set_ignore_cursor(win: &WebviewWindow, ignore: bool) {
    let target = i8::from(ignore);
    if win.label() == "main" && IGNORE_CURSOR.load(Ordering::Relaxed) == target {
        return;
    }
    if win.set_ignore_cursor_events(ignore).is_ok() && win.label() == "main" {
        IGNORE_CURSOR.store(target, Ordering::Relaxed);
    }
}
pub fn is_topmost(win: &WebviewWindow) -> bool {
    win.is_always_on_top().unwrap_or(false)
}
pub fn set_topmost(win: &WebviewWindow, on: bool) {
    let _ = win.set_always_on_top(on);
}

pub fn move_window_toward(
    win: &WebviewWindow,
    tx: f64,
    ty: f64,
    max_speed: f64,
    dt: f64,
    clamp: bool,
    model_bounds: Option<(i32, i32, i32, i32)>,
) -> bool {
    remember_scale(win);
    if win.is_visible().ok() == Some(false) {
        return true;
    }
    let Ok(position) = win.outer_position() else {
        return false;
    };
    let (tx, ty) = if clamp {
        clamp_model_position(win, tx.round() as i32, ty.round() as i32, model_bounds, false)
    } else { (tx.round() as i32, ty.round() as i32) };
    let dx = tx as f64 - position.x as f64;
    let dy = ty as f64 - position.y as f64;
    let distance = dx.hypot(dy);
    if distance < 1.0 {
        return true;
    }
    let step = (max_speed.max(0.0) * dt.max(0.0)).min(distance);
    let nx = (position.x as f64 + dx / distance * step).round() as i32;
    let ny = (position.y as f64 + dy / distance * step).round() as i32;
    set_pet_position(win, nx, ny);
    false
}

fn set_pet_position(win: &WebviewWindow, x: i32, y: i32) {
    let window = win.clone();
    // setFrameTopLeftPoint constrains even a borderless window below the menu
    // bar. Set its frame directly so only the visible character is constrained.
    let _ = win.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(primary) = NSScreen::screens(mtm).firstObject() else { return };
        let Ok(pointer) = window.ns_window() else { return };
        // The window is retained by this closure and AppKit runs on its UI thread.
        let native = unsafe { &*pointer.cast::<objc2_app_kit::NSWindow>() };
        let scale = native.backingScaleFactor().max(1.0);
        let mut frame = native.frame();
        frame.origin.x = x as f64 / scale;
        frame.origin.y = primary.frame().size.height - y as f64 / scale - frame.size.height;
        native.setFrame_display(frame, true);
    });
}

fn clamp_to_model(area: &WorkArea, x: i32, y: i32, bounds: (i32, i32, i32, i32), locked_y: bool) -> (i32, i32) {
    let (left, top, right, bottom) = bounds;
    let min_x = area.left - left;
    let max_x = (area.left + area.width - right).max(min_x);
    let min_y = area.top - top;
    let max_y = (area.top + area.height - bottom).max(min_y);
    (x.clamp(min_x, max_x), if locked_y { y } else { y.clamp(min_y, max_y) })
}

fn clamp_model_position(win: &WebviewWindow, x: i32, y: i32, bounds: Option<(i32, i32, i32, i32)>, locked_y: bool) -> (i32, i32) {
    let Ok(size) = win.outer_size() else { return (x, y) };
    let bounds = bounds.unwrap_or((0, 0, size.width as i32, size.height as i32));
    // The grab point can cross to another display while the large transparent
    // window's center is still on the previous one. Select using the character.
    let area = work_area_at(x + (bounds.0 + bounds.2) / 2, y + (bounds.1 + bounds.3) / 2);
    clamp_to_model(&area, x, y, bounds, locked_y)
}

pub fn drag_offset(win: &WebviewWindow) -> Option<(i32, i32)> {
    remember_scale(win);
    let cursor = physical_cursor(win)?;
    let position = win.outer_position().ok()?;
    Some((
        cursor.x.round() as i32 - position.x,
        cursor.y.round() as i32 - position.y,
    ))
}

pub fn drag_follow(
    win: &WebviewWindow,
    off_x: i32,
    off_y: i32,
    locked_y: Option<i32>,
    model_bounds: Option<(i32, i32, i32, i32)>,
    _scale: f64,
) {
    remember_scale(win);
    if win.is_visible().ok() == Some(false) {
        return;
    }
    let Some(cursor) = physical_cursor(win) else {
        return;
    };
    let nx = cursor.x.round() as i32 - off_x;
    let ny = locked_y.unwrap_or(cursor.y.round() as i32 - off_y);
    // Drag bounds arrive in physical pixels; never apply backing scale twice.
    let (nx, ny) = clamp_model_position(win, nx, ny, model_bounds, locked_y.is_some());
    set_pet_position(win, nx, ny);
}

pub fn set_window_size(win: &WebviewWindow, width: i32, height: i32) {
    if width > 0 && height > 0 {
        let _ = win.set_size(PhysicalSize::new(width as u32, height as u32));
    }
}

pub fn cursor_client_pos(win: &WebviewWindow) -> Option<(i32, i32)> {
    remember_scale(win);
    let cursor = physical_cursor(win)?;
    let position = win.inner_position().ok()?;
    Some((
        cursor.x.round() as i32 - position.x,
        cursor.y.round() as i32 - position.y,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn character_can_reach_all_work_area_edges_at_each_scale() {
        for scale in [1, 2] {
            let area = WorkArea { left: -1920 * scale, top: 30 * scale, width: 1920 * scale, height: 1050 * scale };
            let bounds = (200 * scale, 100 * scale, 500 * scale, 600 * scale);
            let (x, y) = clamp_to_model(&area, -10000, -10000, bounds, false);
            assert_eq!((x + bounds.0, y + bounds.1), (area.left, area.top));
            let (x, y) = clamp_to_model(&area, 10000, 10000, bounds, false);
            assert_eq!((x + bounds.2, y + bounds.3), (area.left + area.width, area.top + area.height));
            assert_eq!(clamp_to_model(&area, 10000, -900, bounds, true).1, -900);
        }
    }
    #[test]
    fn nearest_screen_handles_negative_desktop_coordinates() {
        let screen = ScreenFrame {
            left: -1920.0,
            top: 100.0,
            width: 1920.0,
            height: 1080.0,
            work: [-1920.0, 125.0, 1920.0, 1000.0],
        };
        assert_eq!(distance_to_frame(&screen, -1000.0, 500.0), 0.0);
        assert_eq!(distance_to_frame(&screen, 20.0, 500.0), 400.0);
    }
}
