# Upstream source record

- Project: https://github.com/farion1231/cc-switch
- Version: 3.20.3
- Commit: `d695a2d77fd9081eafd3e9eedcbf2a97b3410928`
- License: MIT; the original license is retained in the vendor distribution.
- Imported source: upstream `src-tauri/src` and Rust dependencies.

MollyCloud converts the desktop entry point into the `molly-ccswitch` Tauri plugin.
The host owns windows, lifecycle, tray, startup and URL registration. An explicit
console-only IPC allowlist and private path/configuration checks replace upstream
global profile discovery. Imports update the private provider database without
activation; CLI launch uses a separate explicit host action. Proxy ownership and
restore checks preserve external edits. History migration, external configuration
sync and independent lifecycle commands are disabled.

Validate changes with `cargo check --lib` and
`cargo test --lib embedded::tests::molly_private_import_is_db_only_and_idempotent`.
The targeted test uses temporary private storage. The retained upstream fixtures
assume the standalone application and must not run against real user profiles.
See [README.md](README.md) for host APIs and detailed boundaries.
