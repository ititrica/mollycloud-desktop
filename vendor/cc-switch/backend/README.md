# MollyCloud embedded CC Switch backend

Source: CC Switch 3.20.4, MIT, upstream commit `43e1d99084ed9b2f5dc252fd35c5adaf29d6876e`.
This library has no executable entry point. MollyCloud owns its process, windows,
tray, updater, startup registration and URL associations.
Initialization errors disable only this optional module; the pet and Molly console
remain available. `get_init_error` stays callable while all other module IPC fails
closed until initialization succeeds.

## Host API

- Register `molly_ccswitch::init()` and grant `molly-ccswitch:default` only to the
  `console` window. The command gateway independently checks that window label.
- Call `import_molly_provider(app, MollyProviderImport)` from Rust. Import upserts
  a stable account/key identity into SQLite only and returns the provider ID.
- Call `prepare_cli_launch(app, app_type, provider_id)` only following an explicit
  launch action. Supported launch targets are Claude, Codex and Gemini. The host
  resolves the configured real tool directory and supplies CLI-specific child
  environment variables. The library never changes process HOME, USERPROFILE or CODEX_HOME.
- `cli_system_home()` checks and returns the system user home without activating a
  provider, so the host can validate the working directory before activation.
- `test-hooks` exposes `init_for_test(app_data_root, system_home)` for an isolated
  native smoke process with disposable manager and CLI directories.

## Optional Gemini OAuth refresh

The Gemini usage refresh path reads `MOLLY_GEMINI_OAUTH_CLIENT_ID` and
`MOLLY_GEMINI_OAUTH_CLIENT_SECRET` from the process environment. They are never
embedded in source or release binaries. When either variable is absent, only
refreshing an expired Gemini access token is skipped; the rest of the embedded
module remains available.

## Boundaries

Manager files live beneath `<Molly app_data>/ccswitch`: `data` (SQLite,
OAuth state and backup ledger), `home/.cc-switch` (device settings), `cache`, and
`logs`. User-authorized tool operations use real CLI directories and supported
path overrides. The standalone manager's default data directory is never a valid
tool target, including through aliases. Path errors fail closed. Private configuration and backup directories reject
symlinks and Windows reparse points. IPC checks exclude CLI workspaces and session
histories; exact file reads, writes and copies still validate their target paths.
CLI history migrations are disabled.
Windows MSIX launchers may virtualize an AppData child while its parent resolves
to the normal Roaming directory. Initialization rejects links/reparse points at
the requested root before canonicalizing it, then uses the actual storage location
as the boundary for every descendant. It does not hard-code a launcher package
path or fall back to the standalone CC Switch profile. IO errors retain the failed
path and OS error, and partial initialization does not publish the private root.
Claude MCP defaults to the user's `.claude.json`, or `.claude.json` inside an explicit
Claude directory. MiniMax Code uses `.minimax`, `MINIMAX_DATA_DIR`, or legacy
`MAVIS_DATA_DIR`. Its YAML and MCP writes validate paths before locking or writing,
preserve native account/default-model settings, and respect the native tool lock.
Test hooks ignore inherited CLI directory overrides.

`src/allowed_commands.rs` is the explicit IPC allowlist. Provider management,
MCP/prompts, Skills, sessions, environment conflict management, local proxy,
pricing/usage, managed authentication, file import/export and configured cloud
sync are available. The user controls writes through the relevant UI. Standalone
lifecycle operations and arbitrary terminal commands are not exposed.

Molly imports only upsert the private provider database. Explicit activation writes
the selected tool's real configuration. Native path and credential-store policies
are preserved. Model catalogs use bundled
templates and do not run a globally installed CLI during provider activation.

The proxy defaults to `127.0.0.1:24327`, never kills a process holding that port,
and uses `MOLLY_CCSWITCH_PROXY_MANAGED`. A hash ledger tracks each exact file,
including Codex auth, config and generated model catalog. Initial takeover also
records files it preserves; subsequent writes update only the file written.
Shutdown and crash recovery restore tool proxy files only if the ownership
marker and recorded bytes still match. External edits are retained and reported
instead of overwritten.

## Validation

Run the adapted isolation regression without running upstream environment-mutating
fixtures (the latter target the standalone app):

```powershell
cargo check --lib
cargo test --lib embedded::tests -- --test-threads=1
cargo test --lib database::schema::tests::migrate_v18_to_v19_preserves_existing_molly_data
```

This regression exercises temporary directories, DB-only first/repeat imports,
the `/v1` endpoint, standalone manager protection, proxy restoration,
preservation of external edits/markers and occupied proxy ports. It also
checks a disposable Roaming child under the current launcher, rejection of root
junctions, and useful errors when a file occupies the private directory. The host also
has a separate native WebView2 smoke test covering IPC
and packaged assets. Upstream tests were retained as reference; their standalone
HOME override assumptions do not apply to this embedded library. The upstream
standalone-startup fixture is excluded because its launcher does not exist here.
The v18→v19 schema regression checks existing provider activation, MCP/Skills flags
and Molly's one-time migration marker. MiniMax regression covers DB-only import,
native locking, additive changes, default-model protection and MCP preservation.
