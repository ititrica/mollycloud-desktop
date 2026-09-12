# MollyCloud embedded CC Switch backend

Source: CC Switch 3.20.3, MIT, upstream commit `d695a2d77fd9081eafd3e9eedcbf2a97b3410928`.
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
  receives the private home and supplies documented CLI-specific child environment
  variables. The library never changes process HOME, USERPROFILE or CODEX_HOME.
- `cli_private_home()` checks and returns the private home without activating a
  provider, so the host can validate the working directory before activation.
- `test-hooks` exposes `init_for_test(app_data_root)` for an isolated native smoke
  process. It uses the same setup and adds the `ccswitch` child directory.

## Optional Gemini OAuth refresh

The Gemini usage refresh path reads `MOLLY_GEMINI_OAUTH_CLIENT_ID` and
`MOLLY_GEMINI_OAUTH_CLIENT_SECRET` from the process environment. They are never
embedded in source or release binaries. When either variable is absent, only
refreshing an expired Gemini access token is skipped; the rest of the embedded
module remains available.

## Boundaries

All persistent files live beneath `<Molly app_data>/ccswitch`: `data` (SQLite,
OAuth state and backup ledger), `home` (CLI configuration), `cache`, and `logs`.
Path errors fail closed. Managed configuration and backup directories reject
symlinks and Windows reparse points. IPC checks exclude CLI workspaces and session
histories; exact file reads, writes and copies still validate their target paths.
CLI history migrations are disabled.
Windows MSIX launchers may virtualize an AppData child while its parent resolves
to the normal Roaming directory. Initialization rejects links/reparse points at
the requested root before canonicalizing it, then uses the actual storage location
as the boundary for every descendant. It does not hard-code a launcher package
path or fall back to the standalone CC Switch profile. IO errors retain the failed
path and OS error, and partial initialization does not publish the private root.
Configuration overrides cannot change the prescribed private CLI directories.
Claude MCP lives in `home/.claude/.claude.json`, matching the explicit
`CLAUDE_CONFIG_DIR=home/.claude` used by the host launcher.

`src/allowed_commands.rs` is the explicit IPC allowlist. Provider management,
private MCP/prompts, local proxy, pricing/usage and managed authentication are
available. Standalone lifecycle operations, global environment mutation, arbitrary
terminal commands, external file import/export, cloud configuration synchronization,
and skill installation/migration are not exposed. Their upstream source remains
for future audited adaptation; retaining source does not grant IPC permission.

Provider creation and Molly imports do not auto-activate the first provider.
Activation writes private configuration. Configuration content cannot override
home/config environment variables or point Codex SQLite/logs outside private storage;
Codex credential stores are forced to private files. Model catalogs use bundled
templates and do not run a globally installed CLI during provider activation.

The proxy defaults to `127.0.0.1:24327`, never kills a process holding that port,
and uses `MOLLY_CCSWITCH_PROXY_MANAGED`. A hash ledger tracks each exact file,
including Codex auth, config and generated model catalog. Initial takeover also
records files it preserves; subsequent writes update only the file written.
Shutdown and crash recovery restore private proxy files only if the ownership
marker and recorded bytes still match. External edits are retained and reported
instead of overwritten.

## Validation

Run the adapted isolation regression without running upstream environment-mutating
fixtures (the latter target the standalone app):

```powershell
cargo check --lib
cargo test --lib embedded::tests
```

This regression exercises temporary directories, DB-only first/repeat imports,
the `/v1` endpoint, blocked configuration escapes, private proxy restoration,
preservation of external edits/markers and occupied proxy ports. It also
checks a disposable Roaming child under the current launcher, rejection of root
junctions, and useful errors when a file occupies the private directory. The host also
has a separate native WebView2 smoke test covering IPC
and packaged assets. Upstream tests were retained as reference; their standalone
HOME override assumptions do not apply to this embedded library.
