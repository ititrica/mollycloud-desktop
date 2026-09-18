import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => { vi.unstubAllGlobals(); vi.resetModules(); });
async function bridge(native?: Record<string, unknown>, sameOrigin = true, preview = false) {
  const parent = { location: { origin: sameOrigin ? "http://localhost" : "https://other.test", search: preview ? "?ui-preview=console" : "" }, __TAURI_INTERNALS__: native };
  const location = { origin: "http://localhost", search: preview ? "?ui-preview=skills" : "" };
  vi.stubGlobal("window", { parent, location });
  vi.stubGlobal("parent", parent);
  vi.stubGlobal("location", location);
  return import("../../vendor/skills-manager/frontend/src/embedded/bridge");
}

describe("Skill Manager host bridge", () => {
  it("initializes once before concurrent native operations", async () => {
    const invoke = vi.fn(async (command: string) => command.endsWith("initialize") ? undefined : []);
    const api = await bridge({ invoke });
    await Promise.all([api.invoke("get_managed_skills"), api.invoke("get_presets")]);
    expect(invoke.mock.calls.map(c => c[0])).toEqual(["plugin:molly-skills|initialize", "plugin:molly-skills|get_managed_skills", "plugin:molly-skills|get_presets"]);
  });

  it("retries initialization after a failure instead of reporting empty success", async () => {
    const invoke = vi.fn().mockRejectedValueOnce(new Error("DB locked")).mockResolvedValue(undefined);
    const api = await bridge({ invoke });
    await expect(api.invoke("get_managed_skills")).rejects.toThrow("DB locked");
    await api.invoke("get_managed_skills");
    expect(invoke).toHaveBeenCalledTimes(3);
  });

  it("rejects lifecycle and unrelated host commands without invoking native code", async () => {
    const invoke = vi.fn();
    const api = await bridge({ invoke });
    await expect(api.invoke("app_exit")).rejects.toThrow();
    await expect(api.invoke("fetch_dashboard")).rejects.toThrow();
    await expect(api.invoke("plugin:molly-ccswitch|delete_provider")).rejects.toThrow();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not use a different-origin parent's native bridge", async () => {
    const invoke = vi.fn();
    const api = await bridge({ invoke }, false);
    await expect(api.invoke("get_managed_skills")).rejects.toThrow("桌面控制台");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("preview does not simulate successful installation or removal", async () => {
    const api = await bridge(undefined, true, true);
    expect(await api.invoke("get_managed_skills")).toEqual([]);
    await expect(api.invoke("install_local", {sourcePath:"C:/test"})).rejects.toThrow("桌面端");
    await expect(api.invoke("delete_managed_skill", {skillId:"test"})).rejects.toThrow("桌面端");
  });

  it("event subscription failure releases the parent's callback", async () => {
    const unregisterCallback = vi.fn();
    const api = await bridge({ transformCallback: () => 17, unregisterCallback, invoke: vi.fn().mockRejectedValue(new Error("event unavailable")) });
    await expect(api.listen("app-files-changed", () => {})).rejects.toThrow("event unavailable");
    expect(unregisterCallback).toHaveBeenCalledWith(17);
    await expect(api.listen("petra-assistant-send", () => {})).rejects.toThrow("未授权");
  });
});
