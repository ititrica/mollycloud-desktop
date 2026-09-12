import { describe, expect, it } from "vitest";
import { petDraftSchema } from "./petSettings";

const draft = { model: { type: "manifest", name: "Molly.psd" }, modelScale: 1, boundsPadding: { left: 0, right: 0, top: 0, bottom: 0 }, debugBorder: false, debugModelBounds: false, params: { irisScale: 1 }, auto: { autoBlink: true }, assistant: { enabled: true, provider: "custom", model: "test-model", persona: "", customBaseUrl: "https://example.com/v1", greetInterval: 20 } };
describe("settings received from the console", () => {
  it("accepts valid settings and rejects invalid model ranges", () => {
    expect(petDraftSchema.safeParse(draft).success).toBe(true);
    expect(petDraftSchema.safeParse({ ...draft, modelScale: 5 }).success).toBe(false);
    expect(petDraftSchema.safeParse({ ...draft, params: { irisScale: 99 } }).success).toBe(false);
    expect(petDraftSchema.safeParse({ ...draft, params: { unknown: 1 } }).success).toBe(false);
    expect(petDraftSchema.safeParse({ ...draft, auto: { unknown: true } }).success).toBe(false);
  });
  it("rejects remote HTTP and credentials in endpoints, allows local services", () => {
    for (const customBaseUrl of ["http://example.com", "https://user:pass@example.com", "https://example.com?key=test"]) {
      expect(petDraftSchema.safeParse({ ...draft, assistant: { ...draft.assistant, customBaseUrl } }).success).toBe(false);
    }
    expect(petDraftSchema.safeParse({ ...draft, assistant: { ...draft.assistant, customBaseUrl: "http://127.0.0.1:11434/v1" } }).success).toBe(true);
  });
  it("never includes an API key in a settings snapshot", () => {
    const parsed = petDraftSchema.parse({ ...draft, apiKey: "synthetic", assistant: { ...draft.assistant, apiKey: "synthetic" } });
    expect(JSON.stringify(parsed)).not.toContain("synthetic");
  });
});
