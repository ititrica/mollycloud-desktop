<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { NAlert, NButton, NInput, NInputNumber, NModal, NSelect, NSwitch } from 'naive-ui';
import { desktopApi } from '../ipc';
import type { KeyGroup, KeyGroups, CreateKeyRequest } from '../contracts';
import { providerNames } from '../keyGroups';
import AppIcon from './AppIcon.vue';
import KeyGroupPicker from './KeyGroupPicker.vue';
const props = defineProps<{show:boolean;preview:boolean}>();
const emit = defineEmits<{'update:show':[boolean];created:[Record<string,unknown>];closed:[]}>();
const name=ref(''), platform=ref('all'), group=ref<KeyGroup|null>(null), data=ref<KeyGroups>({groups:[],rates_available:false});
const loading=ref(false), saving=ref(false), error=ref(''), custom=ref(false), ips=ref(false), limits=ref(false), expiry=ref(false);
const customKey=ref(''), whitelist=ref(''), blacklist=ref(''), quota=ref<number|null>(null), days=ref<number|null>(30);
const five=ref<number|null>(null), day=ref<number|null>(null), week=ref<number|null>(null);
let generation=0;
const platforms=computed(()=>[{label:'全部供应商',value:'all'},...Array.from(new Set(data.value.groups.map(g=>g.platform))).map(p=>({value:p,label:providerNames[p]??p}))]);
const filtered=computed(()=>({...data.value,groups:data.value.groups.filter(g=>platform.value==='all'||g.platform===platform.value)}));
watch(platform,()=>{if(group.value&&!filtered.value.groups.some(g=>g.id===group.value?.id))group.value=null;});
watch(()=>props.show,async show=>{
  const current=++generation;
  if(!show){customKey.value='';return;}
  name.value='';group.value=null;platform.value='all';error.value='';data.value={groups:[],rates_available:false};
  custom.value=ips.value=limits.value=expiry.value=false;customKey.value=whitelist.value=blacklist.value='';quota.value=five.value=day.value=week.value=null;days.value=30;
  loading.value=true;
  try{const result=await desktopApi.fetchKeyGroups();if(current===generation)data.value=result;}
  catch(e){if(current===generation)error.value=String(e instanceof Error?e.message:e);}
  finally{if(current===generation)loading.value=false;}
});
function close(){if(!saving.value)emit('update:show',false);}
function lines(text:string){return text.split(/\r?\n/).map(s=>s.trim()).filter(Boolean);}
async function submit(){
  if(saving.value||loading.value||!name.value.trim()||!group.value)return;
  if(custom.value&&!/^[A-Za-z0-9_-]{16,512}$/.test(customKey.value)){error.value='自定义密钥须为 16–512 位字母、数字、下划线或连字符。';return;}
  if(expiry.value&&(!days.value||!Number.isInteger(days.value)||days.value<1||days.value>36500)){error.value='有效期须为 1–36500 天。';return;}
  const request:CreateKeyRequest={name:name.value.trim(),group_id:group.value.id,quota:quota.value??0,
    ...(custom.value?{custom_key:customKey.value}:{}),
    ip_whitelist:ips.value?lines(whitelist.value):[],ip_blacklist:ips.value?lines(blacklist.value):[],
    ...(expiry.value?{expires_in_days:days.value!}:{}),
    ...(limits.value?{rate_limit_5h:five.value??0,rate_limit_1d:day.value??0,rate_limit_7d:week.value??0}:{})};
  saving.value=true;error.value='';
  try{const item=await desktopApi.createApiKey(request);emit('created',item);emit('update:show',false);}
  catch(e){error.value=`${e instanceof Error?e.message:String(e)} 若请求超时，请先刷新密钥列表确认是否已创建。`;}
  finally{saving.value=false;}
}
</script>
<template>
  <n-modal :show="show" to="#console-settings-layer" :mask-closable="!saving" :close-on-esc="!saving" transform-origin="center" @update:show="close" @after-leave="emit('closed')">
    <div class="console-settings-dialog create-key-dialog" role="dialog" aria-modal="true" aria-labelledby="create-key-title" :aria-busy="saving">
      <header class="console-settings-header"><span class="console-settings-icon"><AppIcon name="key"/></span><div><h2 id="create-key-title">创建 API 密钥</h2><p>为工具分配独立密钥，按需设置额度与访问权限。</p></div></header>
      <form class="console-settings-form" @submit.prevent="submit">
        <div class="console-settings-body create-key-fields">
          <label class="native-field"><span>名称 <small>必填</small></span><n-input v-model:value="name" placeholder="例如：我的开发工具" :maxlength="128" :disabled="saving" aria-label="密钥名称" autofocus/></label>
          <label class="native-field"><span>供应商</span><n-select v-model:value="platform" :options="platforms" :loading="loading" :disabled="saving||loading" aria-label="密钥供应商"/></label>
          <div class="native-field"><span>分组 <small>必选</small></span><KeyGroupPicker :value="group" :groups="filtered" :disabled="saving||loading" @selected="group=$event"/><small>后续请求按此分组路由与计费；创建后可随时切换。</small></div>
          <label class="native-field"><span>总额度（USD）</span><n-input-number v-model:value="quota" :min="0" :precision="2" placeholder="0 或留空表示不限额" :disabled="saving" aria-label="总额度" :show-button="false"/></label>
          <div class="native-option"><span>自定义密钥</span><n-switch v-model:value="custom" :disabled="saving" aria-label="自定义密钥"/></div>
          <n-input v-if="custom" v-model:value="customKey" type="password" show-password-on="click" placeholder="16–512 位字母、数字、下划线或连字符" :disabled="saving" aria-label="自定义密钥内容" autocomplete="off"/>
          <div class="native-option"><span>IP 访问限制</span><n-switch v-model:value="ips" :disabled="saving" aria-label="IP 访问限制"/></div>
          <div v-if="ips" class="native-columns"><label class="native-field"><span>白名单</span><n-input v-model:value="whitelist" type="textarea" placeholder="每行一个 IP 或 CIDR" :disabled="saving" aria-label="IP 白名单"/></label><label class="native-field"><span>黑名单</span><n-input v-model:value="blacklist" type="textarea" placeholder="每行一个 IP 或 CIDR" :disabled="saving" aria-label="IP 黑名单"/></label></div>
          <div class="native-option"><span>周期额度限制 <small>USD · 0 表示不限额</small></span><n-switch v-model:value="limits" :disabled="saving" aria-label="周期额度限制"/></div>
          <div v-if="limits" class="native-columns native-columns--three"><label class="native-field"><span>5 小时</span><n-input-number v-model:value="five" :min="0" :disabled="saving" :show-button="false" aria-label="5 小时额度"/></label><label class="native-field"><span>1 天</span><n-input-number v-model:value="day" :min="0" :disabled="saving" :show-button="false" aria-label="1 天额度"/></label><label class="native-field"><span>7 天</span><n-input-number v-model:value="week" :min="0" :disabled="saving" :show-button="false" aria-label="7 天额度"/></label></div>
          <div class="native-option"><span>设置有效期 <small>默认永不过期</small></span><n-switch v-model:value="expiry" :disabled="saving" aria-label="设置有效期"/></div>
          <label v-if="expiry" class="native-field"><span>有效天数</span><n-input-number v-model:value="days" :min="1" :max="36500" :precision="0" :disabled="saving" aria-label="有效天数"/></label>
          <p v-if="preview" class="console-settings-hint">界面预览：仅创建演示密钥。</p>
          <n-alert v-if="error" type="error" :bordered="false" :show-icon="false" role="alert">{{error}}</n-alert>
        </div>
        <footer class="console-settings-actions"><n-button size="large" :disabled="saving" @click="close">取消</n-button><n-button type="primary" size="large" attr-type="submit" :disabled="loading||saving||!name.trim()||!group" :loading="saving">创建密钥</n-button></footer>
      </form>
    </div>
  </n-modal>
</template>
