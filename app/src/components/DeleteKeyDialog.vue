<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue';
import { NAlert, NButton, NModal } from 'naive-ui';
import { desktopApi } from '../ipc';
import AppIcon from './AppIcon.vue';

const props = defineProps<{ show: boolean; keyId: string; keyName: string; preview: boolean }>();
const emit = defineEmits<{ 'update:show': [boolean]; deleted: [string]; closed: [] }>();
const deleting = ref(false);
const error = ref('');
let generation = 0;
watch(() => [props.show, props.keyId], () => {
  generation++;
  error.value = '';
  deleting.value = false;
});
onBeforeUnmount(() => { generation++; });

function close() {
  if (!deleting.value) emit('update:show', false);
}
async function confirmDelete() {
  if (!props.show || !props.keyId || deleting.value) return;
  const current = generation;
  const keyId = props.keyId;
  deleting.value = true;
  error.value = '';
  try {
    await desktopApi.deleteApiKey(keyId);
    if (current !== generation) return;
    emit('deleted', keyId);
    emit('update:show', false);
  } catch (reason) {
    if (current === generation) {
      error.value = `${reason instanceof Error ? reason.message : String(reason)}。若请求超时，请取消并刷新列表，确认删除结果。`;
    }
  } finally {
    if (current === generation) deleting.value = false;
  }
}
</script>

<template>
  <n-modal :show="show" to="#console-settings-layer" :mask-closable="!deleting" :close-on-esc="!deleting" transform-origin="center" @update:show="close" @after-leave="emit('closed')">
    <!-- Naive UI locates its focus-trap content by the div root. -->
    <div class="console-settings-dialog delete-key-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-key-title" aria-describedby="delete-key-description" :aria-busy="deleting">
      <header class="console-settings-header">
        <span class="console-settings-icon"><AppIcon name="key" /></span>
        <div><h2 id="delete-key-title">删除 API 密钥</h2><p>此操作无法撤销。</p></div>
      </header>
      <div class="console-settings-body delete-key-body">
        <p id="delete-key-description">确定删除密钥「<strong>{{ keyName }}</strong>」吗？删除后，使用此密钥的工具将无法继续访问模型服务，需要替换为其他有效密钥。</p>
        <p v-if="preview" class="console-settings-hint">界面预览：仅删除演示密钥。</p>
        <n-alert v-if="error" type="error" :bordered="false" :show-icon="false" role="alert">{{ error }}</n-alert>
      </div>
      <footer class="console-settings-actions">
        <n-button class="cancel-delete-key" size="large" :disabled="deleting" autofocus @click="close">取消</n-button>
        <n-button class="confirm-delete-key" type="error" size="large" :loading="deleting" :disabled="deleting" @click="confirmDelete">确认删除</n-button>
      </footer>
    </div>
  </n-modal>
</template>

<style scoped>
.delete-key-dialog { width: min(480px, calc(100vw - 48px)); }
.delete-key-body { display: grid; gap: 16px; line-height: 1.7; overflow-wrap: anywhere; }
</style>
