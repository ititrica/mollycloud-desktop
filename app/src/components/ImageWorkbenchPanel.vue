<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { NButton } from 'naive-ui';
import { forwardImageRequest, imageDatabase } from '../imageWorkbench';

const props = defineProps<{ account: string; preview: boolean }>();
const emit = defineEmits<{ settled: [] }>();
const frame = ref<HTMLIFrameElement | null>(null);
const ready = ref(false);
const error = ref('');
const version = ref(0);
const native = '__TAURI_INTERNALS__' in window;
const frameUrl = native ? 'http://molly-image.localhost/index.html' : '/image-workbench/index.html';
let port: MessagePort | undefined;
let timer: ReturnType<typeof setTimeout>;
let disposed = false;
const requests = new Map<number, AbortController>();
const storageName = `molly-image-${props.preview ? 'preview' : 'account'}-${encodeURIComponent(props.account)}`;
const session = crypto.randomUUID();

function disconnect() {
  port?.close();
  port = undefined;
  requests.forEach(controller => controller.abort());
  requests.clear();
  clearTimeout(timer);
}
function reload() {
  disconnect();
  ready.value = false;
  error.value = '';
  version.value++;
  timer = setTimeout(() => { error.value = '生图工作台加载超时，请重新加载。'; }, 20000);
}
function connect(event: MessageEvent) {
  // sandbox 子页面没有宿主来源权限。只接受当前 iframe 发出的一次握手。
  if (disposed || port || event.origin !== 'null' || event.source !== frame.value?.contentWindow || event.data?.source !== 'molly-image' || event.data.type !== 'connect') return;
  try {
    const settings = localStorage.getItem(storageName);
    const channel = new MessageChannel();
    port = channel.port1;
    const connectedPort = port;
    port.onmessage = async ({ data }) => {
      if (disposed) return;
      const send = (value: object) => { if (!disposed && port === connectedPort) connectedPort.postMessage({ id: data.id, ...value }); };
      try {
        if (data.type === 'ready') { ready.value = true; clearTimeout(timer); return; }
        if (data.type === 'load-error') { error.value = '作品库读取失败，请检查本地存储后重新加载。'; clearTimeout(timer); return; }
        if (data.type === 'settled') { emit('settled'); return; }
        if (!Number.isSafeInteger(data.id)) return;
        if (data.type === 'settings') {
          if (data.value !== null && typeof data.value !== 'string') throw new Error('无效的配置。');
          if (data.value === null) localStorage.removeItem(storageName);
          else localStorage.setItem(storageName, data.value);
          send({ value: null });
        } else if (data.type === 'database') {
          send({ value: await imageDatabase(storageName, data.operations) });
        } else if (data.type === 'cancel') {
          requests.get(data.id)?.abort();
        } else if (data.type === 'fetch') {
          if (requests.size >= 12 || requests.has(data.id)) throw new Error('同时进行的请求过多，请稍后重试。');
          const controller = new AbortController();
          requests.set(data.id, controller);
          try { await forwardImageRequest(data, `${session}-${data.id}`, send, controller.signal); }
          finally { requests.delete(data.id); }
        }
      } catch (reason) { send({ error: reason instanceof Error ? reason.message : '工作台操作失败。' }); }
    };
    frame.value!.contentWindow!.postMessage({ type: 'molly-image-connect', settings }, '*', [channel.port2]);
  } catch { error.value = '无法读取工作台配置，请检查本地存储权限。'; clearTimeout(timer); }
}
onMounted(() => {
  window.addEventListener('message', connect);
  timer = setTimeout(() => { error.value = '生图工作台加载超时，请重新加载。'; }, 20000);
});
onBeforeUnmount(() => { disposed = true; disconnect(); window.removeEventListener('message', connect); });
</script>

<template>
  <section class="embedded-panel" aria-label="生图工作台">
    <iframe :key="version" ref="frame" :src="frameUrl" class="embedded-frame image-workbench-frame" title="GPT Image Playground 生图工作台"
      sandbox="allow-scripts allow-downloads allow-forms allow-popups allow-popups-to-escape-sandbox" allow="clipboard-read; clipboard-write" />
    <div v-if="!ready || error" class="embedded-load-state" role="status" aria-live="polite">
      <template v-if="error"><strong>生图工作台暂时无法加载</strong><p>{{ error }}</p><n-button secondary @click="reload">重新加载</n-button></template>
      <template v-else><div class="loader-line"><span /></div><p>正在加载生图工作台…</p></template>
    </div>
  </section>
</template>
