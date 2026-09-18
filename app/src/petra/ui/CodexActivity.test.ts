import { describe, expect, it } from "vitest";
import { primaryActivity, progressLabel, type ActivityTask } from "./CodexActivity";

const task = (status: ActivityTask["status"], updatedAt = "2026-09-16T10:00:00Z"): ActivityTask => ({
  key: status, threadId: "thread", turnId: "turn", model: "", provider: "", status,
  unread: true, updatedAt,
});
describe("head capsule activity", () => {
  it("ignores terminal unread tasks and prefers input among active tasks", () => {
    expect(primaryActivity([task("blocked"), task("ready"), task("running")])?.status).toBe("running");
    expect(primaryActivity([task("running"), task("needs_input")])?.status).toBe("needs_input");
    expect(primaryActivity([task("blocked"), task("ready"), task("stopped")])).toBeUndefined();
  });
  it("uses the most recently updated running task and actual step labels", () => {
    const latest = { ...task("running", "2026-09-16T10:01:00Z"), progress: "command_complete" };
    expect(primaryActivity([task("running"), latest])).toBe(latest);
    expect(progressLabel(latest)).toBe("运行了命令");
    expect(progressLabel({ ...latest, progress: "retrying" })).toBe("正在重试");
    expect(progressLabel({ ...latest, progress: "thinking" })).toBe("正在思考");
    expect(progressLabel({ ...latest, progress: "future_event" })).toBe("正在进行任务");
    expect(progressLabel({ ...task("ready"), progress: "starting" })).toBe("任务已完成");
  });
  it("shows the original summary title only while thinking", () => {
    const summary = { ...task("running"), progress: "thinking", progressTitle: "Appending theme token CSS" };
    expect(progressLabel(summary)).toBe("Appending theme token CSS");
    expect(progressLabel({ ...summary, progressTitle: " " })).toBe("正在思考");
    expect(progressLabel({ ...summary, progress: "command_complete" })).toBe("运行了命令");
    expect(progressLabel({ ...summary, status: "stopped" })).toBe("任务已停止");
  });
});
