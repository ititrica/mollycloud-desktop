import allowed from "../allowed-commands.json";
type Internals = { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>; transformCallback(callback: (event: unknown) => void, once?: boolean): number; unregisterCallback(id: number): void };
type Host = Window & { __TAURI_INTERNALS__?: Internals; __TAURI_EVENT_PLUGIN_INTERNALS__?: { unregisterListener(event: string, id: number): void } };
export function native(): Internals | undefined {
  try { if (window.parent !== window && window.parent.location.origin === location.origin) return (window.parent as Host).__TAURI_INTERNALS__; } catch { /* unrelated origin */ }
}
export function isPreview(): boolean {
  try { return !native() && window.parent !== window && parent.location.origin === location.origin && new URLSearchParams(parent.location.search).get("ui-preview") === "console" && new URLSearchParams(location.search).get("ui-preview") === "skills"; } catch { return false; }
}
let initialization: Promise<unknown> | undefined;
export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!allowed.includes(command)) throw new Error("此操作由 Molly 控制台管理");
  const host = native();
  if (!host) {
    if (isPreview()) return preview(command, args) as T;
    throw new Error("请从 MollyCloud 桌面控制台打开 Skill 管理器");
  }
  if (!initialization) initialization = host.invoke("plugin:molly-skills|initialize").catch(error => { initialization = undefined; throw error; });
  await initialization;
  return host.invoke<T>(`plugin:molly-skills|${command}`, args);
}
function preview(command: string, args: Record<string, unknown>): unknown {
  if (["get_managed_skills", "get_projects", "get_presets", "get_all_tags", "get_skills_for_preset", "get_central_repo_warnings", "get_global_local_skills", "git_backup_list_versions", "git_backup_pending_conflicts", "get_tool_order_cmd"].includes(command)) return [];
  if (command === "get_active_preset") return null;
  if (command === "get_settings") return args.key === "language" ? "zh" : null;
  if (command === "log_startup_event") return undefined;
  if (command === "get_central_repo_path") return "桌面端首次打开后创建 Molly 私有技能库";
  if (command === "get_central_repo_path_override") return null;
  if (command === "get_tool_status") return ["Claude Code", "Codex", "OpenCode", "Cursor"].map((name, i) => ({key: ["claude_code", "codex", "opencode", "cursor"][i], display_name: name, installed: false, enabled: true, skills_dir: "桌面端检测实际目录", category: "coding", is_custom: false, has_path_override: false, project_relative_skills_dir: null, has_project_path_override: false}));
  if (command === "git_backup_status") return { is_repo: false, remote_url: null, branch: null, has_changes: false, changed_skill_count: 0, ahead: 0, behind: 0, last_commit: null, last_commit_time: null, current_snapshot_tag: null, restored_from_tag: null, upstream_health: "no_remote" };
  if (command === "backup_device_name") return "浏览器预览";
  throw new Error("浏览器仅提供界面预览，请在桌面端使用此功能");
}
const events = new Set(["app-files-changed", "tray-open-updates", "install-progress", "batch-import-progress", "backup-auto-completed", "skills-auto-updated", "git-backup-progress"]);
export async function listen<T>(event: string, handler: (event: { event: string; id: number; payload: T }) => void): Promise<() => void> {
  if (!events.has(event)) throw new Error("未授权的 Skill 事件");
  const host = native();
  if (!host) return () => {};
  const callback = host.transformCallback(handler as (event: unknown) => void);
  try {
    const id = await host.invoke<number>("plugin:event|listen", {event, target: {kind: "Any"}, handler: callback});
    return () => {
      (window.parent as Host).__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener(event, id);
      host.unregisterCallback(callback);
      void host.invoke("plugin:event|unlisten", {event, eventId: id});
    };
  } catch (error) { host.unregisterCallback(callback); throw error; }
}
export const open = (options: {directory?: boolean; multiple?: boolean; filters?: unknown; title?: string; defaultPath?: string} = {}): Promise<string | string[] | null> => invoke("pick_path", {directory: !!options.directory, multiple: !!options.multiple});
export const openUrl = (url: string): Promise<void> => invoke("open_external", {url});
export const writeText = (text: string): Promise<void> => invoke("copy_text", {text});
