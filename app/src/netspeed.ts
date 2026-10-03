import { storage, invoke, emit } from '../../vendor/netspeed-dynamic/frontend/bridge';
import { migrateLegacyClock } from '../../vendor/netspeed-dynamic/frontend/systemClock';
export { storage, invoke, emit, listen } from '../../vendor/netspeed-dynamic/frontend/bridge';

export type IslandMode = 'speed' | 'resource' | 'music' | 'fps' | 'time' | 'custom';
export const islandDefaults = {
  widget_visible: true, system_time: false, music_ctrl: false, sys_resource: false, fps_monitor: false, custom_display: false,
  msg_notify: false, clipboard: false, msg_mode: false, autohide_fs: false, autohide_fs_hover: false,
  position_locked: false, glow_border: false, taskbar_plugin: false, activity_api: false,
  island_opacity: 100, base_width: 150, base_height: 34, music_base_width: 260,
  music_expanded_width: 320, msg_expanded_width: 360, border_radius: 100, app_scale: 1,
  lyric_delay: 0, island_theme: 'black', spring_style: 'bouncy', target_player: 'other',
};
export type IslandSettings = typeof islandDefaults;
export function readIslandSettings(): IslandSettings {
  migrateLegacyClock(storage);
  const settings = { ...islandDefaults };
  for (const key of Object.keys(settings) as (keyof IslandSettings)[]) {
    const value = storage.getItem('nsd_' + key);
    if (value === null) continue;
    const fallback = islandDefaults[key];
    const parsed = typeof fallback === 'boolean' ? value === 'true' : typeof fallback === 'number' ? Number(value) : value;
    if (typeof parsed !== 'number' || Number.isFinite(parsed)) (settings as Record<string, unknown>)[key] = parsed;
  }
  return settings;
}
export function readIslandMode(settings: IslandSettings): IslandMode {
  return settings.custom_display ? 'custom' : settings.system_time ? 'time' : settings.music_ctrl ? 'music' : settings.sys_resource ? 'resource' : settings.fps_monitor ? 'fps' : 'speed';
}
export function readSlots(): (string | null)[] {
  try {
    const slots = JSON.parse(storage.getItem('nsd_custom_slots') || '["speed","resource",null]');
    return Array.from({ length: 3 }, (_, i) => ['speed', 'resource', 'fps', 'cover', 'time'].includes(slots?.[i]) ? slots[i] : null);
  } catch { return ['speed', 'resource', null]; }
}
const switches: Partial<Record<keyof IslandSettings, string>> = {
  widget_visible: 'control-island-visibility', music_ctrl: 'control-music-ctl', sys_resource: 'control-sys-resource',
  fps_monitor: 'control-fps-monitor', msg_mode: 'control-msg-mode', clipboard: 'control-clipboard',
  autohide_fs: 'control-autohide-fs', autohide_fs_hover: 'control-autohide-fs-hover',
  glow_border: 'control-glow', position_locked: 'control-lock',
};
export async function saveIslandSetting<K extends keyof IslandSettings>(key: K, value: IslandSettings[K], current: IslandSettings): Promise<void> {
  // Start optional services before committing their switches. Failure leaves the previous value intact.
  if (key === 'activity_api') await invoke('configure_activity_api', { enabled: value });
  if (key === 'taskbar_plugin') await invoke('toggle_taskbar_plugin', { enable: value });
  if (key === 'target_player') await invoke('set_target_player', { player: value });
  const old = storage.getItem('nsd_' + key);
  try { storage.setItem('nsd_' + key, String(value)); }
  catch (e) {
    if (key === 'activity_api') await invoke('configure_activity_api', { enabled: current[key] });
    if (key === 'taskbar_plugin') await invoke('toggle_taskbar_plugin', { enable: current[key] });
    throw e;
  }
  try {
    const event = switches[key];
    if (event) await emit(event, key === 'widget_visible' ? { show: value } : { enabled: value });
    if (key === 'island_opacity') await emit('control-island-opacity', { opacity: value });
    if (key === 'island_theme') await emit('control-island-theme', { theme: value });
    if (['base_width','base_height','music_base_width','music_expanded_width','msg_expanded_width','border_radius','spring_style','app_scale','lyric_delay'].includes(key)) {
      const s = { ...current, [key]: value };
      await emit('sync-dynamic-settings', { baseWidth: s.base_width, baseHeight: s.base_height, musicBaseWidth: s.music_base_width, musicExpandedWidth: s.music_expanded_width, msgExpandedWidth: s.msg_expanded_width, borderRadius: s.border_radius, springStyle: s.spring_style, appScale: s.app_scale, lyricDelay: s.lyric_delay });
    }
  } catch (error) { if (old === null) storage.removeItem('nsd_' + key); else storage.setItem('nsd_' + key, old); throw error; }
}
export async function saveIslandMode(mode: IslandMode, slots: (string | null)[]): Promise<void> {
  const fps = mode === 'fps' || (mode === 'custom' && slots.includes('fps'));
  const music = mode === 'music' || (mode === 'custom' && slots.includes('cover'));
  await invoke('toggle_fps_plugin', { enable: fps });
  const values = { music_ctrl: music, sys_resource: mode === 'resource', fps_monitor: mode === 'fps', system_time: mode === 'time', custom_display: mode === 'custom' };
  const previous = new Map([...Object.keys(values).map(key => 'nsd_' + key), 'nsd_custom_slots'].map(key => [key, storage.getItem(key)]));
  try {
    for (const [key, active] of Object.entries(values)) storage.setItem('nsd_' + key, String(active));
    storage.setItem('nsd_custom_slots', JSON.stringify(slots));
    await emit('control-mode', { mode, slots });
  } catch (error) {
    for (const [key, value] of previous) { if (value === null) storage.removeItem(key); else storage.setItem(key, value); }
    const oldSlots = readSlots();
    await invoke('toggle_fps_plugin', { enable: storage.getItem('nsd_fps_monitor') === 'true' || (storage.getItem('nsd_custom_display') === 'true' && oldSlots.includes('fps')) }).catch(() => {});
    throw error;
  }
}
export function formatTraffic(bytes: number, speed = false): string {
  const safe = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  const unit = Math.min(4, Math.floor(Math.log(safe || 1) / Math.log(1024)));
  return `${(safe / 1024 ** unit).toFixed(unit ? 1 : 0)} ${['B','KB','MB','GB','TB'][unit]}${speed ? '/s' : ''}`;
}
