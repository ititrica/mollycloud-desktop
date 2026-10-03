//! Platform-specific screen support.
#[cfg(target_os = "windows")]
#[path = "screen_windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "screen_macos.rs"]
mod platform;
pub use platform::*;
