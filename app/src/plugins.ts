import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import definitions from "../plugins/definitions.json";
export type PluginId = "ccswitch" | "images" | "skills";
export type PluginAction = "install" | "uninstall" | "update" | "rollback";
export interface PluginStatus {
  id: PluginId; name: string; version: string; upstreamVersion: string; description: string;
  repository: string; installedVersion: string | null; upstreamVersionInstalled: string | null;
  installed: boolean; usable: boolean; compatible: boolean; restartRequired: boolean; canRollback: boolean;
}
export interface PluginCheck {
  officialVersion: string | null; officialError: string | null; catalogError: string | null;
  package: { version: string; upstreamVersion: string; notes: string } | null; compatible: boolean; updateAvailable: boolean;
}
const native = "__TAURI_INTERNALS__" in window;
export const pluginPreview = !native && import.meta.env.DEV && new URLSearchParams(location.search).get("ui-preview") === "console";
const previewKey = "mollycloud:preview:plugins";
const defaults = () => definitions.map(d => ({...d, id:d.id as PluginId, installedVersion:d.version, upstreamVersionInstalled:d.upstreamVersion, installed:true, usable:pluginPreview, compatible:true, restartRequired:false, canRollback:false}));
export const plugins = ref<PluginStatus[]>(defaults());
export const pluginBusy = ref("");
export const pluginError = ref("");
export const pluginChecks = ref<Partial<Record<PluginId, PluginCheck>>>({});
export const pluginRevision = ref<Record<string, number>>({});
export const pluginNeedsRestart = computed(() => plugins.value.some(p => p.restartRequired));
export function pluginUsable(id: string): boolean { return plugins.value.some(p => p.id === id && p.usable); }
export function pluginKey(id: string): string { return `${id}:${pluginRevision.value[id] ?? 0}`; }
export async function loadPlugins() {
  try {
    if (native) plugins.value = await invoke<PluginStatus[]>("list_console_plugins");
    else if (pluginPreview) {
      const disabled: unknown = JSON.parse(localStorage.getItem(previewKey) ?? "[]");
      if (Array.isArray(disabled)) plugins.value = defaults().map(p => disabled.includes(p.id) ? {...p, installed:false, usable:false, installedVersion:null} : p);
    }
    pluginError.value = "";
  } catch (error) { pluginError.value = String(error); }
}
export async function changePlugin(id: PluginId, action: PluginAction): Promise<boolean> {
  if (pluginBusy.value) return false;
  pluginBusy.value = id;
  pluginError.value = "";
  try {
    if (native) plugins.value = await invoke<PluginStatus[]>("change_console_plugin", {id, action});
    else if (pluginPreview && (action === "install" || action === "uninstall")) {
      const next = plugins.value.map(p => p.id === id ? {...p, installed:action === "install", usable:action === "install", installedVersion:action === "install" ? p.version : null} : p);
      localStorage.setItem(previewKey, JSON.stringify(next.filter(p => !p.installed).map(p => p.id)));
      plugins.value = next;
    } else throw new Error("请在 MollyCloud 桌面客户端中管理插件更新。");
    pluginRevision.value[id] = (pluginRevision.value[id] ?? 0) + 1;
    return true;
  } catch (error) { pluginError.value = String(error); return false; }
  finally { pluginBusy.value = ""; }
}
export async function checkPlugin(id: PluginId) {
  if (pluginBusy.value) return;
  pluginBusy.value = id;
  pluginError.value = "";
  try {
    if (!native) throw new Error("浏览器预览只演示安装状态；请在桌面客户端检查官方更新。");
    pluginChecks.value[id] = await invoke<PluginCheck>("check_console_plugin", {id});
  } catch (error) { pluginError.value = String(error); }
  finally { pluginBusy.value = ""; }
}
