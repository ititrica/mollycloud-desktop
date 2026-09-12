# CC Switch frontend in MollyCloud

Source: https://github.com/farion1231/cc-switch

Pinned upstream: `d695a2d77fd9081eafd3e9eedcbf2a97b3410928` (3.20.3).

The React source, styles, component library, icons and translations are copied from that commit. Copyright (c) 2025 Jason Young, MIT; see `LICENSE` and the in-app About section. The original React UI is built independently and embedded inside the Vue console using a same-origin iframe. Its CSS never enters the parent document.

```powershell
npm --prefix vendor/cc-switch/frontend ci
npm --prefix vendor/cc-switch/frontend run test:embedded
npm --prefix vendor/cc-switch/frontend run build
```

`build` includes TypeScript checks and outputs into `app/public/ccswitch`. Molly's normal production build includes that directory. There is no separate frontend development server or desktop launcher.

`src/embedded/bootstrap.ts` sets up the parent Tauri bridge before loading the upstream React entry. Local and session storage use the `mollycloud:ccswitch:` prefix. Upstream application commands are namespaced as `plugin:molly-ccswitch|COMMAND`. The deliberate host mappings are `open_provider_terminal` → `launch_ccswitch_cli` (selected app, provider ID and optional working directory; Rust reads private credentials) and `open_external` → `open_url` (HTTP/HTTPS URLs only). System deep links, file URLs and JavaScript URLs are rejected.

Tauri injects readonly globals into native iframes, so the embedded adapters never replace `__TAURI_INTERNALS__`, `__TAURI_EVENT_PLUGIN_INTERNALS__` or `isTauri`. `native-bridge.ts` explicitly resolves the same-origin parent's IPC and callback registry. Separate `core`, `event`, `app` and `window` Vite aliases prevent upstream relative `./core.js` imports from using the child registry. Event listeners register and clean up callbacks in the parent; failed subscriptions clean up too. `app.getVersion()` reports the pinned embedded CC Switch version. Browser preview uses a module-local mock, without mutating these globals. Regression tests use non-writable, non-configurable globals on both synthetic windows.

Windows, exit/restart, upstream updating, external deep links and onboarding, global environment management, editable config roots, external file/cloud imports, session browsing and Skills modification are unavailable in embedded mode. Available tool tabs are Claude Code, Codex and Gemini. Their settings point to Molly's private roots; only the explicit private CLI launch uses those roots. The Rust command allowlist remains the final enforcement boundary.

The parent/child message protocol uses exact origin and source checks:

- Child → parent: `{ source: "molly-ccswitch", type: "ready" }` after the listener mounts.
- Child → parent: `{ source: "molly-ccswitch", type: "load-error" }` after rendering an initialization error; the parent must uncover the iframe's error instead of leaving a loading mask over it.
- Parent → child: `{ source: "mollycloud", type: "navigate" | "provider-imported", app?: "codex" | "claude" | "gemini", providerId?: string }`.
- Messages contain no credentials. Import navigation refreshes queries and highlights the matching provider after it renders.

Read-only UI preview requires both parent `ui-preview=console` and child `ui-preview=ccswitch`, a same-origin iframe, and absence of a real Tauri bridge. Preview shows labeled sample data and rejects every data mutation. It is never a fallback for failed native commands.

When updating upstream, audit new commands and lifecycle imports before adding them. Preserve the separate build, storage namespace and no-system-config defaults; do not replace the adapters with upstream `run()` or a `ccswitch://` link.
