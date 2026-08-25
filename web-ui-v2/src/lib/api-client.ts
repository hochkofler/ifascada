/**
 * Single point where an Authorization header would be added once real auth exists
 * (see the spec's "Auth: door left open" section). Empty today.
 */
export function getAuthHeader(): Record<string, string> {
  return {};
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { ...getAuthHeader(), ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    throw new Error(`${init?.method ?? "GET"} ${path} failed: ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export function getJson<T>(path: string): Promise<T> {
  return request<T>(path);
}

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
  tag_status?: string;
  expected_interval_ms?: number | null;
};

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
  action_metrics: Record<string, unknown>;
};

export type TagHistory = {
  ts: string;
  site_code: string;
  edge_code: string;
  tag_code: string;
  value: unknown;
  quality_status: string;
};

type LiveFilter = { site?: string; line?: string; area?: string; cell?: string; edge?: string };

function toQuery(params: Record<string, string | number | undefined>): string {
  const qs = new URLSearchParams();
  Object.entries(params).forEach(([k, v]) => {
    if (v !== undefined && v !== "") qs.set(k, String(v));
  });
  return qs.toString();
}

export function fetchTagsCurrent(limit = 200, filter?: LiveFilter): Promise<TagCurrent[]> {
  const qs = toQuery({ limit, ...filter });
  return getJson<TagCurrent[]>(`/api/tags/current?${qs}`);
}

export function fetchEdgesCurrent(limit = 200, filter?: LiveFilter): Promise<EdgeCurrent[]> {
  const qs = toQuery({ limit, ...filter });
  return getJson<EdgeCurrent[]>(`/api/edges/current?${qs}`);
}

export function fetchTagHistory(tagCode: string, limit = 200, offset = 0): Promise<TagHistory[]> {
  return getJson<TagHistory[]>(`/api/tags/${encodeURIComponent(tagCode)}/history?limit=${limit}&offset=${offset}`);
}

/**
 * Posts an edge action. The real central-server route is `POST /api/edges/action` (singular,
 * `edge_code`/`site_code` in the JSON body -- see crates/central-server/src/api.rs's
 * `edge_action` handler, registered as `.route("/api/edges/action", post(edge_action))`), NOT
 * `/api/edges/{edge}/actions` (plural, edge in the path). This was verified live against the
 * real running central-server while building the History page's print flow (Task 12): the
 * previous body shape 404'd against the real server (confirmed by curl), because it's a
 * different route than the one central-server actually registers.
 */
export function postEdgeAction(
  site: string,
  edge: string,
  actionType: string,
  payload: Record<string, unknown>,
  meta: { source: string; target: string }
): Promise<unknown> {
  return request(`/api/edges/action`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      site_code: site,
      edge_code: edge,
      action_type: actionType,
      payload,
      source: meta.source,
      target: meta.target,
    }),
  });
}
