import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import process from "node:process";
import { resolve } from "node:path";
// @ts-expect-error Development middleware is plain Node ESM.
import { pluginAssetsMiddleware } from "./scripts/plugin-assets.mjs";
// @ts-expect-error Shared build helpers are plain Node ESM.
import { targetPlatform, pluginRegistryRoot } from "./scripts/platform.mjs";
const host = process.env.TAURI_DEV_HOST;
const platform = targetPlatform();

// https://vite.dev/config/
export default defineConfig(() => ({
  define: { __MOLLY_PLATFORM__: JSON.stringify(platform) },
  plugins: [{
    name: 'molly-platform-entry',
    enforce: 'pre',
    transform(source, id) {
      if (platform === 'windows' || id !== resolve(import.meta.dirname, 'src/App.vue')) return;
      // Remove the Windows import before Vue/Rollup discover the module graph.
      // Tree shaking alone still emits dynamic-import chunks and preload references.
      const windowsPanel = /const NetSpeedPanel = isWindows \? defineAsyncComponent\(\(\) => import\("\.\/components\/NetSpeedPanel\.vue"\)\) : undefined;/;
      if (!windowsPanel.test(source)) throw new Error('macOS 灵动岛入口规则已变化，请同步平台构建排除逻辑。');
      return { code: source.replace(windowsPanel, 'const NetSpeedPanel = undefined;'), map: null };
    },
  }, vue(), {
    name: 'molly-image-sandbox-assets',
    configureServer(server) {
      const registry = pluginRegistryRoot(platform);
      if (registry) server.middlewares.use(pluginAssetsMiddleware(
        registry, resolve(import.meta.dirname, "public"),
      ));
      server.middlewares.use((request, response, next) => {
        if (request.url?.startsWith('/image-workbench/')) response.setHeader('Access-Control-Allow-Origin', '*');
        next();
      });
    },
  }],
  resolve: { dedupe: ['vue'], alias: { '@tauri-apps/api': resolve(import.meta.dirname, 'node_modules/@tauri-apps/api'), '@tauri-apps/plugin-opener': resolve(import.meta.dirname, 'node_modules/@tauri-apps/plugin-opener'), 'vue': resolve(import.meta.dirname, 'node_modules/vue') } },
  build: {
    rollupOptions: {
      input: {
        console: resolve(import.meta.dirname, "index.html"),
        overlay: resolve(import.meta.dirname, "overlay.html"),
        ...(platform === 'windows' ? { island: resolve(import.meta.dirname, "island.html") } : {}),
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 24320,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 24321,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
    proxy: {
      "/molly-api": {
        target: "https://mollycloud.cn",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/molly-api/, ""),
      },
    },
  },
}));
