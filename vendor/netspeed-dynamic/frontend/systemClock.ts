import { computed, ref, watch, type Ref } from 'vue';

type ClockStorage = { getItem(key: string): string | null; setItem(key: string, value: string): unknown; removeItem(key: string): unknown };

// Preserve the old additional clock as a custom combination when upgrading.
export function migrateLegacyClock(storage: ClockStorage): void {
  const legacy = storage.getItem('nsd_show_system_time');
  if (legacy === null) return;
  const keys = ['nsd_custom_slots', 'nsd_custom_display', 'nsd_music_ctrl', 'nsd_sys_resource', 'nsd_fps_monitor', 'nsd_system_time', 'nsd_show_system_time'];
  const previous = new Map(keys.map(key => [key, storage.getItem(key)]));
  try {
    if (legacy === 'true') {
      let slots: (string | null)[];
      if (storage.getItem('nsd_custom_display') === 'true') {
        let saved: unknown;
        try { saved = JSON.parse(storage.getItem('nsd_custom_slots') || 'null'); } catch { saved = null; }
        slots = Array.from({ length: 3 }, (_, index) => Array.isArray(saved) && ['speed', 'resource', 'fps', 'cover', 'time'].includes(saved[index]) ? saved[index] : null);
      } else {
        const primary = storage.getItem('nsd_music_ctrl') === 'true' ? 'cover' : storage.getItem('nsd_sys_resource') === 'true' ? 'resource' : storage.getItem('nsd_fps_monitor') === 'true' ? 'fps' : 'speed';
        slots = [primary, null, null];
      }
      if (!slots.includes('time')) slots[slots.includes(null) ? slots.indexOf(null) : 2] = 'time';
      storage.setItem('nsd_custom_slots', JSON.stringify(slots));
      storage.setItem('nsd_custom_display', 'true');
      storage.setItem('nsd_music_ctrl', String(slots.includes('cover')));
      storage.setItem('nsd_sys_resource', 'false');
      storage.setItem('nsd_fps_monitor', 'false');
      storage.setItem('nsd_system_time', 'false');
    }
    storage.removeItem('nsd_show_system_time');
  } catch {
    for (const [key, value] of previous) {
      try { if (value === null) storage.removeItem(key); else storage.setItem(key, value); } catch { /* Keep the previous preference for a later retry. */ }
    }
  }
}

export function useSystemClock(active: Ref<boolean>) {
  const now = ref(new Date());
  watch(active, (enabled, _, cleanup) => {
    if (!enabled) return;
    const tick = () => { now.value = new Date(); };
    tick();
    const timer = setInterval(tick, 1000);
    cleanup(() => clearInterval(timer));
  }, { immediate: true });
  const text = computed(() => [now.value.getHours(), now.value.getMinutes(), now.value.getSeconds()].map(n => String(n).padStart(2, '0')).join(':'));
  return { now, text };
}
