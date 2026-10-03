import { defineConfig } from "vitest/config";
import { fileURLToPath } from 'node:url';
export default defineConfig({resolve:{alias:{'@':fileURLToPath(new URL('./src',import.meta.url))}}, test: { environment: "jsdom", include: ["src/embedded/*.test.ts", "src/embedded/*.test.tsx"] } });
