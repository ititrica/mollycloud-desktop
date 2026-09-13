<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NAlert, NButton, NInput, NModal, NSelect } from "naive-ui";
import { desktopApi, type CcSwitchAgent, type CcSwitchImportResult } from "../ipc";

const props = defineProps<{
  show: boolean;
  keyId: string;
  initialName: string;
}>();
const emit = defineEmits<{
  "update:show": [show: boolean];
  closed: [];
  imported: [keyId: string, provider: CcSwitchImportResult];
}>();

const agentOptions: Array<{ label: string; value: CcSwitchAgent }> = [
  { label: "Claude Code", value: "claude" },
  { label: "Claude Desktop", value: "claude-desktop" },
  { label: "Codex", value: "codex" },
  { label: "Gemini CLI", value: "gemini" },
  { label: "Grok Build", value: "grokbuild" },
  { label: "OpenCode", value: "opencode" },
  { label: "OpenClaw", value: "openclaw" },
  { label: "Hermes", value: "hermes" },
  { label: "Pi", value: "pi" },
];

const draftName = ref("");
const draftAgent = ref<CcSwitchAgent>("codex");
const draftModel = ref("gpt-5.5");
const selectedFetchedModel = ref<string | null>(null);
const fetchedModels = ref<string[]>([]);
const fetchingModels = ref(false);
const importing = ref(false);
const errorMessage = ref("");
let requestVersion = 0;

const modelOptions = computed(() => fetchedModels.value.map((model) => ({ label: model, value: model })));

function resetDraft(): void {
  draftName.value = props.initialName;
  draftAgent.value = "codex";
  draftModel.value = "gpt-5.5";
  selectedFetchedModel.value = null;
  fetchedModels.value = [];
  errorMessage.value = "";
}

function cancel(): void {
  if (!importing.value) emit("update:show", false);
}

function chooseFetchedModel(model: string | null): void {
  selectedFetchedModel.value = model;
  if (model) draftModel.value = model;
}

async function fetchModels(): Promise<void> {
  if (!props.keyId || fetchingModels.value || importing.value) return;
  const version = ++requestVersion;
  fetchingModels.value = true;
  errorMessage.value = "";
  try {
    const models = await desktopApi.fetchCcSwitchImportModels(props.keyId);
    if (version !== requestVersion || !props.show) return;
    fetchedModels.value = models;
    const current = models.includes(draftModel.value) ? draftModel.value : (models[0] ?? null);
    chooseFetchedModel(current);
  } catch (error) {
    if (version === requestVersion) errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    if (version === requestVersion) fetchingModels.value = false;
  }
}

async function submit(): Promise<void> {
  const name = draftName.value.trim();
  const model = draftModel.value.trim();
  if (!name) {
    errorMessage.value = "请输入供应商名称。";
    return;
  }
  if (!model) {
    errorMessage.value = "请选择或输入默认模型。";
    return;
  }
  importing.value = true;
  errorMessage.value = "";
  try {
    const provider = await desktopApi.importApiKeyToCcSwitch({
      keyId: props.keyId,
      name,
      agent: draftAgent.value,
      model,
    });
    emit("imported", props.keyId, provider);
    emit("update:show", false);
  } catch (error) {
    errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    importing.value = false;
  }
}

watch(() => props.show, (show) => {
  ++requestVersion;
  fetchingModels.value = false;
  if (show) resetDraft();
}, { immediate: true });
</script>

<template>
  <n-modal
    :show="show"
    to="#console-settings-layer"
    :mask-closable="!importing"
    :close-on-esc="!importing"
    transform-origin="center"
    @update:show="cancel"
    @after-leave="emit('closed')"
  >
    <div class="console-settings-dialog ccs-import-dialog" role="dialog" aria-modal="true" aria-labelledby="ccs-import-title" aria-describedby="ccs-import-description" data-agent-count="9" :aria-busy="fetchingModels || importing">
      <header class="console-settings-header">
        <span class="console-settings-icon"><span class="ccs-import-mark">CC</span></span>
        <div><h2 id="ccs-import-title">导入到内置 CC Switch</h2><p id="ccs-import-description">保存为供应商；导入后不会自动启用。</p></div>
      </header>
      <form class="console-settings-form" @submit.prevent="submit">
        <div class="console-settings-body ccs-import-body">
          <label class="ccs-import-field">
            <span>名称</span>
            <n-input v-model:value="draftName" maxlength="80" placeholder="输入供应商名称" :disabled="importing" />
          </label>

          <label class="ccs-import-field">
            <span>导入到的 Agent</span>
            <n-select v-model:value="draftAgent" :options="agentOptions" :disabled="importing" aria-label="导入到的 Agent" />
            <small>包含 CC Switch 当前支持的全部 Agent 与 Claude Desktop。</small>
          </label>

          <fieldset class="ccs-import-model" :disabled="importing">
            <legend>默认模型</legend>
            <div class="ccs-import-model__input">
              <n-input v-model:value="draftModel" maxlength="160" placeholder="选择或输入模型 ID" @update:value="selectedFetchedModel = null" />
              <n-button secondary :loading="fetchingModels" :disabled="fetchingModels" @click="fetchModels">拉取模型</n-button>
            </div>
            <n-select v-if="fetchedModels.length" :value="selectedFetchedModel" :options="modelOptions" filterable placeholder="从已拉取列表选择" aria-label="已拉取的模型" @update:value="chooseFetchedModel" />
            <small>{{ fetchedModels.length ? `已拉取 ${fetchedModels.length} 个模型，也可以继续手动输入。` : "可从 MollyCloud 拉取模型列表，也可以直接输入模型 ID。" }}</small>
          </fieldset>

          <n-alert v-if="errorMessage" class="console-settings-error" type="error" :bordered="false" :show-icon="false" role="alert">{{ errorMessage }}</n-alert>
        </div>
        <footer class="console-settings-actions">
          <n-button size="large" :disabled="importing" @click="cancel">取消</n-button>
          <n-button class="ccs-import-submit" type="primary" size="large" attr-type="submit" :loading="importing">导入</n-button>
        </footer>
      </form>
    </div>
  </n-modal>
</template>
