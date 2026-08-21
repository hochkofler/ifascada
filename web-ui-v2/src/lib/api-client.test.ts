import { describe, it, expect, vi, beforeEach } from "vitest";
import { getJson, getAuthHeader } from "./api-client";

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
