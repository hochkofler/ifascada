/**
 * `POST /api/edges/reset` -- see crates/central-server/src/api.rs's `edge_reset` handler
 * (registered as `.route("/api/edges/reset", post(edge_reset))`). Confirmed real and live by
 * Task 13: it genuinely restarts the edge-agent runtime end-to-end (graceful stop -> engine
 * stopped -> clean restart, verified against real edge-agent logs). BUT central-server never
 * receives/records the `control/reset/result` MQTT acknowledgment it's wired to ingest (zero
 * `edge.reset.*` rows ever recorded, despite the ingestion path being live and correctly
 * wired) -- so this call's `accepted: true` only proves central-server successfully published
 * the command to MQTT, not that the edge actually reset. Callers must poll for independent
 * recovery evidence (e.g. `last_seen_at` advancing) rather than trusting this response alone.
 */
export type ResetEdgeRequest = { site_code: string; edge_code: string; reason?: string };
export type ResetEdgeResponse = { accepted: boolean; topic: string; request_id: string | null };

export async function resetEdge(req: ResetEdgeRequest): Promise<ResetEdgeResponse> {
  const res = await fetch("/api/edges/reset", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`reset failed: ${res.status}`);
  return res.json();
}
