<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue';
import { NButton, NInput, NPopover, NSpin } from 'naive-ui';
import { desktopApi } from '../ipc';
import type { KeyGroup, KeyGroupChanged, KeyGroups } from '../contracts';
import { providerNames } from '../keyGroups';
import GroupBadge from './GroupBadge.vue';
import AppIcon from './AppIcon.vue';
const props = defineProps<{ value: KeyGroup | null; keyId?: string; keyName?: string; disabled?: boolean; groups?: KeyGroups }>();
const emit = defineEmits<{ selected:[KeyGroup]; changed:[KeyGroupChanged]; opened:[boolean] }>();
const open = ref(false), loading = ref(false), saving = ref(false), query = ref(''), error = ref('');
const data = ref<KeyGroups>({groups:[],rates_available:false});
const search = ref<InstanceType<typeof NInput> | null>(null), trigger = ref<HTMLButtonElement | null>(null);
const activeIndex = ref(0), listId = useId();
let generation = 0;
let disposed=false;
onBeforeUnmount(()=>{disposed=true;++generation;});
onMounted(()=>{if(!props.groups)void load();});
const available = computed(() => props.groups ?? data.value);
const current = computed(() => available.value.groups.find(g=>g.id===props.value?.id) ?? props.value);
const filtered = computed(() => available.value.groups.filter(g=>`${g.name} ${g.description} ${providerNames[g.platform] ?? g.platform}`.toLowerCase().includes(query.value.toLowerCase().trim())));
const activeId = computed(() => filtered.value[activeIndex.value] ? `${listId}-${activeIndex.value}` : undefined);
watch(query,()=>activeIndex.value=0);
async function load() {
  if(props.groups) return;
  const request = ++generation;
  loading.value=true; error.value='';
  try { const result=await desktopApi.fetchKeyGroups(); if(request===generation) data.value=result; }
  catch(e){if(request===generation)error.value=e instanceof Error?e.message:String(e);}
  finally{if(request===generation)loading.value=false;}
}
async function toggle(show:boolean) {
  if(saving.value) return;
  open.value=show; emit('opened',show);
  if(show){query.value='';activeIndex.value=0;await load();await nextTick();search.value?.focus();}
}
function close(){open.value=false;emit('opened',false);void nextTick(()=>trigger.value?.focus());}
async function select(group:KeyGroup){
  if(saving.value || loading.value) return;
  if(group.id===props.value?.id){close();return;}
  saving.value=true;error.value='';
  try {
    if(props.keyId){const result=await desktopApi.changeKeyGroup(props.keyId,group.id);if(disposed)return;emit('changed',result);}
    else emit('selected',group);
    close();
  }catch(e){error.value=e instanceof Error?e.message:String(e);}
  finally{saving.value=false;}
}
function keydown(event:KeyboardEvent){
  if(event.key==='Escape'){event.preventDefault();event.stopPropagation();if(!saving.value)close();return;}
  if(event.key==='ArrowDown'||event.key==='ArrowUp'){
    event.preventDefault();activeIndex.value=Math.max(0,Math.min(filtered.value.length-1,activeIndex.value+(event.key==='ArrowDown'?1:-1)));
    void nextTick(()=>document.getElementById(activeId.value ?? '')?.scrollIntoView({block:'nearest'}));
  }
  if(event.key==='Enter'){event.preventDefault();const group=filtered.value[activeIndex.value];if(group)void select(group);}
}
</script>
<template>
  <n-popover :show="open" trigger="click" placement="bottom-start" :show-arrow="false" :disabled="disabled" :style="{padding:'0',maxWidth:'calc(100vw - 40px)'}" @update:show="toggle">
    <template #trigger><button ref="trigger" class="key-group-trigger" type="button" :disabled="disabled || saving" aria-haspopup="listbox" :aria-expanded="open" :aria-controls="listId" :aria-label="keyId ? `切换 ${keyName ?? '密钥'} 的分组` : '选择密钥分组'" @keydown.down.prevent="toggle(true)"><GroupBadge v-if="current" :group="current" show-rate/><span v-else class="key-group-empty">未选择</span><span class="key-group-trigger__hint">选择分组</span><AppIcon name="chevron"/></button></template>
    <div class="key-group-menu" :aria-busy="loading || saving" @keydown="keydown">
      <div class="key-group-search"><n-input ref="search" v-model:value="query" placeholder="搜索分组…" aria-label="搜索分组" role="combobox" aria-autocomplete="list" :aria-expanded="true" :aria-controls="listId" :aria-activedescendant="activeId" :disabled="saving" clearable><template #prefix><AppIcon name="search"/></template></n-input></div>
      <div v-if="loading" class="key-group-message" role="status"><n-spin size="small"/> 正在加载分组…</div>
      <div v-else :id="listId" class="key-group-options" role="listbox" aria-label="可用分组">
        <button v-for="(group,index) in filtered" :id="`${listId}-${index}`" :key="group.id" class="key-group-option" :class="{'is-current':group.id===value?.id,'is-active':index===activeIndex}" type="button" role="option" :aria-selected="group.id===value?.id" :disabled="saving" :tabindex="-1" @pointermove="activeIndex=index" @click="select(group)">
          <span class="key-group-option__info"><GroupBadge :group="group"/><small>{{ group.description || (group.subscription ? '订阅分组' : '供应商') }}</small></span>
          <span class="key-group-rate" :data-rate-platform="['openai','anthropic','gemini'].includes(group.platform)?group.platform:'other'"><s v-if="group.custom_rate && group.default_rate !== null && group.default_rate!==group.rate">{{ group.default_rate }}x</s>{{ group.rate === null ? '倍率未知' : `${group.rate}x 倍率` }}<span v-if="group.id===value?.id" aria-label="当前分组"> ✓</span></span>
        </button>
        <p v-if="!filtered.length" class="key-group-message">{{ query ? '没有匹配的分组' : '暂无可用分组' }}</p>
      </div>
      <p v-if="saving" class="key-group-message" role="status">正在应用分组…</p>
      <p v-if="!loading && !available.rates_available && available.groups.length" class="key-group-note">暂显示默认倍率，实际计费以服务端为准。</p>
      <div v-if="error" class="key-group-error" role="alert">{{ error }}<n-button v-if="!saving" text size="small" @click="load">刷新分组</n-button></div>
    </div>
  </n-popover>
</template>
