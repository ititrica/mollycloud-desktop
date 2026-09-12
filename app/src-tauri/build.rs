fn main() {
    // Let the linker supply one manifest for every executable target. Tauri's
    // default resource manifest would duplicate it in the main Windows binary;
    // icons and version information still come from Tauri's resource builder.
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new_without_app_manifest(),
    ))
    .expect("failed to build MollyCloud resources");
    // Native file dialogs require Common Controls v6 before rfd's
    // TaskDialogIndirect import loads, including examples and unit-test hosts.
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
