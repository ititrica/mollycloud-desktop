<script setup lang="ts">
import { ref, watch } from "vue";
import { systemLoginName, trayName } from "../platform";
import { version as bundledAppVersion } from "../../package.json";
import { NAlert, NButton, NModal, NSwitch } from "naive-ui";
import PluginManager from "./PluginManager.vue";
import { loadPlugins, pluginBusy, pluginNeedsRestart } from "../plugins";
import { invoke } from "@tauri-apps/api/core";
import AppIcon from "./AppIcon.vue";
import { desktopApi, type ConsoleCloseAction, type DesktopUpdate } from "../ipc";
import { setThemePreference, themeOptions, themePreference, type ThemePreference } from "../appearance";

const props = defineProps<{ show: boolean; initialTab?: "general" | "plugins" }>();
const emit = defineEmits<{ "update:show": [show: boolean]; closed: []; "check-update": [update: DesktopUpdate | null] }>();
const activeTab = ref<"general" | "plugins">("general");
const draftCloseAction = ref<ConsoleCloseAction>("tray");
const draftAutostart = ref(false);
const draftAutostartMinimized = ref(false);
const draftTheme = ref<ThemePreference>(themePreference.value);
const loading = ref(false);
const loaded = ref(false);
const saving = ref(false);
const errorMessage = ref("");
const updateMessage = ref("");
const appVersion = ref(bundledAppVersion);
let loadVersion = 0;

async function loadSettings(): Promise<void> {
  const version = ++loadVersion;
  loading.value = true;
  loaded.value = false;
  errorMessage.value = "";
  updateMessage.value = "";
  draftTheme.value = themePreference.value;
  try {
    const settings = await desktopApi.getConsoleSettings();
    if (version !== loadVersion || !props.show) return;
    draftCloseAction.value = settings.closeAction;
    draftAutostart.value = settings.autostart;
    draftAutostartMinimized.value = settings.autostartMinimized;
    loaded.value = true;
  } catch (error) {
    if (version === loadVersion) errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    if (version === loadVersion) loading.value = false;
  }
}

async function loadAppVersion(): Promise<void> {
  appVersion.value = await desktopApi.getAppVersion().catch(() => bundledAppVersion);
}

const checkingUpdate = ref(false);
async function checkForUpdate(): Promise<void> {
  if (checkingUpdate.value || saving.value) return;
  checkingUpdate.value = true;
  errorMessage.value = "";
  updateMessage.value = "";
  try {
    const update = await desktopApi.checkForDesktopUpdate();
    emit("check-update", update);
    if (!update) updateMessage.value = "当前已是最新版本";
  } catch (error) {
    errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    checkingUpdate.value = false;
  }
}

function cancel(): void {
  if (!saving.value) emit("update:show", false);
}

async function saveSettings(): Promise<void> {
  if (!loaded.value || saving.value) return;
  saving.value = true;
  errorMessage.value = "";
  updateMessage.value = "";
  try {
    await desktopApi.saveConsoleSettings({
      closeAction: draftCloseAction.value,
      autostart: draftAutostart.value,
      autostartMinimized: draftAutostart.value && draftAutostartMinimized.value,
    });
    if (draftTheme.value !== themePreference.value) setThemePreference(draftTheme.value);
    emit("update:show", false);
  } catch (error) {
    errorMessage.value = String(error instanceof Error ? error.message : error);
  } finally {
    saving.value = false;
  }
}

watch(() => props.show, (show) => {
  if (show) {
    activeTab.value = props.initialTab ?? "general";
    void loadPlugins();
    void loadSettings();
    void loadAppVersion();
  }
  else ++loadVersion;
}, { immediate: true });

watch(draftAutostart, (enabled) => {
  if (!enabled) draftAutostartMinimized.value = false;
});
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
      <nav class="settings-tabs" aria-label="设置分类"><button type="button" :aria-current="activeTab === 'general' ? 'page' : undefined" @click="activeTab = 'general'">通用设置</button><button type="button" :aria-current="activeTab === 'plugins' ? 'page' : undefined" @click="activeTab = 'plugins'">功能插件<span>3</span></button></nav>
      <form class="console-settings-form" @submit.prevent="saveSettings">
        <div v-show="activeTab === 'general'" class="console-settings-body">
          <fieldset class="console-settings-section" :disabled="loading || saving || !loaded">
            <legend>外观</legend>
            <p class="console-settings-hint">控制台与内嵌工作台使用同一外观，默认跟随系统。</p>
            <div class="console-theme-options">
              <label v-for="option in themeOptions" :key="option.value" class="console-theme-option" :class="{ 'is-selected': draftTheme === option.value }">
                <span class="theme-preview" :class="`theme-preview--${option.value}`" aria-hidden="true"><i /></span>
                <AppIcon :name="option.icon" /><span>{{ option.label }}</span>
                <input v-model="draftTheme" type="radio" name="console-theme" :value="option.value" :aria-label="option.label" />
              </label>
            </div>
          </fieldset>
          <fieldset class="console-settings-section" :disabled="loading || saving || !loaded">
            <legend>关闭窗口</legend>
            <p class="console-settings-hint">关闭控制台时，退出程序或保留桌宠继续运行。</p>
            <div class="console-settings-options">
              <label class="console-close-option" :class="{ 'is-selected': draftCloseAction === 'quit' }">
                <span class="console-close-option__icon"><AppIcon name="power" /></span>
                <span class="console-close-option__copy"><strong>退出程序</strong></span>
                <input v-model="draftCloseAction" type="radio" name="console-close-action" value="quit" aria-label="退出程序" />
              </label>
              <label class="console-close-option" :class="{ 'is-selected': draftCloseAction === 'tray' }">
                <span class="console-close-option__icon"><AppIcon name="tray" /></span>
                <span class="console-close-option__copy"><strong>最小化到{{ trayName }}</strong></span>
                <input v-model="draftCloseAction" type="radio" name="console-close-action" value="tray" :aria-label="`最小化到${trayName}`" />
              </label>
            </div>
          </fieldset>
          <fieldset class="console-settings-section" :disabled="loading || saving || !loaded">
            <legend>系统启动</legend>
            <p class="console-settings-hint">登录 {{ systemLoginName }} 后自动启动 MollyCloud</p>
            <div class="console-settings-startup">
            <label class="console-settings-toggle" :class="{ 'is-selected': draftAutostart }">
              <span><strong>开机自启动</strong></span>
              <n-switch v-model:value="draftAutostart" size="small" aria-label="开机自启动" />
            </label>
            <label class="console-settings-toggle console-settings-toggle--dependent" :class="{ 'is-selected': draftAutostartMinimized, 'is-disabled': !draftAutostart }">
              <span><strong>启动后最小化到{{ trayName }}</strong></span>
              <n-switch v-model:value="draftAutostartMinimized" size="small" :aria-label="`启动后最小化到${trayName}`" :disabled="!draftAutostart" />
            </label>
            </div>
          </fieldset>
          <p v-if="loading" class="console-settings-status" role="status">正在读取设置…</p>
          <p v-if="updateMessage" class="console-settings-status" role="status">{{ updateMessage }}</p>
          <n-alert v-if="errorMessage" class="console-settings-error" type="error" :bordered="false" :show-icon="false" role="alert">
            {{ errorMessage }}
            <n-button v-if="!loaded && !loading" size="small" secondary @click="loadSettings">重新读取</n-button>
          </n-alert>
        </div>
        <div v-if="activeTab === 'plugins'" class="console-settings-body"><PluginManager /></div>
        <footer class="console-settings-actions console-settings-actions--with-about">
          <div class="console-settings-release">
            <span>v{{ appVersion }}</span>
            <n-button class="console-settings-download" size="small" secondary attr-type="button" aria-label="检查更新" :disabled="saving || checkingUpdate" :loading="checkingUpdate" @click="checkForUpdate"><AppIcon name="update" /><span>检查更新</span></n-button>
          </div>
          <div v-if="activeTab === 'plugins'" class="console-settings-actions__primary">
            <n-button v-if="pluginNeedsRestart" secondary :disabled="!!pluginBusy" @click="invoke('restart_app')">重启应用</n-button>
            <n-button type="primary" size="large" :disabled="!!pluginBusy" @click="cancel">完成</n-button>
          </div>
          <div v-else class="console-settings-actions__primary">
            <n-button class="console-settings-cancel" size="large" :disabled="saving" @click="cancel">取消</n-button>
            <n-button class="console-settings-save" type="primary" size="large" attr-type="submit" :loading="saving" :disabled="!loaded || loading">保存</n-button>
          </div>
        </footer>
      </form>
    </div>
  </n-modal>
</template>
