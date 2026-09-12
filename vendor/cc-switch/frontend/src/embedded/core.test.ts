import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "./core";
import { mockEmbeddedWindow } from "./test-window";

afterEach(() => { vi.unstubAllGlobals(); });
describe("embedded command boundaries", () => {
  it("namespaces upstream commands and retains native plugin names", async () => {
    const native = vi.fn().mockResolvedValue(true);
    mockEmbeddedWindow({ invoke: native });
    await invoke("switch_provider", { id: "provider-id", app: "codex" });
    await invoke("plugin:dialog|open", {});
    expect(native).toHaveBeenNthCalledWith(1, "plugin:molly-ccswitch|switch_provider", { id: "provider-id", app: "codex" }, undefined);
    expect(native).toHaveBeenNthCalledWith(2, "plugin:dialog|open", {}, undefined);
  });
  it("launches a private CLI by provider ID via the host command", async () => {
    const native = vi.fn().mockResolvedValue(true);
    mockEmbeddedWindow({ invoke: native });
    await invoke("open_provider_terminal", { providerId: "provider-id", app: "codex" });
    expect(native).toHaveBeenCalledWith("launch_ccswitch_cli", { appType: "codex", providerId: "provider-id", cwd: undefined });
  });
  it("does not invent success without a desktop bridge", async () => {
    await expect(invoke("add_provider", {})).rejects.toThrow("桌面客户端");
  });
  it("rejects native process, updater and window lifecycle operations", async () => {
    const native = vi.fn();
    mockEmbeddedWindow({ invoke: native });
    for (const command of ["plugin:process|exit", "plugin:updater|check", "plugin:window|close"]) {
      await expect(invoke(command)).rejects.toThrow("不能更新、退出");
    }
    expect(native).not.toHaveBeenCalled();
  });
  it("uses the host web opener and rejects system deep links", async () => {
    const native = vi.fn().mockResolvedValue(undefined);
    mockEmbeddedWindow({ invoke: native });
    await invoke("open_external", { url: "https://mollycloud.cn" });
    expect(native).toHaveBeenCalledWith("open_url", { url: "https://mollycloud.cn/" });
    native.mockClear();
    for (const url of ["ccswitch://v1/import", "file:///secret.txt", "javascript:alert(1)"]) {
      await expect(invoke("open_external", { url })).rejects.toThrow("HTTP 或 HTTPS");
    }
    expect(native).not.toHaveBeenCalled();
  });
});
