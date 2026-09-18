<script setup lang="ts">
import { NButton, NModal } from "naive-ui";
import AppIcon from "./AppIcon.vue";
defineProps<{ show: boolean }>();
defineEmits<{ acknowledge: []; cancel: [] }>();
</script>

<template>
  <n-modal :show="show" to="#console-settings-layer" :mask-closable="false" :close-on-esc="false" transform-origin="center">
    <div class="console-settings-dialog" role="alertdialog" aria-modal="true" aria-labelledby="mcp-experimental-title" aria-describedby="mcp-experimental-description">
      <header class="console-settings-header"><span class="console-settings-icon"><AppIcon name="shield" /></span><div><h2 id="mcp-experimental-title">这是一个实验性功能</h2><p>MCP 市场 · 首次使用提示</p></div></header>
      <div class="console-settings-body"><p id="mcp-experimental-description">MCP 市场仍在完善中，部分服务可能存在兼容性问题。安装、更新和卸载会修改所选 Agent 的本机配置，请确认来源和安装参数后操作。</p><p class="console-settings-hint">确认后即可进入，后续打开不再重复提示。</p></div>
      <footer class="console-settings-actions"><n-button @click="$emit('cancel')">暂不进入</n-button><n-button type="primary" @click="$emit('acknowledge')">我已知晓</n-button></footer>
    </div>
  </n-modal>
</template>
