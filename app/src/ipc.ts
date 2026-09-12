import { invoke } from "@tauri-apps/api/core";
import {
  assistantConfigSchema,
  assistantReplySchema,
  assistantStatusSchema,
  dashboardPayloadSchema,
  loginOutcomeSchema,
  serviceBootstrapSchema,
  type DashboardPayload,
  type AssistantMessage,
  type AssistantConfig,
  type AssistantConfigUpdate,
  type AssistantReply,
  type AssistantStatus,
  type LoginOutcome,
  type ServiceBootstrap,
} from "./contracts";

const browserApiRoot = "/molly-api/api/v1";
let browserAccessToken = "";
let browserRefreshToken = "";
let browserTempToken = "";
let browserPetVisible = true;

export interface CcSwitchImportResult {
  provider_id: string;
  app: string;
}

export type ConsoleCloseAction = "tray" | "quit";
export interface ConsoleSettings { closeAction: ConsoleCloseAction; autostart: boolean }
const consoleSettingsPreviewKey = "mollycloud:preview:console-settings";

function isConsoleSettingsPreview(): boolean {
  return import.meta.env.DEV && !isTauriRuntime()
    && new URLSearchParams(window.location.search).get("ui-preview") === "console";
}

function parseConsoleSettings(value: unknown): ConsoleSettings {
  const settings = value as Partial<ConsoleSettings> | null;
  const action = settings?.closeAction;
  if (action !== "tray" && action !== "quit") throw new Error("关闭窗口设置无效，请重新选择。");
  if (settings?.autostart != null && typeof settings.autostart !== "boolean") throw new Error("开机自启动设置无效，请重新选择。");
  return { closeAction: action, autostart: settings?.autostart === true };
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function openExternalPage(url: string): Promise<void> {
  if (isTauriRuntime()) {
    await invoke("open_url", { url });
    return;
  }
  const opened = window.open(url, "_blank");
  if (!opened) throw new Error("浏览器阻止了新窗口，请允许弹出窗口后重试");
  opened.opener = null;
}

async function browserEnvelope(path: string, init?: RequestInit): Promise<unknown> {
  const response = await fetch(`${browserApiRoot}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(browserAccessToken ? { Authorization: `Bearer ${browserAccessToken}` } : {}),
      ...init?.headers,
    },
  });
  const body = await response.json() as { code?: number; message?: string; data?: unknown };
  if (!response.ok || (body.code ?? 0) !== 0) {
    throw new Error(body.message || `服务请求失败（HTTP ${response.status}）`);
  }
  return body.data;
}

async function browserBootstrap(): Promise<ServiceBootstrap> {
  const [healthResponse, settings] = await Promise.all([
    fetch("/molly-api/health"),
    browserEnvelope("/settings/public"),
  ]);
  if (!healthResponse.ok) throw new Error("MollyCloud 服务当前不可用");
  return serviceBootstrapSchema.parse({
    service_origin: "https://mollycloud.cn",
    health: await healthResponse.json(),
    settings,
  });
}

async function browserLogin(email: string, password: string): Promise<LoginOutcome> {
  const data = await browserEnvelope("/auth/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  }) as Record<string, unknown>;
  if (data.requires_2fa === true) {
    browserTempToken = String(data.temp_token ?? "");
    return loginOutcomeSchema.parse({
      requires_two_factor: true,
      user_email_masked: data.user_email_masked ?? null,
      user: null,
    });
  }
  browserAccessToken = String(data.access_token ?? "");
  browserRefreshToken = String(data.refresh_token ?? "");
  return loginOutcomeSchema.parse({ requires_two_factor: false, user: data.user });
}

async function browserDashboard(): Promise<DashboardPayload> {
  const [user, subscriptions, subscriptionProgress, usage, keys] = await Promise.all([
    browserEnvelope("/auth/me"),
    browserEnvelope("/subscriptions/summary"),
    browserEnvelope("/subscriptions/progress"),
    browserEnvelope("/usage/dashboard/stats"),
    browserEnvelope("/keys?page=1&page_size=100"),
  ]);
  const keyPage = keys && typeof keys === "object" ? keys as { items?: Array<Record<string, unknown>> } : {};
  for (const item of keyPage.items ?? []) {
    const raw = typeof item.key === "string" ? item.key : "";
    item.key = `sk-••••${raw.slice(-4)}`;
  }
  return dashboardPayloadSchema.parse({
    user,
    subscriptions,
    subscription_progress: subscriptionProgress,
    usage,
    keys,
  });
}

export const desktopApi = {
  async getConsoleSettings(): Promise<ConsoleSettings> {
    if (isTauriRuntime()) return parseConsoleSettings(await invoke("get_console_settings"));
    if (isConsoleSettingsPreview()) {
      const saved = localStorage.getItem(consoleSettingsPreviewKey);
      return saved ? parseConsoleSettings(JSON.parse(saved)) : { closeAction: "tray", autostart: false };
    }
    throw new Error("请在 MollyCloud 桌面客户端中设置关闭窗口的行为。");
  },

  async saveConsoleSettings(settings: ConsoleSettings): Promise<ConsoleSettings> {
    const validated = parseConsoleSettings(settings);
    if (isTauriRuntime()) return parseConsoleSettings(await invoke("save_console_settings", { settings: validated }));
    if (isConsoleSettingsPreview()) {
      localStorage.setItem(consoleSettingsPreviewKey, JSON.stringify(validated));
      return validated;
    }
    throw new Error("请在 MollyCloud 桌面客户端中保存设置。");
  },

  async bootstrapPublic(): Promise<ServiceBootstrap> {
    const value = isTauriRuntime()
      ? await invoke("bootstrap_public")
      : await browserBootstrap();
    return serviceBootstrapSchema.parse(value);
  },

  async login(email: string, password: string, rememberLogin: boolean): Promise<LoginOutcome> {
    const value = isTauriRuntime()
      ? await invoke("login", { email, password, rememberLogin })
      : await browserLogin(email, password);
    return loginOutcomeSchema.parse(value);
  },

  async completeTwoFactor(totpCode: string, rememberLogin: boolean): Promise<LoginOutcome> {
    if (!isTauriRuntime()) {
      const data = await browserEnvelope("/auth/login/2fa", {
        method: "POST",
        body: JSON.stringify({ temp_token: browserTempToken, totp_code: totpCode }),
      }) as Record<string, unknown>;
      browserAccessToken = String(data.access_token ?? "");
      browserRefreshToken = String(data.refresh_token ?? "");
      return loginOutcomeSchema.parse({ requires_two_factor: false, user: data.user });
    }
    return loginOutcomeSchema.parse(await invoke("complete_two_factor", { totpCode, rememberLogin }));
  },

  async restoreSession(): Promise<unknown | null> {
    if (!isTauriRuntime()) return null;
    return invoke("restore_session");
  },

  async fetchDashboard(): Promise<DashboardPayload> {
    const value = isTauriRuntime()
      ? await invoke("fetch_dashboard")
      : await browserDashboard();
    return dashboardPayloadSchema.parse(value);
  },

  async logout(): Promise<void> {
    if (isTauriRuntime()) {
      await invoke("logout");
    } else {
      if (browserRefreshToken) {
        await browserEnvelope("/auth/logout", {
          method: "POST",
          body: JSON.stringify({ refresh_token: browserRefreshToken }),
        }).catch(() => undefined);
      }
      browserAccessToken = "";
      browserRefreshToken = "";
      browserTempToken = "";
    }
  },

  async copyApiEndpoint(): Promise<void> {
    if (isTauriRuntime()) {
      await invoke("copy_api_endpoint");
    } else {
      await navigator.clipboard.writeText("https://mollycloud.cn/v1");
    }
  },

  async copyApiKey(keyId: string, browserFallback: string): Promise<void> {
    if (isTauriRuntime()) {
      await invoke("copy_api_key", { keyId });
    } else {
      await navigator.clipboard.writeText(browserFallback);
    }
  },

  async importApiKeyToCcSwitch(keyId: string): Promise<CcSwitchImportResult> {
    if (!isTauriRuntime()) {
      throw new Error("请在 MollyCloud 客户端中导入到内置 CC Switch，浏览器预览不会保存真实密钥");
    }
    return invoke<CcSwitchImportResult>("import_api_key_to_ccswitch", { keyId });
  },

  async isPetVisible(): Promise<boolean> {
    if (!isTauriRuntime()) return browserPetVisible;
    return invoke<boolean>("is_pet_visible");
  },

  async setPetVisible(visible: boolean): Promise<void> {
    if (!isTauriRuntime()) {
      browserPetVisible = visible;
      return;
    }
    await invoke(visible ? "show_pet" : "hide_pet");
  },

  async setOverlayInteractive(interactive: boolean): Promise<void> {
    if (isTauriRuntime()) await invoke("set_overlay_interactive", { interactive });
  },

  async setConsoleDashboardActive(active: boolean): Promise<void> {
    if (isTauriRuntime()) await invoke("set_console_dashboard_active", { active });
  },

  async playMotion(group: "Idle" | "Blink" | "Nod" | "Shake"): Promise<void> {
    if (isTauriRuntime()) await invoke("play_overlay_motion", { group });
  },

  async assistantStatus(): Promise<AssistantStatus> {
    if (!isTauriRuntime()) {
      return assistantStatusSchema.parse({
        ready: false,
        keys: [],
        selected_key_id: null,
        models: [],
        message: "AI 助手需在 MollyCloud 客户端中使用",
      });
    }
    return assistantStatusSchema.parse(await invoke("assistant_status"));
  },

  async assistantChat(history: AssistantMessage[]): Promise<AssistantReply> {
    if (!isTauriRuntime()) {
      throw new Error("AI 助手需在 MollyCloud 客户端中使用");
    }
    return assistantReplySchema.parse(await invoke("assistant_chat", {
      history,
    }));
  },

  async getAssistantConfig(): Promise<AssistantConfig> {
    if (!isTauriRuntime()) {
      return assistantConfigSchema.parse({
        enabled: false,
        provider: "mollycloud",
        model: "",
        persona: "你叫 Molly，语气自然、简洁、友好。",
        custom_base_url: "",
        greet_interval: 20,
        molly_key_id: "",
        api_key_configured: false,
      });
    }
    return assistantConfigSchema.parse(await invoke("get_assistant_config"));
  },

  async saveAssistantConfig(config: AssistantConfigUpdate): Promise<AssistantConfig> {
    if (!isTauriRuntime()) {
      return assistantConfigSchema.parse({ ...config, api_key_configured: Boolean(config.api_key) });
    }
    return assistantConfigSchema.parse(await invoke("save_assistant_config", { config }));
  },

  async openRecharge(): Promise<void> {
    await openExternalPage("https://mollycloud.cn/purchase");
  },

  async openSubscriptions(): Promise<void> {
    await openExternalPage("https://mollycloud.cn/subscriptions");
  },

  async showPetBubble(message: string): Promise<void> {
    if (!isTauriRuntime()) return;
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("main", "petra-bubble", message.slice(0, 180));
  },
};
