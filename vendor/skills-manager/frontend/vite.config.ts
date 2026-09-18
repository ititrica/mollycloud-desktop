import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";
const bridge = fileURLToPath(new URL("./src/embedded/bridge.ts", import.meta.url));
export default defineConfig({
  base: "./", plugins: [react()],
  resolve: { alias: Object.fromEntries(["@tauri-apps/api/event", "@tauri-apps/plugin-dialog", "@tauri-apps/plugin-opener", "@tauri-apps/plugin-clipboard-manager"].map(name => [name, bridge])) },
  build: { outDir: "../../../app/public/skills-manager", emptyOutDir: true, target: "es2022" },
});
