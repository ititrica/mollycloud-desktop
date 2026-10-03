import { describe, expect, it, vi } from "vitest";
import { listenToHost } from "./bridge";

describe("embedded parent messages", () => {
  it("accepts only the three host-managed views", () => {
    const receive = vi.fn(); const off = listenToHost(receive);
    for (const view of ["skills", "settings", "bogus", null, 42]) {
      window.dispatchEvent(new MessageEvent("message", {data:{source:"mollycloud",type:"view",view},origin:window.location.origin,source:window.parent}));
    }
    expect(receive).not.toHaveBeenCalled();
    for (const view of ["providers", "prompts", "mcp"]) {
      window.dispatchEvent(new MessageEvent("message", {data:{source:"mollycloud",type:"view",view},origin:window.location.origin,source:window.parent}));
    }
    expect(receive).toHaveBeenCalledTimes(3); off();
  });
  it("requires both the exact parent and the same origin", () => {
    const receive = vi.fn();
    const off = listenToHost(receive);
    const data = { source: "mollycloud", type: "provider-imported", app: "codex", providerId: "provider-id" };
    window.dispatchEvent(new MessageEvent("message", { data, origin: "https://foreign.invalid", source: window.parent }));
    window.dispatchEvent(new MessageEvent("message", { data, origin: window.location.origin, source: null }));
    window.dispatchEvent(new MessageEvent("message", { data: { ...data, providerId: "x".repeat(257) }, origin: window.location.origin, source: window.parent }));
    expect(receive).not.toHaveBeenCalled();
    window.dispatchEvent(new MessageEvent("message", { data, origin: window.location.origin, source: window.parent }));
    expect(receive).toHaveBeenCalledTimes(1);
    expect(receive).toHaveBeenCalledWith(data);
    off();
    window.dispatchEvent(new MessageEvent("message", { data, origin: window.location.origin, source: window.parent }));
    expect(receive).toHaveBeenCalledTimes(1);
  });
});
