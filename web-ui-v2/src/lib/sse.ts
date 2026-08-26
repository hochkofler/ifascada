/**
 * Ported from web-ui/lib/sse.ts's subscribeSse. This connects to central-server's real,
 * already-existing GET /api/stream/events SSE endpoint. Polling (fetchEdgesCurrent/
 * fetchDevicesCurrent/fetchTagsCurrent, refetchInterval: 2500) stays the reliable base state
 * everywhere this is used -- this is an additive low-latency layer on top, never the sole
 * source of truth. If the SSE connection silently drops, the next 2.5s poll self-heals; no
 * separate reconnect/staleness-detection logic is needed as a result.
 */
export type RtEvent = {
  event_type: string;
  site: string;
  agent: string;
  payload: unknown;
  published_at: string;
  received_at_ms?: number;
};

export type SseOptions = {
  site?: string;
  line?: string;
  area?: string;
  cell?: string;
  edge?: string;
  tag?: string;
  excludeRaw?: boolean;
  replay?: boolean;
};

export function subscribeSse(onMessage: (evt: RtEvent) => void, options?: SseOptions): () => void {
  const baseUrl = "/api/stream/events";
  const qs = new URLSearchParams();
  if (options?.site) qs.set("site", options.site);
  if (options?.line) qs.set("line", options.line);
  if (options?.area) qs.set("area", options.area);
  if (options?.cell) qs.set("cell", options.cell);
  if (options?.edge) qs.set("edge", options.edge);
  if (options?.tag) qs.set("tag", options.tag);
  if (options?.excludeRaw !== undefined) qs.set("exclude_raw", String(options.excludeRaw));
  if (options?.replay !== undefined) qs.set("replay", String(options.replay));
  const url = qs.size > 0 ? `${baseUrl}?${qs.toString()}` : baseUrl;

  const es = new EventSource(url);
  const handler = (ev: MessageEvent) => {
    try {
      const parsed = JSON.parse(ev.data) as RtEvent;
      parsed.received_at_ms = Date.now();
      onMessage(parsed);
    } catch {
      // ignore malformed payloads
    }
  };
  es.onmessage = handler;
  es.addEventListener("runtime", handler as EventListener);
  return () => es.close();
}
