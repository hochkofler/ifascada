import { z } from "zod";
import { ApiError } from "@/lib/api-error";
import {
  connectionCurrentSchema,
  contextOptionSchema,
  deviceCurrentSchema,
  edgeCurrentSchema,
  opsEventSchema,
  tagCurrentSchema,
  tagHistorySchema,
} from "@/lib/api-schemas";

/**
 * Los tipos de las respuestas ya no se escriben a mano aca: se infieren de los esquemas de
 * api-schemas.ts, que a su vez estan derivados de los DTO de crates/central-server/src/api.rs.
 * Una sola fuente de verdad, y validacion real en el borde en vez de `res.json() as Promise<T>`.
 */
export type {
  ConnectionCurrent,
  TagCurrent,
  EdgeCurrent,
  DeviceCurrent,
  TagHistory,
  OpsEvent,
  ContextOption,
} from "@/lib/api-schemas";

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
    // ApiError en vez de un Error generico: lleva `status` y `body`, que es lo que
    // notify.apiError() necesita para mostrarle al operador un motivo legible en vez de la
    // palabra "error", y de donde saldra el correlationId cuando el backend lo emita.
    const body = await res.text().catch(() => "");
    throw new ApiError(res.status, body || `${init?.method ?? "GET"} ${path}`);
  }
  return res.json() as Promise<T>;
}

export function getJson<T>(path: string): Promise<T> {
  return request<T>(path);
}

/**
 * GET + validacion contra un esquema. Toda lectura tipada pasa por aca; `getJson` queda como
 * escotilla cruda para lo que todavia no tiene esquema.
 */
async function getParsed<T>(path: string, schema: z.ZodType<T>): Promise<T> {
  return schema.parse(await request<unknown>(path));
}

/**
 * POSTs a JSON body through the single `request()` auth-injection point. Every write call in
 * this app (edge actions, edge reset) should go through this rather than a raw `fetch`, so real
 * auth (when it lands -- see getAuthHeader's doc comment) only has to change in one place.
 */
export function postJson<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export type LiveFilter = {
  site?: string;
  line?: string;
  area?: string;
  cell?: string;
  edge?: string;
};

function toQuery(params: Record<string, string | number | undefined>): string {
  const qs = new URLSearchParams();
  Object.entries(params).forEach(([k, v]) => {
    if (v !== undefined && v !== "") qs.set(k, String(v));
  });
  return qs.toString();
}

export function fetchTagsCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getParsed(`/api/tags/current?${qs}`, z.array(tagCurrentSchema));
}

export function fetchEdgesCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getParsed(`/api/edges/current?${qs}`, z.array(edgeCurrentSchema));
}

export function fetchTagHistory(tagCode: string, limit = 200, offset = 0) {
  return getParsed(
    `/api/tags/${encodeURIComponent(tagCode)}/history?limit=${limit}&offset=${offset}`,
    z.array(tagHistorySchema)
  );
}

/**
 * Real route: `GET /api/ops/events?edge={edge_code}&limit=N` -- see
 * crates/central-server/src/api.rs's `list_operational_events` handler (registered as
 * `.route("/api/ops/events", get(list_operational_events))`), which serializes rows into
 * `OperationalEventDto` (api.rs:116-130). Confirmed live and field-matched against that DTO by
 * Task 13's review; el esquema de api-schemas.ts esta derivado de ese mismo DTO.
 */
export function fetchEdgeEvents(edgeCode: string, limit = 20) {
  return getParsed(
    `/api/ops/events?edge=${encodeURIComponent(edgeCode)}&limit=${limit}`,
    z.array(opsEventSchema)
  );
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

export function fetchDevicesCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getParsed(`/api/devices/current?${qs}`, z.array(deviceCurrentSchema));
}

export function fetchLines(site?: string) {
  const qs = toQuery({ site });
  return getParsed(`/api/context/lines?${qs}`, z.array(contextOptionSchema));
}

export function fetchAreas(site?: string, line?: string) {
  const qs = toQuery({ site, line });
  return getParsed(`/api/context/areas?${qs}`, z.array(contextOptionSchema));
}

export function fetchCells(site?: string, line?: string, area?: string) {
  const qs = toQuery({ site, line, area });
  return getParsed(`/api/context/cells?${qs}`, z.array(contextOptionSchema));
}

/**
 * Conexiones actuales. El endpoint existia en el backend desde siempre y el frontend nunca lo
 * llamaba: las conexiones en `failed` no se veian en ninguna parte de la app.
 */
export function fetchConnectionsCurrent(limit = 200, filter?: LiveFilter) {
  const qs = toQuery({ limit, ...filter });
  return getParsed(`/api/connections/current?${qs}`, z.array(connectionCurrentSchema));
}
