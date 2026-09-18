#[path = "src/allowed_commands.rs"]
mod allowed_commands;
fn main() {
    tauri_plugin::Builder::new(allowed_commands::ALLOWED_COMMANDS).build();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Native file dialogs require Common Controls v6 in test hosts too.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
