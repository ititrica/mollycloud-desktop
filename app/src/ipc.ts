import { invoke } from "@tauri-apps/api/core";
import { version as bundledAppVersion } from "../package.json";
import {
  accountBalanceSchema,
  keyGroupsSchema, keyGroupChangedSchema, type KeyGroups, type KeyGroupChanged, type CreateKeyRequest,
  assistantConfigSchema,
  assistantReplySchema,
  assistantStatusSchema,
  dashboardPayloadSchema,
  loginOutcomeSchema,
  serviceBootstrapSchema,
  type DashboardPayload,
  type AccountBalance,
  type AssistantMessage,
  type AssistantConfig,
  type AssistantConfigUpdate,
  type AssistantReply,
  type AssistantStatus,
  type LoginOutcome,
  type ServiceBootstrap,
} from "./contracts";

const browserApiRoot = "/molly-api/api/v1";
const browserAppVersion = bundledAppVersion;
let browserAccessToken = "";
let browserRefreshToken = "";
let browserTempToken = "";
let browserPetVisible = true;
let groupRequest: Promise<KeyGroups> | null = null;

export interface CcSwitchImportResult {
  provider_id: string;
  app: string;
}

export type CcSwitchAgent = "claude" | "claude-desktop" | "codex" | "gemini" | "grokbuild" | "opencode" | "openclaw" | "hermes" | "pi" | "mcode";

export interface CcSwitchImportRequest {
  keyId: string;
  name: string;
  agent: CcSwitchAgent;
  model: string;
}

export interface DesktopUpdate {
  version: string;
  currentVersion: string;
  downloadUrl: string;
  notes?: string;
  sha256: string;
  sizeBytes: number;
}

export type ConsoleCloseAction = "tray" | "quit";
export interface ConsoleSettings { closeAction: ConsoleCloseAction; autostart: boolean; autostartMinimized: boolean }
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
  if (settings?.autostartMinimized != null && typeof settings.autostartMinimized !== "boolean") throw new Error("启动后最小化设置无效，请重新选择。");
  const autostart = settings?.autostart === true;
  return { closeAction: action, autostart, autostartMinimized: autostart && settings?.autostartMinimized === true };
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function openExternalPage(url: string): Promise<void> {
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
  const keyIds=(keyPage.items??[]).map(item=>Number(item.id)).filter(id=>Number.isSafeInteger(id)&&id>0).slice(0,100);
  const keyStats = keyIds.length ? await browserEnvelope('/usage/dashboard/api-keys-usage', {method:'POST',body:JSON.stringify({api_key_ids:keyIds})}).catch(()=>null) as {stats?:Record<string,Record<string,unknown>>}|null : null;
  for(const item of keyPage.items??[]) { const stats=keyStats?.stats?.[String(item.id)];item.usage={total_actual_cost:stats?.total_actual_cost??null,today_actual_cost:stats?.today_actual_cost??null}; }
  return dashboardPayloadSchema.parse({
    user,
    subscriptions,
    subscription_progress: subscriptionProgress,
    usage,
    keys,
  });
}

async function browserAccountBalance(): Promise<AccountBalance> {
  const [user, usage] = await Promise.all([
    browserEnvelope("/auth/me"),
    browserEnvelope("/usage/dashboard/stats"),
  ]);
  const userRecord = user && typeof user === "object" ? user as Record<string, unknown> : {};
  const usageRecord = usage && typeof usage === "object" ? usage as Record<string, unknown> : {};
  return accountBalanceSchema.parse({
    balance: userRecord.balance ?? null,
    today_tokens: usageRecord.today_tokens ?? null,
  });
}

export const desktopApi = {
  async getAppVersion(): Promise<string> {
    if (!isTauriRuntime()) return browserAppVersion;
    const { getVersion } = await import("@tauri-apps/api/app");
    return getVersion();
  },

  async getConsoleSettings(): Promise<ConsoleSettings> {
    if (isTauriRuntime()) return parseConsoleSettings(await invoke("get_console_settings"));
    if (isConsoleSettingsPreview()) {
      const saved = localStorage.getItem(consoleSettingsPreviewKey);
      return saved ? parseConsoleSettings(JSON.parse(saved)) : { closeAction: "tray", autostart: false, autostartMinimized: false };
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

  async fetchAccountBalance(): Promise<AccountBalance> {
    const value = isTauriRuntime()
      ? await invoke("fetch_account_balance")
      : await browserAccountBalance();
    return accountBalanceSchema.parse(value);
  },

  async fetchKeyGroups(): Promise<KeyGroups> {
    if (isTauriRuntime()) {
      // A directory can contain many keys: share only concurrent reads, never cache permissions.
      groupRequest ??= invoke("fetch_key_groups").then(value=>keyGroupsSchema.parse(value)).finally(()=>{groupRequest=null;});
      return groupRequest;
    }
    if (isConsoleSettingsPreview()) return { rates_available: true, groups: [
      {id:"1",name:"Molly Pro",platform:"openai",subscription:true,rate:1,custom_rate:false,description:"OpenAI 订阅专属通道",default_rate:1},
      {id:"2",name:"Molly Standard",platform:"openai",subscription:false,rate:0.8,custom_rate:true,description:"OpenAI 标准通道",default_rate:1},
      {id:"3",name:"Claude",platform:"anthropic",subscription:false,rate:1.2,custom_rate:false,description:"Claude 高质量模型",default_rate:1.2},
    ] };
    throw new Error("请在 MollyCloud 客户端中管理密钥分组。");
  },

  async changeKeyGroup(keyId: string, groupId: string): Promise<KeyGroupChanged> {
    if (isTauriRuntime()) return keyGroupChangedSchema.parse(await invoke("change_key_group", { keyId, groupId }));
    if (isConsoleSettingsPreview() && keyId.startsWith("demo-")) {
      const group = (await this.fetchKeyGroups()).groups.find(group => group.id === groupId);
      if (group) return { key_id: keyId, group };
    }
    throw new Error("请在 MollyCloud 客户端中修改密钥分组。");
  },

  async createApiKey(request: CreateKeyRequest): Promise<Record<string, unknown>> {
    if (isTauriRuntime()) return await invoke("create_api_key", {request});
    if (isConsoleSettingsPreview()) {
      const group = (await this.fetchKeyGroups()).groups.find(g=>g.id===request.group_id);
      return {id:`demo-${Date.now()}`,name:request.name,key:"sk-••••DEMO",status:"active",quota_used:0,group_id:group?.id,group:group?{...group,rate_multiplier:group.rate}:null};
    }
    throw new Error("请在 MollyCloud 客户端中创建密钥。");
  },

  async deleteApiKey(keyId: string): Promise<void> {
    if (isTauriRuntime()) return await invoke("delete_api_key", { keyId });
    if (isConsoleSettingsPreview() && keyId.startsWith("demo-")) return;
    throw new Error("请在 MollyCloud 客户端中删除密钥。");
  },

  async openRechargeView(dark: boolean, viewId: string): Promise<void> {
    if (!isTauriRuntime()) throw new Error("请在桌面客户端中打开充值页面。");
    await invoke("open_recharge_view", { dark, viewId });
  },
  async closeRechargeView(viewId?: string): Promise<void> {
    if (isTauriRuntime()) await invoke("close_recharge_view", {viewId:viewId??null});
  },

  async syncPetAccountBalance(account: AccountBalance): Promise<void> {
    if (!isTauriRuntime()) return;
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("main", "account-balance-updated", account).catch(() => undefined);
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

  async fetchCcSwitchImportModels(keyId: string): Promise<string[]> {
    if (!isTauriRuntime()) {
      throw new Error("请在 MollyCloud 客户端中拉取真实模型列表");
    }
    return invoke<string[]>("fetch_ccswitch_import_models", { keyId });
  },

  async importApiKeyToCcSwitch(request: CcSwitchImportRequest): Promise<CcSwitchImportResult> {
    if (!isTauriRuntime()) {
      throw new Error("请在 MollyCloud 客户端中导入到内置 CC Switch，浏览器预览不会保存真实密钥");
    }
    return invoke<CcSwitchImportResult>("import_api_key_to_ccswitch", { ...request });
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

  async openSubscriptions(): Promise<void> {
    await openExternalPage("https://mollycloud.cn/subscriptions");
  },

  async checkForDesktopUpdate(): Promise<DesktopUpdate | null> {
    if (!isTauriRuntime()) return null;
    return invoke<DesktopUpdate | null>("check_for_desktop_update");
  },

  async openDesktopUpdate(downloadUrl: string): Promise<void> {
    if (isTauriRuntime()) {
      await invoke("open_desktop_update", { downloadUrl });
      return;
    }
    await openExternalPage(downloadUrl);
  },

  async showPetBubble(message: string): Promise<void> {
    if (!isTauriRuntime()) return;
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("main", "petra-bubble", message.slice(0, 180));
  },
};
