# Upstream source record

- Project: https://github.com/farion1231/cc-switch
- Version: 3.20.4 (`v3.20.4`)
- Commit: `43e1d99084ed9b2f5dc252fd35c5adaf29d6876e`
- License: MIT; the original license is retained in the vendor distribution.
- Imported source: upstream `src-tauri/src` and Rust dependencies.

MollyCloud converts the desktop entry point into the `molly-ccswitch` Tauri plugin.
The host owns windows, lifecycle, tray, startup and URL registration. An explicit
console-only IPC allowlist and path checks keep manager storage private while
explicit tool operations manage the user's actual CLI configuration. Molly API key
imports update the private provider database without activation; CLI launch uses a
separate explicit host action. Proxy ownership and restore checks preserve external
edits. Independent lifecycle commands and upstream startup migrations stay disabled.
Tool configuration, MCP, Skills, sessions, user-controlled environment changes,
backup/import/export and configured cloud sync retain the adapted upstream behavior.

Validate changes with `cargo check --lib` and
`cargo test --lib embedded::tests -- --test-threads=1` and
`cargo test --lib database::schema::tests::migrate_v18_to_v19_preserves_existing_molly_data`.
The targeted tests use temporary user/manager storage or in-memory databases. The retained upstream fixtures
assume the standalone application and must not run against real user profiles.
See [README.md](README.md) for host APIs and detailed boundaries.


2026-10-02：工作台由 Molly 功能插件目录管理。必须保留受控安装状态、原生命令门禁、主题/滚动条与既有嵌入适配。独立资源包发布及原生接口版本约束见仓库 `docs/功能插件方案.md`。
