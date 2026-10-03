import type { JsonRecord } from './contracts';
export function validCost(value: unknown): number | null {
  if(value == null || typeof value === 'string' && !value.trim() || !['number','string'].includes(typeof value)) return null;
  const n=Number(value);return Number.isFinite(n)&&n>=0?n:null;
}
export function keyUsage(item: JsonRecord) {
  const usage = item.usage && typeof item.usage==='object' ? item.usage as JsonRecord : {};
  return { total: validCost(usage.total_actual_cost), today: validCost(usage.today_actual_cost) };
}
export function costLabel(value: number | null) { return value == null ? '—' : `US$${value.toFixed(4)}`; }
export function keyQuota(item: JsonRecord) {
  const limit=validCost(item.quota),used=validCost(item.quota_used);
  if(!limit || used==null) return null;
  return {limit,used,percentage:Math.min(100,used/limit*100)};
}
