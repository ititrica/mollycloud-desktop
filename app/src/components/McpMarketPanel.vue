<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { NAlert, NButton, NCheckbox, NInput, NModal, NSelect } from "naive-ui";
import { openUrl } from "@tauri-apps/plugin-opener";
import AppIcon from "./AppIcon.vue";
import { directory, filterCatalog, marketApi, parseCustom, readRegistryCache, templates, type CatalogEntry, type Installation, type MarketStatus, type SourcePlan, type LocalPackage } from "../mcp/client";

const emit = defineEmits<{ modal: [open: boolean] }>();
const props = defineProps<{ active: boolean }>();
let focusReturn: HTMLElement | null = null;
const tab = ref<"discover" | "installed">("discover");
const source = ref("curated");
const query = ref("");
const category = ref("");
const limit = ref(24);
const status = ref<MarketStatus | null>(null);
const loading = ref(false);
const searching = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const online = ref<CatalogEntry[]>([]);
const onlineHint = ref("");
const selected = ref<CatalogEntry | null>(null);
const customOpen = ref(false);
const customId = ref("");
const defaultCustom = '{\n  "type": "http",\n  "url": "https://example.com/mcp"\n}';
const customText = ref(defaultCustom);
const targets = ref<string[]>([]);
const values = ref<Record<string, string>>({});
const modalError = ref("");
const origin = ref<CatalogEntry | null>(null);
const plans = ref<SourcePlan[]>([]);
const planId = ref("");
const sourceNotes = ref<string[]>([]);
const currentPlan = computed(() => plans.value.find(p => p.planId === planId.value));
const cleanupTarget = ref<LocalPackage | null>(null);
const cleanPackage = ref(true);
const removeTarget = ref<Installation | null>(null);
const modal = computed(() => Boolean(selected.value || origin.value || customOpen.value || removeTarget.value || cleanupTarget.value));
const entries = computed(() => source.value === "curated" ? templates : source.value === "awesome" ? directory : online.value);
const categories = computed(() => [{ label: "全部分类", value: "" }, ...[...new Set(entries.value.map(e => e.category))].sort().map(value => ({ label: value, value }))]);
const filtered = computed(() => source.value === "registry" ? entries.value : filterCatalog(entries.value, query.value, category.value));
const installed = computed(() => (status.value?.installations ?? []).filter(row => `${row.name} ${row.agent}`.toLowerCase().includes(query.value.toLowerCase())));
const sourceOptions = [{ label: "安装模板", value: "curated" }, { label: "社区目录", value: "awesome" }, { label: "官方 Registry", value: "registry" }];
const isDesktop = "__TAURI_INTERNALS__" in window;
const failure = (e: unknown) => e instanceof Error ? e.message : String(e);
const agentName = (id: string) => status.value?.agents.find(a => a.id === id)?.name ?? id;
const version = (entry: CatalogEntry) => entry.recipe?.kind === "remote" ? "远程连接" : entry.recipe ? `v${entry.recipe.version}` : entry.source === "registry" ? "Registry 收录" : "社区收录";
const installedFor = (entry: CatalogEntry) => (status.value?.installations ?? []).filter(r => r.id === entry.id && r.managed).map(r => agentName(r.agent));
watch(modal, async value => {
  if (value) focusReturn = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  emit("modal", value);
  if (!value) { await nextTick(); focusReturn?.focus(); }
});
watch(() => props.active, active => { if (active) void refresh(); });
onBeforeUnmount(() => emit("modal", false));
watch([query, category, source], () => { limit.value = 24; });
watch(source, () => { category.value = ""; });
async function refresh() {
  if (busy.value) return;
  loading.value = true; error.value = "";
  try { status.value = await marketApi.status(); } catch (e) { error.value = failure(e); }
  finally { loading.value = false; }
}
async function openLink(url: string) {
  try {
    if (!url.startsWith("https://")) throw new Error("项目链接必须使用 HTTPS。");
    if (isDesktop) await openUrl(url); else window.open(url, "_blank", "noopener,noreferrer");
  } catch (e) { error.value = failure(e); }
}
async function searchOnline() {
  searching.value = true; error.value = "";
  try {
    online.value = await marketApi.search(query.value);
    onlineHint.value = `找到 ${online.value.length} 个结果；点击“从来源安装”读取安装方式。`;
    try { localStorage.setItem("mollycloud:mcp-registry-cache", JSON.stringify({ query: query.value, entries: online.value })); } catch { /* Search remains available without storage. */ }
  } catch (e) { error.value = failure(e); }
  finally { searching.value = false; }
}
function startInstall(entry?: CatalogEntry) {
  selected.value = entry ?? null; customOpen.value = !entry; values.value = Object.fromEntries((entry?.fields ?? []).map(f => [f.key, f.default ?? ""])); modalError.value = "";
  targets.value = status.value?.agents.filter(a => a.detected && !a.error && !status.value?.installations.some(row => row.id === entry?.id && row.agent === a.id)).slice(0, 1).map(a => a.id) ?? [];
}
function choosePlan(id: string) {
  planId.value = id;
  const plan = plans.value.find(p => p.planId === id);
  if (plan) startInstall(plan.entry);
}
async function resolveSource(entry: CatalogEntry) {
  if (busy.value) return;
  origin.value = entry; selected.value = null; plans.value = []; planId.value = ""; sourceNotes.value = []; modalError.value = ""; busy.value = true;
  try {
    const result = await marketApi.resolve(entry.source, entry.id);
    plans.value = result.plans; sourceNotes.value = result.notes;
    if (plans.value[0]) choosePlan(plans.value[0].planId);
  } catch (e) { modalError.value = failure(e); }
  finally { busy.value = false; }
}
async function cleanup() {
  const target = cleanupTarget.value;
  if (!target || busy.value) return;
  busy.value = true; modalError.value = "";
  try { status.value = await marketApi.cleanup(target.id, target.version); notice.value = status.value.message ?? "软件包已清理。"; cleanupTarget.value = null; }
  catch (e) { modalError.value = failure(e); }
  finally { busy.value = false; }
}
function closeModal() { if (!busy.value) { selected.value = null; customOpen.value = false; removeTarget.value = null; cleanupTarget.value = null; origin.value = null; plans.value = []; planId.value = ""; sourceNotes.value = []; values.value = {}; customText.value = defaultCustom; modalError.value = ""; } }
function setTarget(id: string, checked: boolean) { targets.value = checked ? [...targets.value, id] : targets.value.filter(value => value !== id); }
async function install() {
  if (busy.value) return;
  modalError.value = "";
  if (!targets.value.length) { modalError.value = "请选择至少一个目标 Agent。"; return; }
  const id = selected.value?.id ?? customId.value.trim();
  if (!/^[a-zA-Z0-9_-]{1,80}$/.test(id)) { modalError.value = "MCP 名称请使用字母、数字、连字符或下划线。"; return; }
  const missing = selected.value?.fields.find(field => field.required && !values.value[field.key]?.trim());
  if (missing) { modalError.value = `请填写${missing.label}。`; return; }
  busy.value = true;
  try {
    const custom = customOpen.value ? parseCustom(customText.value) : undefined;
    status.value = await marketApi.install({ id, agents: targets.value, values: { ...values.value }, ...(custom ? { custom } : {}), ...(planId.value ? { planId: planId.value } : {}) });
    selected.value = null; origin.value = null; plans.value = []; planId.value = ""; sourceNotes.value = []; customOpen.value = false; values.value = {}; customText.value = defaultCustom; customId.value = ""; query.value = ""; tab.value = "installed";
    notice.value = "Agent 配置已写入。请查看各项连接结果，并在目标 Agent 中重新加载 MCP。";
  } catch (e) { modalError.value = failure(e); }
  finally { busy.value = false; }
}
async function action(row: Installation, name: string) {
  if (busy.value) return;
  busy.value = true; error.value = ""; modalError.value = ""; notice.value = "";
  try {
    status.value = await marketApi.action(row.key, name);
    notice.value = status.value.message ?? (name === "remove" ? "已移除此 Agent 的 MCP 配置，软件包和服务数据保留。" : name === "test" ? "连接检查已完成，结果显示在服务条目中。" : "配置已更新，请在目标 Agent 中重新加载 MCP。");
    removeTarget.value = null;
  } catch (e) { if (removeTarget.value) modalError.value = failure(e); else error.value = failure(e); }
  finally { busy.value = false; }
}
onMounted(() => {
  void refresh();
  try {
    const cache = JSON.parse(localStorage.getItem("mollycloud:mcp-registry-cache") ?? "null");
    if (cache && Array.isArray(cache.entries)) {
      online.value = readRegistryCache(cache.entries);
      onlineHint.value = `上次搜索“${String(cache.query).slice(0, 160)}”的缓存，点击搜索更新。`;
    }
  } catch { /* A damaged discovery cache must not prevent installation management. */ }
});
</script>

<template>
  <section class="mcp-market" aria-label="MCP 市场">
    <div class="mcp-toolbar">
      <div class="mcp-tabs" role="group" aria-label="市场视图">
        <button type="button" :aria-pressed="tab === 'discover'" @click="tab = 'discover'">发现</button>
        <button type="button" :aria-pressed="tab === 'installed'" @click="tab = 'installed'">已安装 <span>{{ status?.installations.length ?? 0 }}</span></button>
      </div>
      <div class="mcp-actions"><n-button :disabled="busy || !status" @click="startInstall()">手动添加</n-button><n-button :loading="loading" :disabled="busy" aria-label="刷新 MCP 状态" @click="refresh"><AppIcon name="refresh" /></n-button></div>
    </div>
    <p class="mcp-intro">为你的 Agent 添加工具。安装后由 Agent 直接运行或连接，退出 Molly 后仍可使用。</p>
    <n-alert v-if="error" type="error" :show-icon="false">{{ error }}</n-alert>
    <n-alert v-if="notice" type="success" :show-icon="false" closable @close="notice = ''">{{ notice }}</n-alert>
    <n-alert v-if="!isDesktop" type="info" :show-icon="false">目录预览。安装、连接测试和读取实际配置需在桌面端完成。</n-alert>
    <div class="mcp-filters">
      <n-input v-model:value="query" clearable :input-props="{ 'aria-label': '搜索 MCP' }" placeholder="搜索名称、功能或软件包…" @keydown.enter="source === 'registry' && searchOnline()"><template #prefix><AppIcon name="spark" /></template></n-input>
      <template v-if="tab === 'discover'"><n-select v-model:value="source" :options="sourceOptions" aria-label="目录来源" /><n-select v-if="source !== 'registry'" v-model:value="category" :options="categories" aria-label="MCP 分类" /><n-button v-else type="primary" :loading="searching" @click="searchOnline">搜索官方目录</n-button></template>
    </div>
    <template v-if="tab === 'discover'">
      <div class="mcp-result-meta"><span>{{ source === 'registry' ? onlineHint || '输入关键词，搜索官方 MCP Registry。' : `${filtered.length} 个项目` }}</span><span v-if="source === 'curated'">Molly 安装模板 · 固定版本</span><button v-else class="mcp-text-link" type="button" @click="openLink(source === 'awesome' ? 'https://github.com/punkpeye/awesome-mcp-servers/blob/main/README-zh.md' : 'https://registry.modelcontextprotocol.io')">目录来源 ↗</button></div>
      <div class="mcp-grid">
        <article v-for="entry in filtered.slice(0, limit)" :key="entry.id" class="mcp-card" :data-mcp-id="entry.id">
          <div class="mcp-card-heading"><span class="mcp-card-icon"><AppIcon :name="entry.recipe ? 'mcp' : 'document'" /></span><div><h2>{{ entry.name }}</h2><span>{{ entry.category }}</span></div></div>
          <p>{{ entry.description }}</p>
          <div class="mcp-card-meta"><span>{{ version(entry) }}</span><span v-if="entry.recipe?.kind === 'npm'">Node.js</span><span v-if="entry.recipe?.kind === 'pypi'">Python · uv</span><span v-if="entry.recipe?.oauth">Agent 内授权</span></div>
          <small v-if="installedFor(entry).length" class="mcp-installed-label">已配置：{{ installedFor(entry).join('、') }}</small>
          <div class="mcp-card-actions"><n-button quaternary size="small" @click="openLink(entry.homepage)">查看来源 ↗</n-button><n-button v-if="entry.recipe" type="primary" size="small" :disabled="busy || !status" @click="startInstall(entry)">安装到 Agent</n-button><n-button v-else size="small" :disabled="busy || !status" @click="resolveSource(entry)">从来源安装</n-button></div>
        </article>
      </div>
      <div v-if="!filtered.length" class="mcp-empty"><AppIcon name="mcp" /><h2>暂时没有匹配的项目</h2><p>试试其他关键词，或从来源文档手动添加连接。</p></div>
      <n-button v-if="filtered.length > limit" class="mcp-more" @click="limit += 24">加载更多</n-button>
    </template>
    <template v-else>
      <p class="mcp-result-meta">配置直接保存在目标 Agent。显示“外部修改”时，请先检查现有配置再操作。</p>
      <div v-if="!installed.length" class="mcp-empty"><AppIcon name="mcp" /><h2>还没有找到 MCP</h2><p>从市场选择工具，或手动添加已有的连接配置。</p><n-button type="primary" @click="tab = 'discover'; query = ''">浏览市场</n-button></div>
      <article v-for="row in installed" :key="row.key" class="mcp-installed-card">
        <div class="mcp-installed-heading"><div><h2>{{ row.name }}</h2><p>{{ agentName(row.agent) }} <span v-if="row.version">· {{ row.version === 'remote' ? '远程连接' : row.version === 'custom' ? '手动配置' : `v${row.version}` }}</span></p></div><span class="mcp-state" :class="{ 'mcp-state--warning': row.changed }">{{ row.changed ? '外部修改' : !row.managed ? '外部配置' : row.enabled ? '已配置' : '已禁用' }}</span></div>
        <p class="mcp-check-result">{{ row.check }}</p><code class="mcp-config-path">{{ row.path }}</code>
        <div v-if="row.managed" class="mcp-card-actions"><n-button size="small" :disabled="busy || row.changed" @click="action(row, 'test')">测试连接</n-button><n-button v-if="row.canUpdate" size="small" :disabled="busy || row.changed" @click="action(row, 'update')">检查并更新</n-button><n-button size="small" :disabled="busy || row.changed" @click="action(row, row.enabled ? 'disable' : 'enable')">{{ row.enabled ? '禁用' : '启用' }}</n-button><n-button size="small" quaternary type="error" :disabled="busy || row.changed" @click="removeTarget = row; cleanPackage = true; modalError = ''">卸载</n-button></div>
      </article>
      <template v-if="status?.packages.length">
        <h2 class="mcp-section-title">保留的软件包</h2>
        <p class="mcp-storage-note">更新、移除配置或清理失败后留下的版本。清理保留服务数据，不影响 Node.js / uv 环境。</p>
        <article v-for="pkg in status.packages" :key="`${pkg.id}:${pkg.version}`" class="mcp-installed-card">
          <div class="mcp-installed-heading"><h2>{{ pkg.id }} · {{ pkg.version }}</h2><n-button size="small" :disabled="busy || Boolean(pkg.reason)" @click="cleanupTarget = pkg; modalError = ''">清理软件包</n-button></div>
          <code class="mcp-config-path">{{ pkg.path }}</code><p v-if="pkg.reason" class="mcp-check-result">{{ pkg.reason }}</p>
        </article>
      </template>
      <p v-if="status" class="mcp-storage-note">本地软件包：<code>{{ status.packageRoot }}</code></p>
    </template>
    <n-alert v-if="busy && !modal" type="info" :show-icon="false">正在处理 MCP，请稍候。下载较大依赖时可能需要几分钟。</n-alert>

    <n-modal :show="modal" to="#console-settings-layer" :mask-closable="!busy" :close-on-esc="!busy" transform-origin="center" @update:show="closeModal">
      <div class="console-settings-dialog mcp-install-dialog" role="dialog" aria-modal="true" aria-labelledby="mcp-dialog-title">
        <header class="console-settings-header"><span class="console-settings-icon"><AppIcon name="mcp" /></span><div><h2 id="mcp-dialog-title">{{ cleanupTarget ? '清理软件包' : removeTarget ? '卸载 MCP' : origin ? `从来源安装 ${origin.name}` : selected ? `安装 ${selected.name}` : '手动添加 MCP' }}</h2><p>{{ cleanupTarget ? '清理未使用的版本，保留服务数据。' : removeTarget ? '管理所选 Agent 的配置与本地软件包。' : '选择需要使用这个工具的 Agent。' }}</p></div></header>
        <div class="console-settings-body mcp-install-body">
          <template v-if="cleanupTarget"><p>将把此版本的软件包移入回收站：</p><code class="mcp-config-path">{{ cleanupTarget.path }}</code><p class="console-settings-hint">清理前会重新检查四种 Agent 的用户级配置。若项目级配置也使用此路径，请先停用相关连接。</p></template>
          <template v-else-if="removeTarget"><p>将从 {{ agentName(removeTarget.agent) }} 卸载 {{ removeTarget.name }}。</p><n-checkbox v-if="removeTarget.canUpdate" v-model:checked="cleanPackage" :disabled="busy">同时清理未被其他 Agent 使用的软件包</n-checkbox><p class="console-settings-hint">其他 Agent 仍在使用时会保留软件包。清理的文件移入回收站，服务数据始终保留。检查范围为四种 Agent 的用户级配置；手动安装的程序和远程服务不会删除。</p></template>
          <template v-else>
            <template v-if="origin">
              <p v-if="busy && !plans.length" role="status">正在读取来源安装信息…</p>
              <label v-if="plans.length" class="mcp-field"><span>安装方式</span><n-select :value="planId" :options="plans.map(p => ({ label: p.label, value: p.planId }))" :disabled="busy" aria-label="来源安装方式" @update:value="choosePlan" /></label>
              <p v-for="note in sourceNotes" :key="note" class="console-settings-hint">{{ note }}</p>
              <div class="mcp-actions"><button class="mcp-text-link" type="button" @click="openLink(currentPlan?.sourceUrl ?? origin.homepage)">查看安装来源 ↗</button><n-button v-if="!plans.length && !busy" size="small" @click="origin = null; startInstall()">手动添加</n-button></div>
              <details v-if="currentPlan" class="mcp-plan-preview"><summary>查看将安装的软件包与连接参数</summary><pre>{{ JSON.stringify(currentPlan.entry.recipe, null, 2) }}</pre></details>
            </template>
            <template v-if="selected"><p class="mcp-dialog-description">{{ selected.description }}</p><p class="mcp-dialog-version">{{ version(selected) }} · 用户级配置</p></template>
            <template v-else-if="customOpen"><label class="mcp-field"><span>MCP 名称</span><n-input v-model:value="customId" placeholder="例如 my-mcp" :disabled="busy" :input-props="{ 'aria-label': 'MCP 名称' }" /></label><label class="mcp-field"><span>连接配置 JSON</span><n-input v-model:value="customText" type="textarea" :autosize="{ minRows: 5, maxRows: 10 }" :disabled="busy" :input-props="{ 'aria-label': 'MCP 连接配置' }" /></label><p class="console-settings-hint">粘贴单个服务的 command / args / env，或 type / url / headers。密钥按目标 Agent 的配置格式保存。</p></template>
            <fieldset v-if="selected || customOpen" class="mcp-targets"><legend>安装到</legend><label v-for="agent in status?.agents ?? []" :key="agent.id" class="mcp-target"><n-checkbox :checked="targets.includes(agent.id)" :disabled="busy || Boolean(agent.error) || Boolean(selected && status?.installations.some(row => row.id === selected?.id && row.agent === agent.id))" @update:checked="value => setTarget(agent.id, value)"><strong>{{ agent.name }}</strong></n-checkbox><small>{{ agent.error || (agent.detected ? '已检测到配置' : '将创建用户级配置') }}</small><code>{{ agent.path }}</code></label></fieldset>
            <label v-for="field in selected?.fields ?? []" :key="field.key" class="mcp-field"><span>{{ field.label }}{{ field.required ? '' : '（可选）' }}</span><n-select v-if="field.choices?.length && field.kind !== 'secret'" v-model:value="values[field.key]" :options="field.choices.map(value => ({ label: value, value }))" :disabled="busy" :aria-label="field.label" /><n-input v-else v-model:value="values[field.key]" :type="field.kind === 'secret' ? 'password' : 'text'" show-password-on="click" :disabled="busy" :placeholder="field.placeholder || (field.kind === 'directory' ? '例如 C:\\Users\\你的用户名\\Documents' : field.label)" :input-props="{ 'aria-label': field.label, autocomplete: 'off' }" /></label>
            <n-alert v-if="selected?.recipe?.kind === 'npm' && !status?.node && isDesktop" type="warning" :show-icon="false">需要 Node.js 22.13 或更高版本。安装后重新打开 Molly。<button type="button" class="mcp-text-link" @click="openLink('https://nodejs.org/en/download')">下载 Node.js ↗</button></n-alert>
            <n-alert v-if="selected?.recipe?.kind === 'pypi' && !status?.uv && isDesktop" type="warning" :show-icon="false">需要 uv，Python 环境会按需准备。<button type="button" class="mcp-text-link" @click="openLink('https://docs.astral.sh/uv/getting-started/installation/')">安装 uv ↗</button></n-alert>
            <p v-if="selected?.recipe?.oauth" class="console-settings-hint">配置后在所选 Agent 的 MCP 设置中完成 OAuth 登录。</p>
            <p class="console-settings-hint">安装时会保留配置备份。MCP 由目标 Agent 直接使用，密钥不会发送到 MollyCloud 服务器。</p>
          </template>
          <n-alert v-if="modalError" class="mcp-modal-error" type="error" :show-icon="false">{{ modalError }}</n-alert>
          <p v-if="busy" role="status" aria-live="polite">正在处理，请稍候。首次下载可能需要几分钟。</p>
        </div>
        <footer class="console-settings-actions"><n-button :disabled="busy" @click="closeModal">取消</n-button><n-button :type="removeTarget || cleanupTarget ? 'error' : 'primary'" :loading="busy" :disabled="Boolean(origin && !selected)" @click="cleanupTarget ? cleanup() : removeTarget ? action(removeTarget, cleanPackage ? 'uninstall' : 'remove') : install()">{{ cleanupTarget ? '确认清理' : removeTarget ? cleanPackage ? '确认卸载' : '仅移除配置' : '安装到所选 Agent' }}</n-button></footer>
      </div>
    </n-modal>
  </section>
</template>

<style scoped>
.mcp-section-title { margin:8px 0 0; font-size:18px; }
.mcp-plan-preview { font-size:13px; color:var(--color-text-soft); }.mcp-plan-preview pre { white-space:pre-wrap; overflow-wrap:anywhere; font:12px/1.6 var(--font-mono); max-height:180px; overflow:auto; }.mcp-plan-preview summary { cursor:pointer; }
.mcp-market { display: flex; flex-direction: column; gap: 18px; min-width: 0; }
.mcp-toolbar,.mcp-actions,.mcp-card-actions,.mcp-result-meta,.mcp-installed-heading { display:flex; align-items:center; justify-content:space-between; gap:12px; }
.mcp-actions { justify-content:flex-end; }
.mcp-tabs { display:flex; gap:6px; padding:4px; border:1px solid var(--color-border); border-radius:var(--radius-md); background:var(--color-surface); }
.mcp-tabs button { border:0; border-radius:10px; background:transparent; color:var(--color-text-soft); padding:10px 18px; font:inherit; font-size:14px; cursor:pointer; }
.mcp-tabs button[aria-pressed="true"] { background:var(--color-lime-soft); color:var(--color-lime-deep); font-weight:600; }
.mcp-tabs span { margin-left:6px; font-variant-numeric:tabular-nums; }
.mcp-intro,.mcp-result-meta,.mcp-storage-note { margin:0; font-size:13px; line-height:1.6; color:var(--color-text-soft); }
.mcp-filters { display:grid; grid-template-columns:minmax(150px,1fr) 160px 160px; gap:12px; }
.mcp-text-link { border:0; padding:0; background:transparent; font:inherit; color:var(--color-lime-deep); cursor:pointer; white-space:nowrap; }
.mcp-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:16px; }
.mcp-card,.mcp-installed-card { min-width:0; padding:20px; border:1px solid var(--color-border); border-radius:var(--radius-lg); background:var(--color-surface); box-shadow:var(--shadow-card); }
.mcp-card { display:flex; flex-direction:column; gap:14px; }
.mcp-card-heading { display:flex; align-items:center; gap:12px; min-width:0; }
.mcp-card-heading > div { min-width:0; }
.mcp-card h2,.mcp-installed-card h2 { font-size:17px; line-height:1.4; margin:0; overflow-wrap:anywhere; }
.mcp-card-heading span:not(.mcp-card-icon) { color:var(--color-text-muted); font-size:12px; }
.mcp-card-icon { width:40px; height:40px; flex:none; border-radius:var(--radius-sm); background:var(--color-lime-soft); color:var(--color-lime-deep); display:grid; place-items:center; }
.mcp-card > p { color:var(--color-text-soft); font-size:13px; line-height:1.7; margin:0; overflow-wrap:anywhere; display:-webkit-box; -webkit-line-clamp:3; -webkit-box-orient:vertical; overflow:hidden; }
.mcp-card-meta { display:flex; flex-wrap:wrap; gap:8px; margin-top:auto; color:var(--color-text-muted); font-size:12px; }
.mcp-card-meta span { border:1px solid var(--color-border); border-radius:999px; padding:3px 8px; }
.mcp-card-actions { flex-wrap:wrap; justify-content:flex-end; }
.mcp-card-actions > :first-child { margin-right:auto; }
.mcp-installed-label { color:var(--color-lime-deep); }
.mcp-more { align-self:center; }
.mcp-empty { text-align:center; padding:48px 16px; color:var(--color-text-soft); }
.mcp-empty > .app-icon { width:36px; height:36px; color:var(--color-lime-deep); }
.mcp-empty h2 { font-size:18px; color:var(--color-text); }.mcp-empty p { font-size:14px; margin-bottom:20px; }
.mcp-installed-heading { align-items:flex-start; }.mcp-installed-heading p { margin:6px 0; color:var(--color-text-soft); font-size:13px; }
.mcp-state { border-radius:999px; padding:5px 10px; color:var(--color-lime-deep); background:var(--color-lime-soft); white-space:nowrap; font-size:12px; }
.mcp-state--warning { color:var(--color-warning); background:var(--color-warning-soft); }
.mcp-check-result { font-size:13px; line-height:1.6; overflow-wrap:anywhere; }
.mcp-config-path { display:block; font:12px/1.6 var(--font-mono); overflow-wrap:anywhere; color:var(--color-text-muted); margin:10px 0 18px; }.mcp-storage-note code { overflow-wrap:anywhere; }
.mcp-install-dialog { color:var(--color-text); }
.mcp-install-body { display:flex; flex-direction:column; gap:18px; }
.mcp-dialog-description,.mcp-dialog-version { margin:0; font-size:13px; line-height:1.7; color:var(--color-text-soft); }
.mcp-field { display:flex; flex-direction:column; gap:8px; font-size:14px; font-weight:600; }
.mcp-targets { border:0; padding:0; margin:0; min-width:0; }.mcp-targets legend { font-size:14px; font-weight:600; margin-bottom:10px; }
.mcp-target { display:grid; grid-template-columns:1fr auto; gap:6px 8px; padding:12px; margin-bottom:8px; border:1px solid var(--color-border); border-radius:var(--radius-sm); background:var(--color-surface-soft); }
.mcp-target small { font-size:12px; color:var(--color-text-muted); }.mcp-target code { grid-column:1/-1; font:11px/1.6 var(--font-mono); color:var(--color-text-soft); overflow-wrap:anywhere; }
@media(min-width:1350px) { .mcp-grid { grid-template-columns:repeat(3,minmax(0,1fr)); } }
@media(max-width:1020px) { .mcp-filters { grid-template-columns:1fr 145px; }.mcp-filters > :first-child { grid-column:1/-1; } }
</style>
