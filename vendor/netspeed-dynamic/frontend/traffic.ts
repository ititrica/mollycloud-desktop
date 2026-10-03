import { storage } from './bridge';
export type TrafficDay = { up: number; down: number };
export type TrafficHistory = Record<string, TrafficDay>;
export function readTraffic(): TrafficHistory {
  try {
    const raw = JSON.parse(storage.getItem('nsd_traffic_stats') || '{}');
    return Object.fromEntries(Object.entries(raw).filter(([date, value]: [string, any]) => /^\d{4}-\d{2}-\d{2}$/.test(date) && Number.isFinite(value?.up) && value.up >= 0 && Number.isFinite(value?.down) && value.down >= 0)) as TrafficHistory;
  } catch { return {}; }
}
export function counterDelta(previous: [number, number] | null, next: [number, number]): [number, number] {
  if (!previous || next.some((n, i) => !Number.isFinite(n) || n < previous[i])) return [0, 0];
  return [next[0] - previous[0], next[1] - previous[1]];
}
let previous: [number, number] | null = null;
let lastSave = 0;
let history = readTraffic();
export function recordTraffic(next: [number, number], now = new Date()): void {
  const [down, up] = counterDelta(previous, next);
  previous = next;
  const day = `${now.getFullYear()}-${String(now.getMonth()+1).padStart(2,'0')}-${String(now.getDate()).padStart(2,'0')}`;
  const item = history[day] ??= { up: 0, down: 0 };
  item.up += up; item.down += down;
  if (now.getTime() - lastSave > 15000) {
    history = Object.fromEntries(Object.entries(history).sort(([a],[b]) => a.localeCompare(b)).slice(-366));
    storage.setItem('nsd_traffic_stats', JSON.stringify(history));
    lastSave = now.getTime();
  }
}
