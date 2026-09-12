<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { NButton } from "naive-ui";
import type { CcSwitchImportResult } from "../ipc";

const props = defineProps<{
  preview: boolean;
  target: CcSwitchImportResult | null;
}>();

const frame = ref<HTMLIFrameElement | null>(null);
const ready = ref(false);
const childReportedError = ref(false);
const loadFailed = ref(false);
const frameVersion = ref(0);
let loadTimer: number | undefined;

const frameUrl = computed(() => {
  const params = new URLSearchParams({ embedded: "1" });
  if (props.preview) params.set("ui-preview", "ccswitch");
  return `/ccswitch/index.html?${params.toString()}`;
});

function sendProvider(type: "navigate" | "provider-imported", provider: CcSwitchImportResult): void {
  if (!ready.value || !frame.value?.contentWindow) return;
  frame.value.contentWindow.postMessage({
    source: "mollycloud",
    type,
    providerId: provider.provider_id,
    app: provider.app,
  }, window.location.origin);
}

function notifyProviderImported(provider: CcSwitchImportResult): void {
  sendProvider("provider-imported", provider);
}

function receiveMessage(event: MessageEvent): void {
  if (event.origin !== window.location.origin || event.source !== frame.value?.contentWindow) return;
  const data = event.data;
  if (!data || typeof data !== "object" || data.source !== "molly-ccswitch") return;
  if (data.type === "load-error") {
    childReportedError.value = true;
    if (loadTimer) window.clearTimeout(loadTimer);
    return;
  }
  if (data.type !== "ready") return;
  ready.value = true;
  childReportedError.value = false;
  loadFailed.value = false;
  if (loadTimer) window.clearTimeout(loadTimer);
  if (props.target) sendProvider("navigate", props.target);
}

function startLoadTimer(): void {
  if (loadTimer) window.clearTimeout(loadTimer);
  loadTimer = window.setTimeout(() => {
    if (!ready.value) loadFailed.value = true;
  }, 20_000);
}

function reloadFrame(): void {
  ready.value = false;
  childReportedError.value = false;
  loadFailed.value = false;
  frameVersion.value += 1;
  startLoadTimer();
}

watch(() => props.target, (target) => {
  if (target) sendProvider("navigate", target);
});

onMounted(() => {
  window.addEventListener("message", receiveMessage);
  startLoadTimer();
});

onBeforeUnmount(() => {
  window.removeEventListener("message", receiveMessage);
  if (loadTimer) window.clearTimeout(loadTimer);
});

defineExpose({ notifyProviderImported });
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
