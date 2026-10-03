//! Platform-specific audio support.
#[cfg(target_os = "windows")]
#[path = "audio_windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "audio_macos.rs"]
mod platform;
pub use platform::*;
