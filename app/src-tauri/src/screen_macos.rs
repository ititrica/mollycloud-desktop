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
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if app.run_on_main_thread(refresh_screens).is_err() {
            break;
        }
    });
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
) -> bool {
    remember_scale(win);
    if win.is_visible().ok() == Some(false) {
        return true;
    }
    let Ok(position) = win.outer_position() else {
        return false;
    };
    let dx = tx - position.x as f64;
    let dy = ty - position.y as f64;
    let distance = dx.hypot(dy);
    if distance < 1.0 {
        return true;
    }
    let step = (max_speed.max(0.0) * dt.max(0.0)).min(distance);
    let mut nx = (position.x as f64 + dx / distance * step).round() as i32;
    let mut ny = (position.y as f64 + dy / distance * step).round() as i32;
    if clamp {
        if let Ok(size) = win.outer_size() {
            let area = work_area_at(nx, ny);
            nx = nx.clamp(
                area.left + 4,
                (area.left + area.width - size.width as i32 - 4).max(area.left + 4),
            );
            ny = ny.clamp(
                area.top + 4,
                (area.top + area.height - size.height as i32 - 4).max(area.top + 4),
            );
        }
    }
    let _ = win.set_position(PhysicalPosition::new(nx, ny));
    false
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
    let mut nx = cursor.x.round() as i32 - off_x;
    let mut ny = locked_y.unwrap_or(cursor.y.round() as i32 - off_y);
    if let (Some((bl, bt, br, bb)), Ok(size)) = (model_bounds, win.outer_size()) {
        let area = work_area_at(nx + size.width as i32 / 2, ny + size.height as i32 / 2);
        nx = nx.max(area.left - bl).min(area.left + area.width - br);
        if locked_y.is_none() {
            ny = ny.max(area.top - 60 - bt).min(area.top + area.height - bb);
        }
    }
    let _ = win.set_position(PhysicalPosition::new(nx, ny));
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
