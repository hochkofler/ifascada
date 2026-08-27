import { describe, it, expect } from "vitest";
import { edgeConnected, lampFromDeviceState, EDGE_STALE_AFTER_SECS, ONLINE_STATUSES } from "./connectivity";
import type { EdgeCurrent, DeviceCurrent } from "./api-client";

const NOW = new Date("2026-08-26T18:00:00Z").getTime();

function edge(status: string, secsAgo: number): EdgeCurrent {
  const lastSeen = new Date(NOW - secsAgo * 1000).toISOString();
  return { site_code: "plant-a", line_code: null, area_code: null, cell_code: null, edge_code: "e1", status, last_seen_at: lastSeen, outbox_depth: 0, outbox_oldest_secs: null, action_metrics: {} };
}

describe("ONLINE_STATUSES / EDGE_STALE_AFTER_SECS", () => {
  it("recognizes both real backend literals", () => {
    expect(ONLINE_STATUSES.has("online")).toBe(true);
    expect(ONLINE_STATUSES.has("ok")).toBe(true);
  });
  it("is 45 seconds", () => {
    expect(EDGE_STALE_AFTER_SECS).toBe(45);
  });
});

describe("edgeConnected", () => {
  it("returns false for undefined edge", () => {
    expect(edgeConnected(undefined, NOW)).toBe(false);
  });
  it("returns true for status='online', fresh last_seen_at", () => {
    expect(edgeConnected(edge("online", 10), NOW)).toBe(true);
  });
  it("returns true for status='ok', fresh last_seen_at (the badge-bug fix)", () => {
    expect(edgeConnected(edge("ok", 10), NOW)).toBe(true);
  });
  it("returns false for status='disconnected' even if last_seen_at is fresh", () => {
    expect(edgeConnected(edge("disconnected", 1), NOW)).toBe(false);
  });
  it("returns false when last_seen_at is older than 45s even if status looks online", () => {
    expect(edgeConnected(edge("online", 46), NOW)).toBe(false);
  });
  it("returns true exactly at the 45s boundary", () => {
    expect(edgeConnected(edge("online", 45), NOW)).toBe(true);
  });
});

describe("lampFromDeviceState", () => {
  const connectedDevice: DeviceCurrent = { site_code: "plant-a", line_code: null, area_code: null, cell_code: null, edge_code: "e1", device_code: "d1", connection_id: null, state: "connected", severity: "info", reason: null, tags_connected: 5, tags_stale: 0, tags_disconnected: 0, last_change_at: "", last_seen_at: "" };

  it("returns 'bad' if the edge itself is not connected, regardless of device state", () => {
    expect(lampFromDeviceState(connectedDevice, false)).toBe("bad");
  });
  it("returns 'good' for a connected device on a connected edge", () => {
    expect(lampFromDeviceState(connectedDevice, true)).toBe("good");
  });
  it("returns 'warn' for a stale device on a connected edge", () => {
    expect(lampFromDeviceState({ ...connectedDevice, state: "stale" }, true)).toBe("warn");
  });
  it("returns 'bad' for a disconnected device on a connected edge", () => {
    expect(lampFromDeviceState({ ...connectedDevice, state: "disconnected" }, true)).toBe("bad");
  });
  it("returns 'warn' for an undefined device on a connected edge (unknown state, not a hard failure)", () => {
    expect(lampFromDeviceState(undefined, true)).toBe("warn");
  });
});
