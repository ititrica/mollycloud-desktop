import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createActivityDisplay, type ActivityStatus, type ActivityTask } from "./CodexActivityState";
import { readFloatingWindowTool, registerFloatingWindowReader } from "../assistant/FloatingWindowInfo";

const task = (status: ActivityStatus, key = "thread:turn"): ActivityTask => ({
  key, threadId: "thread", turnId: key, status, unread: true,
  model: "test-model", provider: "test", updatedAt: new Date().toISOString(),
});
const snapshot = (...tasks: ActivityTask[]) => ({ tasks, status: tasks[0]?.status ?? null });

describe("capsule completion lifecycle", () => {
  beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(new Date("2026-09-16T10:00:00Z")); });
  afterEach(() => vi.useRealTimers());

  it("shows resumed progress immediately after an unanswered question in the capsule and Molly", () => {
    const display = createActivityDisplay(vi.fn());
    display.update(snapshot({ ...task("needs_input"), progress: "waiting_input" }));
    expect(display.info.text).toBe("等待你的输入");
    vi.advanceTimersByTime(120_000);
    for (const [progress, title, expected] of [
      ["thinking", "Continuing the task", "Continuing the task"],
      ["retrying", "", "正在重试"],
      ["command_complete", "", "运行了命令"],
    ]) {
      display.update(snapshot({ ...task("running"), progress, progressTitle: title }));
      expect(display.task?.status).toBe("running");
      expect(display.info.text).toBe(expected);
      expect(display.info.task?.text).toBe(expected);
      expect(display.info.completionExpiresAt).toBeNull();
    }
  });

  it("holds completion for 20 seconds without extending on duplicate or removed unread entries", () => {
    const expired = vi.fn();
    const display = createActivityDisplay(expired);
    display.update(snapshot(task("running")));
    display.update(snapshot(task("ready")));
    expect(display.info.text).toBe("任务已完成");
    const deadline = display.info.completionExpiresAt;
    vi.advanceTimersByTime(10_000);
    display.update(snapshot(task("ready")));
    display.update(snapshot());
    display.update(snapshot(task("ready")));
    expect(display.info.completionExpiresAt).toBe(deadline);
    vi.advanceTimersByTime(9_999);
    expect(display.task?.status).toBe("ready");
    vi.advanceTimersByTime(1);
    expect(display.task).toBeUndefined();
    expect(display.info.completionExpiresAt).toBeNull();
    expect(expired).toHaveBeenCalledOnce();
    display.update(snapshot(task("ready")));
    expect(display.task).toBeUndefined();
  });

  it("replaces completion immediately and cancels its timeout when another task starts", () => {
    const expired = vi.fn();
    const display = createActivityDisplay(expired);
    display.update(snapshot(task("running")));
    display.update(snapshot(task("ready")));
    vi.advanceTimersByTime(5_000);
    const next = { ...task("running", "next"), progress: "thinking", progressTitle: "Appending theme token CSS" };
    display.update(snapshot(next));
    expect(display.info.text).toBe(next.progressTitle);
    expect(display.info.completionExpiresAt).toBeNull();
    vi.advanceTimersByTime(20_000);
    expect(display.task).toBe(next);
    expect(expired).not.toHaveBeenCalled();
    display.update(snapshot(task("stopped", "next")));
    expect(display.task).toBeUndefined();
  });

  it("ignores startup and old history but captures quick tasks between watcher snapshots", () => {
    const display = createActivityDisplay(vi.fn());
    const historical = task("ready", "old");
    display.update(snapshot(historical));
    expect(display.task).toBeUndefined();
    vi.advanceTimersByTime(1_000);
    display.update(snapshot(historical, { ...historical, key: "older" }));
    expect(display.task).toBeUndefined();
    display.update(snapshot(historical, task("ready", "quick")));
    expect(display.task?.key).toBe("quick");
  });

  it("keeps another active task ahead of completion and never replays the old hold", () => {
    const display = createActivityDisplay(vi.fn());
    display.update(snapshot(task("running"), task("needs_input", "other")));
    display.update(snapshot(task("ready"), task("needs_input", "other")));
    expect(display.task?.status).toBe("needs_input");
    display.update(snapshot(task("ready"), task("blocked", "other")));
    expect(display.task).toBeUndefined();
  });

  it("shares current display and expiry with Molly without passing extra event fields", () => {
    const display = createActivityDisplay(vi.fn());
    registerFloatingWindowReader(() => ({
      mode: display.task ? "capsule" : "orb",
      balance: { compact: "128$", full: "128.90$", low: false },
      todayTokens: "128,400 token", activity: display.info,
    }));
    display.update(snapshot({ ...task("running"), progress: "thinking", progressTitle: "Appending theme token CSS",
      ...{ command: "private command", output: "private output", apiKey: "private key" },
      steps: [{ progress: "command_complete", at: new Date().toISOString() }],
    }));
    const response = () => JSON.parse(readFloatingWindowTool()).data;
    expect(response().activity.text).toBe("Appending theme token CSS");
    expect(response().activity.task.steps[0].text).toBe("运行了命令");
    expect(readFloatingWindowTool()).not.toContain("private");
    display.update(snapshot(task("ready")));
    expect(response().activity.text).toBe("任务已完成");
    expect(response().activity.completionExpiresAt).toBe(display.info.completionExpiresAt);
    vi.advanceTimersByTime(20_000);
    expect(response().mode).toBe("orb");
    expect(response().activity.status).toBe("idle");
    expect(response().activity.tasks[0].status).toBe("ready");
  });
});
