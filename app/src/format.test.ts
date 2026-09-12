import { describe, expect, it } from "vitest";
import { formatTokens, numberValue, textValue } from "./format";

describe("display formatters", () => {
  it("formats compact token counts", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1_250)).toBe("1.3K");
    expect(formatTokens(2_500_000)).toBe("2.50M");
  });

  it("keeps invalid API values out of the UI", () => {
    expect(numberValue("not-a-number")).toBe(0);
    expect(textValue(null)).toBe("—");
  });
});
