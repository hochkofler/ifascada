import { describe, it, expect, vi } from "vitest";
import { resetEdge, type ResetEdgeRequest, type ResetEdgeResponse } from "./edge-actions";
import { postJson } from "./api-client";

// Regression guard for the finding that resetEdge used a raw `fetch("/api/edges/reset", ...)`,
// bypassing api-client.ts's single auth-injection point (request()/getAuthHeader()). Mocking
// the whole "./api-client" module (rather than spying on getAuthHeader, which wouldn't affect
// api-client.ts's own internal calls under ESM live bindings) proves resetEdge is wired through
// postJson -- not a second, independent HTTP layer that would silently skip auth headers once
// real auth lands.
vi.mock("./api-client", () => ({ postJson: vi.fn() }));

describe("resetEdge", () => {
  it("routes through api-client's postJson instead of a raw fetch, preserving request/response shape", async () => {
    const req: ResetEdgeRequest = { site_code: "plant-a", edge_code: "edge-1", reason: "manual reset from diagnostics panel" };
    const expected: ResetEdgeResponse = {
      accepted: true,
      request_id: "r1",
    };
    vi.mocked(postJson).mockResolvedValue(expected);

    const result = await resetEdge(req);

    expect(postJson).toHaveBeenCalledTimes(1);
    expect(postJson).toHaveBeenCalledWith("/api/edges/reset", req);
    expect(result).toBe(expected);
  });

  it("propagates a rejection from postJson (e.g. a non-ok response) rather than swallowing it", async () => {
    vi.mocked(postJson).mockRejectedValue(new Error("POST /api/edges/reset failed: 500"));

    await expect(resetEdge({ site_code: "plant-a", edge_code: "edge-1" })).rejects.toThrow(/500/);
  });
});
