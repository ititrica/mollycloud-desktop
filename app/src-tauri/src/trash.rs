//! Platform-specific trash support.
#[cfg(target_os = "windows")]
#[path = "trash_windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "trash_macos.rs"]
mod platform;
pub use platform::*;
