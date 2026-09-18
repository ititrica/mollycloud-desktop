import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { i18nReady } from "./i18n";
import { logStartupEvent } from "./lib/tauri";
import "./index.css";
import "../../../../app/src/design-tokens.css";
import App from "./App.tsx";

await i18nReady;
logStartupEvent("i18n_ready", performance.now()).catch(() => {});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
logStartupEvent("root_rendered", performance.now()).catch(() => {});
