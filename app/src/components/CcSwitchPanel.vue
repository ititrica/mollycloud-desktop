<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { NButton } from "naive-ui";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import type { CcSwitchImportResult } from "../ipc";
export interface CcKeyAction { action: "create" | "delete" | "group" | "configure" | "configure-enable"; app: string; keyId?: string; model?: string }
const emitEvent = defineEmits<{ 'key-action': [CcKeyAction] }>();

const props = defineProps<{
  preview: boolean;
  target: CcSwitchImportResult | null;
  previewKeys?: Record<string, unknown>[];
}>();

const frame = ref<HTMLIFrameElement | null>(null);
const ready = ref(false);
const importReady = ref(false);
const childReportedError = ref(false);
const loadFailed = ref(false);
const frameVersion = ref(0);
let loadTimer: number | undefined;

const frameUrl = computed(() => {
  const params = new URLSearchParams({ embedded: "1" });
  if (props.preview) params.set("ui-preview", "ccswitch");
  return `${props.preview ? "/ccswitch" : "/molly-plugins/ccswitch"}/index.html?${params.toString()}`;
});

function sendProvider(type: "navigate" | "provider-imported" | "apply-provider", provider: CcSwitchImportResult): void {
  if (!ready.value || !frame.value?.contentWindow) return;
  frame.value.contentWindow.postMessage({
    source: "mollycloud",
    type,
    providerId: provider.provider_id,
    app: provider.app,
  }, window.location.origin);
}

function notifyProviderImported(provider: CcSwitchImportResult, apply = false): void {
  sendProvider(apply ? "apply-provider" : "provider-imported", provider);
}
function notifyKeysChanged(notice?: string): void {
  frame.value?.contentWindow?.postMessage({source:"mollycloud",type:"keys-changed",notice},window.location.origin);
}
function sendPreviewKeys(): void {
  if (!props.preview || !ready.value) return;
  const keys = (props.previewKeys ?? []).map(item=>({
    id:String(item.id),name:item.name,status:item.status,
    key:`sk-••••${String(item.key ?? '').slice(-4)}`,
    group:item.group,quota:item.quota,quota_used:item.quota_used,usage:item.usage,
  }));
  frame.value?.contentWindow?.postMessage({source:'mollycloud',type:'preview-keys',keys:JSON.parse(JSON.stringify(keys))},window.location.origin);
  notifyKeysChanged();
}
function focusKeyAction(keyId?: string, action = "create"): void {
  const doc = frame.value?.contentDocument;
  const selector = keyId ? `[data-key-id="${CSS.escape(keyId)}"] [data-key-action="${CSS.escape(action)}"]` : '.create-key-button';
  (doc?.querySelector<HTMLButtonElement>(selector) ?? doc?.querySelector<HTMLButtonElement>('.create-key-button'))?.focus();
}

function receiveMessage(event: MessageEvent): void {
  if (event.origin !== window.location.origin || event.source !== frame.value?.contentWindow) return;
  const data = event.data;
  if (!data || typeof data !== "object" || data.source !== "molly-ccswitch") return;
  if (data.type === "key-action") {
    if (!["create","delete","group","configure","configure-enable"].includes(data.action)
      || !["claude","claude-desktop","codex","gemini","grokbuild","opencode","openclaw","hermes","pi","mcode"].includes(data.app)
      || (data.keyId!==undefined && (typeof data.keyId!=="string" || !/^(?:[1-9]\d*|demo-[\w-]+)$/.test(data.keyId)))
      || (data.model!==undefined && (typeof data.model!=="string" || data.model.length>160 || /[\x00-\x1f\x7f]/.test(data.model)))) return;
    emitEvent('key-action',{action:data.action,app:data.app,keyId:data.keyId,model:data.model});
    return;
  }
  if (data.type === "load-error") {
    childReportedError.value = true;
    if (loadTimer) window.clearTimeout(loadTimer);
    return;
  }
  if (data.type === "import-ready") {
    importReady.value = true;
    if (!props.preview) void flushExternalImports();
    return;
  }
  if (data.type !== "ready") return;
  ready.value = true;
  sendPreviewKeys();
  childReportedError.value = false;
  loadFailed.value = false;
  if (loadTimer) window.clearTimeout(loadTimer);
  if (props.target) sendProvider("navigate", props.target);
  if (!props.preview) void flushExternalImports();
}

async function flushExternalImports(): Promise<void> {
  if (!ready.value || !importReady.value) return;
  try {
    const imports = await invoke<unknown[]>("take_ccswitch_external_imports");
    for (const request of imports) await emit("deeplink-import", request);
  } catch (error) {
    console.error("无法打开 CC Switch 网页导入确认窗口", error);
  }
}

function startLoadTimer(): void {
  if (loadTimer) window.clearTimeout(loadTimer);
  loadTimer = window.setTimeout(() => {
    if (!ready.value) loadFailed.value = true;
  }, 20_000);
}

function reloadFrame(): void {
  ready.value = false;
  importReady.value = false;
  childReportedError.value = false;
  loadFailed.value = false;
  frameVersion.value += 1;
  startLoadTimer();
}

watch(() => props.target, (target) => {
  if (target) sendProvider("navigate", target);
});
watch(()=>props.previewKeys,sendPreviewKeys,{deep:true});

onMounted(() => {
  window.addEventListener("message", receiveMessage);
  startLoadTimer();
});

onBeforeUnmount(() => {
  window.removeEventListener("message", receiveMessage);
  if (loadTimer) window.clearTimeout(loadTimer);
});

defineExpose({ notifyProviderImported, notifyKeysChanged, focusKeyAction, flushExternalImports });
</script>

<template>
  <section class="ccswitch-panel" aria-label="内置 CC Switch">
    <iframe
      :key="frameVersion"
      ref="frame"
      class="ccswitch-frame"
      :src="frameUrl"
      title="内置 CC Switch 供应商管理"
      allow="clipboard-write"
      @error="loadFailed = true"
    />
    <n-button v-if="childReportedError" class="ccswitch-reload" size="small" secondary @click="reloadFrame">重新加载</n-button>
    <div v-if="!ready && !childReportedError" class="ccswitch-load-state" role="status" aria-live="polite">
      <template v-if="loadFailed">
        <strong>CC Switch 暂时无法加载</strong>
        <p>重新加载内置界面后重试。</p>
        <n-button secondary @click="reloadFrame">重新加载</n-button>
      </template>
      <template v-else>
        <div class="loader-line"><span /></div>
        <p>正在加载内置 CC Switch…</p>
      </template>
    </div>
  </section>
</template>
