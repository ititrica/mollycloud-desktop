import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import bundled from "./catalog.json";
import community from "./community.json";

export interface CatalogEntry {
  id: string; name: string; description: string; category: string; homepage: string;
  source: string; fields: Array<{ key: string; label: string; kind: string; required: boolean; default?: string; placeholder?: string; choices?: string[] }>;
  recipe: null | { kind: string; version: string; package?: string; oauth?: boolean; url?: string };
}
export const templates = bundled as CatalogEntry[];
export const directory = community as CatalogEntry[];
const agentSchema = z.object({ id: z.enum(["codex", "claude", "opencode", "gemini"]), name: z.string(), path: z.string().nullable(), detected: z.boolean(), error: z.string().nullable() });
const installationSchema = z.object({ key: z.string(), id: z.string(), name: z.string(), agent: z.string(), path: z.string().nullable(), enabled: z.boolean(), managed: z.boolean(), changed: z.boolean(), version: z.string(), check: z.string(), canUpdate: z.boolean() });
const packageSchema = z.object({ id: z.string(), version: z.string(), path: z.string(), reason: z.string().nullable() });
const statusSchema = z.object({ agents: z.array(agentSchema), installations: z.array(installationSchema), packageRoot: z.string(), packages: z.array(packageSchema).default([]), message: z.string().optional(), node: z.boolean(), uv: z.boolean() });
const fieldSchema = z.object({ key: z.string(), label: z.string(), kind: z.string(), required: z.boolean(), default: z.string().optional(), placeholder: z.string().optional(), choices: z.array(z.string()).optional() });
const planEntrySchema = z.object({ id: z.string().regex(/^[a-zA-Z0-9_-]{1,80}$/), name: z.string(), description: z.string(), category: z.string(), homepage: z.string().url(), source: z.string(), fields: z.array(fieldSchema), recipe: z.object({ kind: z.enum(['npm','pypi','remote']), version: z.string(), package: z.string().optional(), oauth: z.boolean().optional(), url: z.string().optional() }).passthrough() });
const resolutionSchema = z.object({ plans: z.array(z.object({ planId: z.string(), label: z.string(), sourceUrl: z.string().url().startsWith('https://'), entry: planEntrySchema })), notes: z.array(z.string()) });
export type SourcePlan = z.infer<typeof resolutionSchema>["plans"][number];
export type LocalPackage = z.infer<typeof packageSchema>;
const registryEntrySchema = z.object({ id: z.string().max(300), name: z.string().max(300), description: z.string().max(5000), category: z.string().max(100), homepage: z.string().url().startsWith("https://"), source: z.literal("registry") });
export function readRegistryCache(value: unknown): CatalogEntry[] {
  if (!Array.isArray(value)) return [];
  return value.slice(0, 40).flatMap(entry => {
    const parsed = registryEntrySchema.safeParse(entry);
    return parsed.success ? [{ ...parsed.data, fields: [], recipe: null }] : [];
  });
}
export type MarketStatus = z.infer<typeof statusSchema>;
export type Installation = z.infer<typeof installationSchema>;
export interface InstallRequest { id: string; agents: string[]; values: Record<string, string>; custom?: unknown; planId?: string }
const native = () => "__TAURI_INTERNALS__" in window;
function requireDesktop(): void { if (!native()) throw new Error("浏览器预览不能修改本机配置，请在 MollyCloud 桌面端安装 MCP。"); }
export function filterCatalog(entries: CatalogEntry[], query: string, category: string): CatalogEntry[] {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return entries.filter(entry => (!category || entry.category === category) && words.every(word => `${entry.name} ${entry.description} ${entry.recipe?.package ?? ""} ${entry.category}`.toLowerCase().includes(word)));
}
export function parseCustom(text: string): Record<string, unknown> {
  const value = JSON.parse(text);
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("请粘贴单个 MCP 的 JSON 连接配置。");
  if (value.mcpServers || value.mcp_servers || value.mcp) throw new Error("请只粘贴一个 MCP 的连接配置，不包含外层 mcpServers 或 mcp_servers。");
  if (value.url && !value.type) value.type = "http";
  return value;
}
export const marketApi = {
  async status(): Promise<MarketStatus> {
    if (!native()) return { agents: [{ id: "codex", name: "Codex" }, { id: "claude", name: "Claude Code" }, { id: "opencode", name: "OpenCode" }, { id: "gemini", name: "Gemini CLI" }].map(agent => ({ ...agent, path: "桌面端检测实际配置目录", detected: false, error: null })) as MarketStatus["agents"], installations: [], packages: [], packageRoot: "桌面端使用独立的 MCP 安装目录", node: false, uv: false };
    return statusSchema.parse(await invoke("mcp_market_status"));
  },
  async install(request: InstallRequest): Promise<MarketStatus> { requireDesktop(); return statusSchema.parse(await invoke("mcp_market_install", { request })); },
  async resolve(source: string, id: string) { requireDesktop(); return resolutionSchema.parse(await invoke("mcp_market_resolve", { source, id })); },
  async cleanup(id: string, version: string): Promise<MarketStatus> { requireDesktop(); return statusSchema.parse(await invoke("mcp_market_cleanup", { id, version })); },
  async action(key: string, action: string): Promise<MarketStatus> { requireDesktop(); return statusSchema.parse(await invoke("mcp_market_action", { key, action })); },
  async search(query: string): Promise<CatalogEntry[]> {
    requireDesktop();
    const schema = z.array(registryEntrySchema.extend({ fields: z.array(z.never()), recipe: z.null() }));
    return schema.parse(await invoke("mcp_market_search", { query }));
  },
};
