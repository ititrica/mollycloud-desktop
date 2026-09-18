export type ActivityStatus = "running" | "needs_input" | "ready" | "blocked" | "stopped";
export interface ActivityStep { progress: string; at: string }
export interface ActivityTask {
  key: string;
  threadId: string;
  turnId: string;
  model: string;
  provider: string;
  status: ActivityStatus;
  unread: boolean;
  updatedAt: string;
  progress?: string;
  progressTitle?: string;
  steps?: ActivityStep[];
}
export interface ActivitySnapshot { tasks: ActivityTask[]; status: ActivityStatus | null; warning?: string | null }
const labels: Record<ActivityStatus, string> = { running: "正在进行任务", needs_input: "等待你的输入", ready: "任务已完成", blocked: "任务失败", stopped: "任务已停止" };
export const progressLabels: Record<string, string> = {
  starting: "正在开始任务", thinking: "正在思考", retrying: "正在重试",
  command_running: "正在运行命令", command_complete: "运行了命令", command_failed: "命令未成功",
  tool_running: "正在调用工具", tool_complete: "调用了工具", editing: "正在修改文件", edited: "修改了文件",
  searching: "正在搜索", searched: "完成了搜索", reading: "正在读取内容", read: "读取了内容",
  responding: "正在回复", compacting: "正在整理上下文", waiting_input: "等待你的输入",
  ready: "任务已完成", blocked: "任务失败", stopped: "任务已停止",
};
export function progressLabel(task: ActivityTask): string {
  if (task.status !== "running") return labels[task.status] ?? "状态未知";
  if (task.progress === "thinking" && task.progressTitle?.trim()) return task.progressTitle.trim();
  return progressLabels[task.progress ?? ""] ?? labels[task.status] ?? "正在进行任务";
}
export function primaryActivity(tasks: ActivityTask[]): ActivityTask | undefined {
  return tasks.filter(task => task.status === "running" || task.status === "needs_input")
    .sort((a, b) => Number(b.status === "needs_input") - Number(a.status === "needs_input")
      || b.updatedAt.localeCompare(a.updatedAt))[0];
}

export const COMPLETION_HOLD_MS = 20_000;

function taskInfo(task: ActivityTask) {
  // Explicit projection: only the same labels and metadata available in the pet UI.
  return {
    threadId: task.threadId, turnId: task.turnId, status: task.status,
    text: progressLabel(task), model: task.model, provider: task.provider,
    updatedAt: task.updatedAt,
    steps: (task.steps ?? []).slice(-8).map(step => ({
      text: progressLabels[step.progress] ?? "任务步骤已更新", at: step.at,
    })),
  };
}

/** One display state for the capsule and Molly's read-only tool. */
export function createActivityDisplay(expired: () => void) {
  let snapshot: ActivitySnapshot = { tasks: [], status: null };
  let displayed: ActivityTask | undefined;
  let completionExpiresAt: number | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let previous = new Map<string, ActivityStatus>();
  let initialized = false;
  const startedAt = Date.now();

  const clearHold = () => {
    clearTimeout(timer);
    timer = undefined;
    completionExpiresAt = null;
  };
  return {
    get task() { return displayed; },
    get info() {
      return {
        status: displayed?.status ?? "idle",
        text: displayed ? progressLabel(displayed) : "",
        completionExpiresAt: completionExpiresAt == null ? null : new Date(completionExpiresAt).toISOString(),
        task: displayed ? taskInfo(displayed) : null,
        tasks: snapshot.tasks.map(taskInfo), warning: snapshot.warning ?? null,
      };
    },
    update(next: ActivitySnapshot) {
      const now = Date.now();
      const active = primaryActivity(next.tasks);
      // Ignore unread history at startup and duplicate snapshots. A quick task
      // may start and finish between watcher batches, so accept a fresh terminal
      // event even if its running snapshot was not observed.
      const completed = initialized ? next.tasks.filter(task => {
        if (task.status !== "ready" || previous.get(task.key) === "ready") return false;
        const wasActive = ["running", "needs_input"].includes(previous.get(task.key) ?? "");
        const at = Date.parse(task.updatedAt);
        return wasActive || (at > startedAt && at <= now && now - at < COMPLETION_HOLD_MS);
      }).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))[0] : undefined;
      // Retain terminal identities when unread entries disappear, so opening a
      // task or a repeated watcher delivery cannot restart its completion timer.
      for (const task of next.tasks) previous.set(task.key, task.status);
      if (previous.size > 1024) previous = new Map([...previous].slice(-512));
      initialized = true;
      snapshot = next;
      if (active) {
        clearHold();
        displayed = active;
      } else if (completed) {
        clearHold();
        displayed = completed;
        completionExpiresAt = now + COMPLETION_HOLD_MS;
        timer = setTimeout(() => {
          clearHold();
          displayed = undefined;
          expired();
        }, COMPLETION_HOLD_MS);
      } else if (completionExpiresAt == null || next.tasks.some(task =>
        task.key === displayed?.key && task.status !== "ready")) {
        clearHold();
        displayed = undefined;
      }
    },
  };
}

export type ActivityDisplayInfo = ReturnType<typeof createActivityDisplay>["info"];
