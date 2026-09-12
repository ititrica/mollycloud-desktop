import type { MollyRenderer, MotionGroup } from "../live2d/renderer";

export type ActivityLevel = "quiet" | "normal" | "lively";

const ACTIVITY_DELAYS: Record<ActivityLevel, { blink: [number, number]; gesture: [number, number] }> = {
  quiet: { blink: [7_000, 12_000], gesture: [45_000, 80_000] },
  normal: { blink: [5_000, 9_000], gesture: [24_000, 46_000] },
  lively: { blink: [4_000, 7_000], gesture: [14_000, 28_000] },
};

/**
 * Small, model-agnostic behavior scheduler following Petra's separation between
 * rendering, autonomous behavior and UI. It deliberately excludes Petra's
 * operating-system and destructive assistant tools.
 */
export class PetraBehaviorEngine {
  private stopped = true;
  private blockedUntil = 0;
  private timers = new Set<number>();

  constructor(
    private readonly renderer: MollyRenderer,
    private activity: ActivityLevel = "normal",
  ) {}

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    this.scheduleBlink();
    this.scheduleGesture();
  }

  stop(): void {
    this.stopped = true;
    for (const timer of this.timers) window.clearTimeout(timer);
    this.timers.clear();
  }

  setActivity(activity: ActivityLevel): void {
    this.activity = activity;
  }

  suspend(milliseconds = 2_000): void {
    this.blockedUntil = Math.max(this.blockedUntil, Date.now() + milliseconds);
  }

  async playRequested(group: MotionGroup): Promise<void> {
    this.suspend(group === "Idle" ? 1_000 : 3_000);
    await this.renderer.play(group);
  }

  private scheduleBlink(): void {
    this.later(this.delay("blink"), async () => {
      await this.playAutomatic("Blink");
      this.scheduleBlink();
    });
  }

  private scheduleGesture(): void {
    this.later(this.delay("gesture"), async () => {
      await this.playAutomatic(Math.random() > 0.22 ? "Nod" : "Shake");
      this.scheduleGesture();
    });
  }

  private async playAutomatic(group: MotionGroup): Promise<void> {
    if (this.stopped || Date.now() < this.blockedUntil) return;
    this.blockedUntil = Date.now() + (group === "Blink" ? 1_200 : 2_500);
    try {
      await this.renderer.play(group);
    } catch {
      // A missing optional motion should never stop the desktop companion.
    }
  }

  private delay(kind: "blink" | "gesture"): number {
    const [min, max] = ACTIVITY_DELAYS[this.activity][kind];
    return min + Math.round(Math.random() * (max - min));
  }

  private later(delay: number, task: () => void | Promise<void>): void {
    const timer = window.setTimeout(() => {
      this.timers.delete(timer);
      if (!this.stopped) void task();
    }, delay);
    this.timers.add(timer);
  }
}
