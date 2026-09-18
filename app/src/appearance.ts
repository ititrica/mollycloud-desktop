import { computed, readonly, ref } from "vue";

export type ThemePreference = "system" | "light" | "dark";
const storageKey = import.meta.env.DEV && new URLSearchParams(location.search).has("ui-preview")
  ? "mollycloud:preview:appearance" : "mollycloud:appearance";
const media = window.matchMedia("(prefers-color-scheme: dark)");
const systemDark = ref(media.matches);
function parsePreference(value: string | null): ThemePreference {
  return value === "light" || value === "dark" ? value : "system";
}
let saved: string | null = null;
try { saved = localStorage.getItem(storageKey); } catch { /* System theme also works without storage. */ }
const preference = ref<ThemePreference>(parsePreference(saved));
export const themePreference = readonly(preference);
export const resolvedTheme = computed(() => preference.value === "system" ? (systemDark.value ? "dark" : "light") : preference.value);
export const themeOptions = [
  { value: "system", label: "跟随系统", icon: "monitor" },
  { value: "light", label: "浅色", icon: "sun" },
  { value: "dark", label: "深色", icon: "moon" },
] as const;

export function setThemePreference(value: ThemePreference): void {
  // Persist before applying: a failed save leaves the current selection intact.
  localStorage.setItem(storageKey, value);
  preference.value = value;
}

const onSystemChange = (event: MediaQueryListEvent) => { systemDark.value = event.matches; };
const onStorage = (event: StorageEvent) => {
  if (event.storageArea === localStorage && (event.key === storageKey || event.key === null)) {
    preference.value = parsePreference(event.newValue);
  }
};
media.addEventListener("change", onSystemChange);
window.addEventListener("storage", onStorage);
if (import.meta.hot) import.meta.hot.dispose(() => {
  media.removeEventListener("change", onSystemChange);
  window.removeEventListener("storage", onStorage);
});
