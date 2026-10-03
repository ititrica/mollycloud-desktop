import { invoke as nativeInvoke, isTauri } from '@tauri-apps/api/core';
import { listen as nativeListen, emit as nativeEmit, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';

const preview = !isTauri() || new URLSearchParams(location.search).has('ui-preview');
const prefix = preview ? 'mollycloud:preview:netspeed:' : 'mollycloud:netspeed:';
export const storage = {
  getItem(key: string): string | null {
    try {
      const value = window.localStorage.getItem(prefix + key);
      if (value !== null && key === 'nsd_custom_slots') {
        const slots = JSON.parse(value);
        if (!Array.isArray(slots) || slots.length !== 3 || slots.some(v => v !== null && !['speed','resource','fps','cover','time'].includes(v))) return null;
      }
      if (value !== null && key === 'nsd_island_pos') {
        const pos = JSON.parse(value);
        if (!pos || !['x','y','w','h'].every(k => Number.isFinite(pos[k]))) return null;
      }
      const limits: Record<string, [number, number]> = {
        base_width:[120,400],base_height:[28,60],music_base_width:[180,420],music_expanded_width:[280,560],
        msg_expanded_width:[280,560],border_radius:[8,100],app_scale:[0.75,1.5],island_opacity:[20,100],lyric_delay:[-5,5],
      };
      const range = limits[key.replace(/^nsd_/, '')];
      if (value !== null && range && (!Number.isFinite(Number(value)) || Number(value) < range[0] || Number(value) > range[1])) return null;
      return value;
    } catch { return null; }
  },
  setItem(key: string, value: string) { window.localStorage.setItem(prefix + key, value); },
  removeItem(key: string) { window.localStorage.removeItem(prefix + key); },
};
export async function invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (preview) {
    if (command === 'get_network_stats') return [0, 0] as T;
    if (command === 'get_network_latency') return 0 as T;
    return undefined as T;
  }
  return nativeInvoke<T>('plugin:molly-netspeed|' + command, args);
}
export async function listen<T>(name: string, callback: EventCallback<T>): Promise<UnlistenFn> {
  if (preview) return () => {};
  return nativeListen<T>('molly-netspeed:' + name, callback);
}
export async function emit(name: string, payload?: unknown): Promise<void> {
  if (!preview) await nativeEmit('molly-netspeed:' + name, payload);
}
