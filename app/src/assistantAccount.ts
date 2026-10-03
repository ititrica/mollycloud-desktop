import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
const quotaSchema = z.object({ period: z.enum(['daily','weekly','monthly']), limit_usd: z.number().finite().positive(), used_usd: z.number().finite().nonnegative(), remaining_usd: z.number().finite().nonnegative(), remaining_percent: z.number().finite().min(0).max(100), window_start: z.string().nullable() });
const subscriptionSchema = z.object({ id: z.union([z.string(),z.number()]).transform(String), name: z.string(), status: z.string(), expires_at: z.string().nullable().optional(), quotas: z.array(quotaSchema) });
export const accountHealthSchema = z.object({ user_id: z.union([z.string(),z.number()]).transform(String), currency: z.literal('USD'), balance: z.number().finite().nullable(), subscriptions: z.array(subscriptionSchema) });
export type AccountHealth = z.infer<typeof accountHealthSchema>;
export type AccountAlert = { id: string; text: string };
const HOUR = 3600000;
export function accountAlerts(health: AccountHealth, now = Date.now()): AccountAlert[] {
  const result: AccountAlert[] = [];
  if (health.balance != null && health.balance < 5) result.push({ id: 'balance-low', text: `账户余额剩余 US$${health.balance.toFixed(4)}，已低于 US$5。` });
  for (const sub of health.subscriptions) {
    const expiry = sub.expires_at ? Date.parse(sub.expires_at) : Infinity;
    if (sub.status !== 'active' || !(expiry > now)) continue;
    for (const quota of sub.quotas) if (quota.remaining_percent < 5) {
      result.push({ id: `quota:${sub.id}:${quota.period}:${quota.window_start ?? 'initial'}`, text: `「${sub.name}」${{daily:'每日',weekly:'每周',monthly:'每月'}[quota.period]}额度剩余不足 5%（US$${quota.remaining_usd.toFixed(2)} / US$${quota.limit_usd.toFixed(2)}）。` });
    }
    if (expiry - now < 24 * HOUR) result.push({ id: `expiry:${sub.id}:${sub.expires_at}`, text: `「${sub.name}」将在 ${new Date(expiry).toLocaleString('zh-CN', {hour12:false})} 到期，剩余不到 24 小时。` });
  }
  return result;
}
/** Only successful messages are deduplicated. Busy chats and failed reads retry later. */
export class AccountReminderMonitor {
  private timer?: ReturnType<typeof setInterval>;
  private generation = 0;
  private inFlight = false;
  private delivered: Record<string,number> = {};
  private readonly storageKey = 'mollycloud:assistant-account-alerts:v1';
  constructor(private ready: () => boolean, private send: (message: string) => boolean,
    private fetchHealth = async () => accountHealthSchema.parse(await invoke('assistant_account_health')),
    private storage: Pick<Storage,'getItem'|'setItem'> = localStorage, private now = Date.now) {
    try { const raw = JSON.parse(storage.getItem(this.storageKey) ?? '{}'); for (const [key,value] of Object.entries(raw)) if(typeof value === 'number' && Number.isFinite(value)) this.delivered[key]=value; } catch { /* Optional deduplication storage. */ }
  }
  start() { if(this.timer) return; this.timer=setInterval(()=>void this.check(),60000); void this.check(); }
  reset() { ++this.generation; }
  stop() { this.reset(); clearInterval(this.timer); this.timer=undefined; }
  async check() {
    if(this.inFlight || !this.ready()) return;
    this.inFlight = true; const version=this.generation;
    try {
      const health=await this.fetchHealth();
      if(version!==this.generation || !this.ready()) return;
      const now=this.now();
      const fresh=accountAlerts(health,now).filter(alert=>now-(this.delivered[`${health.user_id}:${alert.id}`]??0)>=6*HOUR).slice(0,5);
      if(!fresh.length) return;
      if(!this.send(`提醒你留意一下：\n${fresh.map(alert=>alert.text).join('\n')}\n需要时可以到概览的充值或订阅页查看。`)) return;
      for(const alert of fresh) this.delivered[`${health.user_id}:${alert.id}`]=now;
      this.delivered=Object.fromEntries(Object.entries(this.delivered).filter(([,time])=>time>now-7*24*HOUR).slice(-200));
      try { this.storage.setItem(this.storageKey,JSON.stringify(this.delivered)); } catch { /* Keep the in-memory cooldown. */ }
    } catch { /* Authentication/network failures never become low-balance messages. */ }
    finally { this.inFlight=false; }
  }
}
