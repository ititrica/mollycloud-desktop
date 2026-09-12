import { describe, expect, it } from "vitest";
import { dashboardPayloadSchema, serviceBootstrapSchema } from "./contracts";

describe("sub2api contracts", () => {
  it("accepts the public service bootstrap shape", () => {
    const parsed = serviceBootstrapSchema.parse({
      service_origin: "https://mollycloud.cn",
      health: { status: "ok" },
      settings: { site_name: "MollyCloud" },
    });
    expect(parsed.health.status).toBe("ok");
  });

  it("rejects an incomplete dashboard response", () => {
    expect(() => dashboardPayloadSchema.parse({ user: {} })).toThrow();
  });
});
