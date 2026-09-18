import { createActivityDisplay, progressLabel, type ActivitySnapshot } from "./CodexActivityState";
export { primaryActivity, progressLabel } from "./CodexActivityState";
export type { ActivityStatus, ActivityStep, ActivityTask, ActivitySnapshot } from "./CodexActivityState";

export function createCodexActivity(root: HTMLElement, dropdown: HTMLElement, changed: () => void) {
  const progress = document.createElement("span");
  progress.className = "balance-pill__progress"; progress.hidden = true;
  progress.setAttribute("aria-live", "polite");
  root.querySelector("button")!.append(progress);
  dropdown.setAttribute("aria-label", "余额与今日 Token 消耗");
  const sync = () => {
    const active = display.task;
    root.classList.toggle("balance-pill--capsule", Boolean(active));
    root.dataset.activity = active?.status ?? "idle";
    progress.hidden = !active;
    const text = active ? progressLabel(active) : "";
    if (progress.textContent !== text) progress.textContent = text;
    changed();
  };
  const display = createActivityDisplay(sync);
  return {
    get hasTasks() { return Boolean(display.task); },
    get info() { return display.info; },
    get isActive() { return Boolean(display.task); },
    get description() { return display.task ? `，${progressLabel(display.task)}` : ""; },
    update(next: ActivitySnapshot | null) {
      if (!next || !Array.isArray(next.tasks)) return;
      display.update(next);
      sync();
    },
  };
}
