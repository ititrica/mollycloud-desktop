import { afterEach, describe, expect, it, vi } from "vitest";
import { getVersion } from "./app";
import { invoke, isTauri } from "./core";
import { listen, once } from "./event";
import { canPreview, getParentNativeBridge } from "./native-bridge";
import { installReadOnlyPreview } from "./preview";
import { mockEmbeddedWindow } from "./test-window";

afterEach(() => { vi.unstubAllGlobals(); });

describe("immutable native iframe bridges", () => {
  it("uses the parent's IPC without changing injected readonly child globals", async () => {
    const fixture = mockEmbeddedWindow();
    const before = Object.getOwnPropertyDescriptors(fixture.child);
    expect(isTauri()).toBe(true);
    await invoke("get_providers", { app: "codex" });
    expect(fixture.parentInternals.invoke).toHaveBeenCalledWith("plugin:molly-ccswitch|get_providers", { app: "codex" }, undefined);
    expect(fixture.childInternals.invoke).not.toHaveBeenCalled();
    expect(Object.getOwnPropertyDescriptors(fixture.child)).toEqual(before);
    expect(before.__TAURI_INTERNALS__?.writable).toBe(false);
    expect(before.__TAURI_INTERNALS__?.configurable).toBe(false);
    expect(await getVersion()).toBe("3.20.3");
  });

  it("delivers events through the parent's registry and unregisters exactly once", async () => {
    const fixture = mockEmbeddedWindow();
    const before = Object.getOwnPropertyDescriptors(fixture.child);
    const handler = vi.fn();
    const off = await listen("provider-changed", handler, { target: "console" });
    const callbackId = fixture.parentInternals.transformCallback.mock.results[0].value as number;
    const callback = fixture.callbacks.get(callbackId)!;
    expect(fixture.parentInternals.invoke).toHaveBeenCalledWith("plugin:event|listen", {
      event: "provider-changed", target: { kind: "AnyLabel", label: "console" }, handler: callbackId,
    });
    const data = { event: "provider-changed", id: 42, payload: { app: "codex" } };
    callback(data);
    expect(handler).toHaveBeenCalledWith(data);
    await off();
    await off();
    callback(data);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(fixture.parentEvents.unregisterListener).toHaveBeenCalledTimes(1);
    expect(fixture.parentEvents.unregisterListener).toHaveBeenCalledWith("provider-changed", 42);
    expect(fixture.parentInternals.unregisterCallback).toHaveBeenCalledWith(callbackId);
    expect(fixture.callbacks.size).toBe(0);
    expect(fixture.childInternals.transformCallback).not.toHaveBeenCalled();
    expect(fixture.childEvents.unregisterListener).not.toHaveBeenCalled();
    expect(Object.getOwnPropertyDescriptors(fixture.child)).toEqual(before);
  });

  it("cleans the parent callback if native subscription fails", async () => {
    const fixture = mockEmbeddedWindow({ invoke: vi.fn().mockRejectedValue(new Error("listen denied")) });
    await expect(listen("provider-changed", vi.fn())).rejects.toThrow("listen denied");
    expect(fixture.callbacks.size).toBe(0);
    expect(fixture.parentInternals.unregisterCallback).toHaveBeenCalledTimes(1);
  });

  it("handles once subscriptions without duplicate delivery or cleanup", async () => {
    const fixture = mockEmbeddedWindow();
    const handler = vi.fn();
    const off = await once("provider-changed", handler);
    const callback = [...fixture.callbacks.values()][0];
    callback({ id: 42, event: "provider-changed", payload: null });
    callback({ id: 42, event: "provider-changed", payload: null });
    await off();
    expect(handler).toHaveBeenCalledTimes(1);
    expect(fixture.parentEvents.unregisterListener).toHaveBeenCalledTimes(1);
    expect(fixture.callbacks.size).toBe(0);
  });

  it("rejects foreign parents and never falls back to a child's injected IPC", async () => {
    const fixture = mockEmbeddedWindow();
    fixture.parent.location.origin = "https://foreign.invalid";
    expect(getParentNativeBridge()).toBeUndefined();
    await expect(invoke("get_providers")).rejects.toThrow("桌面客户端");
    expect(fixture.childInternals.invoke).not.toHaveBeenCalled();
    expect(fixture.parentInternals.invoke).not.toHaveBeenCalled();
  });

  it("keeps explicitly enabled browser preview module-local and rejects all writes", async () => {
    const fixture = mockEmbeddedWindow(undefined, false);
    const before = Object.getOwnPropertyDescriptors(fixture.child);
    expect(canPreview()).toBe(true);
    installReadOnlyPreview();
    expect(await invoke("get_providers", { app: "codex" })).toHaveProperty("molly-preview-provider");
    await expect(invoke("add_provider", {})).rejects.toThrow("界面预览不执行");
    const off = await listen("provider-changed", vi.fn());
    await off();
    expect(Object.getOwnPropertyDescriptors(fixture.child)).toEqual(before);
    fixture.parent.location.search = "";
    await expect(invoke("get_providers", { app: "codex" })).rejects.toThrow("桌面客户端");
  });

  it("refuses preview in a native WebView even when preview queries are present", () => {
    mockEmbeddedWindow();
    expect(canPreview()).toBe(false);
    expect(() => installReadOnlyPreview()).toThrow("预览模式");
  });
});
