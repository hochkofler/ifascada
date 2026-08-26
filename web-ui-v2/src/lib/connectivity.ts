import type { EdgeCurrent, DeviceCurrent } from "./api-client";

/**
 * Matches central-server's own CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT (crates/central-server/
 * src/api.rs's default_edge_stale_after_secs()). web-ui's NEXT_PUBLIC_EDGE_STALE_SECS env var
 * is Next.js-specific plumbing that doesn't carry over to Vite -- hardcode the same value
 * instead of reintroducing an env var for it.
 */
export const EDGE_STALE_AFTER_SECS = 45;

/**
 * central-server writes two different literals into edge_current_state.status depending on
 * which ingestion path last touched the row: insert_telemetry hardcodes "online"
 * (postgres.rs:642-648); insert_health writes the edge-agent's own health-message literal,
 * "ok"/"degraded" (postgres.rs:681-700, compute_health_status() in edge-agent's mqtt_bridge.rs).
 * This is a real, already-documented backend inconsistency this frontend redesign doesn't fix
 * at the source -- but checking both literals here (exactly what web-ui/components/
 * context-bar.tsx already does) makes the frontend correct regardless of which one is live in
 * the column at read time. This is the fix for the "edges online 0/n" badge bug.
 */
export const ONLINE_STATUSES = new Set(["online", "ok"]);

function ageSecsFromIso(ts: string, nowMs: number): number {
  const t = new Date(ts).getTime();
  if (Number.isNaN(t)) return Number.POSITIVE_INFINITY;
  return Math.max(0, Math.floor((nowMs - t) / 1000));
}

export function edgeConnected(edge: EdgeCurrent | undefined, nowMs: number = Date.now()): boolean {
  if (!edge) return false;
  const status = String(edge.status || "").toLowerCase();
  const okState = ONLINE_STATUSES.has(status);
  return okState && ageSecsFromIso(edge.last_seen_at, nowMs) <= EDGE_STALE_AFTER_SECS;
}

export function lampFromDeviceState(device: DeviceCurrent | undefined, edgeConn: boolean): "good" | "warn" | "bad" {
  if (!edgeConn) return "bad";
  const state = String(device?.state || "").toLowerCase();
  if (state === "connected") return "good";
  if (state === "stale") return "warn";
  if (state === "disconnected") return "bad";
  return "warn";
}
