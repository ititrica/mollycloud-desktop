<script setup lang="ts">
import { computed, defineAsyncComponent, nextTick, KeepAlive, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { isWindows, isMacOS, credentialStoreName } from "./platform";
import {
  NAlert,
  NButton,
  NCard,
  NCheckbox,
  NDropdown,
  NInput,
  NModal,
  NSwitch,
  NTag,
  type DropdownOption,
} from "naive-ui";
import { plugins, loadPlugins, pluginUsable, pluginKey } from "./plugins";
import { useSlidingSelection } from "./useSlidingSelection";
import { enterNavPanel, leaveNavPanel, cancelNavPanel } from "./navigationMotion";
import AppIcon from "./components/AppIcon.vue";
import CcSwitchImportDialog from "./components/CcSwitchImportDialog.vue";
import CcSwitchPanel from "./components/CcSwitchPanel.vue";
import ImageWorkbenchPanel from "./components/ImageWorkbenchPanel.vue";
import ConsoleSettingsDialog from "./components/ConsoleSettingsDialog.vue";
import PetSettingsDialog from "./components/PetSettingsDialog.vue";
import AssistantConversation from "./components/AssistantConversation.vue";
import { keyUsage, keyQuota, costLabel } from "./keyUsage";
import SkillManagerPanel from "./components/SkillManagerPanel.vue";
const NetSpeedPanel = isWindows ? defineAsyncComponent(() => import("./components/NetSpeedPanel.vue")) : undefined;
import KeyGroupPicker from "./components/KeyGroupPicker.vue";
import CreateKeyDialog from "./components/CreateKeyDialog.vue";
import DeleteKeyDialog from "./components/DeleteKeyDialog.vue";
import { groupFromKey } from "./keyGroups";
import RechargePanel from "./components/RechargePanel.vue";
import SubscriptionsPanel from "./components/SubscriptionsPanel.vue";
import type { SpeechStatus } from "./speech";
import { emitTo } from "@tauri-apps/api/event";
import { asArray, asRecord, type AccountBalance, type AssistantConfig, type DashboardPayload, type ServiceBootstrap, type KeyGroupChanged } from "./contracts";
import { formatBalance, formatDate, formatMoney, formatTokens, numberValue, textValue } from "./format";
import { desktopApi, type CcSwitchImportResult, type DesktopUpdate } from "./ipc";

type Page = "overview" | "subscriptions" | "keys" | "usage" | "recharge" | "assistant" | "ccswitch" | "netspeed" | "images" | "skills";
type Phase = "starting" | "login" | "two-factor" | "dashboard";
type AssistantDisplayMessage = { role: "user" | "assistant"; content: string };
type OverviewIcon = "wallet" | "spend" | "subscription" | "key" | "request" | "token" | "database" | "clock";
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
const settingsTab = ref<"general" | "plugins">("general");
function openPluginSettings() { settingsTab.value = "plugins"; consoleSettingsOpen.value = true; }
watch(phase, (value) => { if (value === "dashboard") void loadPlugins(); });
const consoleSettingsButton = ref<HTMLButtonElement | null>(null);
const petSettingsOpen = ref(false);
const skillsVisited = ref(false);
const skillsModalOpen = ref(false);
const subscriptionModalOpen = ref(false);
const sidebarCollapsed = ref(false);
const ccSwitchImportOpen = ref(false);
const ccSwitchImportKeyId = ref("");
const ccSwitchImportName = ref("");
const createKeyOpen = ref(false);
const deleteKeyOpen = ref(false);
const deleteKeyTarget = ref({ id: "", name: "" });
const deleteKeyTrigger = ref<HTMLElement | null>(null);
// Ignore stale dashboard responses that started before a confirmed deletion.
const deletedKeyIds = ref(new Set<string>());
const createKeyTrigger = ref<HTMLElement | null>(null);
const keyGroupSuccess = ref("");
const rechargeVisited = ref(false);
const userMenuOpen = ref(false);
const submitting = ref(false);
const refreshing = ref(false);
const errorMessage = ref("");
const twoFactorHint = ref("");
const lastUpdated = ref<Date | null>(null);
const assistantConfig = ref<AssistantConfig | null>(null);
const petVisible = ref(true);
const petToggleLoading = ref(false);
const copiedKeyId = ref("");
const importedProviders = ref<Record<string, CcSwitchImportResult>>({});
const ccSwitchVisited = ref(false);
const imageWorkbenchVisited = ref(false);
const ccSwitchTarget = ref<CcSwitchImportResult | null>(null);
const ccSwitchPanel = ref<InstanceType<typeof CcSwitchPanel> | null>(null);
const sidebarNav = ref<HTMLElement | null>(null);
const overviewNav = ref<HTMLElement | null>(null);
const primarySelection = computed(() => ["overview", "subscriptions", "usage", "recharge"].includes(activePage.value) ? "overview" : activePage.value);
useSlidingSelection(sidebarNav, primarySelection);
useSlidingSelection(overviewNav, activePage, true);
const consolePreview = import.meta.env.DEV && new URLSearchParams(window.location.search).get("ui-preview") === "console";
const endpointCopied = ref(false);
const assistantMessages = ref<AssistantDisplayMessage[]>([]);
const assistantDraft = ref("");
const assistantSending = ref(false);
const assistantChatError = ref("");
const assistantSpeechStatus = ref<SpeechStatus>({ phase: "idle" });
async function stopSpeech() {
  assistantSpeechStatus.value = { phase: "idle" };
  if (!consolePreview) await emitTo("main", "assistant-speech-stop").catch(() => {});
}
const assistantPanel = ref<{ scrollToEnd?: () => void } | null>(null);
const assistantTab = ref<"chat" | "settings">("chat");
const assistantNav = ref<HTMLElement | null>(null);
const assistantSettingsBusy = ref(false);
useSlidingSelection(assistantNav, assistantTab, true);
const assistantPanelProps = computed(() => assistantTab.value === "chat" ? { messages: assistantMessages.value, username: textValue(user.value.username, "我"), draft: assistantDraft.value, sending: assistantSending.value, error: assistantChatError.value, speechStatus: assistantSpeechStatus.value } : { show: true, embedded: true });
const desktopUpdate = ref<DesktopUpdate | null>(null);
const updateDialogOpen = ref(false);
const updateDialogError = ref("");
const updateOpening = ref(false);
const notifiedUpdateVersion = ref<string | null>(null);
watch([desktopUpdate, phase, consoleSettingsOpen, petSettingsOpen, ccSwitchImportOpen, skillsModalOpen, subscriptionModalOpen, createKeyOpen, deleteKeyOpen], ([update, currentPhase]) => {
  if (!update || currentPhase !== "dashboard") {
    updateDialogOpen.value = false;
    return;
  }
  if (consoleSettingsOpen.value || petSettingsOpen.value || ccSwitchImportOpen.value || skillsModalOpen.value || subscriptionModalOpen.value || createKeyOpen.value || deleteKeyOpen.value) return;
  if (currentPhase === "dashboard" && update && notifiedUpdateVersion.value !== update.version) {
    notifiedUpdateVersion.value = update.version;
    updateDialogError.value = "";
    updateDialogOpen.value = true;
  }
});
let refreshTimer: number | undefined;
let balanceRefreshTimer: number | undefined;
let balanceRefreshInFlight = false;
let copyFeedbackTimer: number | undefined;
let unlistenAssistantConfig: (() => void) | undefined;
let unlistenAssistantHistory: (() => void) | undefined;
let unlistenSpeech: (() => void) | undefined;
let unlistenAssistantState: (() => void) | undefined;
let unlistenPetVisibility: (() => void) | undefined;
let unlistenExternalImport: (() => void) | undefined;
let unlistenNavigation: (() => void) | undefined;
const pendingExternalImport = ref(false);
watch(phase, (next) => {
  if (next === "dashboard" && pendingExternalImport.value) {
    pendingExternalImport.value = false;
    selectPage("ccswitch");
  }
});

const navigation: Array<{ id: Page; label: string; icon: "overview" | "subscription" | "project-key" | "key" | "usage" | "assistant" | "code" | "image" | "skills" | "island" }> = [
  { id: "overview", label: "概览", icon: "overview" },
  { id: "keys", label: "API 密钥", icon: "project-key" },
  { id: "ccswitch", label: "CC Switch", icon: "code" },
  { id: "assistant", label: "Molly助手", icon: "assistant" },
  ...(isWindows ? [{ id: "netspeed" as const, label: "灵动岛", icon: "island" as const }] : []),
  { id: "images", label: "生图工作台", icon: "image" },
  { id: "skills", label: "Skill 管理器", icon: "skills" },
];

const availablePageIds = new Set<Page>([
  ...navigation.map(item => item.id), "subscriptions", "usage", "recharge",
]);

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
const keys = computed(() => asArray(keyPage.value.items).filter(item => !deletedKeyIds.value.has(String(item.id))));
const userInitial = computed(() => textValue(user.value.username ?? user.value.email, "M").slice(0, 1).toUpperCase());
const overviewPages = [
  { id: "overview", label: "账户概览" },
  { id: "subscriptions", label: "订阅" },
  { id: "usage", label: "用量" },
  { id: "recharge", label: "充值" },
] as const;
const isOverviewPage = computed(() => overviewPages.some((item) => item.id === activePage.value));
const isEmbeddedPage = computed(() => ["assistant", "ccswitch", "images"].includes(activePage.value));
const isToolPage = computed(() => ["netspeed", "skills"].includes(activePage.value));
const showAccountHeader = computed(() => !isEmbeddedPage.value && !isToolPage.value);
const apiEndpoint = "https://mollycloud.cn/v1";
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

const stats = computed<Array<{ label: string; value: string; detail: string; tone: string; icon: OverviewIcon; target: OverviewTarget }>>(() => [
  { label: "余额", value: formatBalance(user.value.balance), detail: "可用余额", tone: "lime", icon: "wallet", target: "recharge" },
  { label: "API 密钥", value: String(numberValue(keyPage.value.total ?? usage.value.active_api_keys)), detail: `${numberValue(usage.value.active_api_keys)} 个启用`, tone: "neutral", icon: "key", target: "keys" },
  { label: "今日请求", value: numberValue(usage.value.today_requests).toLocaleString("zh-CN"), detail: `累计 ${numberValue(usage.value.total_requests).toLocaleString("zh-CN")}`, tone: "lime", icon: "request", target: "usage" },
  { label: "今日消费", value: formatMoney(usage.value.today_actual_cost), detail: `累计 ${formatMoney(usage.value.total_actual_cost)}`, tone: "neutral", icon: "spend", target: "usage" },
  { label: "今日 Token", value: formatTokens(usage.value.today_tokens), detail: "今日累计使用", tone: "amber", icon: "token", target: "usage" },
  { label: "累计 Token", value: formatTokens(usage.value.total_tokens), detail: "全部历史用量", tone: "neutral", icon: "database", target: "usage" },
  { label: "活跃订阅", value: String(numberValue(subscriptionSummary.value.active_count)), detail: "查看订阅权益", tone: "lime", icon: "subscription", target: "subscriptions" },
  { label: "平均响应", value: usage.value.average_duration_ms == null ? "—" : `${(numberValue(usage.value.average_duration_ms) / 1000).toFixed(2)}s`, detail: "平均请求耗时", tone: "rose", icon: "clock", target: "usage" },
]);
const tokenBreakdown = computed(() => {
  const parts = [
    { label: "输入", value: numberValue(usage.value.total_input_tokens), tone: "blue" },
    { label: "输出", value: numberValue(usage.value.total_output_tokens), tone: "lime" },
    { label: "缓存创建", value: numberValue(usage.value.total_cache_creation_tokens), tone: "amber" },
    { label: "缓存读取", value: numberValue(usage.value.total_cache_read_tokens), tone: "violet" },
  ];
  const total = parts.reduce((sum, part) => sum + part.value, 0);
  return parts.map((part) => ({ ...part, share: total ? part.value / total * 100 : 0 }));
});

const userMenuOptions: DropdownOption[] = [
  { label: "退出登录", key: "sign-out", props: { class: "naive-user-menu__logout" } },
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
    if (previewPage && availablePageIds.has(previewPage as Page)) selectPage(previewPage as Page);
    bootstrap.value = { service_origin: "https://mollycloud.cn", health: { status: "ok" }, settings: { site_name: "MollyCloud" } };
    dashboard.value = {
      user: { username: "演示用户", email: "demo@mollycloud.cn", balance: 12.35 },
      subscriptions: { active_count: 2, subscriptions: [
        { id: 9101, status: "active", expires_at: new Date(Date.now() + 6 * 86_400_000).toISOString(), group: { name: "GPT Pro 高级推理周订阅", weekly_limit_usd: 120 } },
        { id: 9102, status: "active", expires_at: new Date(Date.now() + 24 * 86_400_000).toISOString(), monthly_usage_usd: 28.4, group: { name: "Claude Plus 月订阅", monthly_limit_usd: 200 } },
      ] },
      subscription_progress: [],
      usage: {
        today_tokens: 128400, today_requests: 48, today_actual_cost: 1.28,
        total_requests: 2110, total_tokens: 197159800, total_actual_cost: 11.7333,
        total_input_tokens: 17010000, total_output_tokens: 809800,
        total_cache_creation_tokens: 0, total_cache_read_tokens: 179340000,
        active_api_keys: 2,
      },
      keys: { items: [{ id: "demo-key", name: "Molly Desktop", key: "sk-••••942A", status: "active", quota_used: 0, usage: { total_actual_cost: 12.6032, today_actual_cost: 0.0278 }, group_id: 1, group: { name: "Molly Pro", platform: "openai", rate_multiplier: 1, subscription_type: "subscription" } }] },
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
      startBalanceRefreshTimer();
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
    startBalanceRefreshTimer();
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
    startBalanceRefreshTimer();
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    submitting.value = false;
  }
}

let subscriptionRefreshQueued = false;
function refreshSubscriptionDashboard(): void {
  // A read started before a paid reset may still return the old balance.
  if (refreshing.value) subscriptionRefreshQueued = true;
  else void refreshDashboard();
}
async function refreshDashboard(): Promise<void> {
  if (refreshing.value) return;
  refreshing.value = true;
  errorMessage.value = "";
  try {
    dashboard.value = await desktopApi.fetchDashboard();
    void desktopApi.syncPetAccountBalance(accountBalanceFromDashboard(dashboard.value));
    lastUpdated.value = new Date();
  } catch (reason) {
    if (!dashboard.value) errorMessage.value = friendlyError(reason);
  } finally {
    refreshing.value = false;
    if (subscriptionRefreshQueued) {
      subscriptionRefreshQueued = false;
      void refreshDashboard();
    }
  }
}

function accountBalanceFromDashboard(payload: DashboardPayload): AccountBalance {
  return {
    balance: asRecord(payload.user).balance ?? null,
    today_tokens: asRecord(payload.usage).today_tokens ?? null,
  };
}

function applyAccountBalance(account: AccountBalance): void {
  if (!dashboard.value) return;
  dashboard.value = {
    ...dashboard.value,
    user: { ...asRecord(dashboard.value.user), balance: account.balance },
    usage: { ...asRecord(dashboard.value.usage), today_tokens: account.today_tokens },
  };
}

async function refreshAccountBalance(): Promise<void> {
  if (balanceRefreshInFlight) return;
  balanceRefreshInFlight = true;
  try {
    const account = await desktopApi.fetchAccountBalance();
    applyAccountBalance(account);
    void desktopApi.syncPetAccountBalance(account);
  } catch (reason) {
    if (!dashboard.value) errorMessage.value = friendlyError(reason);
  } finally {
    balanceRefreshInFlight = false;
  }
}


function startRefreshTimer(): void {
  if (refreshTimer) window.clearInterval(refreshTimer);
  refreshTimer = window.setInterval(() => void refreshDashboard(), 60_000);
}

function startBalanceRefreshTimer(): void {
  if (balanceRefreshTimer) window.clearInterval(balanceRefreshTimer);
  balanceRefreshTimer = window.setInterval(() => void refreshAccountBalance(), 12_000);
}

async function signOut(): Promise<void> {
  deleteKeyOpen.value = false;
  createKeyOpen.value = false;
  rechargeVisited.value = false;
  await desktopApi.logout();
  if (refreshTimer) window.clearInterval(refreshTimer);
  if (balanceRefreshTimer) window.clearInterval(balanceRefreshTimer);
  void desktopApi.syncPetAccountBalance({ balance: null, today_tokens: null });
  dashboard.value = null;
  deletedKeyIds.value.clear();
  keyGroupSuccess.value = "";
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
  await assistantPanel.value?.scrollToEnd?.();
}

function applyAssistantHistory(payload: { messages?: unknown[] }): void {
  const messages = Array.isArray(payload?.messages) ? payload.messages : [];
  assistantMessages.value = messages.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const message = item as Record<string, unknown>;
    if ((message.role !== "user" && message.role !== "assistant") || typeof message.content !== "string") return [];
    const content = message.content.trim().slice(0, 2000);
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
  if (assistantSettingsBusy.value) return;
  activePage.value = page;
  if (page === "recharge") rechargeVisited.value = true;
  if (page === "ccswitch") ccSwitchVisited.value = true;
  if (page === "images") imageWorkbenchVisited.value = true;
  if (page === "skills") skillsVisited.value = true;
  if (page === "assistant") {
    void loadPetVisibility();
    void requestAssistantHistory();
  }
}

function toggleSidebar(): void {
  sidebarCollapsed.value = !sidebarCollapsed.value;
}

async function withConsoleWindow(action: "minimize" | "toggleMaximize" | "close"): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[action]();
  } catch (error) {
    console.error(`窗口操作失败：${action}`, error);
  }
}

function minimizeConsoleWindow(): void {
  void withConsoleWindow("minimize");
}

function toggleConsoleMaximize(): void {
  void withConsoleWindow("toggleMaximize");
}

function closeConsoleWindow(): void {
  void withConsoleWindow("close");
}


function openRecharge(): void { selectPage("recharge"); }

function openCreateKey(event: MouseEvent): void {
  createKeyTrigger.value = event.currentTarget as HTMLElement;
  createKeyOpen.value = true;
}
function openDeleteKey(item: Record<string, unknown>, event: MouseEvent): void {
  deleteKeyTrigger.value = event.currentTarget as HTMLElement;
  deleteKeyTarget.value = { id: String(item.id), name: textValue(item.name, '未命名') };
  deleteKeyOpen.value = true;
}
function handleKeyDeleted(keyId: string): void {
  deletedKeyIds.value.add(keyId);
  if (dashboard.value) asRecord(dashboard.value.keys).items = keys.value;
  delete importedProviders.value[keyId];
  if (copiedKeyId.value === keyId) copiedKeyId.value = "";
  keyGroupSuccess.value = `已删除密钥 ${deleteKeyTarget.value.name}`;
  if (!consolePreview) void refreshDashboard();
}
function restoreDeleteKeyFocus(): void {
  const target = deleteKeyTrigger.value;
  deleteKeyTrigger.value = null;
  // Wait for the modal focus trap to unmount before restoring the trigger.
  void nextTick(() => {
    if (deleteKeyOpen.value || activePage.value !== 'keys') return;
    if (target?.isConnected) target.focus();
    else document.querySelector<HTMLButtonElement>('.create-key-button')?.focus();
  });
}
function handleKeyCreated(item: Record<string, unknown>): void {
  if (dashboard.value) {
    const page = asRecord(dashboard.value.keys);
    page.items = [item, ...keys.value];
  }
  keyGroupSuccess.value = `已创建密钥 ${textValue(item.name)}`;
  if (!consolePreview) void refreshDashboard();
}
function handleKeyGroupChanged(result: KeyGroupChanged): void {
  const item = keys.value.find(item => String(item.id) === result.key_id);
  if (item) {
    item.group_id = result.group.id;
    item.group_name = result.group.name;
    item.group = { ...asRecord(item.group), id: result.group.id, name: result.group.name, platform: result.group.platform, rate_multiplier: result.group.rate, description: result.group.description, subscription_type: result.group.subscription ? "subscription" : "standard" };
  }
  keyGroupSuccess.value = `分组已切换为 ${result.group.name}`;
  if (!consolePreview) void refreshDashboard();
}

async function checkForDesktopUpdate(): Promise<void> {
  if (consolePreview) return;
  try {
    desktopUpdate.value = await desktopApi.checkForDesktopUpdate();
  } catch (reason) {
    // 启动检查是静默的：网络或发布端暂时不可用时不干扰登录和控制台。
    console.info("MollyCloud 更新检查未完成", friendlyError(reason));
  }
}

async function openDesktopUpdate(): Promise<void> {
  const update = desktopUpdate.value;
  if (!update || updateOpening.value) return;
  updateOpening.value = true;
  updateDialogError.value = "";
  try {
    await desktopApi.openDesktopUpdate(update.downloadUrl);
    updateDialogOpen.value = false;
  } catch (reason) {
    updateDialogError.value = `无法打开更新下载：${friendlyError(reason)}`;
  } finally {
    updateOpening.value = false;
  }
}

function handleSettingsUpdateCheck(update: DesktopUpdate | null): void {
  if (!update) return;
  desktopUpdate.value = update;
  consoleSettingsOpen.value = false;
  void nextTick(() => { updateDialogError.value = ""; updateDialogOpen.value = true; });
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

function openCcSwitchImport(item: Record<string, unknown>): void {
  const keyId = String(item.id ?? "");
  if (!keyId) return;
  ccSwitchImportKeyId.value = keyId;
  const keyName = textValue(item.name, "未命名").trim();
  ccSwitchImportName.value = (keyName ? `MollyCloud · ${keyName}` : "MollyCloud").slice(0, 80);
  errorMessage.value = "";
  ccSwitchImportOpen.value = true;
}

function restoreCcSwitchImportFocus(): void {
  const keyId = ccSwitchImportKeyId.value;
  window.setTimeout(() => {
    const buttons = document.querySelectorAll<HTMLButtonElement>(".ccs-import-button");
    Array.from(buttons).find((button) => button.dataset.keyId === keyId)?.focus();
  }, 0);
}

function handleCcSwitchImported(keyId: string, provider: CcSwitchImportResult): void {
  importedProviders.value[keyId] = provider;
  ccSwitchPanel.value?.notifyProviderImported(provider);
}

onMounted(async () => {
  if ("__TAURI_INTERNALS__" in window) {
    const { listen } = await import("@tauri-apps/api/event");
    if (isWindows) {
      unlistenNavigation = await listen<string>("molly:navigate", event => {
        if (event.payload === "netspeed" && phase.value === "dashboard") selectPage("netspeed");
      });
    }
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
    unlistenSpeech = await listen<SpeechStatus>("assistant-speech-state", event => { assistantSpeechStatus.value = event.payload; });
    unlistenPetVisibility = await listen<boolean>("pet-visibility-changed", (event) => {
      petVisible.value = event.payload;
    });
    unlistenExternalImport = await listen("ccswitch-external-import", () => {
      if (phase.value === "dashboard") {
        selectPage("ccswitch");
        void ccSwitchPanel.value?.flushExternalImports();
      }
      else pendingExternalImport.value = true;
    });
    const { invoke } = await import("@tauri-apps/api/core");
    if (await invoke<boolean>("has_ccswitch_external_import").catch(() => false)) {
      pendingExternalImport.value = true;
    }
  }
  void initialize();
  void checkForDesktopUpdate();
});
onBeforeUnmount(() => {
  if (refreshTimer) window.clearInterval(refreshTimer);
  if (balanceRefreshTimer) window.clearInterval(balanceRefreshTimer);
  if (copyFeedbackTimer) window.clearTimeout(copyFeedbackTimer);
  unlistenAssistantConfig?.();
  unlistenAssistantHistory?.();
  unlistenAssistantState?.();
  unlistenSpeech?.();
  unlistenPetVisibility?.();
  unlistenExternalImport?.();
  unlistenNavigation?.();
});
</script>

<template>
  <div class="console-window" :class="{ 'console-window--macos': isMacOS, 'console-window--sidebar-collapsed': sidebarCollapsed }">
    <header :inert="skillsModalOpen" class="window-titlebar" data-tauri-drag-region @dblclick="toggleConsoleMaximize">
      <button v-if="phase === 'dashboard'" class="sidebar-toggle window-titlebar__sidebar-toggle" type="button" :aria-label="sidebarCollapsed ? '展开菜单栏' : '收起菜单栏'" :aria-pressed="sidebarCollapsed" :title="sidebarCollapsed ? '展开菜单栏' : '收起菜单栏'" @mousedown.stop @dblclick.stop @click="toggleSidebar">
        <AppIcon name="sidebar" />
      </button>
      <div class="window-titlebar__drag" data-tauri-drag-region />
      <div v-if="!isMacOS" class="window-titlebar__controls">
        <button type="button" aria-label="最小化窗口" title="最小化" @mousedown.stop @dblclick.stop @click="minimizeConsoleWindow"><span class="window-control-icon window-control-icon--minimize" /></button>
        <button type="button" aria-label="最大化或还原窗口" title="最大化或还原" @mousedown.stop @dblclick.stop @click="toggleConsoleMaximize"><span class="window-control-icon window-control-icon--maximize" /></button>
        <button class="window-titlebar__close" type="button" aria-label="关闭窗口" title="关闭" @mousedown.stop @dblclick.stop @click="closeConsoleWindow"><span class="window-control-icon window-control-icon--close" /></button>
      </div>
    </header>

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

        <div class="secure-note"><AppIcon name="shield" /> {{ autoLogin ? `登录令牌由 ${credentialStoreName}加密保存` : "未开启自动登录，关闭应用后需重新登录" }}</div>
      </div>
    </section>
  </main>

  <main v-else class="app-shell" :class="{ 'app-shell--sidebar-collapsed': sidebarCollapsed }" :inert="consoleSettingsOpen || petSettingsOpen || ccSwitchImportOpen || updateDialogOpen || createKeyOpen || deleteKeyOpen || subscriptionModalOpen">
    <aside :inert="skillsModalOpen" class="sidebar">
      <div class="sidebar-project-bar">
        <div class="sidebar-project-brand" aria-label="MollyCloud">
          <img src="/brand/mollycloud-logo.png" alt="" />
          <strong>MollyCloud</strong>
        </div>
      </div>
      <div class="sidebar-nav-viewport">
        <nav ref="sidebarNav" aria-label="主导航">
          <i class="selection-indicator" aria-hidden="true" />
          <button v-for="item in navigation" :key="item.id" :disabled="assistantSettingsBusy" type="button" class="nav-item" :class="{ active: item.id === 'overview' ? isOverviewPage : activePage === item.id }" :data-page="item.id" :aria-label="item.label" :title="sidebarCollapsed ? item.label : undefined" :aria-current="item.id === 'overview' ? (isOverviewPage ? (activePage === 'overview' ? 'page' : 'true') : undefined) : (activePage === item.id ? 'page' : undefined)" @click="selectPage(item.id)">
            <AppIcon :name="item.icon" /><span>{{ item.label }}</span>
          </button>
        </nav>
      </div>
      <div class="sidebar-footer">
        <button ref="consoleSettingsButton" class="sidebar-settings" type="button" aria-label="设置" :title="sidebarCollapsed ? '设置' : undefined" aria-haspopup="dialog" :aria-expanded="consoleSettingsOpen" @click="settingsTab = 'general'; consoleSettingsOpen = true">
          <AppIcon name="settings" /><span>设置</span>
        </button>
      </div>
    </aside>

    <section class="workspace" :class="{ 'workspace--assistant': activePage === 'assistant', 'workspace--embedded': isEmbeddedPage, 'workspace--tool': isToolPage }">
      <header :inert="skillsModalOpen" v-if="showAccountHeader" class="topbar">
        <nav v-if="isOverviewPage" ref="overviewNav" class="overview-subnav molly-subnav" aria-label="概览二级菜单">
          <i class="selection-indicator" aria-hidden="true" />
          <button v-for="item in overviewPages" :key="item.id" type="button" :data-page="item.id" :aria-current="activePage === item.id ? 'page' : undefined" @click="selectPage(item.id)">{{ item.label }}</button>
        </nav>
        <div v-else-if="activePage === 'keys'" class="key-toolbar" aria-label="密钥操作">
          <n-button type="primary" class="create-key-button" @click="openCreateKey">＋ 创建密钥</n-button>
          <button class="endpoint-copy-button" type="button" :class="{ copied: endpointCopied }" :aria-label="`复制 API 端点 ${apiEndpoint}`" :title="`API 端点：${apiEndpoint}，点击复制`" @click="copyEndpoint">
            <code>{{ apiEndpoint }}</code><AppIcon name="copy" /><span v-if="endpointCopied" class="endpoint-copy-feedback" role="status">已复制</span>
          </button>
        </div>
        <div class="topbar-actions">
            <button class="account-refresh-button" :class="{ 'is-refreshing': refreshing }" type="button" :disabled="refreshing" :aria-busy="refreshing" aria-label="刷新账户数据" title="刷新账户数据" @click="refreshDashboard">
              <AppIcon name="refresh" />
            </button>
          <n-button v-if="desktopUpdate" class="update-available" secondary :title="`发现 MollyCloud ${desktopUpdate.version}，查看更新说明`" :aria-label="`软件可更新，版本 ${desktopUpdate.version}，查看更新说明`" @click="updateDialogError = ''; updateDialogOpen = true">
            <AppIcon name="update" /><span>软件可更新</span>
          </n-button>
          <n-button class="balance-chip" quaternary :aria-label="`账户余额 ${formatBalance(user.balance)}，前往充值`" @click="openRecharge">
            <span class="balance-chip__face balance-chip__amount"><AppIcon class="balance-chip__icon" name="wallet" /><strong>{{ formatBalance(user.balance) }}</strong></span>
            <span class="balance-chip__face balance-chip__recharge">前往充值</span>
          </n-button>
          <div class="topbar-account-actions">
            <n-dropdown trigger="click" :options="userMenuOptions" @select="handleUserMenuSelect" @update:show="userMenuOpen = $event">
              <n-button class="user-chip" quaternary type="default">
                <span>{{ userInitial }}</span><div><strong>{{ textValue(user.username, 'Molly 用户') }}</strong><small>{{ textValue(user.email) }}</small></div><AppIcon name="chevron" />
              </n-button>
            </n-dropdown>

          </div>
        </div>
      </header>

      <n-alert :inert="skillsModalOpen" v-if="errorMessage && !isEmbeddedPage" class="page-alert" type="error" :show-icon="false">{{ errorMessage }}</n-alert>
      <n-alert :inert="skillsModalOpen" v-if="visibleAccountReminder && showAccountHeader && activePage !== 'recharge'" class="account-reminder" :class="{ 'account-reminder--subscription': visibleAccountReminder.kind === 'subscription' }" type="warning" :bordered="false" :show-icon="false">
        <div class="account-reminder-content">
          <div class="account-reminder-copy"><span class="reminder-spark"><AppIcon v-if="visibleAccountReminder.kind === 'subscription'" name="spark" /><template v-else>!</template></span><div><strong>{{ visibleAccountReminder.title }}</strong><small>{{ visibleAccountReminder.detail }}</small></div></div>
          <n-button size="small" type="primary" @click="openRecharge">前往充值</n-button>
        </div>
      </n-alert>

      <Transition :css="false" mode="out-in" @enter="enterNavPanel" @leave="leaveNavPanel" @enter-cancelled="cancelNavPanel" @leave-cancelled="cancelNavPanel">
      <div v-if="activePage === 'overview'" key="overview" class="page-content overview-page">
        <section class="stats-grid overview-stats" aria-label="账户概览">
          <n-card v-for="card in stats" :key="card.label" class="stat-card overview-stat-card" :class="`stat-card--${card.tone}`" :bordered="false" role="button" tabindex="0" @click="openOverviewMetric(card.target)" @keydown.enter.prevent="openOverviewMetric(card.target)" @keydown.space.prevent="openOverviewMetric(card.target)">
            <div class="stat-card__content">
              <span class="overview-stat-card__icon"><AppIcon :name="card.icon" /></span>
              <div class="stat-card__copy"><span>{{ card.label }}</span><strong :title="card.value">{{ card.value }}</strong><small>{{ card.detail }}</small></div>
            </div>
          </n-card>
        </section>
        <section class="overview-detail-grid">
          <n-card class="panel overview-token-panel" :bordered="false">
            <div class="overview-panel-heading"><h2>Token 分布</h2><button type="button" class="text-action" @click="selectPage('usage')">查看用量<AppIcon name="arrow" /></button></div>
            <p class="overview-caption">累计 Token 按输入、输出和缓存拆分</p>
            <div class="token-distribution" role="img" :aria-label="tokenBreakdown.map(part => `${part.label} ${part.share.toFixed(1)}%`).join('，')">
              <span v-for="part in tokenBreakdown" :key="part.label" :class="`metric-${part.tone}`" :style="{ width: `${part.share}%` }" />
            </div>
            <dl class="token-legend"><div v-for="part in tokenBreakdown" :key="part.label"><dt><i :class="`metric-${part.tone}`" />{{ part.label }}</dt><dd>{{ formatTokens(part.value) }}<small>{{ part.share.toFixed(1) }}%</small></dd></div></dl>
          </n-card>
          <n-card class="panel overview-activity-panel" :bordered="false">
            <div class="overview-panel-heading"><h2>运行指标</h2><span class="updated">{{ lastUpdated ? lastUpdated.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }) + ' 更新' : '正在同步' }}</span></div>
            <dl class="activity-metrics"><div><dt>每分钟请求 <small>RPM</small></dt><dd>{{ usage.rpm == null ? '—' : numberValue(usage.rpm).toFixed(1) }}</dd></div><div><dt>每分钟 Token <small>TPM</small></dt><dd>{{ usage.tpm == null ? '—' : formatTokens(usage.tpm) }}</dd></div><div><dt>累计实际消费</dt><dd>{{ formatMoney(usage.total_actual_cost) }}</dd></div></dl>
            <button type="button" class="overview-subscription-link" @click="selectPage('subscriptions')"><AppIcon name="subscription" /><span>管理我的订阅<small>{{ numberValue(subscriptionSummary.active_count) }} 个活跃订阅</small></span><AppIcon name="arrow" /></button>
          </n-card>
        </section>
      </div>

      <div v-else-if="activePage === 'subscriptions'" key="subscriptions" class="page-content subscription-page">
        <SubscriptionsPanel :preview="consolePreview" :obscured="consoleSettingsOpen || petSettingsOpen || ccSwitchImportOpen || updateDialogOpen || createKeyOpen || deleteKeyOpen || userMenuOpen" :server-utc-offset="textValue(asRecord(bootstrap?.settings).server_utc_offset, '')" @changed="refreshSubscriptionDashboard" @modal="subscriptionModalOpen = $event" />
      </div>

      <div v-else-if="activePage === 'keys'" key="keys" class="page-content keys-page">
        <section class="key-directory" aria-label="API 密钥列表">
          <p v-if="keyGroupSuccess" class="key-group-success" role="status">{{ keyGroupSuccess }}</p>
          <div class="key-directory-heading">
            <span class="key-directory-count">共 {{ keys.length }} 个密钥</span>
          </div>
          <div v-if="keys.length" class="key-list" role="list">
            <article v-for="item in keys" :key="String(item.id)" class="key-record" role="listitem">
              <div class="key-record__identity"><div><strong :title="textValue(item.name)">{{ textValue(item.name, '未命名') }}</strong><n-tag size="small" :bordered="false" :type="item.status === 'active' ? 'success' : 'default'">{{ item.status === 'active' ? '正常' : textValue(item.status) }}</n-tag></div><div class="key-cell"><code :title="textValue(item.key)">{{ textValue(item.key, 'sk-••••') }}</code><n-button class="key-copy-button" size="small" quaternary :aria-label="`复制 ${textValue(item.name, '未命名')} 的密钥`" @click="copyKey(item)"><AppIcon name="copy" />{{ copiedKeyId === String(item.id) ? '已复制' : '复制' }}</n-button></div></div>
              <div class="key-record__group"><span class="key-field-label">分组</span><KeyGroupPicker :value="groupFromKey(item)" :key-id="String(item.id)" :key-name="textValue(item.name, '未命名')" @changed="handleKeyGroupChanged" /><small>{{ platformLabel(textValue(asRecord(item.group).platform, '')) }}</small></div>
              <div class="key-record__quota"><span class="key-field-label">累计消费 <small>USD</small></span><strong>{{ costLabel(keyUsage(item).total) }}</strong><small>今日 {{ costLabel(keyUsage(item).today) }}</small><div v-if="keyQuota(item)" class="key-quota-limit"><span>额度 {{ keyQuota(item)!.used.toFixed(2) }} / {{ keyQuota(item)!.limit.toFixed(2) }}</span><i><b :style="{width: `${keyQuota(item)!.percentage}%`}" /></i></div></div>
              <div class="ccs-import-actions"><n-button class="ccs-import-button" size="small" secondary :data-key-id="String(item.id)" @click="openCcSwitchImport(item)">导入到内置 CC Switch</n-button><n-button class="key-delete-button" size="small" quaternary type="error" :aria-label="`删除 ${textValue(item.name, '未命名')} 的密钥`" @click="openDeleteKey(item, $event)">删除</n-button></div>
            </article>
          </div>
          <div v-else class="empty-state"><AppIcon name="key" /><h2>还没有 API 密钥</h2><p>创建一个密钥，开始接入模型服务。</p><n-button type="primary" @click="openCreateKey">创建密钥</n-button></div>
        </section>
      </div>

      <div v-else-if="activePage === 'usage'" key="usage" class="page-content usage-page">
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

      <div v-else-if="activePage === 'assistant'" key="assistant" class="page-content assistant-page">
        <div class="assistant-chat__toolbar">
          <nav ref="assistantNav" class="assistant-tabs molly-subnav" aria-label="Molly助手分类">
            <i class="selection-indicator" aria-hidden="true" />
            <button type="button" :aria-current="assistantTab === 'chat' ? 'page' : undefined" :disabled="assistantSettingsBusy" @click="assistantTab = 'chat'">对话</button>
            <button type="button" :aria-current="assistantTab === 'settings' ? 'page' : undefined" :disabled="assistantSettingsBusy" @click="assistantTab = 'settings'">设置</button>
          </nav>
          <div class="assistant-toggle-cell"><span class="assistant-toggle-state">{{ petVisible ? '开启' : '隐藏' }}</span><n-switch :value="petVisible" :loading="petToggleLoading" aria-label="显示或隐藏 Live2D 桌宠" @update:value="setPetVisibility" /></div>
        </div>
        <Transition :css="false" mode="out-in" @enter="enterNavPanel" @leave="leaveNavPanel" @enter-cancelled="cancelNavPanel" @leave-cancelled="cancelNavPanel">
          <KeepAlive>
            <component :is="assistantTab === 'chat' ? AssistantConversation : PetSettingsDialog" ref="assistantPanel" :key="assistantTab" v-bind="assistantPanelProps" @update:draft="assistantDraft = $event" @keydown="handleAssistantKeydown" @send="sendAssistantMessage" @stop="stopSpeech" @busy="assistantSettingsBusy = $event" />
          </KeepAlive>
        </Transition>
      </div>

      </Transition>

      <div v-if="isWindows && activePage === 'netspeed'" class="page-content netspeed-page">
        <NetSpeedPanel :preview="consolePreview" :active="activePage === 'netspeed'" />
      </div>

      <section v-if="['ccswitch', 'images', 'skills'].includes(activePage) && !pluginUsable(activePage)" class="plugin-empty page-content">
        <AppIcon :name="activePage === 'ccswitch' ? 'code' : activePage === 'images' ? 'image' : 'skills'" />
        <p>{{ plugins.find(p => p.id === activePage)?.installed ? '插件需要重启或更新适配版本后启用。' : '安装插件后，即可在控制台内使用。已有数据会在重新安装后恢复。' }}</p>
        <n-button type="primary" @click="openPluginSettings">管理功能插件</n-button>
      </section>

      <div v-if="skillsVisited && pluginUsable('skills')" :key="pluginKey('skills')" v-show="activePage === 'skills'" class="page-content skills-page">
        <SkillManagerPanel :preview="consolePreview" @modal="skillsModalOpen = $event" />
      </div>

      <div v-if="ccSwitchVisited && pluginUsable('ccswitch')" :key="pluginKey('ccswitch')" v-show="activePage === 'ccswitch'" class="page-content ccswitch-page">
        <CcSwitchPanel ref="ccSwitchPanel" :preview="consolePreview" :target="ccSwitchTarget" />
      </div>

      <div v-if="imageWorkbenchVisited && pluginUsable('images')" :key="pluginKey('images')" v-show="activePage === 'images'" class="page-content embedded-page">
        <ImageWorkbenchPanel v-if="imageAccount" :key="imageAccount" :account="imageAccount" :preview="consolePreview" @settled="refreshDashboard" />
        <p v-else role="status">无法识别当前账户，请刷新账户信息后重试。</p>
      </div>

      <div v-if="rechargeVisited" v-show="activePage === 'recharge'" class="page-content recharge-page">
        <RechargePanel :active="activePage === 'recharge'" :obscured="consoleSettingsOpen || petSettingsOpen || ccSwitchImportOpen || createKeyOpen || deleteKeyOpen || updateDialogOpen || userMenuOpen" :preview="consolePreview" @refresh="refreshDashboard" />
      </div>
    </section>
  </main>

  <div id="console-settings-layer" class="console-settings-layer" />
  <n-modal v-if="phase === 'dashboard' && desktopUpdate" v-model:show="updateDialogOpen" preset="card" class="desktop-update-dialog" :bordered="false" title="MollyCloud 有新版本" :mask-closable="true">
    <div class="desktop-update-content">
      <p>当前版本 {{ desktopUpdate.currentVersion }}，可更新至 {{ desktopUpdate.version }}。</p>
      <div v-if="desktopUpdate.notes" class="desktop-update-notes"><strong>更新内容</strong><p>{{ desktopUpdate.notes }}</p></div>
      <p class="desktop-update-hint">点击下载后将在浏览器中打开安装包。安装由你确认，已有设置会保留。</p>
      <p class="desktop-update-checksum">文件大小 {{ (desktopUpdate.sizeBytes / 1024 / 1024).toFixed(1) }} MB · SHA-256 {{ desktopUpdate.sha256 }}</p>
      <n-alert v-if="updateDialogError" type="error" :show-icon="false">{{ updateDialogError }}</n-alert>
    </div>
    <template #action><div class="desktop-update-actions"><n-button :disabled="updateOpening" @click="updateDialogOpen = false">稍后</n-button><n-button type="primary" :loading="updateOpening" @click="openDesktopUpdate">下载更新</n-button></div></template>
  </n-modal>
  <ConsoleSettingsDialog v-if="phase === 'dashboard'" v-model:show="consoleSettingsOpen" :initial-tab="settingsTab" @check-update="handleSettingsUpdateCheck" @closed="consoleSettingsButton?.focus()" />
  <CreateKeyDialog v-if="phase === 'dashboard'" v-model:show="createKeyOpen" :preview="consolePreview" @created="handleKeyCreated" @closed="createKeyTrigger?.focus()" />
  <DeleteKeyDialog v-if="phase === 'dashboard'" v-model:show="deleteKeyOpen" :key-id="deleteKeyTarget.id" :key-name="deleteKeyTarget.name" :preview="consolePreview" @deleted="handleKeyDeleted" @closed="restoreDeleteKeyFocus" />
  <CcSwitchImportDialog
    v-if="phase === 'dashboard'"
    v-model:show="ccSwitchImportOpen"
    :key-id="ccSwitchImportKeyId"
    :initial-name="ccSwitchImportName"
    @imported="handleCcSwitchImported"
    @closed="restoreCcSwitchImportFocus"
  />

  <n-modal v-model:show="agreementOpen" preset="card" class="agreement-modal" title="登录协议" :bordered="false" :mask-closable="true">
      <div class="agreement-content"><article v-for="doc in agreementDocuments" :key="textValue(doc.id)"><h3>{{ textValue(doc.title) }}</h3><p>{{ textValue(doc.content_md) }}</p></article></div>
      <template #action><n-button type="primary" @click="acceptedAgreement = true; agreementOpen = false">我已阅读并同意</n-button></template>
  </n-modal>
  </div>
</template>
