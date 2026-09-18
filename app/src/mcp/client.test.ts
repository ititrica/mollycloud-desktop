import { describe, expect, it, vi, afterEach } from "vitest";
import { directory, filterCatalog, marketApi, parseCustom, readRegistryCache, templates } from "./client";

afterEach(() => vi.unstubAllGlobals());
describe("MCP installation boundary", () => {
  it("filters by multiple words and category including package names", () => {
    expect(filterCatalog(templates, "modelcontextprotocol filesystem", "文件").map(e => e.id)).toEqual(["filesystem"]);
    expect(filterCatalog(templates, "filesystem", "搜索")).toEqual([]);
    expect(directory.every(e => e.recipe === null && e.fields.length === 0)).toBe(true);
  });
  it("accepts a single remote connection and rejects a whole Agent config", () => {
    expect(parseCustom('{"url":"https://example.test/mcp"}').type).toBe("http");
    for (const value of ['[]', 'null', '{"mcpServers":{}}', '{"mcp":{}}', '{']) expect(() => parseCustom(value)).toThrow();
  });
  it("never accepts executable recipes from cached search data", () => {
    const entry = { id:"registry/test", name:"Test", description:"Test", category:"官方目录", homepage:"https://example.test", source:"registry", recipe:{kind:"npm",package:"untrusted"}, fields:[{key:"secret"}] };
    expect(readRegistryCache([null, {}, {...entry,homepage:"javascript:alert(1)"}, entry])).toEqual([{id:entry.id,name:entry.name,description:entry.description,category:entry.category,homepage:entry.homepage,source:entry.source,recipe:null,fields:[]}]);
  });
  it("never reports successful local operations from browser preview", async () => {
    vi.stubGlobal("window", {});
    expect((await marketApi.status()).installations).toEqual([]);
    await expect(marketApi.install({id:"test",agents:["codex"],values:{}})).rejects.toThrow("桌面端");
    await expect(marketApi.action("codex:test","remove")).rejects.toThrow("桌面端");
    await expect(marketApi.search("test")).rejects.toThrow("桌面端");
    await expect(marketApi.resolve("awesome", "test")).rejects.toThrow("桌面端");
    await expect(marketApi.cleanup("test", "1.0.0")).rejects.toThrow("桌面端");
  });
});
