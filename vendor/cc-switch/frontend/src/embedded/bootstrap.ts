import { reportLoadError } from "./bridge";
import { canPreview, getNativeBridge } from "./native-bridge";

const prefix = "mollycloud:ccswitch:";
function isolateStorage(name: "localStorage" | "sessionStorage") {
  const backing = window[name];
  const keys = () => Array.from({ length: backing.length }, (_, index) => backing.key(index))
    .filter((key): key is string => key !== null && key.startsWith(prefix));
  const isolated: Storage = {
    get length() { return keys().length; },
    clear() { keys().forEach((key) => backing.removeItem(key)); },
    key(index) { return keys()[index]?.slice(prefix.length) ?? null; },
    getItem(key) { return backing.getItem(prefix + key); },
    setItem(key, value) { backing.setItem(prefix + key, value); },
    removeItem(key) { backing.removeItem(prefix + key); },
  };
  Object.defineProperty(window, name, { configurable: false, value: isolated });
}

async function bootstrap() {
  if (window.parent === window || window.parent.location.origin !== window.location.origin) {
    throw new Error("请从 MollyCloud 控制台打开内置 CC Switch。");
  }
  isolateStorage("localStorage");
  isolateStorage("sessionStorage");
  if (canPreview()) {
    const { installReadOnlyPreview } = await import("./preview");
    installReadOnlyPreview();
    document.documentElement.dataset.mollyPreview = "true";
  }
  if (!getNativeBridge()) throw new Error("请在 MollyCloud 桌面客户端中打开内置 CC Switch。");
  if (!localStorage.getItem("language")) localStorage.setItem("language", "zh");
  if (!localStorage.getItem("cc-switch-last-app")) localStorage.setItem("cc-switch-last-app", "codex");
  const theme = localStorage.getItem("cc-switch-theme") ?? "light";
  document.documentElement.classList.toggle("dark", theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches));
  await import("../main");
}

void bootstrap().catch((error: unknown) => {
  const root = document.getElementById("root");
  if (root) {
    root.setAttribute("role", "alert");
    root.textContent = error instanceof Error ? error.message : "内置 CC Switch 初始化失败。";
  }
  reportLoadError();
});
