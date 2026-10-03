import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

const amount = z.number().finite().nonnegative();
const date = z.string().nullish();
const groupSchema = z.object({
  name: z.string(), platform: z.string().default(''), description: z.string().nullish(),
  rate_multiplier: amount.nullish(), daily_limit_usd: amount.nullish(), weekly_limit_usd: amount.nullish(), monthly_limit_usd: amount.nullish(),
  peak_rate_enabled: z.boolean().optional(), peak_start: z.string().optional(), peak_end: z.string().optional(), peak_rate_multiplier: amount.optional(),
});
export const subscriptionSchema = z.object({
  id: z.number().int().positive().max(Number.MAX_SAFE_INTEGER), user_id: z.number().int().positive().optional(),
  group_id: z.number().int().positive(), status: z.string(), starts_at: date, expires_at: date,
  auto_renew_enabled: z.boolean().default(false), auto_renew_error: z.string().default(''),
  daily_usage_usd: amount.default(0), weekly_usage_usd: amount.default(0), monthly_usage_usd: amount.default(0),
  daily_window_start: date, weekly_window_start: date, monthly_window_start: date, group: groupSchema.nullish(),
});
export type Subscription = z.infer<typeof subscriptionSchema>;
export const subscriptionStatuses: Record<string, string> = { active: '有效', expired: '已过期', revoked: '已撤销', suspended: '已暂停' };
export const autoRenewNotice = '开启后当额度耗尽或者订阅时间结束时，将自动开启新的一轮订阅，费用将从余额扣除，请确保余额充足';
const DAY = 86_400_000;

export function canEnableAutoRenew(item: Subscription, now = Date.now()): boolean {
  return item.status === 'active' && (!item.expires_at || Date.parse(item.expires_at) > now);
}
export function subscriptionExpiry(item: Subscription, now = Date.now()): { label: string; tone: string } {
  if (!item.expires_at) return { label: '无到期时间', tone: '' };
  const timestamp = Date.parse(item.expires_at);
  if (!Number.isFinite(timestamp)) return { label: '到期时间无效', tone: 'danger' };
  const diff = timestamp - now;
  if (diff <= 0) return { label: '已过期', tone: 'danger' };
  const end = new Date(timestamp), current = new Date(now);
  const calendarDays = Math.round((Date.UTC(end.getFullYear(), end.getMonth(), end.getDate()) - Date.UTC(current.getFullYear(), current.getMonth(), current.getDate())) / DAY);
  const pad = (n: number) => String(n).padStart(2, '0');
  const formatted = `${end.getFullYear()}/${pad(end.getMonth() + 1)}/${pad(end.getDate())} ${pad(end.getHours())}:${pad(end.getMinutes())}`;
  const days = Math.ceil(diff / DAY);
  return { label: calendarDays === 0 ? `${formatted} (今天)` : calendarDays === 1 ? `${formatted} (明天)` : `剩余 ${days} 天 (${formatted})`, tone: days <= 3 ? 'danger' : days <= 7 ? 'warning' : '' };
}
function duration(target: number, now: number): string {
  if (!Number.isFinite(target) || target <= now) return '';
  const minutes = Math.floor((target - now) / 60_000), days = Math.floor(minutes / 1440), hours = Math.floor(minutes % 1440 / 60);
  return days > 0 ? `${days}d ${hours}h` : hours > 0 ? `${hours}h ${minutes % 60}m` : `${minutes}m`;
}
export function subscriptionQuotas(item: Subscription, now = Date.now()) {
  return (['daily', 'weekly', 'monthly'] as const).flatMap((period) => {
    const limit = item.group?.[`${period}_limit_usd`];
    if (!limit) return [];
    const used = item[`${period}_usage_usd`];
    const percentage = Math.max(0, Math.min(100, used / limit * 100));
    const start = item[`${period}_window_start`];
    const oneTime = period === 'daily' && item.starts_at && item.expires_at && Date.parse(item.expires_at) <= Date.parse(item.starts_at) + DAY;
    const remaining = duration(oneTime ? Date.parse(item.expires_at!) : Date.parse(start ?? '') + ({ daily: 1, weekly: 7, monthly: 30 }[period]) * DAY, now);
    return [{ period, label: { daily: '每日', weekly: '每周', monthly: '每月' }[period], used, limit, percentage,
      tone: percentage >= 90 ? 'danger' : percentage >= 70 ? 'warning' : 'normal',
      reset: start ? remaining ? oneTime ? `额度将在 ${remaining} 后结束` : `${remaining} 后重置` : '等待首次使用' : '' }];
  });
}
function subscriptionId(id: number): string {
  if (!Number.isSafeInteger(id) || id <= 0) throw new Error('订阅 ID 无效');
  return String(id);
}
function isPreview() {
  return import.meta.env.DEV && !('__TAURI_INTERNALS__' in window) && new URLSearchParams(window.location.search).get('ui-preview') === 'console';
}
function requireDesktop() {
  if (!('__TAURI_INTERNALS__' in window)) throw new Error('请在 MollyCloud 客户端中管理订阅。');
}
function subscriptionResult(value: unknown): Subscription {
  const parsed = subscriptionSchema.safeParse(value);
  if (!parsed.success) throw new Error('服务返回了无法识别的订阅数据，请刷新并核对结果');
  return parsed.data;
}
let demoSubscriptions: Subscription[] | undefined;
function demos() {
  return demoSubscriptions ??= [
    subscriptionSchema.parse({ id: 9101, user_id: 1, group_id: 1, status: 'active', starts_at: new Date(Date.now() - DAY).toISOString(), expires_at: new Date(Date.now() + 6 * DAY).toISOString(), group: { name: 'GPT Pro 高级推理周订阅', platform: 'openai', rate_multiplier: 1, weekly_limit_usd: 120 } }),
    subscriptionSchema.parse({ id: 9102, user_id: 1, group_id: 2, status: 'active', expires_at: new Date(Date.now() + 24 * DAY).toISOString(), daily_usage_usd: 2.6, monthly_usage_usd: 28.4, auto_renew_enabled: true, group: { name: 'Claude Plus 月订阅', platform: 'anthropic', rate_multiplier: 1.2, daily_limit_usd: 10, monthly_limit_usd: 200 } }),
  ];
}
export const subscriptionApi = {
  async list(): Promise<Subscription[]> {
    if (isPreview()) return structuredClone(demos());
    requireDesktop(); return z.array(subscriptionSchema).parse(await invoke('fetch_subscriptions'));
  },
  async reset(id: number): Promise<Subscription> {
    const subscriptionIdValue = subscriptionId(id);
    if (isPreview()) {
      const index = demos().findIndex(item => item.id === id), item = demos()[index];
      if (!item || !canEnableAutoRenew(item)) throw new Error('仅可重置有效且未过期的订阅');
      const term = item.expires_at && item.starts_at ? Math.max(DAY, Date.parse(item.expires_at) - Date.parse(item.starts_at)) : 30 * DAY;
      const updated = { ...item, id: Math.max(...demos().map(row => row.id)) + 1, starts_at: new Date().toISOString(), expires_at: new Date(Date.now() + term).toISOString(), daily_usage_usd: 0, weekly_usage_usd: 0, monthly_usage_usd: 0, daily_window_start: null, weekly_window_start: null, monthly_window_start: null, auto_renew_error: '' };
      demos()[index] = updated; return structuredClone(updated);
    }
    requireDesktop(); return subscriptionResult(await invoke('reset_subscription', { subscriptionId: subscriptionIdValue }));
  },
  async autoRenew(id: number, input: { enabled?: boolean; clear_error?: boolean }): Promise<Subscription> {
    const subscriptionIdValue = subscriptionId(id);
    if (input.enabled == null && input.clear_error !== true) throw new Error('自动续杯设置无效');
    if (isPreview()) {
      const item = demos().find(row => row.id === id);
      if (!item || input.enabled && !canEnableAutoRenew(item)) throw new Error('仅可为有效且未过期的订阅开启自动续杯');
      if (input.enabled != null) item.auto_renew_enabled = input.enabled;
      item.auto_renew_error = ''; return structuredClone(item);
    }
    requireDesktop(); return subscriptionResult(await invoke('update_subscription_auto_renew', { subscriptionId: subscriptionIdValue, input }));
  },
};
