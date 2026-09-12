import { describe, expect, it } from "vitest";
import { formatOverlayBalance, formatTodayTokens } from "./BalancePill";

describe("overlay account display", () => {
  it("uses a dollar suffix and keeps real zero and negative balances", () => {
    expect(formatOverlayBalance(3.72)).toBe("3.72$");
    expect(formatOverlayBalance("0")).toBe("0.00$");
    expect(formatOverlayBalance(-1.25)).toBe("-1.25$");
  });

  it("does not turn unavailable balances into zero", () => {
    for (const missing of [null, undefined, "", " ", false, NaN, Infinity, "unavailable"]) {
      expect(formatOverlayBalance(missing)).toBeNull();
    }
  });

  it("distinguishes unavailable usage from actual zero and preserves the full count", () => {
    expect(formatTodayTokens(0)).toBe("0 token");
    expect(formatTodayTokens("128400")).toBe("128,400 token");
    expect(formatTodayTokens(987654321)).toBe("987,654,321 token");
    for (const missing of [null, undefined, "", false, -1, Infinity]) {
      expect(formatTodayTokens(missing)).toBe("暂无法获取今日用量");
    }
  });
});
