import { describe, it, expect, vi, beforeEach } from "vitest";
import { getJson, getAuthHeader, postJson } from "./api-client";

describe("getAuthHeader", () => {
  it("returns an empty object today (no auth implemented yet)", () => {
    expect(getAuthHeader()).toEqual({});
  });
});

describe("getJson", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 })));
  });

  it("calls fetch with the auth header spread into request headers", async () => {
    await getJson("/api/tags/current");
    const [, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(init.headers).toMatchObject(getAuthHeader());
  });
});

describe("postJson", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 })));
  });

  it("POSTs a JSON body with the auth header spread into request headers", async () => {
    await postJson("/api/edges/reset", { site_code: "plant-a", edge_code: "edge-1" });
    const [path, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(path).toBe("/api/edges/reset");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({ site_code: "plant-a", edge_code: "edge-1" });
    expect(init.headers).toMatchObject(getAuthHeader());
  });

  it("throws when the response is not ok, same as getJson", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 500 })));
    await expect(postJson("/api/edges/reset", {})).rejects.toThrow(/500/);
  });
});
