"use client";

export type RtEvent = {
  event_type: string;
  site: string;
  agent: string;
  payload: unknown;
  published_at: string;
  received_at_ms?: number;
};

export type OpsEvent = {
  id: number;
  ts: string;
  severity: string;
  event_type: string;
  site_code: string;
  edge_code?: string | null;
  connection_id?: string | null;
  device_code?: string | null;
  tag_code?: string | null;
  config_hash?: string | null;
  op_id?: string | null;
  message: string;
  payload_json: unknown;
};

type SseOptions = {
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
  const baseUrl = process.env.NEXT_PUBLIC_SSE_URL || "/api/stream/events";
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

type OpsSseOptions = {
  site?: string;
  edge?: string;
  device?: string;
  tag?: string;
  severity?: string;
  event_type?: string;
  q?: string;
  replay?: boolean;
};

export function subscribeOpsSse(
  onMessage: (evt: OpsEvent) => void,
  options?: OpsSseOptions
): () => void {
  const baseUrl =
    process.env.NEXT_PUBLIC_OPS_SSE_URL || "/api/ops/events/stream";
  const qs = new URLSearchParams();
  if (options?.site) qs.set("site", options.site);
  if (options?.edge) qs.set("edge", options.edge);
  if (options?.device) qs.set("device", options.device);
  if (options?.tag) qs.set("tag", options.tag);
  if (options?.severity) qs.set("severity", options.severity);
  if (options?.event_type) qs.set("event_type", options.event_type);
  if (options?.q) qs.set("q", options.q);
  if (options?.replay !== undefined) qs.set("replay", String(options.replay));
  const url = qs.size > 0 ? `${baseUrl}?${qs.toString()}` : baseUrl;
  const es = new EventSource(url);
  const handler = (ev: MessageEvent) => {
    try {
      onMessage(JSON.parse(ev.data) as OpsEvent);
    } catch {
      // ignore malformed payloads
    }
  };
  es.onmessage = handler;
  es.addEventListener("ops", handler as EventListener);
  return () => es.close();
}
