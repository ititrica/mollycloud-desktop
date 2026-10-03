import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "./core";
import { mockEmbeddedWindow } from "./test-window";

afterEach(() => { vi.unstubAllGlobals(); });
describe("embedded command boundaries", () => {
  it("routes only the explicit account-key operations through the host", async () => {
    const native=vi.fn().mockResolvedValue(true);mockEmbeddedWindow({invoke:native});
    await invoke('sync_molly_key_providers',{agent:'codex'});
    await invoke('copy_api_key',{keyId:'42'});
    expect(native).toHaveBeenNthCalledWith(1,'sync_molly_key_providers',{agent:'codex'});
    expect(native).toHaveBeenNthCalledWith(2,'copy_api_key',{keyId:'42'});
  });
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
    await invoke("open_external", { url: "https://mollycloud.cn/v1" });
    expect(native).toHaveBeenLastCalledWith("open_url", { url: "https://mollycloud.cn/" });
    native.mockClear();
    for (const url of ["ccswitch://v1/import", "file:///secret.txt", "javascript:alert(1)"]) {
      await expect(invoke("open_external", { url })).rejects.toThrow("HTTP 或 HTTPS");
    }
    expect(native).not.toHaveBeenCalled();
  });
  it("routes Molly account balance through the host and keeps upstream usage for other cards", async () => {
    const native = vi.fn().mockResolvedValueOnce({ success: true, data: [{ remaining: 12, unit: "USD" }] })
      .mockResolvedValueOnce(null).mockResolvedValueOnce({ success: true });
    mockEmbeddedWindow({ invoke: native });
    expect(await invoke("queryProviderUsage", { providerId: "molly-imported", app: "codex" })).toMatchObject({ success: true });
    expect(native).toHaveBeenNthCalledWith(1, "query_molly_provider_usage", { agent: "codex", providerId: "molly-imported" });
    await invoke("queryProviderUsage", { providerId: "other", app: "codex" });
    expect(native).toHaveBeenNthCalledWith(2, "plugin:molly-ccswitch|queryProviderUsage", { providerId: "other", app: "codex" }, undefined);
  });
  it("tests imported Molly balance through the host without executing an upstream script", async () => {
    const native = vi.fn().mockResolvedValue({ success: true, data: [{ remaining: 0, unit: "USD" }] });
    mockEmbeddedWindow({ invoke: native });
    expect(await invoke("testUsageScript", { providerId: "molly-example", app: "mcode", scriptCode: "invalid" })).toMatchObject({ success: true });
    expect(native).toHaveBeenCalledTimes(1);
    expect(native).toHaveBeenCalledWith("query_molly_provider_usage", { agent: "mcode", providerId: "molly-example" });
  });
  it("keeps external-import registration in the host and parsing in the audited plugin", async () => {
    const native = vi.fn().mockResolvedValue(null);
    mockEmbeddedWindow({ invoke: native });
    await invoke("ccswitch_web_import_handler");
    await invoke("register_ccswitch_web_import");
    await invoke("unregister_ccswitch_web_import");
    await invoke("parse_deeplink", { url: "ccswitch://v1/import?resource=provider&app=codex&name=Mock" });
    expect(native.mock.calls.map(([name]) => name)).toEqual([
      "ccswitch_web_import_handler", "register_ccswitch_web_import", "unregister_ccswitch_web_import", "plugin:molly-ccswitch|parse_deeplink",
    ]);
  });
});
