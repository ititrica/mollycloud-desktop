import { createApp, h, shallowRef, watch } from "vue";
import { createPinia } from "pinia";
import { darkTheme, NConfigProvider } from "naive-ui";
import App from "./App.vue";
import { createThemeOverrides } from "./theme";
import { resolvedTheme, themePreference } from "./appearance";
import "./styles.css";
import "vfonts/FiraCode.css";

const themeOverrides = shallowRef<ReturnType<typeof createThemeOverrides>>();
watch([resolvedTheme, themePreference], ([theme, preference]) => {
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.themePreference = preference;
  themeOverrides.value = createThemeOverrides();
}, { immediate: true, flush: "sync" });

createApp({
  // The shared stylesheet owns document typography; Naive's preflight uses its default font.
  render: () => h(NConfigProvider, { theme: resolvedTheme.value === "dark" ? darkTheme : null, themeOverrides: themeOverrides.value, preflightStyleDisabled: true }, { default: () => h(App) }),
})
  .use(createPinia())
  .mount("#app");
