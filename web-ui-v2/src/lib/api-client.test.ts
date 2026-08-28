import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  getJson,
  getAuthHeader,
  postJson,
  fetchDevicesCurrent,
  fetchLines,
  fetchAreas,
  fetchCells,
} from "./api-client";

describe("getAuthHeader", () => {
  it("returns an empty object today (no auth implemented yet)", () => {
    expect(getAuthHeader()).toEqual({});
  });
});

describe("getJson", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }))
    );
  });

  it("calls fetch with the auth header spread into request headers", async () => {
    await getJson("/api/tags/current");
    const [, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(init.headers).toMatchObject(getAuthHeader());
  });
});

describe("postJson", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }))
    );
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
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("{}", { status: 500 }))
    );
    await expect(postJson("/api/edges/reset", {})).rejects.toThrow(/500/);
  });
});

/** Forma completa de `DeviceCurrentDto` (crates/central-server/src/api.rs). El stub anterior era
 *  parcial: pasaba solo porque la respuesta se casteaba sin validar. */
const deviceCurrentPayload = {
  site_code: "plant-a",
  line_code: null,
  area_code: null,
  cell_code: null,
  edge_code: "edge-mix-1",
  device_code: "dev-1",
  connection_id: null,
  state: "connected",
  severity: "info",
  reason: null,
  tags_connected: 1,
  tags_stale: 0,
  tags_disconnected: 0,
  last_change_at: "2026-08-26T18:31:13Z",
  last_seen_at: "2026-08-26T18:31:13Z",
};

describe("fetchDevicesCurrent", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify([deviceCurrentPayload]), { status: 200 }))
    );
  });

  it("rechaza una respuesta que no cumple el contrato del backend", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify([{ device_code: "dev-1" }]), { status: 200 }))
    );
    await expect(fetchDevicesCurrent(10)).rejects.toThrow();
  });

  it("calls /api/devices/current with limit and filter", async () => {
    await fetchDevicesCurrent(50, { site: "plant-a", edge: "edge-mix-1" });
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/devices/current?");
    expect(url).toContain("limit=50");
    expect(url).toContain("site=plant-a");
    expect(url).toContain("edge=edge-mix-1");
  });
});

describe("fetchLines / fetchAreas / fetchCells", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify([{ code: "line-main", name: "Line Main" }]), { status: 200 })
      )
    );
  });

  it("fetchLines calls /api/context/lines with site", async () => {
    await fetchLines("plant-a");
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/context/lines?");
    expect(url).toContain("site=plant-a");
  });

  it("fetchAreas calls /api/context/areas with site and line", async () => {
    await fetchAreas("plant-a", "line-main");
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/context/areas?");
    expect(url).toContain("site=plant-a");
    expect(url).toContain("line=line-main");
  });

  it("fetchCells calls /api/context/cells with site, line, and area", async () => {
    await fetchCells("plant-a", "line-main", "area-pack");
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/context/cells?");
    expect(url).toContain("area=area-pack");
  });
});
