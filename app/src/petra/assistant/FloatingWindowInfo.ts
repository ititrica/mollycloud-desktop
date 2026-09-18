import type { ActivityDisplayInfo } from "../ui/CodexActivityState";

interface FloatingWindowInfo {
  mode: "capsule" | "orb";
  balance: { compact: string; full: string; low: boolean };
  todayTokens: string;
  activity: ActivityDisplayInfo;
}

let reader: (() => FloatingWindowInfo) | undefined;

/** Registered by the pet window; console chat uses this same assistant runtime. */
export function registerFloatingWindowReader(read: () => FloatingWindowInfo) {
  reader = read;
}

export const floatingWindowTool = {
  type: "function",
  function: {
    name: "get_floating_window_info",
    description: "只读获取 Molly 悬浮球/头顶胶囊当前显示的余额、今日 Token、Codex 任务进度原文、完成提示及保留截止时间，以及任务列表中的状态和最近步骤。数据来自悬浮窗，无需登录即可读取任务状态。",
    parameters: { type: "object", properties: {}, additionalProperties: false },
  },
};

export function readFloatingWindowTool(): string {
  return JSON.stringify(reader
    ? { ok: true, data: reader() }
    : { ok: false, error: "悬浮窗尚未初始化，暂时无法读取显示信息。" });
}
