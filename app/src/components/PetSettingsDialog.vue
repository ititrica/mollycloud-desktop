<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NAlert, NButton, NCheckbox, NInput, NInputNumber, NModal, NSelect, NSwitch } from "naive-ui";
import AppIcon from "./AppIcon.vue";
import { importPetModel, modelAutoOptions, modelParameters, petDraftSchema, requestPetSettings, type PetDraft, type PetSettingsRequest, type PetSettingsSnapshot } from "../petSettings";
import { PROVIDERS } from "../petra/assistant/AssistantClient";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ "update:show": [show: boolean]; closed: [] }>();
const tab = ref<"model" | "assistant">("model");
const snapshot = ref<PetSettingsSnapshot | null>(null);
const draft = ref<PetDraft | null>(null);
const loading = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const apiKey = ref("");
const clearApiKey = ref(false);
const availableModels = ref<string[]>([]);
const confirmDelete = ref("");
const confirmClear = ref(false);
const fileInput = ref<HTMLInputElement | null>(null);
let version = 0;
const dirty = computed(() => JSON.stringify(draft.value) !== JSON.stringify(snapshot.value?.draft ?? null) || Boolean(apiKey.value.trim()) || clearApiKey.value);
const savedKeyAvailable = computed(() => snapshot.value?.apiKeyConfigured && draft.value?.assistant.provider === snapshot.value.draft.assistant.provider && draft.value?.assistant.customBaseUrl === snapshot.value.draft.assistant.customBaseUrl);
const providerOptions = Object.entries(PROVIDERS).map(([value, info]) => ({ value, label: info.label }));
const sides = [{ key: "left", label: "左边界" }, { key: "right", label: "右边界" }, { key: "top", label: "上边界" }, { key: "bottom", label: "下边界" }] as const;
function receive(value: PetSettingsSnapshot) {
  snapshot.value = value;
  draft.value = structuredClone(value.draft);
  apiKey.value = "";
  clearApiKey.value = false;
}
function fail(reason: unknown) { error.value = reason instanceof Error ? reason.message : String(reason); }
async function load() {
  const current = ++version;
  loading.value = true;
  error.value = "";
  try { const result = await requestPetSettings({ action: "read" }); if (current === version && props.show) receive(result); }
  catch (reason) { if (current === version) fail(reason); }
  finally { if (current === version) loading.value = false; }
}
function cancel() { if (!busy.value) emit("update:show", false); }
async function save() {
  if (!draft.value || busy.value) return;
  const parsed = petDraftSchema.safeParse(draft.value);
  if (!parsed.success) { error.value = parsed.error.issues[0]?.message ?? "请检查设置"; return; }
  busy.value = true;
  error.value = "";
  try {
    receive(await requestPetSettings({ action: "save", draft: parsed.data, apiKey: apiKey.value, clearApiKey: clearApiKey.value }));
    emit("update:show", false);
  } catch (reason) { fail(reason); }
  finally { busy.value = false; }
}
async function manage(request: PetSettingsRequest, message: string) {
  if (busy.value) return;
  if (dirty.value) { error.value = "请先保存当前修改，或取消后重新打开，再进行此操作。"; return; }
  busy.value = true;
  error.value = "";
  notice.value = "";
  try { receive(await requestPetSettings(request)); notice.value = message; confirmDelete.value = ""; confirmClear.value = false; }
  catch (reason) { fail(reason); }
  finally { busy.value = false; }
}
async function importModel(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  if (dirty.value) { error.value = "请先保存当前修改，再导入模型。"; return; }
  busy.value = true;
  error.value = "";
  try { receive(await importPetModel(file)); notice.value = "模型已导入并应用。"; }
  catch (reason) { fail(reason); }
  finally { busy.value = false; }
}
async function fetchModels() {
  if (!draft.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const result = await requestPetSettings({ action: "fetch-models", assistant: { ...draft.value.assistant }, apiKey: apiKey.value, useSavedKey: Boolean(savedKeyAvailable.value && !clearApiKey.value) });
    availableModels.value = result.availableModels ?? [];
    if (!availableModels.value.length) throw new Error("服务商未返回可用模型，请手动填写模型名。");
    if (!availableModels.value.includes(draft.value.assistant.model)) draft.value.assistant.model = availableModels.value[0]!;
    notice.value = `已获取 ${availableModels.value.length} 个模型。`;
  } catch (reason) { fail(reason); }
  finally { busy.value = false; }
}
watch(() => draft.value?.assistant.provider, () => { availableModels.value = []; });
watch(() => props.show, show => {
  if (show) { tab.value = "model"; snapshot.value = null; draft.value = null; notice.value = ""; confirmDelete.value = ""; confirmClear.value = false; availableModels.value = []; void load(); }
  else { ++version; apiKey.value = ""; clearApiKey.value = false; }
}, { immediate: true });
</script>

<template>
  <n-modal :show="show" to="#console-settings-layer" :mask-closable="!busy" :close-on-esc="!busy" transform-origin="center" @update:show="cancel" @after-leave="emit('closed')">
    <div class="console-settings-dialog pet-settings-dialog" role="dialog" aria-modal="true" aria-labelledby="pet-settings-title" :aria-busy="loading || busy">
      <header class="console-settings-header">
        <span class="console-settings-icon"><AppIcon name="settings" /></span>
        <div><h2 id="pet-settings-title">桌宠设置</h2><p>调整 Molly 的外观与小助手偏好。</p></div>
      </header>
      <div class="pet-settings-tabs" aria-label="设置分类">
        <button type="button" :aria-pressed="tab === 'model'" @click="tab = 'model'">模型</button>
        <button type="button" :aria-pressed="tab === 'assistant'" @click="tab = 'assistant'">小助手设置</button>
      </div>
      <form class="console-settings-form" @submit.prevent="save">
        <div class="console-settings-body pet-settings-body">
          <p v-if="loading" class="console-settings-status" role="status">正在读取桌宠设置…</p>
          <n-alert v-if="error" class="console-settings-error" type="error" :bordered="false" :show-icon="false" role="alert">{{ error }}<n-button v-if="!draft && !loading" @click="load">重新读取</n-button></n-alert>
          <p v-if="notice" class="pet-settings-notice" role="status">{{ notice }}</p>
          <fieldset v-if="draft && snapshot" :disabled="busy || loading" class="pet-settings-fields">
            <div v-show="tab === 'model'" class="pet-settings-section">
              <section aria-labelledby="pet-model-list-title">
                <h3 id="pet-model-list-title">模型设置</h3>
                <p class="console-settings-hint">模型切换、导入和删除立即生效；其余调整保存后生效。</p>
                <div class="pet-model-list">
                  <div v-for="model in snapshot.models" :key="`${model.type}:${model.name}`" class="pet-model-row">
                    <div><strong>{{ model.name.replace(/\.psd$/i, '') }}</strong><small>{{ model.type === 'import' ? '已导入' : '内置模型' }}</small></div>
                    <span v-if="draft.model.name === model.name && draft.model.type === model.type" class="pet-model-active">使用中</span>
                    <n-button v-else size="small" :disabled="busy || dirty" @click="manage({ action: 'select', model }, '模型已切换。')">使用</n-button>
                    <n-button v-if="model.type === 'import'" size="small" :disabled="busy || dirty" @click="confirmDelete = model.name">删除</n-button>
                  </div>
                </div>
                <n-alert v-if="confirmDelete" class="pet-settings-confirm" type="warning" :bordered="false" :show-icon="false">
                  删除「{{ confirmDelete }}」后需重新导入才能恢复。
                  <div class="pet-settings-inline-actions"><n-button :disabled="busy" @click="confirmDelete = ''">保留</n-button><n-button type="error" :loading="busy" @click="manage({ action: 'delete', name: confirmDelete }, '模型已删除。')">确认删除</n-button></div>
                </n-alert>
                <input ref="fileInput" type="file" accept=".psd" hidden @change="importModel" />
                <n-button class="pet-model-import" :disabled="busy || dirty" @click="fileInput?.click()">导入 PSD 模型</n-button>
              </section>
              <section><h3>模型大小</h3><label class="pet-setting-range"><span>缩放 <output>{{ Math.round(draft.modelScale * 100) }}%</output></span><input v-model.number="draft.modelScale" aria-label="模型大小" type="range" min="0.2" max="2" step="0.01" /></label></section>
              <details class="pet-settings-details"><summary>调整模型边界</summary>
                <p class="console-settings-hint">正值放大边界，负值收紧边界，单位为像素。</p>
                <label v-for="side in sides" :key="side.key" class="pet-setting-range"><span>{{ side.label }}<output>{{ draft.boundsPadding[side.key] }} px</output></span><input v-model.number="draft.boundsPadding[side.key]" :aria-label="side.label" type="range" min="-120" max="120" step="1" /></label>
                <n-button size="small" :disabled="busy" @click="draft.boundsPadding = { left: 0, right: 0, top: 0, bottom: 0 }">重置边界</n-button>
              </details>
              <div class="pet-setting-switch"><span>显示边框</span><n-switch v-model:value="draft.debugBorder" :disabled="busy" aria-label="显示边框" /></div>
              <div class="pet-setting-switch"><span>显示模型边框</span><n-switch v-model:value="draft.debugModelBounds" :disabled="busy" aria-label="显示模型边框" /></div>
              <details v-if="snapshot.supportsAdjust" class="pet-settings-details"><summary>模型调节（测试）</summary>
                <div v-for="option in modelAutoOptions" :key="option.key" class="pet-setting-switch"><span>{{ option.label }}</span><n-switch v-model:value="draft.auto[option.key]" :disabled="busy" :aria-label="option.label" /></div>
                <label v-for="parameter in modelParameters" :key="parameter.key" class="pet-setting-range"><span>{{ parameter.label }}<output>{{ draft.params[parameter.key]?.toFixed(2) }}</output></span><input v-model.number="draft.params[parameter.key]" :aria-label="parameter.label" type="range" :min="parameter.min" :max="parameter.max" :step="parameter.step" /></label>
                <n-button size="small" :disabled="busy" @click="draft.params = { ...snapshot.defaults }; draft.auto = { autoBlink: true, autoRand: true, autoIdle: true }">重置默认参数</n-button>
              </details>
            </div>
            <div v-show="tab === 'assistant'" class="pet-settings-section">
              <div class="pet-setting-switch"><span>小助手模式</span><n-switch v-model:value="draft.assistant.enabled" :disabled="busy" aria-label="小助手模式" /></div>
              <div class="pet-setting-field"><label id="pet-provider-label">提供商</label><n-select v-model:value="draft.assistant.provider" :options="providerOptions" :disabled="busy" aria-labelledby="pet-provider-label" /></div>
              <div v-if="draft.assistant.provider === 'custom'" class="pet-setting-field"><label for="pet-base-url">API 端点</label><n-input v-model:value="draft.assistant.customBaseUrl" :disabled="busy" :input-props="{ id: 'pet-base-url' }" placeholder="https://api.example.com/v1" /></div>
              <p v-if="draft.assistant.provider === 'ollama'" class="console-settings-hint">在本机启动 Ollama 并下载所需模型，API 密钥通常可留空。</p>
              <div class="pet-setting-field"><label for="pet-api-key">API 密钥</label><n-input v-model:value="apiKey" type="password" show-password-on="click" :disabled="busy || clearApiKey" :input-props="{ id: 'pet-api-key', autocomplete: 'new-password' }" :placeholder="savedKeyAvailable ? '已保存，留空保持原密钥' : '手动填写 API 密钥'" />
                <small>密钥仅保存在本机，不随安装包分发。更换提供商或端点后需重新填写。</small>
                <n-checkbox v-if="snapshot.apiKeyConfigured" v-model:checked="clearApiKey" :disabled="busy">清除已保存的密钥</n-checkbox>
              </div>
              <div class="pet-setting-field"><label for="pet-assistant-model">模型名</label><n-input v-model:value="draft.assistant.model" :disabled="busy" :input-props="{ id: 'pet-assistant-model' }" :placeholder="PROVIDERS[draft.assistant.provider].defaultModel || '填写模型名或自动获取'" />
                <n-select v-if="availableModels.length" v-model:value="draft.assistant.model" :disabled="busy" :options="availableModels.map(value => ({ label: value, value }))" aria-label="模型列表" filterable />
                <n-button :disabled="busy" @click="fetchModels">自动获取模型</n-button>
              </div>
              <div class="pet-setting-field"><label for="pet-persona">人格设定</label><n-input v-model:value="draft.assistant.persona" type="textarea" maxlength="1200" :disabled="busy" :input-props="{ id: 'pet-persona' }" :autosize="{ minRows: 3, maxRows: 6 }" /></div>
              <div class="pet-setting-field"><label for="pet-greet">主动问候间隔（分钟）</label><n-input-number v-model:value="draft.assistant.greetInterval" :min="5" :max="120" :step="5" :disabled="busy" :input-props="{ id: 'pet-greet' }" :parse="value => Number(value) || 20" /></div>
              <p class="console-settings-hint">主动问候会将当前前台窗口标题和进程名发送给所选 AI 服务。</p>
              <n-button :disabled="busy || dirty" @click="confirmClear = true">清空对话历史</n-button>
              <n-alert v-if="confirmClear" class="pet-settings-confirm" type="warning" :bordered="false" :show-icon="false">清空桌宠与控制台共享的对话历史，长期记忆保留。<div class="pet-settings-inline-actions"><n-button :disabled="busy" @click="confirmClear = false">保留</n-button><n-button type="error" :loading="busy" @click="manage({ action: 'clear-history' }, '对话历史已清空，长期记忆保留。')">确认清空</n-button></div></n-alert>
            </div>
          </fieldset>
        </div>
        <footer class="console-settings-actions"><n-button size="large" :disabled="busy" @click="cancel">取消</n-button><n-button type="primary" size="large" attr-type="submit" :loading="busy" :disabled="!draft || loading">保存</n-button></footer>
      </form>
    </div>
  </n-modal>
</template>
