<script setup lang="ts">
import { ref } from "vue";
import { NAlert, NButton } from "naive-ui";
import AppIcon from "./AppIcon.vue";
import { plugins, pluginBusy, pluginError, pluginChecks, pluginPreview, changePlugin, checkPlugin, type PluginId, type PluginAction } from "../plugins";
import { openExternalPage } from "../ipc";
const confirming = ref<PluginId | null>(null);
async function apply(id: PluginId, action: PluginAction) {
  if (await changePlugin(id, action)) confirming.value = null;
}
</script>
<template>
  <section class="plugin-manager" aria-label="功能插件">
    <p class="console-settings-hint">按需安装工作台。官方新版本通过兼容适配后，可在这里单独更新。</p>
    <n-alert v-if="pluginPreview" type="info" :bordered="false" :show-icon="false">当前为界面预览，安装状态仅用于演示。</n-alert>
    <n-alert v-if="pluginError" type="error" :bordered="false" :show-icon="false" role="alert">{{ pluginError }}</n-alert>
    <article v-for="plugin in plugins" :key="plugin.id" class="plugin-card" :data-plugin="plugin.id">
      <header><span class="plugin-icon"><AppIcon :name="plugin.id === 'ccswitch' ? 'code' : plugin.id === 'images' ? 'image' : 'skills'" /></span><div><h3>{{ plugin.name }}</h3><p>{{ plugin.description }}</p></div><span class="plugin-state" :class="{'is-installed':plugin.installed}">{{ plugin.installed ? '已安装' : '未安装' }}</span></header>
      <div class="plugin-version"><span>{{ plugin.installedVersion ? `适配版本 ${plugin.installedVersion}` : `随附版本 ${plugin.version}` }}</span><button type="button" class="text-action" @click="openExternalPage(`https://github.com/${plugin.repository}`)">官方项目<AppIcon name="arrow" /></button></div>
      <p v-if="plugin.installed && !plugin.compatible" class="plugin-notice">当前插件与主程序不兼容，请安装随附版本或检查更新。</p>
      <p v-if="plugin.restartRequired" class="plugin-notice" role="status">重启 MollyCloud 后{{ plugin.installed ? '启用原生功能' : '完全停止后台服务并清理插件文件' }}，已有数据会保留。</p>
      <div v-if="pluginChecks[plugin.id]" class="plugin-check-result" role="status">
        <p v-if="pluginChecks[plugin.id]!.officialVersion">官方最新版本：{{ pluginChecks[plugin.id]!.officialVersion }}</p>
        <p v-if="pluginChecks[plugin.id]!.officialError">官方版本检查失败：{{ pluginChecks[plugin.id]!.officialError }}</p>
        <template v-if="pluginChecks[plugin.id]!.package">
          <p>最新适配版本：{{ pluginChecks[plugin.id]!.package!.version }}{{ pluginChecks[plugin.id]!.package!.version === plugin.installedVersion ? ' · 已是最新' : '' }}</p>
          <p v-if="!pluginChecks[plugin.id]!.compatible">此版本需要更新 MollyCloud，请先检查主程序更新。</p>
          <p v-if="pluginChecks[plugin.id]!.package!.notes">{{ pluginChecks[plugin.id]!.package!.notes }}</p>
        </template>
        <p v-else>{{ pluginChecks[plugin.id]!.catalogError ? `适配目录暂不可用：${pluginChecks[plugin.id]!.catalogError}` : '尚未发布新的适配包。' }}</p>
      </div>
      <div v-if="confirming === plugin.id" class="plugin-confirm">
        <p>卸载会关闭此工作台。请先保存未完成的编辑；配置、技能库和图片记录会保留。</p>
        <n-button size="small" :disabled="!!pluginBusy" @click="confirming = null">取消</n-button>
        <n-button size="small" type="error" secondary :loading="pluginBusy === plugin.id" :disabled="!!pluginBusy" @click="apply(plugin.id, 'uninstall')">确认卸载</n-button>
      </div>
      <footer v-else>
        <n-button v-if="!plugin.installed || !plugin.compatible" size="small" type="primary" :loading="pluginBusy === plugin.id" :disabled="!!pluginBusy" @click="apply(plugin.id, 'install')">安装随附版本</n-button>
        <n-button v-if="pluginChecks[plugin.id]?.compatible && pluginChecks[plugin.id]?.updateAvailable" size="small" type="primary" :disabled="!!pluginBusy" @click="apply(plugin.id, 'update')">安装适配更新</n-button>
        <n-button size="small" secondary :loading="pluginBusy === plugin.id" :disabled="!!pluginBusy" @click="checkPlugin(plugin.id)">检查官方更新</n-button>
        <n-button v-if="plugin.canRollback" size="small" secondary :disabled="!!pluginBusy" @click="apply(plugin.id, 'rollback')">恢复上一版本</n-button>
        <n-button v-if="plugin.installed" size="small" quaternary :disabled="!!pluginBusy" @click="confirming = plugin.id">卸载</n-button>
      </footer>
    </article>
    <p class="plugin-data-note">卸载不会删除用户数据，也不会撤销已写入本机 Agent 的配置或已部署的 Skills。安装与卸载立即保存，关闭设置不会取消。</p>
  </section>
</template>
