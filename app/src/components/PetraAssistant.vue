<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import type { AssistantConfig, AssistantMessage, AssistantStatus } from "../contracts";
import { desktopApi } from "../ipc";

const emit = defineEmits<{ close: [] }>();

type ChatItem = AssistantMessage & { id: string; tools?: string[] };

const providers = [
  { id: "mollycloud", label: "MollyCloud", note: "使用当前账号的普通 API 密钥" },
  { id: "deepseek", label: "DeepSeek", note: "api.deepseek.com" },
  { id: "openai", label: "OpenAI", note: "api.openai.com" },
  { id: "moonshot", label: "Moonshot / Kimi", note: "api.moonshot.cn" },
  { id: "zhipu", label: "智谱 GLM", note: "open.bigmodel.cn" },
  { id: "qwen", label: "通义千问", note: "DashScope 兼容接口" },
  { id: "siliconflow", label: "SiliconFlow", note: "api.siliconflow.cn" },
  { id: "openrouter", label: "OpenRouter", note: "openrouter.ai" },
  { id: "groq", label: "Groq", note: "api.groq.com" },
  { id: "ollama", label: "Ollama（本机）", note: "localhost:11434" },
  { id: "custom", label: "自定义 OpenAI 兼容接口", note: "仅 HTTPS；本机可用 HTTP" },
] as const;

const loading = ref(true);
const saving = ref(false);
const chatting = ref(false);
const settingsOpen = ref(false);
const errorMessage = ref("");
const status = ref<AssistantStatus | null>(null);
const apiKey = ref("");
const input = ref("");
const messages = ref<ChatItem[]>([
  { id: "welcome", role: "assistant", content: "你好，我是 Molly。可以问我余额、订阅和 Token 用量，也可以聊点别的。" },
]);
const draft = reactive({
  enabled: false,
  provider: "mollycloud",
  model: "",
  persona: "你叫 Molly，语气自然、简洁、友好。",
  custom_base_url: "",
  greet_interval: 20,
  molly_key_id: "",
  api_key_configured: false,
});

const providerInfo = computed(() => providers.find((provider) => provider.id === draft.provider) ?? providers[0]);
const usesMollyKey = computed(() => draft.provider === "mollycloud");
const needsApiKey = computed(() => !usesMollyKey.value && draft.provider !== "ollama");
const canChat = computed(() => draft.enabled && Boolean(draft.model) && status.value?.ready === true);

function applyConfig(config: AssistantConfig): void {
  Object.assign(draft, config);
}

function friendlyError(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return typeof reason === "string" ? reason : "操作未完成，请稍后重试";
}

async function load(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    const config = await desktopApi.getAssistantConfig();
    applyConfig(config);
    status.value = await desktopApi.assistantStatus().catch((reason) => ({
      ready: false,
      keys: [],
      selected_key_id: null,
      models: [],
      message: friendlyError(reason),
    }));
    if (!draft.molly_key_id && status.value.selected_key_id) draft.molly_key_id = status.value.selected_key_id;
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    loading.value = false;
  }
}

async function saveSettings(fetchModels = false): Promise<void> {
  saving.value = true;
  errorMessage.value = "";
  try {
    const saved = await desktopApi.saveAssistantConfig({
      enabled: draft.enabled,
      provider: draft.provider,
      model: draft.model,
      persona: draft.persona,
      custom_base_url: draft.custom_base_url,
      greet_interval: Number(draft.greet_interval),
      molly_key_id: draft.molly_key_id,
      api_key: apiKey.value || null,
    });
    applyConfig(saved);
    apiKey.value = "";
    if (fetchModels) {
      status.value = await desktopApi.assistantStatus();
      if (status.value.selected_key_id) draft.molly_key_id = status.value.selected_key_id;
      if (!status.value.models.includes(draft.model)) draft.model = status.value.models[0] ?? "";
      const finalConfig = await desktopApi.saveAssistantConfig({
        enabled: draft.enabled,
        provider: draft.provider,
        model: draft.model,
        persona: draft.persona,
        custom_base_url: draft.custom_base_url,
        greet_interval: Number(draft.greet_interval),
        molly_key_id: draft.molly_key_id,
      });
      applyConfig(finalConfig);
    }
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    saving.value = false;
  }
}

async function clearApiKey(): Promise<void> {
  saving.value = true;
  errorMessage.value = "";
  try {
    const saved = await desktopApi.saveAssistantConfig({
      enabled: draft.enabled,
      provider: draft.provider,
      model: "",
      persona: draft.persona,
      custom_base_url: draft.custom_base_url,
      greet_interval: Number(draft.greet_interval),
      molly_key_id: draft.molly_key_id,
      clear_api_key: true,
    });
    applyConfig(saved);
    status.value = null;
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
  } finally {
    saving.value = false;
  }
}

async function send(): Promise<void> {
  const content = input.value.trim();
  if (!content || !canChat.value || chatting.value) return;
  messages.value.push({ id: crypto.randomUUID(), role: "user", content: content.slice(0, 4_000) });
  input.value = "";
  chatting.value = true;
  errorMessage.value = "";
  try {
    const reply = await desktopApi.assistantChat(messages.value.map(({ role, content }) => ({ role, content })));
    messages.value.push({ id: crypto.randomUUID(), role: "assistant", content: reply.content, tools: reply.tools_used });
    await Promise.all([desktopApi.showPetBubble(reply.content), desktopApi.playMotion("Nod")]).catch(() => undefined);
  } catch (reason) {
    errorMessage.value = friendlyError(reason);
    await desktopApi.playMotion("Shake").catch(() => undefined);
  } finally {
    chatting.value = false;
  }
}

function providerChanged(): void {
  draft.model = "";
  status.value = null;
  apiKey.value = "";
}

onMounted(() => {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).get("ui-preview") === "assistant-settings") {
    settingsOpen.value = true;
  }
  void load();
});
</script>

<template>
  <section class="petra-assistant" aria-label="Molly 助手">
    <header>
      <img src="/brand/mollycloud-logo.png" alt="" />
      <div><strong>Molly 助手</strong><small>{{ draft.enabled ? "已开启" : "已关闭" }}</small></div>
      <button type="button" :class="{ active: settingsOpen }" aria-label="助手设置" @click="settingsOpen = !settingsOpen">⚙</button>
      <button type="button" aria-label="关闭助手面板" @click="emit('close')">×</button>
    </header>

    <div v-if="loading" class="petra-loading"><i /><i /><i /><span>正在读取助手设置</span></div>

    <form v-else-if="settingsOpen" class="petra-settings" @submit.prevent="saveSettings(false)">
      <div class="petra-setting-row">
        <div><strong>启用 Molly 助手</strong><small>关闭后不进行对话或主动提醒</small></div>
        <button type="button" class="petra-switch" :class="{ enabled: draft.enabled }" role="switch" :aria-checked="draft.enabled" @click="draft.enabled = !draft.enabled"><span /></button>
      </div>

      <label>AI 服务商
        <select v-model="draft.provider" @change="providerChanged">
          <option v-for="provider in providers" :key="provider.id" :value="provider.id">{{ provider.label }}</option>
        </select>
        <small>{{ providerInfo.note }}</small>
      </label>

      <label v-if="usesMollyKey">计费密钥
        <select v-model="draft.molly_key_id" :disabled="!status?.keys.length">
          <option v-for="key in status?.keys ?? []" :key="key.id" :value="key.id">{{ key.name }} · {{ key.masked_key }}</option>
        </select>
      </label>

      <label v-if="needsApiKey">API Key
        <input v-model="apiKey" type="password" autocomplete="off" :placeholder="draft.api_key_configured ? '已安全保存，留空则保持不变' : '输入 API Key'" />
        <button v-if="draft.api_key_configured" type="button" class="petra-text-action" @click="clearApiKey">清除已保存密钥</button>
      </label>

      <label v-if="draft.provider === 'custom'">API 地址
        <input v-model.trim="draft.custom_base_url" type="url" placeholder="https://example.com/v1" />
      </label>

      <label>模型
        <div class="petra-model-row"><input v-model.trim="draft.model" list="assistant-models" placeholder="先获取模型列表" /><button type="button" :disabled="saving" @click="saveSettings(true)">获取</button></div>
        <datalist id="assistant-models"><option v-for="model in status?.models ?? []" :key="model" :value="model" /></datalist>
      </label>

      <label>助手人设
        <textarea v-model="draft.persona" rows="3" maxlength="1200" placeholder="例如：语气简洁、温柔，称呼我为主人" />
      </label>

      <label>主动问候间隔（分钟）
        <input v-model.number="draft.greet_interval" type="number" min="5" max="120" />
        <small>仅在助手开启且模型已选时运行；会产生少量 API 用量，不读取前台窗口内容。</small>
      </label>

      <p v-if="status?.message" class="petra-error">{{ status.message }}</p>
      <p v-if="errorMessage" class="petra-error">{{ errorMessage }}</p>
      <button class="petra-primary" type="submit" :disabled="saving">{{ saving ? "正在保存" : "保存设置" }}</button>
      <p class="petra-security">API Key 由 Windows 凭据管理器加密保存，不会显示在控制台或发送给账户工具。</p>
    </form>

    <template v-else>
      <div class="petra-messages" aria-live="polite">
        <div v-for="message in messages" :key="message.id" class="petra-message" :class="`petra-message--${message.role}`">
          <p>{{ message.content }}</p><small v-if="message.tools?.length">已读取：{{ message.tools.join("、") }}</small>
        </div>
        <div v-if="chatting" class="petra-message petra-thinking"><i /><i /><i /></div>
      </div>
      <p v-if="!draft.enabled" class="petra-disabled">助手当前关闭。点击右上角设置后开启。</p>
      <p v-else-if="status?.message" class="petra-error petra-error--chat">{{ status.message }}</p>
      <p v-if="errorMessage" class="petra-error petra-error--chat">{{ errorMessage }}</p>
      <form class="petra-composer" @submit.prevent="send">
        <textarea v-model="input" rows="2" maxlength="4000" placeholder="和 Molly 说点什么……" :disabled="!canChat || chatting" @keydown.ctrl.enter.prevent="send" />
        <button type="submit" :disabled="!input.trim() || !canChat || chatting">发送</button>
      </form>
    </template>
  </section>
</template>

<style scoped>
.petra-assistant { position: absolute; z-index: 4; left: 18px; top: 18px; width: 370px; max-height: 700px; display: flex; flex-direction: column; overflow: hidden; border: 1px solid rgba(20, 20, 20, 0.14); border-radius: 18px 18px 18px 6px; color: #151515; background: rgba(247, 247, 244, 0.96); box-shadow: 0 22px 65px rgba(42, 42, 36, 0.2); font: 13px/1.5 "Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif; backdrop-filter: blur(18px); }
.petra-assistant header { min-height: 62px; display: flex; align-items: center; gap: 10px; padding: 10px 12px; border-bottom: 1px solid rgba(20, 20, 20, 0.1); }
.petra-assistant header img { width: 40px; height: 40px; object-fit: contain; }
.petra-assistant header div { min-width: 0; margin-right: auto; }
.petra-assistant header strong, .petra-assistant header small { display: block; }
.petra-assistant header strong { font-size: 14px; }
.petra-assistant header small { margin-top: 2px; color: #71716b; font-size: 10px; }
.petra-assistant header button { width: 34px; height: 34px; border: 1px solid rgba(20, 20, 20, 0.12); border-radius: 9px; color: #50504b; background: #fff; }
.petra-assistant header button:hover, .petra-assistant header button.active { color: #fff; border-color: #4c49ff; background: #4c49ff; }
.petra-loading { min-height: 210px; display: flex; align-items: center; justify-content: center; gap: 6px; color: #6b6b65; }
.petra-loading i, .petra-thinking i { width: 6px; height: 6px; border-radius: 50%; background: #4c49ff; animation: petra-pulse 700ms ease-in-out infinite alternate; }
.petra-loading span { margin-left: 7px; font-size: 11px; }
.petra-settings { overflow-y: auto; padding: 15px; }
.petra-settings label { display: grid; gap: 6px; margin-top: 13px; color: #4f4f49; font-size: 11px; font-weight: 600; }
.petra-settings label > small { color: #85857e; font-size: 9px; font-weight: 400; }
.petra-settings input, .petra-settings select, .petra-settings textarea, .petra-composer textarea { width: 100%; border: 1px solid #d7d7d1; border-radius: 10px; color: #151515; background: #fff; font: inherit; }
.petra-settings input, .petra-settings select { height: 40px; padding: 0 10px; }
.petra-settings textarea, .petra-composer textarea { resize: none; padding: 9px 10px; line-height: 1.5; }
.petra-settings input:focus, .petra-settings select:focus, .petra-settings textarea:focus, .petra-composer textarea:focus { outline: 3px solid rgba(76, 73, 255, 0.16); border-color: #4c49ff; }
.petra-setting-row { display: flex; align-items: center; justify-content: space-between; gap: 18px; padding: 4px 0 14px; border-bottom: 1px solid rgba(20, 20, 20, 0.1); }
.petra-setting-row strong, .petra-setting-row small { display: block; }
.petra-setting-row small { margin-top: 3px; color: #777771; font-size: 9px; }
.petra-switch { width: 48px; height: 27px; flex: none; padding: 3px; border: 0; border-radius: 99px; background: #d9d9d3; }
.petra-switch span { display: block; width: 21px; height: 21px; border-radius: 50%; background: #fff; box-shadow: 0 2px 5px rgba(0,0,0,.15); transition: transform 180ms ease; }
.petra-switch.enabled { background: #4c49ff; }
.petra-switch.enabled span { transform: translateX(21px); }
.petra-model-row { display: grid; grid-template-columns: 1fr 58px; gap: 7px; }
.petra-model-row button { border: 0; border-radius: 9px; color: #fff; background: #151515; font-weight: 650; }
.petra-text-action { justify-self: start; padding: 0; border: 0; color: #b03a4c; background: transparent; font-size: 9px; }
.petra-primary { width: 100%; min-height: 43px; margin-top: 16px; border: 0; border-radius: 10px; color: #fff; background: #151515; font-weight: 700; }
.petra-primary:hover, .petra-model-row button:hover { background: #4c49ff; }
.petra-security { margin: 10px 2px 2px; color: #85857e; font-size: 9px; line-height: 1.55; }
.petra-messages { min-height: 250px; max-height: 500px; display: flex; flex-direction: column; gap: 9px; overflow-y: auto; padding: 14px; }
.petra-message { align-self: flex-start; max-width: 82%; padding: 9px 11px; border-radius: 5px 13px 13px 13px; background: #e9e9e4; }
.petra-message--user { align-self: flex-end; border-radius: 13px 5px 13px 13px; color: #fff; background: #4c49ff; }
.petra-message p { margin: 0; white-space: pre-wrap; }
.petra-message small { display: block; margin-top: 5px; color: #6f6f69; font-size: 8px; }
.petra-message--user small { color: rgba(255,255,255,.72); }
.petra-thinking { display: flex; gap: 5px; min-width: 48px; }
.petra-composer { display: grid; grid-template-columns: 1fr 62px; gap: 8px; padding: 11px; border-top: 1px solid rgba(20, 20, 20, 0.1); background: #f0f0ec; }
.petra-composer textarea { min-height: 48px; }
.petra-composer button { border: 0; border-radius: 10px; color: #fff; background: #151515; font-weight: 700; }
.petra-composer button:hover { background: #4c49ff; }
.petra-composer button:disabled, .petra-primary:disabled, .petra-model-row button:disabled { cursor: not-allowed; opacity: .45; }
.petra-error, .petra-disabled { margin: 10px 0 0; padding: 8px 10px; border-radius: 9px; color: #9f3042; background: #fff0f2; font-size: 10px; }
.petra-error--chat, .petra-disabled { margin: 0 11px 9px; }
@keyframes petra-pulse { from { opacity: .25; transform: translateY(2px); } to { opacity: 1; transform: translateY(-2px); } }
@media (prefers-reduced-motion: reduce) { *, *::before, *::after { animation: none !important; transition: none !important; } }
</style>
