<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';
import { NAlert, NButton, NSelect, NSlider, NSwitch } from 'naive-ui';
import AppIcon from './AppIcon.vue';
import { useSlidingSelection } from '../useSlidingSelection';
import { enterNavPanel, leaveNavPanel, cancelNavPanel } from '../navigationMotion';
import { emit, invoke, listen, readIslandSettings, readIslandMode, readSlots, saveIslandSetting, saveIslandMode, formatTraffic, type IslandSettings, type IslandMode } from '../netspeed';
import { counterDelta, readTraffic, type TrafficHistory } from '../../../vendor/netspeed-dynamic/frontend/traffic';
const props = defineProps<{ preview: boolean; active: boolean }>();
const settings = ref(readIslandSettings());
const mode = ref<IslandMode>(readIslandMode(settings.value));
const slots = ref(readSlots());
const tab = ref('status');
const tabNav = ref<HTMLElement|null>(null);
useSlidingSelection(tabNav,tab,true);
const busy = ref(false);
const error = ref('');
const down = ref(0), up = ref(0), cpu = ref(0), ram = ref(0);
const history = ref<TrafficHistory>(readTraffic());
const recent = ref<{ down: number; up: number }[]>([]);
const period = ref(7);
let timer: ReturnType<typeof setInterval> | undefined;
let previous: [number, number] | null = null;
let previousTime = 0;
let polling = false;
let disposed = false;
const unlisteners: (() => void)[] = [];
const modeOptions = [{ label: '实时网速', value: 'speed' }, { label: 'CPU / 内存', value: 'resource' }, { label: '媒体与歌词', value: 'music' }, { label: '游戏 FPS', value: 'fps' }, { label: '系统时间', value: 'time' }, { label: '自定义组合', value: 'custom' }];
const players = [{ label:'通用媒体（自动识别）',value:'other' },{label:'网易云音乐',value:'netease'},{label:'QQ 音乐',value:'qqmusic'},{label:'Spotify',value:'spotify'},{label:'酷狗音乐',value:'kugou'},{label:'洛雪音乐',value:'lx-music'},{label:'Apple Music',value:'applemusic'},{label:'PotPlayer',value:'potplayer'},{label:'Microsoft Edge',value:'edge'},{label:'Google Chrome',value:'chrome'},{label:'浏览器 Pro',value:'browserPro'}];
const slotOptions = [{ label:'空',value:'' }, {label:'网速',value:'speed'},{label:'CPU / 内存',value:'resource'},{label:'FPS',value:'fps'},{label:'媒体封面',value:'cover'},{label:'系统时间',value:'time'}];
const toggles: {key: keyof IslandSettings; title: string; detail: string}[] = [
  { key:'msg_notify',title:'系统通知',detail:'在灵动岛中显示 Windows 通知；首次使用可能需要系统授权。' },
  { key:'clipboard',title:'剪贴板链接',detail:'复制网页链接后在灵动岛显示打开入口。' },
  { key:'msg_mode',title:'静默模式',detail:'没有媒体、通知或活动时自动收起。' },
  { key:'autohide_fs',title:'全屏时隐藏',detail:'当前显示器进入全屏时收起灵动岛。' },
  { key:'autohide_fs_hover',title:'全屏时悬停唤出',detail:'在原位置停留鼠标，临时查看灵动岛。' },
  { key:'position_locked',title:'锁定位置',detail:'锁定后不再响应鼠标拖动。' },
  { key:'glow_border',title:'流光边框',detail:'在灵动岛边缘显示动态光效。' },
];
const dimensions: {key:keyof IslandSettings;label:string;min:number;max:number;step:number;unit:string}[] = [
  {key:'base_width',label:'基础宽度',min:120,max:400,step:1,unit:'px'},
  {key:'base_height',label:'基础高度',min:28,max:60,step:1,unit:'px'},
  {key:'music_base_width',label:'媒体宽度',min:180,max:420,step:1,unit:'px'},
  {key:'music_expanded_width',label:'媒体展开宽度',min:280,max:560,step:1,unit:'px'},
  {key:'msg_expanded_width',label:'通知宽度',min:280,max:560,step:1,unit:'px'},
  {key:'border_radius',label:'圆角',min:8,max:100,step:1,unit:'px'},
  {key:'app_scale',label:'整体缩放',min:0.75,max:1.5,step:0.05,unit:'×'},
  {key:'island_opacity',label:'背景不透明度',min:20,max:100,step:1,unit:'%'},
  {key:'lyric_delay',label:'歌词延迟',min:-5,max:5,step:0.1,unit:'s'},
];
const localToday = new Date();
const month = `${localToday.getFullYear()}-${String(localToday.getMonth()+1).padStart(2,'0')}`;
const total = computed(() => Object.values(history.value).reduce((sum, item) => ({up:sum.up+item.up,down:sum.down+item.down}), {up:0,down:0}));
const monthly = computed(() => Object.entries(history.value).filter(([date]) => date.startsWith(month)).reduce((sum,[,item]) => sum+item.up+item.down,0));
const days = computed(() => Array.from({length:period.value},(_,index) => { const date = new Date(); date.setDate(date.getDate()-period.value+index+1); const key = `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`; return {key,label:`${date.getMonth()+1}/${date.getDate()}`, ...(history.value[key] || {up:0,down:0})}; }));
const peak = computed(() => Math.max(1,...recent.value.flatMap(p=>[p.down,p.up])));
const trafficPeak = computed(() => Math.max(1,...days.value.map(d=>d.up+d.down)));
function line(key:'up'|'down') { return recent.value.map((p,i)=>`${i*600/Math.max(1,recent.value.length-1)},${110-p[key]/peak.value*98}`).join(' '); }
async function change<K extends keyof IslandSettings>(key:K,value:IslandSettings[K]) {
  if (busy.value) return;
  busy.value = true; error.value = '';
  try { await saveIslandSetting(key,value,settings.value); settings.value = {...settings.value,[key]:value}; }
  catch(e) { error.value = `设置未保存：${String(e)}`; }
  finally { busy.value = false; }
}
async function changeMode(value: IslandMode, nextSlots = slots.value) {
  if (busy.value) return;
  busy.value=true; error.value='';
  try { await saveIslandMode(value,nextSlots); slots.value=nextSlots; settings.value=readIslandSettings(); mode.value=value; }
  catch(e) { error.value=`切换失败：${String(e)}`; }
  finally { busy.value=false; }
}
async function resetPosition() { try { await emit('tray-reset-pos'); } catch(e) { error.value=String(e); } }
function syncSettings() { settings.value=readIslandSettings(); mode.value=readIslandMode(settings.value); slots.value=readSlots(); history.value=readTraffic(); }
async function poll() {
  if (!props.active || polling || disposed || props.preview) return;
  polling=true;
  try {
    const counters=await invoke<[number,number]>('get_network_stats');
    const now=performance.now();
    const elapsed=previousTime ? (now-previousTime)/1000 : 1;
    const delta=counterDelta(previous,counters); previous=counters; previousTime=now;
    down.value=delta[0]/elapsed; up.value=delta[1]/elapsed;
    recent.value=[...recent.value.slice(-59),{down:down.value,up:up.value}]; history.value=readTraffic();
  } catch(e) { error.value=`读取监测数据失败：${String(e)}`; }
  finally { polling=false; }
}
onMounted(async()=>{
  window.addEventListener('storage',syncSettings);
  if (props.preview) {
    down.value=2432000; up.value=358400; cpu.value=12; ram.value=46;
    recent.value=Array.from({length:40},(_,i)=>({down:1500000+Math.sin(i*.7)*800000,up:280000+Math.cos(i*.6)*100000}));
  }
  const off=await listen<{cpu:number;ram:number}>('resource-event',e=>{cpu.value=e.payload.cpu;ram.value=e.payload.ram;});
  if (disposed) { off(); return; } unlisteners.push(off);
  void poll(); timer=setInterval(poll,1000);
});
onBeforeUnmount(()=>{ disposed=true; clearInterval(timer); window.removeEventListener('storage',syncSettings); unlisteners.forEach(off=>off()); });
</script>

<template>
  <div class="netspeed-panel">
    <n-alert v-if="error" type="error" :bordered="false" closable @close="error=''">{{ error }}</n-alert>
    <div class="island-toolbar">
    <nav ref="tabNav" class="island-tabs molly-subnav" aria-label="灵动岛设置分类">
      <i class="selection-indicator" aria-hidden="true"/>
      <button v-for="item in [{id:'status',label:'实时状态'},{id:'display',label:'显示与交互'},{id:'appearance',label:'外观'}]" :key="item.id" type="button" :aria-current="tab===item.id?'page':undefined" @click="tab=item.id">{{ item.label }}</button>
    </nav>
    <div class="island-enable"><span>{{ settings.widget_visible ? '灵动岛已开启' : '灵动岛已关闭' }}</span><n-switch :value="settings.widget_visible" :disabled="busy" aria-label="启用灵动岛" @update:value="change('widget_visible',$event)" /></div>
    </div>
    <Transition :css="false" mode="out-in" @enter="enterNavPanel" @leave="leaveNavPanel" @enter-cancelled="cancelNavPanel" @leave-cancelled="cancelNavPanel">
    <div v-if="tab==='status'" key="status" class="island-section">
      <div class="island-metrics">
        <article v-for="item in [{label:'下载速度',value:formatTraffic(down,true),icon:'download'},{label:'上传速度',value:formatTraffic(up,true),icon:'upload'},{label:'CPU',value:cpu+'%',icon:'usage'},{label:'内存',value:ram+'%',icon:'overview'}]" :key="item.label" class="island-card island-metric"><span><AppIcon :name="item.icon as any" />{{ item.label }}</span><strong>{{ item.value }}</strong></article>
      </div>
      <section class="island-card"><div class="island-card-heading"><h3>实时网速</h3><span class="island-legend"><i class="down" /> 下载 <i class="up" /> 上传</span></div><div class="island-chart"><span>{{ formatTraffic(peak,true) }}</span><svg viewBox="0 0 600 120" preserveAspectRatio="none" role="img" aria-label="最近一分钟上传与下载速度"><path d="M0 10H600M0 60H600M0 110H600" class="chart-grid"/><polyline :points="line('down')" class="chart-down"/><polyline :points="line('up')" class="chart-up"/></svg><div class="island-chart-axis"><span>60 秒前</span><span>现在</span></div></div></section>
      <section class="island-card"><div class="island-card-heading"><h3>流量统计</h3><n-select v-model:value="period" aria-label="流量统计范围" class="island-period" size="small" :options="[{label:'最近 7 天',value:7},{label:'最近 30 天',value:30}]" /></div><div class="island-totals"><div><span>累计下载</span><strong>{{ formatTraffic(total.down) }}</strong></div><div><span>累计上传</span><strong>{{ formatTraffic(total.up) }}</strong></div><div><span>本月流量</span><strong>{{ formatTraffic(monthly) }}</strong></div></div><div class="island-bars" role="img" aria-label="每日流量柱状图"><div v-for="day in days" :key="day.key" class="island-bar" :title="`${day.key} · ${formatTraffic(day.up+day.down)}`"><i :style="{height:Math.max(2,(day.up+day.down)/trafficPeak*90)+'px'}"/><small v-if="period===7">{{ day.label }}</small></div></div><p class="island-note">仅记录 MollyCloud 运行期间的本机网卡流量，保留最近一年；与账户 API 用量分别统计。</p></section>
    </div>
    <div v-else-if="tab==='display'" key="display" class="island-section">
      <section class="island-card"><div class="island-card-heading"><h3>常驻内容</h3></div><div class="island-mode-grid"><button v-for="item in modeOptions" :key="item.value" type="button" :class="{selected:mode===item.value}" :aria-pressed="mode===item.value" :disabled="busy" @click="changeMode(item.value as IslandMode)">{{ item.label }}</button></div><div v-if="mode==='music'" class="island-setting"><div><strong>媒体来源</strong><p>识别系统媒体会话，显示封面、播放控制与歌词。</p></div><n-select :value="settings.target_player" :options="players" :disabled="busy" aria-label="媒体来源" class="island-select" @update:value="change('target_player',$event)" /></div><div v-if="mode==='custom'" class="island-slots"><label v-for="(_,index) in slots" :key="index">位置 {{ index+1 }}<n-select :value="slots[index]||''" :options="slotOptions.map(o=>({...o,disabled:!!o.value && slots.includes(o.value) && slots[index]!==o.value}))" :disabled="busy" :aria-label="`自定义位置 ${index+1}`" @update:value="changeMode('custom',slots.map((v,i)=>i===index?($event||null):v))" /></label></div><p v-if="mode==='fps'" class="island-note">FPS 采集仅在选择此模式时启用，游戏或系统权限限制可能导致无法采集。</p></section>
      <section class="island-card"><h3>显示与交互</h3><div v-for="item in toggles" :key="item.key" class="island-setting"><div><strong>{{ item.title }}</strong><p>{{ item.detail }}</p></div><n-switch :value="!!settings[item.key]" :disabled="busy||(item.key==='autohide_fs_hover'&&!settings.autohide_fs)" :aria-label="item.title" @update:value="change(item.key,$event)" /></div><div class="island-setting"><div><strong>灵动岛位置</strong><p>可直接拖动灵动岛；重置后返回屏幕顶部中央。</p></div><n-button size="small" @click="resetPosition">重置位置</n-button></div></section>
      <section class="island-card"><h3>扩展显示</h3><div class="island-setting"><div><strong>任务栏显示</strong><p>使用内置辅助组件将信息同步到 Windows 任务栏。</p></div><n-switch :value="settings.taskbar_plugin" :disabled="busy" aria-label="任务栏显示" @update:value="change('taskbar_plugin',$event)" /></div><div class="island-setting"><div><strong>本机活动接口</strong><p>允许本机工具通过 127.0.0.1:47300 提交任务进度；默认关闭。</p></div><n-switch :value="settings.activity_api" :disabled="busy" aria-label="本机活动接口" @update:value="change('activity_api',$event)" /></div><p class="island-note">开机启动统一使用 MollyCloud「设置 → 系统启动」，灵动岛不单独注册。</p></section>
    </div>
    <div v-else key="appearance" class="island-section">
      <section class="island-card"><div class="island-card-heading"><h3>灵动岛外观</h3><span>控制台配色跟随 MollyCloud</span></div><div class="island-setting"><strong>岛体颜色</strong><n-select :value="settings.island_theme" class="island-select" aria-label="岛体颜色" :options="[{label:'深色',value:'black'},{label:'浅色',value:'white'}]" :disabled="busy" @update:value="change('island_theme',$event)" /></div><div class="island-setting"><strong>形变动画</strong><n-select :value="settings.spring_style" class="island-select" aria-label="形变动画" :options="[{label:'轻快回弹',value:'bouncy'},{label:'平稳紧凑',value:'stiff'}]" :disabled="busy" @update:value="change('spring_style',$event)" /></div><div class="island-range-grid"><label v-for="item in dimensions" :key="item.key" class="island-range"><span>{{ item.label }}<b>{{ settings[item.key] }}{{ item.unit }}</b></span><n-slider :value="Number(settings[item.key])" :min="item.min" :max="item.max" :step="item.step" :disabled="busy" :aria-label="item.label" @update:value="change(item.key,$event)" /></label></div></section>
    </div>
    </Transition>
    <p class="island-credit">基于 NetSpeed Dynamic 2.4.6 · 内置功能，随 MollyCloud 更新</p>
  </div>
</template>

<style scoped>
.netspeed-panel { display:grid; gap:20px; min-width:0; }
.island-toolbar,.island-card-heading,.island-setting,.island-enable,.island-legend { display:flex; align-items:center; justify-content:space-between; gap:16px; }
.island-toolbar { min-width:0; }
.island-toolbar nav { flex:1; min-width:0; }
.island-setting p,.island-note,.island-credit { color:var(--color-text-soft); font-size:12px; line-height:1.6; margin:0; }
.island-enable { flex-shrink:0; font-size:13px; }
.island-section { display:grid; gap:16px; min-width:0; }
.island-card { padding:20px; border:1px solid var(--color-border); border-radius:var(--radius-card,20px); background:var(--color-surface); min-width:0; }
.island-card h3 { font-size:15px; font-weight:650; margin:0; }
.island-card-heading { margin-bottom:20px; font-size:12px; color:var(--color-text-soft); }
.island-metrics { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:12px; }
.island-metric { padding:16px; }
.island-metric span { display:flex; gap:8px; align-items:center; color:var(--color-text-soft); font-size:12px; }
.island-metric svg { width:16px; height:16px; }
.island-metric strong { display:block; margin-top:12px; font-size:22px; font-weight:650; font-variant-numeric:tabular-nums; white-space:nowrap; }
.island-legend { gap:8px; }
.island-legend i { display:block; width:8px; height:8px; border-radius:50%; }
.island-legend .down { background:var(--color-lime-deep); }.island-legend .up { background:var(--color-blue); }
.island-chart > span,.island-chart-axis { color:var(--color-text-muted); font-size:11px; }
.island-chart svg { display:block; width:100%; height:120px; margin-top:6px; overflow:visible; }
.island-chart-axis { display:flex; justify-content:space-between; margin-top:8px; }
.chart-grid { stroke:var(--color-border); stroke-width:.7; stroke-dasharray:4 5; }
.chart-down,.chart-up { fill:none; stroke-width:2.5; vector-effect:non-scaling-stroke; stroke-linecap:round; stroke-linejoin:round; }
.chart-down { stroke:var(--color-lime-deep); }.chart-up { stroke:var(--color-blue); }
.island-period { width:132px; }.island-totals { display:grid; grid-template-columns:repeat(3,1fr); gap:16px; }
.island-totals span { display:block; color:var(--color-text-soft); font-size:12px; }.island-totals strong { font-size:20px; font-variant-numeric:tabular-nums; }
.island-bars { display:flex; align-items:end; gap:12px; min-height:130px; margin:12px 0 16px; }.island-bar { display:flex; flex:1; align-items:center; flex-direction:column; gap:8px; min-width:0; }.island-bar i { display:block; width:min(32px,100%); background:var(--color-lime); border-radius:4px; }.island-bar small { color:var(--color-text-muted); font-size:11px; }
.island-setting { padding:16px 0; border-bottom:1px solid var(--color-border); }.island-setting:last-child { border:0; padding-bottom:0; }.island-setting strong { font-size:13px; font-weight:600; }.island-setting p { margin-top:4px; }.island-setting :deep(.n-switch),.island-setting :deep(.n-button) { flex-shrink:0; }
.island-select { width:220px; flex-shrink:0; }.island-mode-grid { display:flex; flex-wrap:wrap; gap:8px; }.island-mode-grid button { font:inherit; font-size:13px; padding:10px 16px; border:1px solid var(--color-border); border-radius:12px; background:var(--color-surface); color:var(--color-text); cursor:pointer; transition:background .18s; }.island-mode-grid button:hover { background:var(--color-surface-soft); }.island-mode-grid button.selected { background:var(--color-lime-soft); border-color:var(--color-lime-border); color:var(--color-lime-deep); }.island-mode-grid + .island-note { margin-top:12px; }
.island-slots { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; margin-top:16px; }.island-slots label { display:grid; gap:8px; font-size:12px; color:var(--color-text-soft); }
.island-range-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); column-gap:40px; row-gap:24px; padding-top:24px; }.island-range { display:grid; gap:10px; font-size:13px; }.island-range > span { display:flex; justify-content:space-between; }.island-range b { font-weight:500; color:var(--color-text-soft); font-variant-numeric:tabular-nums; }
.island-credit { font-size:11px; color:var(--color-text-muted); padding-bottom:12px; }
@media(max-width:1000px) { .island-metric strong { font-size:19px; }.island-metric { padding:14px; }.island-metrics { gap:8px; }.island-enable { gap:8px; } }
@media(prefers-reduced-motion:reduce) { * { transition:none!important; } }
</style>
