import { installPreviewBridge } from "./native-bridge";

// Available only when both iframe and same-origin console explicitly request
// UI preview and no desktop bridge exists. Mutations never report success.
export function installReadOnlyPreview(): void {
  const provider = {
    id: "molly-preview-provider", name: "MollyCloud · 界面示例", category: "third_party",
    websiteUrl: "https://mollycloud.cn", notes: "界面预览数据，未导入真实密钥", createdAt: 1,
    settingsConfig: { auth: { OPENAI_API_KEY: "" }, config: 'model_provider = "mollycloud"\nmodel = "gpt-5.5"\n[model_providers.mollycloud]\nname = "MollyCloud"\nbase_url = "https://example.invalid/v1"\nwire_api = "responses"' },
  };
  const visibleApps = { claude: true, codex: true, gemini: true, "claude-desktop": false, grokbuild: false, opencode: false, openclaw: false, hermes: false, pi: false };
  const takeovers = Object.fromEntries(Object.keys(visibleApps).map((app) => [app, false]));
  const noDataCommands = new Set(["get_installed_skills", "get_skill_repos", "get_skill_backups", "scan_unmanaged_skills", "get_failover_queue", "get_available_providers_for_failover", "get_custom_endpoints", "get_universal_providers", "get_mcp_servers", "get_all_mcp_servers", "get_prompts", "get_usage_trends", "get_provider_stats", "get_model_stats", "get_model_pricing"]);
  let callbackId = 0;
  const internals = {
    metadata: { currentWindow: { label: "console" }, currentWebview: { label: "console" } },
    transformCallback: () => ++callbackId,
    unregisterCallback: () => undefined,
    runCallback: () => undefined,
    async invoke<T>(raw: string, args?: Record<string, unknown>): Promise<T> {
      if (raw === "plugin:event|listen") return ++callbackId as T;
      if (raw === "plugin:event|unlisten") return undefined as T;
      const command = raw.replace(/^plugin:molly-ccswitch\|/, "");
      let result: unknown;
      switch (command) {
        case "get_init_error": case "get_migration_result": case "get_skills_migration_result": case "get_pending_deeplink": result = null; break;
        case "get_app_config_dir_override": result = "[Molly 应用数据]/ccswitch"; break;
        case "get_settings": result = { language: "zh", visibleApps, showProfileSwitcher: false, showInTray: false, enableLocalProxy: false, enableFailoverToggle: false, firstRunNoticeDismissed: true, commonConfigConfirmed: true, minimizeToTrayOnClose: false, launchOnStartup: false }; break;
        case "get_providers": result = args?.app === "codex" ? { [provider.id]: provider } : {}; break;
        case "get_current_provider": case "get_common_config_snippet": case "get_codex_common_config": result = ""; break;
        case "get_proxy_status": result = { running: false, address: "127.0.0.1", port: 24327, active_connections: 0, total_requests: 0, success_requests: 0, failed_requests: 0, success_rate: 0, uptime_seconds: 0, current_provider: null, current_provider_id: null, last_request_at: null, last_error: null, failover_count: 0, active_targets: [] }; break;
        case "get_proxy_takeover_status": result = takeovers; break;
        case "get_global_proxy_config": result = { proxyEnabled: false, listenAddress: "127.0.0.1", listenPort: 24327, enableLogging: false }; break;
        case "get_proxy_config_for_app": result = { appType: args?.appType, enabled: false, autoFailoverEnabled: false }; break;
        case "list_profiles": result = { profiles: [], currentIds: { claude: null, claudeDesktop: null, codex: null } }; break;
        case "is_portable_mode": case "get_auto_launch_status": case "get_auto_failover_enabled": case "has_codex_unify_history_backup": result = false; break;
        case "get_config_dir": result = `[用户目录]/.${String(args?.app ?? "codex")}`; break;
        case "get_app_config_path": result = "[Molly 应用数据]/ccswitch/cc-switch.db"; break;
        case "get_claude_code_config_path": result = "[用户目录]/.claude/settings.json"; break;
        case "get_rectifier_config": case "get_optimizer_config": result = { enabled: false }; break;
        case "get_log_config": result = { enabled: false, level: "error" }; break;
        case "get_usage_data_sources": result = []; break;
        case "get_provider_health": result = null; break;
        case "check_all_env_conflicts": result = {}; break;
        case "check_env_conflicts": result = []; break;
        default:
          if (noDataCommands.has(command)) result = [];
          else throw new Error("界面预览不执行此操作，请在 MollyCloud 桌面客户端中使用。");
      }
      return result as T;
    },
  };
  installPreviewBridge({ internals, events: { unregisterListener: () => undefined } });
}
