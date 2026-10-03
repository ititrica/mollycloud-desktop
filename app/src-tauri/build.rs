fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=native/audio_capture.swift");
        let architecture = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64",
            Ok("x86_64") => "x86_64",
            _ => panic!("unsupported macOS architecture"),
        };
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("molly-audio-capture");
        let status = std::process::Command::new("xcrun")
            .args(["swiftc", "-swift-version", "5", "-O", "-target"])
            .arg(format!("{architecture}-apple-macos13.0"))
            .arg("native/audio_capture.swift")
            .arg("-o")
            .arg(output)
            .status()
            .expect("macOS build requires Xcode Command Line Tools with Swift");
        assert!(
            status.success(),
            "failed to compile macOS system audio helper"
        );
    }
    // Let the linker supply one manifest for every executable target. Tauri's
    // default resource manifest would duplicate it in the main Windows binary;
    // icons and version information still come from Tauri's resource builder.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to build MollyCloud resources");
    // Native file dialogs require Common Controls v6 before rfd's
    // TaskDialogIndirect import loads, including examples and unit-test hosts.
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
