<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { NAlert, NButton } from 'naive-ui';
import { desktopApi } from '../ipc';
import { paymentApi } from '../payments';
import { resolvedTheme } from '../appearance';

const props = defineProps<{ orderId: number; preview: boolean }>();
const emit = defineEmits<{ refresh: []; close: [] }>();
const viewId = crypto.randomUUID();
const error = ref('');
const opening = ref(false);
const loading = ref(false);
let created = false;
let disposed = false;
let timer: number | undefined;
const listeners: (() => void)[] = [];
const message = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);

async function open() {
  if (opening.value || disposed) return;
  if (props.preview) { created = true; return; }
  opening.value = true;
  error.value = '';
  try {
    await paymentApi.open(props.orderId, resolvedTheme.value === 'dark', viewId);
    created = true;
  } catch (reason) {
    if (!disposed) error.value = message(reason);
  } finally {
    opening.value = false;
    if (disposed) await desktopApi.closeRechargeView(viewId).catch(() => undefined);
  }
}
async function syncSession() {
  if (!created || disposed || props.preview) return;
  try { await desktopApi.openRechargeView(resolvedTheme.value === 'dark', viewId); }
  catch (reason) { if (!disposed) error.value = message(reason); }
}
watch(resolvedTheme, () => void syncSession());
onMounted(async () => {
  if (!props.preview) {
    try {
      const { listen } = await import('@tauri-apps/api/event');
      const subscriptions = await Promise.all([
        listen<{ owner: string; loading: boolean; error: string | null }>('recharge-page-status', ({ payload }) => {
          if (disposed || payload.owner !== viewId) return;
          loading.value = payload.loading;
          error.value = payload.error ?? '';
        }),
        listen<{ owner: string }>('recharge-window-closed', ({ payload }) => {
          if (disposed || payload.owner !== viewId) return;
          created = false;
          emit('refresh');
          emit('close');
        }),
        listen<{ owner: string }>('recharge-returned', ({ payload }) => {
          if (!disposed && payload.owner === viewId) emit('refresh');
        }),
      ]);
      if (disposed) { subscriptions.forEach(stop => stop()); return; }
      listeners.push(...subscriptions);
      timer = window.setInterval(() => void syncSession(), 45_000);
    } catch (reason) { error.value = message(reason); return; }
  }
  await open();
});
onBeforeUnmount(() => {
  disposed = true;
  window.clearInterval(timer);
  listeners.forEach(stop => stop());
  if (!props.preview) void desktopApi.closeRechargeView(viewId).catch(() => undefined);
});
defineExpose({ open });
</script>
<template>
  <n-alert class="payment-window-status" :type="error ? 'error' : 'info'" :bordered="false" :show-icon="false" role="status">
    <template v-if="error">订单已创建，支付窗口未就绪：{{ error }} <n-button size="small" :loading="opening" @click="open">重新打开支付窗口</n-button></template>
    <template v-else-if="preview">演示订单已创建。桌面客户端会自动打开独立支付窗口，控制台可继续操作。</template>
    <template v-else>{{ opening || loading ? '正在打开支付窗口…' : '请在独立支付窗口中完成付款。控制台可继续操作。' }}</template>
  </n-alert>
</template>
