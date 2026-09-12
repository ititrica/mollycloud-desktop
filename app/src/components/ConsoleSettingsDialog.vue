<script setup lang="ts">
import { ref, watch } from "vue";
import { NAlert, NButton, NModal, NSwitch } from "naive-ui";
import AppIcon from "./AppIcon.vue";
import { desktopApi, type ConsoleCloseAction } from "../ipc";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ "update:show": [show: boolean]; closed: [] }>();
const draftCloseAction = ref<ConsoleCloseAction>("tray");
const draftAutostart = ref(false);
const loading = ref(false);
const loaded = ref(false);
const saving = ref(false);
const errorMessage = ref("");
let loadVersion = 0;

async function loadSettings(): Promise<void> {
  const version = ++loadVersion;
  loading.value = true;
  loaded.value = false;
  errorMessage.value = "";
  try {
    const settings = await desktopApi.getConsoleSettings();
    if (version !== loadVersion || !props.show) return;
    draftCloseAction.value = settings.closeAction;
    draftAutostart.value = settings.autostart;
    loaded.value = true;
  } catch (error) {
    if (version === loadVersion) errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    if (version === loadVersion) loading.value = false;
  }
}

function cancel(): void {
  if (!saving.value) emit("update:show", false);
}

async function saveSettings(): Promise<void> {
  if (!loaded.value || saving.value) return;
  saving.value = true;
  errorMessage.value = "";
  try {
    await desktopApi.saveConsoleSettings({ closeAction: draftCloseAction.value, autostart: draftAutostart.value });
    emit("update:show", false);
  } catch (error) {
    errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    saving.value = false;
  }
}

watch(() => props.show, (show) => {
  if (show) void loadSettings();
  else ++loadVersion;
}, { immediate: true });
</script>

<template>
  <n-modal
    :show="show"
    to="#console-settings-layer"
    :mask-closable="!saving"
    :close-on-esc="!saving"
    transform-origin="center"
    @update:show="cancel"
    @after-leave="emit('closed')"
  >
    <!-- Naive UI's focus trap locates a div root; the explicit dialog role supplies semantics. -->
    <div class="console-settings-dialog" role="dialog" aria-modal="true" aria-labelledby="console-settings-title" aria-describedby="console-settings-description" :aria-busy="loading || saving">
      <header class="console-settings-header">
        <span class="console-settings-icon"><AppIcon name="settings" /></span>
        <div><h2 id="console-settings-title">设置</h2><p id="console-settings-description">让 MollyCloud 按你的习惯运行。</p></div>
      </header>
      <form class="console-settings-form" @submit.prevent="saveSettings">
        <div class="console-settings-body">
          <fieldset class="console-settings-section" :disabled="loading || saving || !loaded">
            <legend>关闭窗口</legend>
            <p class="console-settings-hint">点击控制台右上角的关闭按钮时</p>
            <div class="console-settings-options">
              <label class="console-close-option" :class="{ 'is-selected': draftCloseAction === 'quit' }">
                <span class="console-close-option__icon"><AppIcon name="power" /></span>
                <span class="console-close-option__copy"><strong>退出程序</strong><small>关闭控制台和桌宠，退出 MollyCloud。</small></span>
                <input v-model="draftCloseAction" type="radio" name="console-close-action" value="quit" aria-label="退出程序" />
              </label>
              <label class="console-close-option" :class="{ 'is-selected': draftCloseAction === 'tray' }">
                <span class="console-close-option__icon"><AppIcon name="tray" /></span>
                <span class="console-close-option__copy"><strong>最小化到托盘</strong><small>隐藏控制台，桌宠继续运行，可从托盘重新打开。</small></span>
                <input v-model="draftCloseAction" type="radio" name="console-close-action" value="tray" aria-label="最小化到托盘" />
              </label>
            </div>
          </fieldset>
          <fieldset class="console-settings-section" :disabled="loading || saving || !loaded">
            <legend>系统启动</legend>
            <p class="console-settings-hint">登录 Windows 后自动启动 MollyCloud</p>
            <label class="console-settings-toggle" :class="{ 'is-selected': draftAutostart }">
              <span><strong>开机自启动</strong><small>自动启动控制台与桌宠，无需手动打开应用。</small></span>
              <n-switch v-model:value="draftAutostart" size="large" aria-label="开机自启动" />
            </label>
          </fieldset>
          <p v-if="loading" class="console-settings-status" role="status">正在读取设置…</p>
          <n-alert v-if="errorMessage" class="console-settings-error" type="error" :bordered="false" :show-icon="false" role="alert">
            {{ errorMessage }}
            <n-button v-if="!loaded && !loading" size="small" secondary @click="loadSettings">重新读取</n-button>
          </n-alert>
        </div>
        <footer class="console-settings-actions">
          <n-button class="console-settings-cancel" size="large" :disabled="saving" @click="cancel">取消</n-button>
          <n-button class="console-settings-save" type="primary" size="large" attr-type="submit" :loading="saving" :disabled="!loaded || loading">保存</n-button>
        </footer>
      </form>
    </div>
  </n-modal>
</template>
