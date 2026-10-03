import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { canEnableAutoRenew, subscriptionApi, subscriptionExpiry, subscriptionQuotas, subscriptionSchema } from './subscriptions';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: ipc.invoke }));
const row = (overrides = {}) => subscriptionSchema.parse({ id: 42, user_id: 1, group_id: 1, status: 'active', ...overrides });
beforeEach(() => { ipc.invoke.mockReset(); vi.stubGlobal('window', { __TAURI_INTERNALS__: {}, location: { search: '' } }); });
afterEach(() => vi.unstubAllGlobals());

describe('website subscription behavior', () => {
  it('does not enable renew on expired, revoked or malformed terms', () => {
    const now = Date.now();
    expect(canEnableAutoRenew(row(), now)).toBe(true);
    for (const expires_at of [new Date(now).toISOString(), new Date(now - 1).toISOString(), 'invalid']) expect(canEnableAutoRenew(row({ expires_at }), now)).toBe(false);
    expect(canEnableAutoRenew(row({ status: 'expired', auto_renew_enabled: true }), now)).toBe(false);
    expect(canEnableAutoRenew(row({ status: 'revoked' }), now)).toBe(false);
  });
  it('uses local calendar dates for today and tomorrow while preserving remaining-day semantics', () => {
    const now = new Date(2026, 9, 2, 23, 30).getTime();
    expect(subscriptionExpiry(row({ expires_at: new Date(2026, 9, 2, 23, 45).toISOString() }), now).label).toContain('(今天)');
    expect(subscriptionExpiry(row({ expires_at: new Date(2026, 9, 3, 1).toISOString() }), now).label).toContain('(明天)');
    expect(subscriptionExpiry(row({ expires_at: new Date(now + 6 * 86400000).toISOString() }), now)).toMatchObject({ tone: 'warning' });
    expect(subscriptionExpiry(row({ expires_at: new Date(now).toISOString() }), now).label).toBe('已过期');
  });
  it('displays every configured quota rather than hiding daily or weekly limits behind monthly usage', () => {
    const quotas = subscriptionQuotas(row({ group: { name: 'Pro', daily_limit_usd: 10, weekly_limit_usd: 120, monthly_limit_usd: 200 }, daily_usage_usd: 12, weekly_usage_usd: 90, monthly_usage_usd: 10 }));
    expect(quotas.map(q => q.label)).toEqual(['每日', '每周', '每月']);
    expect(quotas[0]).toMatchObject({ used: 12, limit: 10, percentage: 100, tone: 'danger' });
    expect(quotas[1]?.tone).toBe('warning');
    expect(subscriptionQuotas(row({ group: { name: 'Unlimited', daily_limit_usd: 0 } }))).toEqual([]);
  });
  it('uses term expiry for a one-day quota, and rolling windows for longer subscriptions', () => {
    const now = Date.parse('2026-10-02T08:00:00Z');
    const item = row({ starts_at: '2026-10-02T00:00:00Z', expires_at: '2026-10-03T00:00:00Z', daily_window_start: '2026-10-02T07:00:00Z', group: { name: 'One day', daily_limit_usd: 10 } });
    expect(subscriptionQuotas(item, now)[0]?.reset).toBe('额度将在 16h 0m 后结束');
    item.expires_at = '2026-10-05T00:00:00Z';
    expect(subscriptionQuotas(item, now)[0]?.reset).toBe('23h 0m 后重置');
  });
});

describe('subscription IPC and paid reset boundary', () => {
  it('refuses unsafe IDs and browser actions before dispatching any write', async () => {
    for (const id of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) await expect(subscriptionApi.reset(id)).rejects.toThrow('ID');
    vi.stubGlobal('window', { location: { search: '' } });
    await expect(subscriptionApi.reset(42)).rejects.toThrow('客户端');
    expect(ipc.invoke).not.toHaveBeenCalled();
  });
  it('does not retry a paid reset after an ambiguous response', async () => {
    ipc.invoke.mockRejectedValue(new Error('连接超时'));
    await expect(subscriptionApi.reset(42)).rejects.toThrow('超时');
    expect(ipc.invoke).toHaveBeenCalledTimes(1);
    expect(ipc.invoke).toHaveBeenCalledWith('reset_subscription', { subscriptionId: '42' });
  });
  it('requires reconciliation when a successful paid response cannot be decoded', async () => {
    ipc.invoke.mockResolvedValue({ message: 'success' });
    await expect(subscriptionApi.reset(42)).rejects.toThrow('无法识别');
    expect(ipc.invoke).toHaveBeenCalledTimes(1);
  });
  it('accepts a replacement term ID and strips unrelated account or credential fields', async () => {
    ipc.invoke.mockResolvedValue({ ...row(), id: 43, auto_renew_enabled: true, access_token: 'mock-secret', user: { password: 'mock-password' } });
    const updated = await subscriptionApi.reset(42);
    expect(updated.id).toBe(43);
    expect(updated.auto_renew_enabled).toBe(true);
    expect(updated).not.toHaveProperty('access_token');
    expect(updated).not.toHaveProperty('user');
  });
  it('keeps failure acknowledgement separate from enabling or disabling renewal', async () => {
    ipc.invoke.mockResolvedValue(row());
    await subscriptionApi.autoRenew(42, { clear_error: true });
    expect(ipc.invoke).toHaveBeenLastCalledWith('update_subscription_auto_renew', { subscriptionId: '42', input: { clear_error: true } });
    await subscriptionApi.autoRenew(42, { enabled: false });
    expect(ipc.invoke).toHaveBeenLastCalledWith('update_subscription_auto_renew', { subscriptionId: '42', input: { enabled: false } });
    await expect(subscriptionApi.autoRenew(42, {})).rejects.toThrow('无效');
    expect(ipc.invoke).toHaveBeenCalledTimes(2);
  });
});
