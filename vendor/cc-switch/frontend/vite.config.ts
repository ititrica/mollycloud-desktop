import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: "src",
  base: "./",
  plugins: [react()],
  resolve: {
    alias: {
      "@tauri-apps/api/core": path.resolve(__dirname, "src/embedded/core.ts"),
      "@tauri-apps/api/event": path.resolve(__dirname, "src/embedded/event.ts"),
      "@tauri-apps/api/app": path.resolve(__dirname, "src/embedded/app.ts"),
      "@tauri-apps/api/window": path.resolve(__dirname, "src/embedded/window.ts"),
      "@tauri-apps/plugin-log": path.resolve(__dirname, "src/embedded/log.ts"),
      "@": path.resolve(__dirname, "src"),
    },
  },
  build: {
    outDir: path.resolve(__dirname, "../../../app/public/ccswitch"),
    emptyOutDir: true,
  },
});
