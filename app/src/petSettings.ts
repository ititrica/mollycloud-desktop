import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { z } from "zod";
import type { AssistantSettings } from "./petra/utils/settings";

export const modelParameters = [
  { key: "physAmp", label: "物理幅度", min: 0, max: 5, step: 0.1 },
  { key: "soft", label: "柔软度", min: 0, max: 5, step: 0.1 },
  { key: "fhAmp", label: "发丝幅度", min: 0, max: 5, step: 0.1 },
  { key: "fhSoft", label: "发丝柔软度", min: 0, max: 2, step: 0.05 },
  { key: "bust", label: "胸腔位置", min: 0, max: 5, step: 0.1 },
  { key: "bustY", label: "胸腔偏移", min: 0, max: 3, step: 0.1 },
  { key: "eyeEase", label: "眼睛平滑", min: 0.05, max: 1, step: 0.05 },
  { key: "mouthEase", label: "嘴型平滑", min: 0.05, max: 1, step: 0.05 },
  { key: "mouthScale", label: "嘴巴缩放", min: 0.3, max: 2, step: 0.05 },
  { key: "irisScale", label: "瞳孔缩放", min: 0.3, max: 2, step: 0.05 },
] as const;
export const modelAutoOptions = [
  { key: "autoBlink", label: "自动眨眼" },
  { key: "autoRand", label: "随机小动作" },
  { key: "autoIdle", label: "待机晃动" },
] as const;
const finite = z.number().finite();
export const petDraftSchema = z.object({
  model: z.object({ type: z.enum(["manifest", "import", "live2d"]), name: z.string().max(255) }),
  modelScale: finite.min(0.2).max(2),
  boundsPadding: z.object({ left: finite.min(-120).max(120), right: finite.min(-120).max(120), top: finite.min(-120).max(120), bottom: finite.min(-120).max(120) }),
  debugBorder: z.boolean(), debugModelBounds: z.boolean(),
  params: z.record(z.string(), finite), auto: z.record(z.string(), z.boolean()),
  assistant: z.object({
    enabled: z.boolean(),
    provider: z.enum(["mollycloud", "deepseek", "openai", "moonshot", "zhipu", "qwen", "siliconflow", "openrouter", "groq", "ollama", "custom"]),
    model: z.string().max(160), persona: z.string().max(1200), customBaseUrl: z.string().max(2048),
    greetInterval: finite.int().min(5).max(120),
  }),
}).superRefine((draft, ctx) => {
  for (const [key, value] of Object.entries(draft.params)) {
    const def = modelParameters.find(item => item.key === key);
    if (!def || value < def.min || value > def.max) ctx.addIssue({ code: "custom", message: "模型参数超出范围", path: ["params", key] });
  }
  if (Object.keys(draft.auto).some(key => !modelAutoOptions.some(item => item.key === key))) ctx.addIssue({ code: "custom", message: "未知自动行为" });
  if (draft.assistant.provider === "custom") {
    try {
      const url = new URL(draft.assistant.customBaseUrl);
      if ((url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) || url.username || url.password || url.search || url.hash) throw new Error();
    } catch { ctx.addIssue({ code: "custom", message: "请输入有效的 HTTPS API 端点（本机服务可用 HTTP）", path: ["assistant", "customBaseUrl"] }); }
  }
});
export type PetDraft = z.infer<typeof petDraftSchema>;
export interface PetSettingsSnapshot {
  draft: PetDraft;
  models: Array<{ type: "manifest" | "import" | "live2d"; name: string }>;
  defaults: Record<string, number>;
  supportsAdjust: boolean;
  apiKeyConfigured: boolean;
  availableModels?: string[];
}
export type PetSettingsRequest =
  | { action: "read" }
  | { action: "save"; draft: PetDraft; apiKey?: string; clearApiKey?: boolean }
  | { action: "select"; model: PetDraft["model"] }
  | { action: "delete"; name: string }
  | { action: "clear-history" }
  | { action: "fetch-models"; assistant: AssistantSettings; apiKey?: string; useSavedKey: boolean };

const previewKey = "mollycloud:preview:pet-settings";
function previewSnapshot(): PetSettingsSnapshot {
  const assistant: AssistantSettings = { enabled: true, provider: "mollycloud", model: "", persona: "你叫 Molly，语气自然、简洁、友好。", customBaseUrl: "", greetInterval: 20 };
  const defaults = Object.fromEntries(modelParameters.map(p => [p.key, p.key.endsWith("Ease") ? 0.2 : 1]));
  const saved = localStorage.getItem(previewKey);
  return { draft: saved ? petDraftSchema.parse(JSON.parse(saved)) : { model: { type: "manifest", name: "Molly.psd" }, modelScale: 1, boundsPadding: { left: 0, right: 0, top: 0, bottom: 0 }, debugBorder: false, debugModelBounds: false, params: defaults, auto: { autoBlink: true, autoRand: true, autoIdle: true }, assistant }, models: [{ type: "manifest", name: "Molly.psd" }, { type: "manifest", name: "seethrough_output.psd" }], defaults, supportsAdjust: true, apiKeyConfigured: false };
}

export async function requestPetSettings(request: PetSettingsRequest): Promise<PetSettingsSnapshot> {
  if (!("__TAURI_INTERNALS__" in window)) {
    if (!import.meta.env.DEV || new URLSearchParams(location.search).get("ui-preview") !== "console") throw new Error("桌宠设置需要在 MollyCloud 桌面端使用。");
    const state = previewSnapshot();
    if (request.action === "save") {
      if (request.apiKey?.trim()) throw new Error("浏览器预览不能保存真实 API 密钥。");
      localStorage.setItem(previewKey, JSON.stringify(petDraftSchema.parse(request.draft)));
      return previewSnapshot();
    }
    if (request.action !== "read") throw new Error("模型管理和对话清理需要在桌面端使用。");
    return state;
  }
  const id = crypto.randomUUID();
  return new Promise((resolve, reject) => {
    let unlisten: (() => void) | undefined;
    let settled = false;
    const finish = (value?: PetSettingsSnapshot, error?: unknown) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      unlisten?.();
      if (error) reject(error); else resolve(value!);
    };
    const timer = window.setTimeout(() => finish(undefined, new Error("桌宠响应超时，请稍后重新读取设置。")), 30_000);
    void listen<{ id: string; value?: PetSettingsSnapshot; error?: string }>("petra-settings-response", event => {
      if (event.payload.id === id) finish(event.payload.value, event.payload.error ? new Error(event.payload.error) : undefined);
    }).then(stop => {
      if (settled) { stop(); return; }
      unlisten = stop;
      return emitTo("main", "petra-settings-request", { id, request });
    }).catch(error => finish(undefined, error));
  });
}

export async function importPetModel(file: File): Promise<PetSettingsSnapshot> {
  if (!("__TAURI_INTERNALS__" in window)) throw new Error("模型导入需要在桌面端使用。");
  if (!file.name.toLowerCase().endsWith(".psd")) throw new Error("请选择 PSD 模型文件。");
  const name = await invoke<string>("save_psd", { name: file.name, bytes: Array.from(new Uint8Array(await file.arrayBuffer())) });
  return requestPetSettings({ action: "select", model: { type: "import", name } });
}
