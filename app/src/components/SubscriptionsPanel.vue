<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { NAlert, NButton, NCheckbox, NModal, NSpin, NSwitch } from 'naive-ui';
import AppIcon from './AppIcon.vue';
import { providerNames, providerTone } from '../keyGroups';
import { autoRenewNotice, canEnableAutoRenew, subscriptionApi, subscriptionExpiry, subscriptionQuotas, subscriptionStatuses, type Subscription } from '../subscriptions';

const props = defineProps<{ preview: boolean; obscured: boolean; serverUtcOffset?: string }>();
const emit = defineEmits<{ changed: []; modal: [boolean] }>();
const items = ref<Subscription[]>([]);
const loading = ref(true), saving = ref(false);
const error = ref(''), notice = ref(''), dialogError = ref('');
const target = ref<Subscription | null>(null);
const action = ref<'reset' | 'auto-renew'>('reset');
const dontRemind = ref(false), uncertainReset = ref(false);
const modalOpen = computed(() => target.value !== null);
let version = 0, disposed = false, loadingRequest = false;
let timer: number | undefined, trigger: HTMLElement | null = null;
watch(modalOpen, value => emit('modal', value), { flush: 'sync' });
const name = (item: Subscription) => item.group?.name || `分组 #${item.group_id}`;
const errorText = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);
function reminderKey(item: Subscription) { return `mollycloud:${props.preview ? 'preview:' : ''}subscription-auto-renew-no-reminder:${item.user_id}`; }
function replace(oldId: number, updated: Subscription) {
  items.value = items.value.map(item => item.id === oldId ? updated : item);
}
function focusTrigger() {
  const previous = trigger;
  trigger = null;
  // The modal's focus trap restores its own target when it unmounts.
  void nextTick(() => {
    if (disposed || modalOpen.value) return;
    if (previous?.isConnected) previous.focus();
    else document.querySelector<HTMLButtonElement>('.subscription-reset')?.focus();
  });
}
function close() {
  if (!saving.value) target.value = null;
}
async function load(silent = false): Promise<boolean> {
  if (loadingRequest) return false;
  const current = ++version;
  loadingRequest = true;
  if (!silent) loading.value = true;
  try {
    const updated = await subscriptionApi.list();
    if (disposed || current !== version) return false;
    const termChanged = silent && updated.map(item => item.id).join(',') !== items.value.map(item => item.id).join(',');
    items.value = updated;
    error.value = '';
    if (termChanged && !props.preview) emit('changed');
    return true;
  } catch (reason) {
    if (!disposed && current === version && !silent) error.value = errorText(reason);
    return false;
  } finally {
    loadingRequest = false;
    if (!disposed) loading.value = false;
  }
}
function refreshVisible() {
  if (document.visibilityState === 'hidden' || props.obscured || disposed || saving.value || modalOpen.value || loadingRequest) return;
  void load(true);
}
async function openReset(item: Subscription, event: Event) {
  if (saving.value || modalOpen.value || !canEnableAutoRenew(item)) return;
  version++;
  trigger = event.currentTarget as HTMLElement;
  error.value = ''; notice.value = ''; dialogError.value = ''; uncertainReset.value = false;
  let updated = item;
  if (item.auto_renew_error) {
    saving.value = true;
    try {
      updated = await subscriptionApi.autoRenew(item.id, { clear_error: true });
      if (disposed) return;
      replace(item.id, updated);
    } catch (reason) {
      if (!disposed) error.value = errorText(reason);
      return;
    } finally { saving.value = false; }
  }
  if (disposed) return;
  action.value = 'reset'; target.value = updated;
}
function toggleAutoRenew(item: Subscription, enabled: boolean, event?: Event) {
  if (saving.value || modalOpen.value || enabled && !canEnableAutoRenew(item)) return;
  version++;
  trigger = event?.currentTarget as HTMLElement ?? document.activeElement as HTMLElement;
  error.value = ''; notice.value = ''; dialogError.value = '';
  let skipReminder = false;
  try { skipReminder = item.user_id != null && localStorage.getItem(reminderKey(item)) === 'true'; } catch { /* Optional preference. */ }
  if (enabled && !skipReminder) {
    action.value = 'auto-renew'; dontRemind.value = false; target.value = item;
  } else { void saveAutoRenew(item, enabled); }
}
async function saveAutoRenew(item: Subscription, enabled: boolean) {
  if (saving.value) return;
  if (enabled && !canEnableAutoRenew(item)) { dialogError.value = '仅可为有效且未过期的订阅开启自动续杯'; return; }
  saving.value = true;
  try {
    const updated = await subscriptionApi.autoRenew(item.id, { enabled });
    if (disposed) return;
    replace(item.id, updated);
    if (enabled && target.value && dontRemind.value && item.user_id != null) {
      try { localStorage.setItem(reminderKey(item), 'true'); } catch { /* Optional preference. */ }
    }
    target.value = null;
    notice.value = enabled ? '自动续杯已开启' : '自动续杯已关闭';
    if (!props.preview) emit('changed');
  } catch (reason) {
    if (!disposed) {
      if (target.value) dialogError.value = errorText(reason);
      else error.value = errorText(reason);
    }
  } finally { saving.value = false; }
}
async function confirm() {
  const item = target.value;
  if (!item || saving.value || uncertainReset.value) return;
  if (action.value === 'auto-renew') { await saveAutoRenew(item, true); return; }
  if (!canEnableAutoRenew(item)) { dialogError.value = '仅可重置有效且未过期的订阅'; return; }
  saving.value = true; dialogError.value = '';
  try {
    const updated = await subscriptionApi.reset(item.id);
    if (disposed) return;
    replace(item.id, updated); target.value = null;
    notice.value = props.preview ? '演示订阅已重置，未扣除真实余额' : '订阅已重置';
    if (!props.preview) emit('changed');
    await load(true);
  } catch (reason) {
    if (!disposed) {
      dialogError.value = errorText(reason);
      uncertainReset.value = /超时|连接|网络|无法识别|未确认/.test(dialogError.value);
    }
  } finally { saving.value = false; }
}
async function reconcile() {
  if (saving.value) return;
  saving.value = true;
  const oldId = target.value?.id;
  try {
    if (!await load()) { dialogError.value = error.value || '刷新失败，请稍后核对订阅'; return; }
    if (!props.preview) emit('changed');
    const current = items.value.find(item => item.id === oldId);
    if (!current) { target.value = null; notice.value = '订阅状态已变化，请核对新的额度、到期时间和余额'; }
    else { target.value = current; uncertainReset.value = false; dialogError.value = '已刷新订阅状态，请确认是否仍需重置。'; }
  } finally { saving.value = false; }
}
function peakRate(item: Subscription) {
  const group = item.group;
  return group?.peak_rate_enabled && group.peak_start && group.peak_end ? `${group.peak_start}-${group.peak_end} ×${group.peak_rate_multiplier ?? 1}${props.serverUtcOffset ? ` (UTC${props.serverUtcOffset})` : ''}` : '';
}
onMounted(() => { void load(); timer = window.setInterval(refreshVisible, 5000); document.addEventListener('visibilitychange', refreshVisible); });
onBeforeUnmount(() => { disposed = true; version++; window.clearInterval(timer); document.removeEventListener('visibilitychange', refreshVisible); emit('modal', false); });
</script>

<template>
  <section class="subscriptions-panel" aria-label="我的订阅" :aria-busy="loading">
    <p class="subscription-intro">在订阅期限内如额度用尽，可使用重置键对额度及到期时间进行重置，费用将从余额扣除</p>
    <n-alert v-if="error" type="error" :bordered="false" :show-icon="false" role="alert">{{ error }}<n-button text :disabled="loading || saving" @click="load()">刷新订阅</n-button></n-alert>
    <p v-if="notice" class="subscription-notice" role="status">{{ notice }}</p>
    <div v-if="loading && !items.length" class="payment-loading"><n-spin size="small" /> 正在获取订阅…</div>
    <div v-else-if="items.length" class="subscription-list">
      <article v-for="item in items" :key="item.id" class="subscription-card" :data-provider="providerTone(item.group?.platform ?? '')" :data-subscription-id="item.id" :aria-label="name(item)">
        <header class="subscription-card-header">
          <div class="subscription-identity"><i aria-hidden="true" /><div>
            <div class="subscription-name"><h2>{{ name(item) }}</h2><span class="subscription-platform">{{ providerNames[item.group?.platform ?? ''] || '其他' }}</span></div>
            <p v-if="item.group?.description" class="subscription-description">{{ item.group.description }}</p>
            <p class="subscription-rate">倍率: ×{{ item.group?.rate_multiplier ?? 1 }}<span v-if="peakRate(item)">高峰倍率: {{ peakRate(item) }}</span></p>
          </div></div>
          <div class="subscription-controls">
            <span class="subscription-status" :data-status="item.status">{{ subscriptionStatuses[item.status] || item.status }}</span>
            <button v-if="item.status === 'active'" class="subscription-reset" type="button" :disabled="saving || modalOpen || !canEnableAutoRenew(item)" :aria-label="`重置 ${name(item)}`" @click="openReset(item, $event)">重置</button>
            <div v-if="['active','expired'].includes(item.status)" class="subscription-auto-renew">
              <label><n-switch :value="item.auto_renew_enabled" size="small" :disabled="saving || modalOpen || !item.auto_renew_enabled && !canEnableAutoRenew(item)" :aria-label="`${name(item)} 自动续杯`" :title="!canEnableAutoRenew(item) ? '仅可为有效且未过期的订阅开启自动续杯' : undefined" @update:value="toggleAutoRenew(item, $event)" /><span>自动续杯</span></label>
              <p v-if="item.auto_renew_error === 'insufficient_balance'" class="subscription-renew-error" role="status">余额不足，续杯失败</p>
            </div>
          </div>
        </header>
        <div class="subscription-card-body">
          <div class="subscription-expiration"><span>到期时间</span><time :datetime="item.expires_at || undefined" :data-tone="subscriptionExpiry(item).tone">{{ subscriptionExpiry(item).label }}</time></div>
          <div v-for="quota in subscriptionQuotas(item)" :key="quota.period" class="subscription-quota">
            <div><strong>{{ quota.label }}</strong><span>${{ quota.used.toFixed(2) }} / ${{ quota.limit.toFixed(2) }}</span></div>
            <div class="subscription-progress" role="progressbar" :aria-label="`${name(item)} ${quota.label}用量`" :aria-valuemin="0" :aria-valuemax="100" :aria-valuenow="quota.percentage" :aria-valuetext="`$${quota.used.toFixed(2)} / $${quota.limit.toFixed(2)}`"><i :data-tone="quota.tone" :style="{ width: `${quota.percentage}%` }" /></div>
            <p v-if="quota.reset">{{ quota.reset }}</p>
          </div>
          <div v-if="!subscriptionQuotas(item).length" class="subscription-unlimited"><strong>∞</strong><span>无限制<small>该订阅无用量限制</small></span></div>
        </div>
      </article>
    </div>
    <div v-else-if="!error" class="empty-state"><AppIcon name="subscription" /><h2>暂无有效订阅</h2><p>余额模式仍可正常使用，购买后的订阅会显示在这里。</p></div>
    <n-modal :show="modalOpen" to="#console-settings-layer" :mask-closable="!saving" :close-on-esc="!saving" transform-origin="center" @update:show="close" @after-leave="focusTrigger">
      <div class="console-settings-dialog subscription-dialog" role="alertdialog" aria-modal="true" aria-labelledby="subscription-dialog-title" aria-describedby="subscription-dialog-description" :aria-busy="saving">
        <header class="console-settings-header"><span class="console-settings-icon"><AppIcon name="subscription" /></span><div><h2 id="subscription-dialog-title">{{ action === 'reset' ? '重置订阅' : '自动续杯' }}</h2><p>{{ target ? name(target) : '' }}</p></div></header>
        <div class="console-settings-body subscription-dialog-body">
          <p id="subscription-dialog-description">{{ action === 'reset' ? '是否确认重置，重置后额度和剩余时长会立刻刷新，费用将从余额扣除' : autoRenewNotice }}</p>
          <n-checkbox v-if="action === 'auto-renew'" v-model:checked="dontRemind" :disabled="saving">不再提醒</n-checkbox>
          <p v-if="preview" class="console-settings-hint">界面预览：仅操作演示订阅，不扣除真实余额。</p>
          <n-alert v-if="dialogError" type="error" :bordered="false" :show-icon="false" role="alert">{{ dialogError }}<template v-if="uncertainReset"><p>请求可能已生效，请先刷新订阅与余额核对结果。</p><n-button text :disabled="saving" @click="reconcile">刷新并核对</n-button></template></n-alert>
        </div>
        <footer class="console-settings-actions"><n-button class="cancel-subscription" size="large" :disabled="saving" autofocus @click="close">取消</n-button><n-button class="confirm-subscription" :type="action === 'reset' ? 'error' : 'primary'" size="large" :loading="saving" :disabled="saving || uncertainReset" @click="confirm">确认</n-button></footer>
      </div>
    </n-modal>
  </section>
</template>
