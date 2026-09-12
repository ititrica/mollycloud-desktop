<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NCheckbox,
  NDataTable,
  NDropdown,
  NInput,
  NModal,
  NSwitch,
  NTag,
  type DataTableColumns,
  type DropdownOption,
} from "naive-ui";
import AppIcon from "./components/AppIcon.vue";
import CcSwitchPanel from "./components/CcSwitchPanel.vue";
import ImageWorkbenchPanel from "./components/ImageWorkbenchPanel.vue";
import ConsoleSettingsDialog from "./components/ConsoleSettingsDialog.vue";
import PetSettingsDialog from "./components/PetSettingsDialog.vue";
import { asArray, asRecord, type AssistantConfig, type DashboardPayload, type ServiceBootstrap } from "./contracts";
import { formatBalance, formatDate, formatMoney, formatTokens, numberValue, textValue } from "./format";
import { desktopApi, type CcSwitchImportResult } from "./ipc";

type Page = "overview" | "subscriptions" | "keys" | "usage" | "assistant" | "ccswitch" | "images";
type Phase = "starting" | "login" | "two-factor" | "dashboard";
type AssistantDisplayMessage = { role: "user" | "assistant"; content: string };
type OverviewIcon = "wallet" | "spend" | "subscription" | "code";
type OverviewTarget = "recharge" | "subscriptions" | "keys" | "usage";

const phase = ref<Phase>("starting");
watch(() => phase.value === "dashboard", (active) => {
  void desktopApi.setConsoleDashboardActive(active).catch((error) => {
    console.error("无法同步控制台关闭行为", error);
  });
}, { immediate: true, flush: "sync" });
const bootstrap = ref<ServiceBootstrap | null>(null);
const dashboard = ref<DashboardPayload | null>(null);
const restoredUser = ref<unknown>(null);
const activePage = ref<Page>("overview");
const email = ref("");
const password = ref("");
const totpCode = ref("");
const autoLogin = ref(false);
const acceptedAgreement = ref(true);
const agreementOpen = ref(false);
const consoleSettingsOpen = ref(false);
const consoleSettingsButton = ref<HTMLButtonElement | null>(null);
const petSettingsOpen = ref(false);
const petSettingsButton = ref<HTMLButtonElement | null>(null);
const submitting = ref(false);
const refreshing = ref(false);
const errorMessage = ref("");
const twoFactorHint = ref("");
const lastUpdated = ref<Date | null>(null);
const assistantConfig = ref<AssistantConfig | null>(null);
const petVisible = ref(true);
const petToggleLoading = ref(false);
const copiedKeyId = ref("");
const importingKeyId = ref("");
const importedProviders = ref<Record<string, CcSwitchImportResult>>({});
const ccSwitchVisited = ref(false);
const imageWorkbenchVisited = ref(false);
const ccSwitchTarget = ref<CcSwitchImportResult | null>(null);
const ccSwitchPanel = ref<InstanceType<typeof CcSwitchPanel> | null>(null);
const consolePreview = import.meta.env.DEV && new URLSearchParams(window.location.search).get("ui-preview") === "console";
const endpointCopied = ref(false);
const assistantMessages = ref<AssistantDisplayMessage[]>([]);
const assistantDraft = ref("");
const assistantSending = ref(false);
const assistantChatError = ref("");
const assistantHistory = ref<HTMLElement | null>(null);
let refreshTimer: number | undefined;
let copyFeedbackTimer: number | undefined;
let unlistenAssistantConfig: (() => void) | undefined;
let unlistenAssistantHistory: (() => void) | undefined;
let unlistenAssistantState: (() => void) | undefined;

const navigation: Array<{ id: Page; label: string; icon: "overview" | "subscription" | "key" | "usage" | "assistant" | "code" | "image" }> = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "subscriptions", label: "订阅", icon: "subscription" },
  { id: "keys", label: "API 密钥", icon: "key" },
  { id: "usage", label: "用量", icon: "usage" },
  { id: "assistant", label: "Molly助手", icon: "assistant" },
  { id: "ccswitch", label: "CC Switch", icon: "code" },
  { id: "images", label: "生图工作台", icon: "image" },
];

const pageMeta: Record<Page, { kicker: string; title: string; description: string }> = {
  overview: { kicker: "账户总览", title: "概览", description: "余额、订阅和今日使用情况集中显示在这里。" },
  subscriptions: { kicker: "账户权益", title: "订阅", description: "查看当前订阅、到期时间和系统分配用量。" },
  keys: { kicker: "开发接入", title: "API 密钥", description: "复制 API 端点并查看已有密钥的使用状态。" },
  usage: { kicker: "使用统计", title: "用量", description: "汇总请求、Token 和实际消费数据。" },
  assistant: { kicker: "桌面伙伴", title: "Molly助手", description: "与桌面 Molly 共用对话记录和发送能力。" },
  ccswitch: { kicker: "工具配置", title: "CC Switch", description: "管理供应商与本机工具配置。" },
  images: { kicker: "图片创作", title: "生图工作台", description: "生成、编辑图片，保存创作记录。" },
};

const settings = computed(() => asRecord(bootstrap.value?.settings));
const siteName = computed(() => textValue(settings.value.site_name, "MollyCloud"));
const agreementEnabled = computed(() => settings.value.login_agreement_enabled === true);
const agreementDocuments = computed(() => asArray(settings.value.login_agreement_documents));
const user = computed(() => asRecord(dashboard.value?.user ?? restoredUser.value));
const imageAccount = computed(() => String(user.value.id ?? user.value.email ?? ""));
const usage = computed(() => asRecord(dashboard.value?.usage));
const subscriptionSummary = computed(() => asRecord(dashboard.value?.subscriptions));
const subscriptions = computed(() => asArray(subscriptionSummary.value.subscriptions));
const subscriptionProgress = computed(() => asArray(dashboard.value?.subscription_progress));
const keyPage = computed(() => asRecord(dashboard.value?.keys));
const keys = computed(() => asArray(keyPage.value.items));
const userInitial = computed(() => textValue(user.value.username ?? user.value.email, "M").slice(0, 1).toUpperCase());
const currentPageMeta = computed(() => pageMeta[activePage.value]);
const apiEndpoint = "https://mollycloud.cn/v1";
const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 6) return "夜深了";
  if (hour < 12) return "早上好";
  if (hour < 18) return "下午好";
  return "晚上好";
});
const accountReminder = computed(() => {
  if (activePage.value === "subscriptions") {
    const expiring = subscriptions.value.find((item) => {
      const days = subscriptionDaysRemaining(item);
      return days != null && days >= 0 && days <= 3;
    });
    if (expiring) {
      const days = subscriptionDaysRemaining(expiring) ?? 0;
      return {
        kind: "subscription",
        id: `subscription-${String(expiring.id ?? "expiring")}`,
        title: "订阅即将到期",
        detail: `${subscriptionName(expiring)} 剩余 ${days} 天`,
      };
    }
  }
  if (!assistantConfig.value?.enabled) return null;
  if (user.value.balance != null) {
    const threshold = 5;
    const balance = numberValue(user.value.balance);
    if (balance <= threshold) {
      return {
        kind: "balance",
        id: balance <= 0 ? "balance-empty" : "balance-low",
        title: balance <= 0 ? "账户余额已用尽" : "账户余额较低",
        detail: `当前 ${formatBalance(balance)}，提醒阈值 ${formatBalance(threshold)}`,
      };
    }
  }
  return null;
});
const visibleAccountReminder = computed(() => {
  const reminder = accountReminder.value;
  if (!reminder) return null;
  if (reminder.kind === "subscription" && activePage.value !== "subscriptions") return null;
  return reminder;
});

const stats = computed<Array<{ label: string; value: string; tone: string; icon: OverviewIcon; target: OverviewTarget }>>(() => [
  { label: "账户余额", value: formatBalance(user.value.balance), tone: "lime", icon: "wallet", target: "recharge" },
  { label: "累计消费", value: formatMoney(usage.value.total_actual_cost), tone: "blue", icon: "spend", target: "usage" },
  { label: "活跃订阅", value: String(numberValue(subscriptionSummary.value.active_count)), tone: "violet", icon: "subscription", target: "subscriptions" },
  { label: "API 密钥", value: String(numberValue(usage.value.active_api_keys)), tone: "white", icon: "code", target: "keys" },
]);

const userMenuOptions: DropdownOption[] = [
  { label: "退出登录", key: "sign-out", props: { class: "naive-user-menu__logout" } },
];

const keyTableColumns: DataTableColumns<Record<string, unknown>> = [
  {
    title: "名称",
    key: "name",
    width: 125,
    render: (row) => h("strong", textValue(row.name, "未命名")),
  },
  {
    title: "密钥",
    key: "key",
    width: 235,
    render: (row) => h("div", { class: "key-cell" }, [
      h("code", textValue(row.key, "sk-••••")),
      h(NButton, {
        class: "key-copy-button",
        size: "small",
        tertiary: true,
        onClick: () => void copyKey(row),
      }, {
        default: () => [
          h(AppIcon, { name: "copy" }),
          copiedKeyId.value === String(row.id) ? "已复制" : "复制",
        ],
      }),
    ]),
  },
  {
    title: "分组",
    key: "group",
    width: 150,
    render: (row) => {
      const group = asRecord(row.group);
      const groupName = textValue(row.group_name ?? group.name, row.group_id ? `分组 ${String(row.group_id)}` : "未分组");
      const platform = textValue(group.platform, "");
      return h("div", { class: "group-cell" }, [
        h("strong", groupName),
        platform ? h("small", platformLabel(platform)) : null,
      ]);
    },
  },
  {
    title: "状态",
    key: "status",
    width: 90,
    render: (row) => h(NTag, {
      size: "small",
      bordered: false,
      type: row.status === "active" ? "success" : "default",
    }, { default: () => row.status === "active" ? "正常" : textValue(row.status) }),
  },
  {
    title: "已用额度",
    key: "quota_used",
    width: 115,
    render: (row) => formatMoney(row.quota_used),
  },
  {
    title: "内置 CC Switch",
    key: "ccs_import",
    width: 218,
    render: (row) => {
      const keyId = String(row.id);
      const imported = importedProviders.value[keyId];
      return h("div", { class: "ccs-import-actions" }, [
        h(NButton, {
          class: "ccs-import-button",
          size: "small",
          secondary: true,
          loading: importingKeyId.value === keyId,
          disabled: Boolean(importingKeyId.value) && importingKeyId.value !== keyId,
          onClick: () => void importKeyToCcSwitch(row),
        }, { default: () => imported ? "已导入" : "导入到内置 CC Switch" }),
        imported ? h(NButton, {
          size: "small",
          quaternary: true,
          onClick: () => openImportedProvider(imported),
          "aria-label": `在内置 CC Switch 中打开 ${textValue(row.name, "此供应商")}`,
        }, { default: () => "打开" }) : null,
      ]);
    },
  },
];

function platformLabel(platform: string): string {
  const labels: Record<string, string> = {
    anthropic: "Claude",
    openai: "OpenAI",
    gemini: "Gemini",
    antigravity: "Antigravity",
    grok: "Grok",
  };
  return labels[platform] ?? platform;
}

function friendlyError(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return typeof reason === "string" ? reason : "操作未完成，请稍后重试";
}

async function initialize(): Promise<void> {
  phase.value = "starting";
  errorMessage.value = "";
  const previewParams = new URLSearchParams(window.location.search);
  const uiPreview = previewParams.get("ui-preview");
  if (import.meta.env.DEV && uiPreview === "login") {
    bootstrap.value = {
      service_origin: "https://mollycloud.cn",
      health: { status: "ok" },
      settings: { site_name: "MollyCloud", login_agreement_enabled: true, login_agreement_documents: [] },
    };
    phase.value = "login";
    return;
  }
  if (import.meta.env.DEV && uiPreview === "console") {
    const previewPage = previewParams.get("preview-page");
    if (previewPage && navigation.some((item) => item.id === previewPage)) selectPage(previewPage as Page);
    bootstrap.value = { service_origin: "https://mollycloud.cn", health: { status: "ok" }, settings: { site_name: "MollyCloud" } };
    dashboard.value = {
      user: { username: "演示用户", email: "demo@mollycloud.cn", balance: 12.35 },
      subscriptions: { active_count: 1, subscriptions: [{ id: "demo", status: "active", expires_at: "2026-10-01T00:00:00Z", monthly_usage_usd: 12.6, group: { name: "Molly Pro", monthly_limit_usd: 50 } }] },
      subscription_progress: [],
      usage: {
        today_tokens: 128400, today_requests: 48, today_actual_cost: 1.28,
        total_requests: 2110, total_tokens: 197159800, total_actual_cost: 11.7333,
        total_input_tokens: 17010000, total_output_tokens: 809800,
        total_cache_creation_tokens: 0, total_cache_read_tokens: 179340000,
        active_api_keys: 2,
      },
      keys: { items: [{ id: "demo-key", name: "Molly Desktop", key: "sk-••••942A", status: "active", quota_used: 12.6, group_id: 1, group: { name: "Molly Pro", platform: "openai" } }] },
    };
    assistantConfig.value = { enabled: true, provider: "mollycloud", model: "gpt-5-mini", persona: "", custom_base_url: "", greet_interval: 20, molly_key_id: "demo-key", api_key_configured: false };
    assistantMessages.value = [
      { role: "assistant", content: "下午好，需要我帮你看看今天的用量，还是聊点别的？" },
      { role: "user", content: "帮我概括一下今天的使用情况。" },
      { role: "assistant", content: "今天共发起 48 次请求，使用约 128.4K Token。需要的话我也可以继续查看订阅状态。" },
    ];
    lastUpdated.value = new Date();
    petVisible.value = true;
    phase.value = "dashboard";
    return;
  }
  try {
    bootstrap.value = await desktopApi.bootstrapPublic();
    const sessionUser = await desktopApi.restoreSession();
    if (sessionUser) {
      restoredUser.value = sessionUser;
      phase.value = "dashboard";
      await loadAssistantConfig();
      await loadPetVisibility();
      await refreshDashboard();
      startRefreshTimer();
    } else {
      phase.value = "login";
    }
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
    phase.value = "login";
  }
}

async function submitLogin(): Promise<void> {
  if (agreementEnabled.value && !acceptedAgreement.value) {
    errorMessage.value = "请先阅读并同意站点服务协议";
    return;
  }
  submitting.value = true;
  errorMessage.value = "";
  try {
    const result = await desktopApi.login(email.value.trim(), password.value, autoLogin.value);
    password.value = "";
    if (result.requires_two_factor) {
      twoFactorHint.value = result.user_email_masked ?? email.value;
      phase.value = "two-factor";
      return;
    }
    restoredUser.value = result.user;
    phase.value = "dashboard";
    await loadAssistantConfig();
    await loadPetVisibility();
    await refreshDashboard();
    startRefreshTimer();
  } catch (reason) {
    password.value = "";
    errorMessage.value = friendlyError(reason);
    await desktopApi.playMotion("Shake").catch(() => undefined);
  } finally {
    submitting.value = false;
  }
}

async function submitTwoFactor(): Promise<void> {
  submitting.value = true;
  errorMessage.value = "";
  try {
    const result = await desktopApi.completeTwoFactor(totpCode.value.trim(), autoLogin.value);
    restoredUser.value = result.user;
    phase.value = "dashboard";
    await loadAssistantConfig();
    await loadPetVisibility();
    await refreshDashboard();
    startRefreshTimer();
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    submitting.value = false;
  }
}

async function refreshDashboard(): Promise<void> {
  if (refreshing.value) return;
  refreshing.value = true;
  errorMessage.value = "";
  try {
    dashboard.value = await desktopApi.fetchDashboard();
    lastUpdated.value = new Date();
    await maybeShowAccountReminder();
  } catch (reason) {
    if (!dashboard.value) errorMessage.value = friendlyError(reason);
  } finally {
    refreshing.value = false;
  }
}

async function maybeShowAccountReminder(): Promise<void> {
  const reminder = visibleAccountReminder.value;
  if (!reminder) return;
  const key = `molly-reminder-${reminder.id}`;
  const lastShown = Number(localStorage.getItem(key) ?? 0);
  if (Date.now() - lastShown < 12 * 60 * 60 * 1000) return;
  localStorage.setItem(key, String(Date.now()));
  await desktopApi.showPetBubble(`${reminder.title}。${reminder.detail}`).catch(() => undefined);
}

function startRefreshTimer(): void {
  if (refreshTimer) window.clearInterval(refreshTimer);
  refreshTimer = window.setInterval(() => void refreshDashboard(), 60_000);
}

async function signOut(): Promise<void> {
  await desktopApi.logout();
  if (refreshTimer) window.clearInterval(refreshTimer);
  dashboard.value = null;
  restoredUser.value = null;
  email.value = "";
  acceptedAgreement.value = true;
  petSettingsOpen.value = false;
  assistantConfig.value = null;
  importedProviders.value = {};
  ccSwitchTarget.value = null;
  ccSwitchVisited.value = false;
  imageWorkbenchVisited.value = false;
  activePage.value = "overview";
  phase.value = "login";
}

async function loadAssistantConfig(): Promise<void> {
  try {
    assistantConfig.value = await desktopApi.getAssistantConfig();
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  }
}

async function loadPetVisibility(): Promise<void> {
  try {
    petVisible.value = await desktopApi.isPetVisible();
  } catch {
    petVisible.value = true;
  }
}

async function setPetVisibility(visible: boolean): Promise<void> {
  if (petToggleLoading.value) return;
  petToggleLoading.value = true;
  errorMessage.value = "";
  try {
    await desktopApi.setPetVisible(visible);
    petVisible.value = visible;
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    petToggleLoading.value = false;
  }
}

async function scrollAssistantToEnd(): Promise<void> {
  await nextTick();
  const element = assistantHistory.value;
  if (element) element.scrollTop = element.scrollHeight;
}

function applyAssistantHistory(payload: { messages?: unknown[] }): void {
  const messages = Array.isArray(payload?.messages) ? payload.messages : [];
  assistantMessages.value = messages.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const message = item as Record<string, unknown>;
    if ((message.role !== "user" && message.role !== "assistant") || typeof message.content !== "string") return [];
    const content = message.content.trim().slice(0, 500);
    return content ? [{ role: message.role, content } as AssistantDisplayMessage] : [];
  });
  void scrollAssistantToEnd();
}

async function requestAssistantHistory(): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const { emitTo } = await import("@tauri-apps/api/event");
  await emitTo("main", "petra-assistant-history-request").catch(() => undefined);
}

async function sendAssistantMessage(): Promise<void> {
  const text = assistantDraft.value.trim();
  if (!text || assistantSending.value) return;
  assistantDraft.value = "";
  assistantChatError.value = "";
  if (!("__TAURI_INTERNALS__" in window)) {
    assistantMessages.value.push({ role: "user", content: text });
    assistantMessages.value.push({ role: "assistant", content: "此处会调用 Petra 的消息发送流程。" });
    void scrollAssistantToEnd();
    return;
  }
  assistantSending.value = true;
  try {
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("main", "petra-assistant-send", { text });
  } catch (reason) {
    assistantSending.value = false;
    assistantDraft.value = text;
    assistantChatError.value = friendlyError(reason);
  }
}

function handleAssistantKeydown(event: KeyboardEvent): void {
  if (event.key !== "Enter" || event.isComposing) return;
  event.preventDefault();
  void sendAssistantMessage();
}

function selectPage(page: Page): void {
  activePage.value = page;
  if (page === "ccswitch") ccSwitchVisited.value = true;
  if (page === "images") imageWorkbenchVisited.value = true;
  if (page === "assistant") {
    void loadPetVisibility();
    void requestAssistantHistory();
  }
}

function openImportedProvider(provider: CcSwitchImportResult): void {
  ccSwitchTarget.value = { ...provider };
  selectPage("ccswitch");
}

async function openRecharge(): Promise<void> {
  errorMessage.value = "";
  try {
    await desktopApi.openRecharge();
  } catch (reason) {
    errorMessage.value = `无法打开充值页面：${friendlyError(reason)}`;
  }
}

async function openSubscriptions(): Promise<void> {
  errorMessage.value = "";
  try {
    await desktopApi.openSubscriptions();
  } catch (reason) {
    errorMessage.value = `无法打开我的订阅：${friendlyError(reason)}`;
  }
}

function openOverviewMetric(target: OverviewTarget): void {
  if (target === "recharge") {
    void openRecharge();
    return;
  }
  selectPage(target);
}

function progressFor(subscriptionId: unknown): Record<string, unknown> {
  return subscriptionProgress.value.find((item) => item.subscription_id === subscriptionId) ?? {};
}

function subscriptionName(item: Record<string, unknown>): string {
  return textValue(item.group_name ?? asRecord(item.group).name, "订阅计划");
}

function subscriptionExpiresAt(item: Record<string, unknown>): string {
  return textValue(item.expires_at ?? progressFor(item.id).expires_at, "");
}

function subscriptionDaysRemaining(item: Record<string, unknown>): number | null {
  const expiresAt = subscriptionExpiresAt(item);
  if (!expiresAt) return null;
  const timestamp = Date.parse(expiresAt);
  if (!Number.isFinite(timestamp)) return null;
  return Math.max(0, Math.ceil((timestamp - Date.now()) / 86_400_000));
}

function subscriptionQuota(item: Record<string, unknown>): { used: number; limit: number; percentage: number } | null {
  const group = asRecord(item.group);
  const progress = progressFor(item.id);
  const directLimit = item.limit_usd ?? item.quota_limit_usd ?? group.limit_usd;
  const directUsed = item.usage_usd ?? item.quota_used_usd ?? item.quota_used;
  const candidates = [
    { used: directUsed, limit: directLimit },
    { used: item.monthly_usage_usd ?? asRecord(progress.monthly).used, limit: group.monthly_limit_usd ?? item.monthly_limit_usd ?? asRecord(progress.monthly).limit },
    { used: item.weekly_usage_usd ?? asRecord(progress.weekly).used, limit: group.weekly_limit_usd ?? item.weekly_limit_usd ?? asRecord(progress.weekly).limit },
    { used: item.daily_usage_usd ?? asRecord(progress.daily).used, limit: group.daily_limit_usd ?? item.daily_limit_usd ?? asRecord(progress.daily).limit },
  ];
  const selected = candidates.find((candidate) => candidate.limit != null && numberValue(candidate.limit) > 0);
  if (!selected) return null;
  const used = numberValue(selected.used);
  const limit = numberValue(selected.limit);
  return { used, limit, percentage: Math.max(0, Math.min(100, (used / limit) * 100)) };
}

async function handleUserMenuSelect(key: string | number): Promise<void> {
  if (key === "sign-out") await signOut();
}

function showCopyFeedback(keyId = ""): void {
  if (copyFeedbackTimer) window.clearTimeout(copyFeedbackTimer);
  copiedKeyId.value = keyId;
  endpointCopied.value = !keyId;
  copyFeedbackTimer = window.setTimeout(() => {
    copiedKeyId.value = "";
    endpointCopied.value = false;
  }, 1800);
}

async function copyEndpoint(): Promise<void> {
  errorMessage.value = "";
  try {
    await desktopApi.copyApiEndpoint();
    showCopyFeedback();
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  }
}

async function copyKey(item: Record<string, unknown>): Promise<void> {
  const keyId = String(item.id ?? "");
  if (!keyId) return;
  errorMessage.value = "";
  try {
    await desktopApi.copyApiKey(keyId, textValue(item.key));
    showCopyFeedback(keyId);
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  }
}

async function importKeyToCcSwitch(item: Record<string, unknown>): Promise<void> {
  const keyId = String(item.id ?? "");
  if (!keyId || importingKeyId.value) return;
  importingKeyId.value = keyId;
  errorMessage.value = "";
  try {
    if (consolePreview) throw new Error("预览模式不会导入真实密钥，请在 MollyCloud 客户端中操作");
    const provider = await desktopApi.importApiKeyToCcSwitch(keyId);
    importedProviders.value[keyId] = provider;
    ccSwitchPanel.value?.notifyProviderImported(provider);
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    importingKeyId.value = "";
  }
}

onMounted(async () => {
  if ("__TAURI_INTERNALS__" in window) {
    const { listen } = await import("@tauri-apps/api/event");
    unlistenAssistantConfig = await listen<AssistantConfig>("assistant-config-changed", (event) => {
      assistantConfig.value = event.payload;
    });
    unlistenAssistantHistory = await listen<{ messages?: unknown[] }>("petra-assistant-history-changed", (event) => {
      applyAssistantHistory(event.payload);
    });
    unlistenAssistantState = await listen<{ busy?: boolean; error?: string }>("petra-assistant-state-changed", (event) => {
      assistantSending.value = event.payload?.busy === true;
      assistantChatError.value = typeof event.payload?.error === "string" ? event.payload.error : "";
      if (!assistantSending.value) void requestAssistantHistory();
    });
  }
  void initialize();
});
onBeforeUnmount(() => {
  if (refreshTimer) window.clearInterval(refreshTimer);
  if (copyFeedbackTimer) window.clearTimeout(copyFeedbackTimer);
  unlistenAssistantConfig?.();
  unlistenAssistantHistory?.();
  unlistenAssistantState?.();
});
</script>

<template>
  <main v-if="phase === 'starting'" class="splash-shell" aria-live="polite">
    <img class="brand-logo brand-logo--large" src="/brand/mollycloud-logo.png" alt="MollyCloud" />
    <div class="loader-line"><span /></div>
    <p>正在连接 MollyCloud</p>
  </main>

  <main v-else-if="phase === 'login' || phase === 'two-factor'" class="auth-shell">
    <section class="auth-art" aria-label="Molly 桌面伙伴预览">
      <div class="ambient ambient--blue" />
      <div class="ambient ambient--lime" />
      <div class="brand-lockup">
        <img class="brand-logo" src="/brand/mollycloud-logo.png" alt="" />
        <div><strong>MollyCloud</strong><span>茉莉云</span></div>
      </div>
      <img src="/brand/molly.png" alt="Molly Live2D 角色" />
    </section>

    <section class="auth-panel">
      <div class="auth-form-wrap">
        <div class="eyebrow"><span /> MollyCloud 安全登录</div>
        <template v-if="phase === 'login'">
          <h1>欢迎回来</h1>
          <p class="lead">使用你在 {{ siteName }} 的现有账号登录。</p>

          <form @submit.prevent="submitLogin" novalidate>
            <label for="email">邮箱地址</label>
            <n-input v-model:value="email" class="login-input" size="large" placeholder="name@example.com" :input-props="{ id: 'email', type: 'email', autocomplete: 'username', 'aria-label': '邮箱地址' }" />

            <div class="label-row"><label for="password">密码</label></div>
            <n-input v-model:value="password" class="login-input" size="large" type="password" show-password-on="click" placeholder="输入你的密码" :input-props="{ id: 'password', autocomplete: 'current-password', 'aria-label': '密码' }" />

            <div class="auth-options">
              <n-checkbox v-model:checked="autoLogin" class="check-row">自动登录</n-checkbox>
              <n-checkbox v-if="agreementEnabled" v-model:checked="acceptedAgreement" class="check-row">
                我已阅读并同意 <button class="agreement-link" type="button" @click.stop="agreementOpen = true" @keydown.stop @keyup.stop>服务条款与使用地区说明</button>
              </n-checkbox>
            </div>

            <n-alert v-if="errorMessage" class="form-error" type="error" :show-icon="false">{{ errorMessage }}</n-alert>
            <n-button class="primary-button" type="primary" size="large" attr-type="submit" :loading="submitting" :disabled="!email || !password || (agreementEnabled && !acceptedAgreement)">
              {{ submitting ? "正在安全登录…" : "登录 MollyCloud" }}
            </n-button>
          </form>
        </template>

        <template v-else>
          <h1>两步验证</h1>
          <p class="lead">请输入验证器中为 {{ twoFactorHint }} 生成的 6 位代码。</p>
          <form @submit.prevent="submitTwoFactor">
            <label for="totp">验证码</label>
            <n-input v-model:value="totpCode" class="login-input totp-input" size="large" maxlength="6" placeholder="000000" :input-props="{ id: 'totp', inputmode: 'numeric', autocomplete: 'one-time-code', 'aria-label': '六位验证码' }" />
            <n-alert v-if="errorMessage" class="form-error" type="error" :show-icon="false">{{ errorMessage }}</n-alert>
            <n-button class="primary-button" type="primary" size="large" attr-type="submit" :loading="submitting" :disabled="totpCode.length !== 6">确认并登录</n-button>
            <n-button class="secondary-button" size="large" @click="phase = 'login'">返回登录</n-button>
          </form>
        </template>

        <div class="secure-note"><AppIcon name="shield" /> {{ autoLogin ? "登录令牌由 Windows 凭据管理器加密保存" : "未开启自动登录，关闭应用后需重新登录" }}</div>
      </div>
    </section>
  </main>

  <main v-else class="app-shell" :inert="consoleSettingsOpen || petSettingsOpen">
    <aside class="sidebar">
      <div class="sidebar-brand"><img class="brand-logo" src="/brand/mollycloud-logo.png" alt="" /><div><strong>MollyCloud</strong><span>茉莉云</span></div></div>
      <nav aria-label="主导航">
        <button v-for="item in navigation" :key="item.id" type="button" class="nav-item" :class="{ active: activePage === item.id }" :aria-current="activePage === item.id ? 'page' : undefined" @click="selectPage(item.id)">
          <AppIcon :name="item.icon" /><span>{{ item.label }}</span>
        </button>
      </nav>
      <div class="sidebar-footer">
        <button ref="consoleSettingsButton" class="sidebar-settings" type="button" aria-haspopup="dialog" :aria-expanded="consoleSettingsOpen" @click="consoleSettingsOpen = true">
          <AppIcon name="settings" /><span>设置</span>
        </button>
      </div>
    </aside>

    <section class="workspace" :class="{ 'workspace--assistant': activePage === 'assistant', 'workspace--embedded': activePage === 'ccswitch' || activePage === 'images' }">
      <header class="topbar">
        <div class="page-heading">
          <span class="page-kicker">{{ currentPageMeta.kicker }}</span>
          <h1>{{ activePage === 'overview' ? `${greeting}，${textValue(user.username, 'Molly 用户')}` : currentPageMeta.title }}</h1>
          <p>{{ currentPageMeta.description }}</p>
        </div>
        <div class="topbar-actions">
          <n-button class="balance-chip" quaternary :aria-label="`账户余额 ${formatBalance(user.balance)}，前往充值`" @click="openRecharge">
            <span class="balance-chip__face balance-chip__amount"><AppIcon class="balance-chip__icon" name="wallet" /><small>账户余额</small><strong>{{ formatBalance(user.balance) }}</strong></span>
            <span class="balance-chip__face balance-chip__recharge">前往充值</span>
          </n-button>
          <n-dropdown trigger="click" :options="userMenuOptions" @select="handleUserMenuSelect">
          <n-button class="user-chip" quaternary type="default">
            <span>{{ userInitial }}</span><div><strong>{{ textValue(user.username, 'Molly 用户') }}</strong><small>{{ textValue(user.email) }}</small></div><AppIcon name="chevron" />
          </n-button>
          </n-dropdown>
        </div>
      </header>

      <n-alert v-if="errorMessage" class="page-alert" type="error" :show-icon="false">{{ errorMessage }}</n-alert>
      <n-alert v-if="visibleAccountReminder" class="account-reminder" :class="{ 'account-reminder--subscription': visibleAccountReminder.kind === 'subscription' }" type="warning" :bordered="false" :show-icon="false">
        <div class="account-reminder-content">
          <div class="account-reminder-copy"><span class="reminder-spark"><AppIcon v-if="visibleAccountReminder.kind === 'subscription'" name="spark" /><template v-else>!</template></span><div><strong>{{ visibleAccountReminder.title }}</strong><small>{{ visibleAccountReminder.detail }}</small></div></div>
          <n-button size="small" type="primary" @click="openRecharge">前往充值</n-button>
        </div>
      </n-alert>

      <div v-if="activePage === 'overview'" class="page-content overview-page">
        <section class="stats-grid overview-stats" aria-label="账户概览">
          <n-card v-for="card in stats" :key="card.label" class="stat-card overview-stat-card" :class="`stat-card--${card.tone}`" :bordered="false" role="button" tabindex="0" @click="openOverviewMetric(card.target)" @keydown.enter.prevent="openOverviewMetric(card.target)" @keydown.space.prevent="openOverviewMetric(card.target)">
            <div class="stat-card__content">
              <span class="overview-stat-card__icon"><AppIcon :name="card.icon" /></span>
              <span>{{ card.label }}</span>
              <strong>{{ card.value }}</strong>
              <AppIcon class="overview-stat-card__arrow" name="arrow" />
            </div>
          </n-card>
        </section>

        <section class="dashboard-grid dashboard-grid--single">
          <n-card class="panel panel--wide overview-usage-panel" :bordered="false">
            <div class="overview-usage-heading"><h2><i />今日用量</h2><span class="updated">{{ lastUpdated ? `更新于 ${lastUpdated.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}` : '正在同步' }}</span></div>
            <div class="today-usage-grid">
              <div class="today-usage-item today-usage-item--lime"><span class="today-usage-item__icon"><AppIcon name="token" /></span><div><span>今日 Token</span><strong>{{ formatTokens(usage.today_tokens) }}</strong></div></div>
              <div class="today-usage-item today-usage-item--blue"><span class="today-usage-item__icon"><AppIcon name="request" /></span><div><span>今日请求</span><strong>{{ numberValue(usage.today_requests).toLocaleString('zh-CN') }}</strong></div></div>
              <div class="today-usage-item today-usage-item--ink"><span class="today-usage-item__icon"><AppIcon name="spend" /></span><div><span>今日消费</span><strong>{{ formatMoney(usage.today_actual_cost) }}</strong></div></div>
            </div>
            <div class="performance-row"><div><span>RPM</span><strong>{{ numberValue(usage.rpm).toFixed(1) }}</strong></div><div><span>TPM</span><strong>{{ formatTokens(usage.tpm) }}</strong></div><div><span>平均响应</span><strong>{{ Math.round(numberValue(usage.average_duration_ms)) }} ms</strong></div></div>
          </n-card>

        </section>
      </div>

      <div v-else-if="activePage === 'subscriptions'" class="page-content subscription-page">
        <div v-if="subscriptions.length" class="subscription-list">
          <n-card v-for="item in subscriptions" :key="String(item.id)" class="panel subscription-card" :bordered="false" role="link" tabindex="0" aria-label="在网页端打开我的订阅" @click="openSubscriptions" @keydown.enter.prevent="openSubscriptions" @keydown.space.prevent="openSubscriptions">
            <div class="subscription-card__plan"><n-tag class="state-pill" size="small" :type="item.status === 'active' ? 'success' : 'default'" :bordered="false">{{ item.status === 'active' ? '使用中' : textValue(item.status, '未知状态') }}</n-tag><h2>{{ subscriptionName(item) }}</h2></div>
            <div class="subscription-card__quota">
              <span>系统分配用量</span>
              <strong v-if="subscriptionQuota(item)"><b>{{ formatBalance(subscriptionQuota(item)?.used) }}</b><em>/ {{ formatBalance(subscriptionQuota(item)?.limit) }}</em></strong>
              <strong v-else><b>未设置上限</b></strong>
              <i><b :style="{ width: `${subscriptionQuota(item)?.percentage ?? 0}%` }" /></i>
            </div>
            <div class="subscription-card__date"><span>到期时间</span><strong>{{ subscriptionExpiresAt(item) ? formatDate(subscriptionExpiresAt(item)) : '长期有效' }}</strong></div>
            <div class="subscription-card__days"><strong>{{ subscriptionDaysRemaining(item) ?? '∞' }}</strong><small>{{ subscriptionDaysRemaining(item) == null ? '长期有效' : '剩余天数' }}</small></div>
            <AppIcon class="subscription-card__arrow" name="arrow" />
          </n-card>
        </div>
        <div v-else class="empty-state"><AppIcon name="subscription" /><h2>暂无活跃订阅</h2><p>余额模式仍可正常使用，购买后的订阅会显示在这里。</p></div>
      </div>

      <div v-else-if="activePage === 'keys'" class="page-content keys-page">
        <n-button class="endpoint-card" block secondary :class="{ copied: endpointCopied }" @click="copyEndpoint">
          <span class="endpoint-card__copy"><AppIcon name="copy" /></span>
          <span class="endpoint-card__content"><small>API 端点</small><code>{{ apiEndpoint }}</code></span>
          <span class="endpoint-card__action" aria-live="polite">{{ endpointCopied ? '已复制到剪贴板' : '点击复制' }}</span>
        </n-button>
        <n-card class="table-panel" :bordered="false" content-style="padding: 0">
          <n-data-table v-if="keys.length" :columns="keyTableColumns" :data="keys" :bordered="false" :single-line="true" :scroll-x="933" />
          <div v-else class="empty-state"><AppIcon name="key" /><h2>还没有 API 密钥</h2><p>请在 MollyCloud 网页控制台中创建密钥。</p></div>
        </n-card>
      </div>

      <div v-else-if="activePage === 'usage'" class="page-content usage-page">
        <section class="usage-stats" aria-label="累计用量与今日用量">
          <n-card class="usage-stat usage-stat--requests" :bordered="false">
            <span class="usage-stat__icon"><AppIcon name="request" /></span>
            <h2>累计请求</h2>
            <strong class="usage-stat__value">{{ numberValue(usage.total_requests).toLocaleString('zh-CN') }}</strong>
            <p>今日 <span>{{ numberValue(usage.today_requests).toLocaleString('zh-CN') }}</span></p>
          </n-card>
          <n-card class="usage-stat usage-stat--tokens" :bordered="false">
            <span class="usage-stat__icon"><AppIcon name="database" /></span>
            <h2>累计 Token</h2>
            <strong class="usage-stat__value">{{ formatTokens(usage.total_tokens) }}</strong>
            <p>今日 <span>{{ formatTokens(usage.today_tokens) }}</span></p>
          </n-card>
          <n-card class="usage-stat usage-stat--cost" :bordered="false">
            <span class="usage-stat__icon"><AppIcon name="spend" /></span>
            <h2>累计实际消费</h2>
            <strong class="usage-stat__value">{{ formatMoney(usage.total_actual_cost) }}</strong>
            <p>今日 <span>{{ formatMoney(usage.today_actual_cost) }}</span></p>
          </n-card>
        </section>
        <n-card class="panel usage-breakdown" :bordered="false">
          <div class="panel-heading"><div><span class="section-kicker">Token 分类</span><h2>Token 构成</h2></div></div>
          <dl class="token-grid">
            <div class="token-category"><dt><span class="token-category__icon"><AppIcon name="edit" /></span>输入</dt><dd>{{ formatTokens(usage.total_input_tokens) }}</dd></div>
            <div class="token-category"><dt><span class="token-category__icon"><AppIcon name="upload" /></span>输出</dt><dd>{{ formatTokens(usage.total_output_tokens) }}</dd></div>
            <div class="token-category"><dt><span class="token-category__icon"><AppIcon name="database" /></span>缓存创建</dt><dd>{{ formatTokens(usage.total_cache_creation_tokens) }}</dd></div>
            <div class="token-category"><dt><span class="token-category__icon"><AppIcon name="document" /></span>缓存读取</dt><dd>{{ formatTokens(usage.total_cache_read_tokens) }}</dd></div>
          </dl>
        </n-card>
      </div>

      <div v-else-if="activePage === 'assistant'" class="page-content assistant-page">
        <n-card class="panel assistant-chat" :bordered="false">
          <div class="assistant-chat__toolbar">
            <div><span class="section-kicker">共享会话</span><h2>对话记录</h2></div>
            <div class="assistant-toggle-cell">
              <span>{{ petVisible ? '开启' : '隐藏' }}</span>
              <n-switch size="large" :value="petVisible" :loading="petToggleLoading" aria-label="显示或隐藏 Live2D 桌宠" @update:value="setPetVisibility" />
              <button ref="petSettingsButton" class="assistant-settings-button" type="button" aria-label="桌宠设置" aria-haspopup="dialog" :aria-expanded="petSettingsOpen" @click="petSettingsOpen = true"><AppIcon name="settings" /></button>
            </div>
          </div>

          <div ref="assistantHistory" class="assistant-chat__history" aria-live="polite" :aria-busy="assistantSending">
            <div v-if="assistantMessages.length === 0" class="assistant-chat__empty">
              <div class="assistant-avatar assistant-avatar--molly"><img src="/brand/molly.png" alt="Molly" /></div>
              <strong>还没有对话</strong>
              <p>和 Molly 打个招呼吧，这里的对话会与桌面 Molly 同步。</p>
            </div>
            <article v-for="(message, index) in assistantMessages" :key="`${index}-${message.role}`" class="assistant-message" :class="`assistant-message--${message.role}`">
              <div v-if="message.role === 'assistant'" class="assistant-avatar assistant-avatar--molly"><img src="/brand/molly.png" alt="Molly" /></div>
              <div class="assistant-message__body">
                <span>{{ message.role === 'assistant' ? 'Molly' : textValue(user.username, '我') }}</span>
                <p>{{ message.content }}</p>
              </div>
              <div v-if="message.role === 'user'" class="assistant-avatar assistant-avatar--user" :aria-label="textValue(user.username, '用户')">{{ userInitial }}</div>
            </article>
            <article v-if="assistantSending && assistantMessages[assistantMessages.length - 1]?.role !== 'assistant'" class="assistant-message assistant-message--assistant">
              <div class="assistant-avatar assistant-avatar--molly"><img src="/brand/molly.png" alt="Molly" /></div>
              <div class="assistant-message__body assistant-message__body--typing"><span>Molly</span><p><i /><i /><i /></p></div>
            </article>
          </div>

          <div class="assistant-composer">
            <n-alert v-if="assistantChatError" class="assistant-chat__error" type="error" :show-icon="false">{{ assistantChatError }}</n-alert>
            <div class="assistant-composer__row">
              <n-input v-model:value="assistantDraft" class="assistant-chat__input" size="large" maxlength="2000" :disabled="assistantSending" :input-props="{ 'aria-label': '给 Molly 发送消息' }" placeholder="给 Molly 发送消息…" @keydown="handleAssistantKeydown" />
              <n-button class="assistant-send-button" type="primary" :loading="assistantSending" :disabled="!assistantDraft.trim() || assistantSending" @click="sendAssistantMessage">发送</n-button>
            </div>
          </div>
        </n-card>
      </div>

      <div v-if="ccSwitchVisited" v-show="activePage === 'ccswitch'" class="page-content ccswitch-page">
        <CcSwitchPanel ref="ccSwitchPanel" :preview="consolePreview" :target="ccSwitchTarget" />
      </div>

      <div v-if="imageWorkbenchVisited" v-show="activePage === 'images'" class="page-content embedded-page">
        <ImageWorkbenchPanel v-if="imageAccount" :key="imageAccount" :account="imageAccount" :preview="consolePreview" @settled="refreshDashboard" />
        <p v-else role="status">无法识别当前账户，请刷新账户信息后重试。</p>
      </div>

    </section>
  </main>

  <div id="console-settings-layer" class="console-settings-layer" />
  <ConsoleSettingsDialog v-if="phase === 'dashboard'" v-model:show="consoleSettingsOpen" @closed="consoleSettingsButton?.focus()" />
  <PetSettingsDialog v-if="phase === 'dashboard'" v-model:show="petSettingsOpen" @closed="petSettingsButton?.focus()" />

  <n-modal v-model:show="agreementOpen" preset="card" class="agreement-modal" title="登录协议" :bordered="false" :mask-closable="true">
      <div class="agreement-content"><article v-for="doc in agreementDocuments" :key="textValue(doc.id)"><h3>{{ textValue(doc.title) }}</h3><p>{{ textValue(doc.content_md) }}</p></article></div>
      <template #action><n-button type="primary" @click="acceptedAgreement = true; agreementOpen = false">我已阅读并同意</n-button></template>
  </n-modal>
</template>
