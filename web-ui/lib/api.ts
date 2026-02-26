export type EdgeCurrent = {
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  status: string;
  last_seen_at: string;
  outbox_depth: number;
  outbox_oldest_secs: number | null;
  action_metrics?: Record<
    string,
    { received_total?: number; accepted_total?: number; failed_total?: number }
  >;
};

export type DeviceCurrent = {
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  device_code: string;
  connection_id: string | null;
  state: string;
  severity: string;
  reason: string | null;
  tags_connected: number;
  tags_stale: number;
  tags_disconnected: number;
  last_change_at: string;
  last_seen_at: string;
};

export type TagCurrent = {
  tag_code: string;
  device_code: string;
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  ts: string;
  value: unknown;
  quality: { status?: string; reason?: string };
  source: string;
  metadata_json?: Record<string, unknown>;
  expected_interval_ms?: number | null;
  tag_status?: string;
};

export type ConnectionCurrent = {
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  connection_id: string;
  state: string;
  severity: string;
  last_change_at: string;
  message: string;
};

export type TagHistory = {
  ts: string;
  site_code: string;
  edge_code: string;
  tag_code: string;
  value: unknown;
  quality_status: string;
};

export type ContextOption = {
  code: string;
  name: string;
};

export type OperationalEvent = {
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

const API_BASE = process.env.NEXT_PUBLIC_API_BASE || "";

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} for ${path}`);
  }
  return res.json() as Promise<T>;
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    cache: "no-store",
  });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} for ${path}`);
  }
  return res.json() as Promise<T>;
}

type LiveFilter = {
  site?: string;
  line?: string;
  area?: string;
  cell?: string;
  edge?: string;
};

function toQuery(params: Record<string, string | number | undefined>) {
  const qs = new URLSearchParams();
  Object.entries(params).forEach(([k, v]) => {
    if (v !== undefined && v !== "") qs.set(k, String(v));
  });
  return qs.toString();
}

export function fetchEdgesCurrent(limit = 50, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getJson<EdgeCurrent[]>(`/api/edges/current?${qs}`);
}

export function fetchDevicesCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getJson<DeviceCurrent[]>(`/api/devices/current?${qs}`);
}

export function fetchTagsCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getJson<TagCurrent[]>(`/api/tags/current?${qs}`);
}

export function fetchConnectionsCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getJson<ConnectionCurrent[]>(`/api/connections/current?${qs}`);
}

export function fetchTagHistory(tagCode: string, limit = 200, offset = 0) {
  return getJson<TagHistory[]>(
    `/api/tags/${encodeURIComponent(tagCode)}/history?limit=${limit}&offset=${offset}`
  );
}

export function fetchOperationalEvents(
  limit = 200,
  filter?: {
    site?: string;
    edge?: string;
    device?: string;
    tag?: string;
    severity?: string;
    event_type?: string;
    q?: string;
  }
) {
  const qs = toQuery({ limit, ...(filter || {}) });
  return getJson<OperationalEvent[]>(`/api/ops/events?${qs}`);
}

export function fetchLines(site?: string) {
  const qs = toQuery({ site });
  return getJson<ContextOption[]>(`/api/context/lines?${qs}`);
}

export function fetchAreas(site?: string, line?: string) {
  const qs = toQuery({ site, line });
  return getJson<ContextOption[]>(`/api/context/areas?${qs}`);
}

export function fetchCells(site?: string, line?: string, area?: string) {
  const qs = toQuery({ site, line, area });
  return getJson<ContextOption[]>(`/api/context/cells?${qs}`);
}

export type EdgeResetResponse = {
  accepted: boolean;
  topic: string;
  request_id?: string | null;
};

export type EdgeActionResponse = {
  accepted: boolean;
  topic: string;
  request_id?: string | null;
};

export function postEdgeReset(
  siteCode: string,
  edgeCode: string,
  payload?: { request_id?: string; reason?: string; operator?: string }
) {
  return postJson<EdgeResetResponse>("/api/edges/reset", {
    site_code: siteCode,
    edge_code: edgeCode,
    request_id: payload?.request_id,
    reason: payload?.reason,
    operator: payload?.operator,
  });
}

export function postEdgeAction(
  siteCode: string,
  edgeCode: string,
  actionType: string,
  payload: unknown,
  opts?: { request_id?: string; source?: string; target?: string }
) {
  return postJson<EdgeActionResponse>("/api/edges/action", {
    site_code: siteCode,
    edge_code: edgeCode,
    action_type: actionType,
    request_id: opts?.request_id,
    source: opts?.source,
    target: opts?.target ?? "edge",
    payload,
  });
}
