import { createApp, h } from "vue";
import { createPinia } from "pinia";
import { NConfigProvider } from "naive-ui";
import App from "./App.vue";
import { createThemeOverrides } from "./theme";
import "./styles.css";
import "vfonts/FiraCode.css";

const themeOverrides = createThemeOverrides();

createApp({
  // The shared stylesheet owns document typography; Naive's preflight uses its default font.
  render: () => h(NConfigProvider, { themeOverrides, preflightStyleDisabled: true }, { default: () => h(App) }),
})
  .use(createPinia())
  .mount("#app");
