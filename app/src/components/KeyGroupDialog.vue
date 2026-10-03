<script setup lang="ts">
import { NButton, NModal } from 'naive-ui';
import { ref } from 'vue';
import KeyGroupPicker from './KeyGroupPicker.vue';
import { groupFromKey } from '../keyGroups';
import { textValue } from '../format';
import type { KeyGroupChanged } from '../contracts';
defineProps<{show:boolean;item:Record<string,unknown>|null}>();
const emit=defineEmits<{'update:show':[boolean];changed:[KeyGroupChanged];closed:[]}>();
const busy=ref(false);
const picker=ref<InstanceType<typeof KeyGroupPicker>|null>(null);
function changed(value:KeyGroupChanged){busy.value=false;emit('changed',value);emit('update:show',false);}
</script>
<template>
  <n-modal :show="show" to="#console-settings-layer" :mask-closable="false" :close-on-esc="!busy" transform-origin="center" @update:show="!busy && emit('update:show',false)" @after-enter="picker?.focusSearch()" @after-leave="emit('closed')">
    <div class="console-settings-dialog key-group-dialog" role="dialog" aria-modal="true" aria-labelledby="key-group-dialog-title">
      <header class="console-settings-header"><div><h2 id="key-group-dialog-title">修改分组</h2><p>{{ textValue(item?.name, '未命名密钥') }}</p></div></header>
      <div class="console-settings-body"><KeyGroupPicker v-if="show && item" ref="picker" inline :value="groupFromKey(item)" :key-id="String(item.id)" :key-name="textValue(item.name)" @changed="changed" @busy="busy=$event" @opened="value=>{if(!value && !busy)emit('update:show',false)}" /></div>
      <footer class="console-settings-actions"><n-button :disabled="busy" @click="emit('update:show',false)">关闭</n-button></footer>
    </div>
  </n-modal>
</template>
