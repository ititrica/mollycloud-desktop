# CC Switch frontend in MollyCloud

Source: https://github.com/farion1231/cc-switch

Pinned upstream: `43e1d99084ed9b2f5dc252fd35c5adaf29d6876e` (3.20.4, tag `v3.20.4`).

The React source, styles, component library, icons and translations are copied from that commit. Copyright (c) 2025 Jason Young, MIT; see `LICENSE` and the in-app About section. The original React UI is built independently and embedded inside the Vue console using a same-origin iframe. Its CSS never enters the parent document.

```powershell
npm --prefix vendor/cc-switch/frontend ci
npm --prefix vendor/cc-switch/frontend run test:embedded
npm --prefix vendor/cc-switch/frontend run build
```

`build` includes TypeScript checks and outputs into `app/public/ccswitch`. Molly's normal production build includes that directory. There is no separate frontend development server or desktop launcher.

`src/embedded/bootstrap.ts` sets up the parent Tauri bridge before loading the upstream React entry. Local and session storage use the `mollycloud:ccswitch:` prefix. Upstream application commands are namespaced as `plugin:molly-ccswitch|COMMAND`. The deliberate host mappings are `open_provider_terminal` → `launch_ccswitch_cli` (selected app, provider ID and optional working directory; Rust reads private credentials) and `open_external` → `open_url` (HTTP/HTTPS URLs only). System deep links, file URLs and JavaScript URLs are rejected.

Tauri injects readonly globals into native iframes, so the embedded adapters never replace `__TAURI_INTERNALS__`, `__TAURI_EVENT_PLUGIN_INTERNALS__` or `isTauri`. `native-bridge.ts` explicitly resolves the same-origin parent's IPC and callback registry. Separate `core`, `event`, `app` and `window` Vite aliases prevent upstream relative `./core.js` imports from using the child registry. Event listeners register and clean up callbacks in the parent; failed subscriptions clean up too. `app.getVersion()` reports the pinned embedded CC Switch version. Browser preview uses a module-local mock, without mutating these globals. Regression tests use non-writable, non-configurable globals on both synthetic windows.

Windows, exit/restart, upstream updating, external deep links and onboarding are managed by Molly. Manager storage stays private, while explicit tool configuration, environment management, MCP, Skills, sessions, file imports/exports and configured cloud sync operate on the user's actual tools. Opt-in inbound `ccswitch://` links enter the original import confirmation dialog after Molly validates and queues them; the bridge never invokes an external CC Switch application. The upstream tool tabs include MiniMax Code in 3.20.4. The host's explicit CLI launch supports Claude Code, Codex and Gemini. The Rust command allowlist remains the final enforcement boundary. Theme follows the Molly console settings without reloading the iframe.

The parent/child message protocol uses exact origin and source checks:

- Child → parent: `{ source: "molly-ccswitch", type: "ready" }` after the listener mounts.
- Child → parent: `{ source: "molly-ccswitch", type: "load-error" }` after rendering an initialization error; the parent must uncover the iframe's error instead of leaving a loading mask over it.
- Parent → child: `{ source: "mollycloud", type: "navigate" | "provider-imported", app?: AppId, providerId?: string }`, accepting only supported tool IDs (including `mcode`).
- Messages contain no credentials. Import navigation refreshes queries and highlights the matching provider after it renders.

Read-only UI preview requires both parent `ui-preview=console` and child `ui-preview=ccswitch`, a same-origin iframe, and absence of a real Tauri bridge. Preview shows labeled sample data and rejects every data mutation. It is never a fallback for failed native commands.

When updating upstream, audit new commands and lifecycle imports before adding them. Preserve the separate build, storage namespace, DB-only Molly imports and explicit activation; do not replace the adapters with upstream `run()` or outbound `ccswitch://` dispatch. Manager data isolation and proxy ownership rules are documented in `docs/CCSwitch内置方案.md`.


2026-10-02：工作台由 Molly 功能插件目录管理。必须保留受控安装状态、原生命令门禁、主题/滚动条与既有嵌入适配。独立资源包发布及原生接口版本约束见仓库 `docs/功能插件方案.md`。
