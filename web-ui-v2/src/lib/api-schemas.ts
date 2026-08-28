import { z } from "zod";

/**
 * Esquemas de validacion del borde HTTP, derivados de los DTO de
 * `crates/central-server/src/api.rs` -- la fuente de verdad del contrato, no de los tipos que
 * antes se escribian a mano en api-client.ts y se imponian con `res.json() as Promise<T>`.
 *
 * Ese cast era una promesa del desarrollador, no una garantia: si el backend cambiaba un campo,
 * el error aparecia tres capas mas arriba como `undefined`. Ahora falla en el borde, con nombre
 * de campo, y el fallo llega al log de sesion por el `queryCache.onError` de lib/query-client.
 *
 * Criterio para los campos `serde_json::Value` (`quality`, `metadata_json`, `action_metrics`,
 * `payload_json`): el DTO NO garantiza que sean objetos, asi que se validan de forma tolerante
 * con `.catch()`. Un `quality: null` degrada a `{}` en vez de tumbar la grilla entera -- que es
 * lo que pasaria con un esquema estricto sobre un campo que el backend declara libre.
 */

/** `chrono::DateTime<Utc>` serializa a RFC3339. No se valida el formato: solo que sea string. */
const timestamp = z.string();

/** Campo JSON libre que el codigo consume como diccionario. Degrada a `{}` si no lo es. */
const looseRecord = z.record(z.string(), z.unknown()).catch({});

/**
 * `nullish()`, no `optional()`: el servidor real emite `{"reason": null, "status": "Good"}`. Con
 * `optional()` el objeto entero fallaba y el `.catch({})` de abajo lo degradaba a `{}`,
 * descartando tambien el `status` -- o sea, la calidad de TODOS los tags se mostraba como "-".
 * Lo detecto el test de contrato contra datos reales; el tipo escrito a mano que habia antes
 * (`reason?: string`) tenia el mismo error y nunca fallo porque nada validaba.
 */
export const qualitySchema = z
  .looseObject({
    status: z.string().nullish(),
    reason: z.string().nullish(),
  })
  .catch({});

export const edgeCurrentSchema = z.object({
  site_code: z.string(),
  line_code: z.string().nullable(),
  area_code: z.string().nullable(),
  cell_code: z.string().nullable(),
  edge_code: z.string(),
  status: z.string(),
  last_seen_at: timestamp,
  outbox_depth: z.number(),
  outbox_oldest_secs: z.number().nullable(),
  action_metrics: looseRecord,
});

export const deviceCurrentSchema = z.object({
  site_code: z.string(),
  line_code: z.string().nullable(),
  area_code: z.string().nullable(),
  cell_code: z.string().nullable(),
  edge_code: z.string(),
  device_code: z.string(),
  connection_id: z.string().nullable(),
  state: z.string(),
  severity: z.string(),
  reason: z.string().nullable(),
  tags_connected: z.number(),
  tags_stale: z.number(),
  tags_disconnected: z.number(),
  last_change_at: timestamp,
  last_seen_at: timestamp,
});

export const tagCurrentSchema = z.object({
  tag_code: z.string(),
  device_code: z.string(),
  site_code: z.string(),
  line_code: z.string().nullable(),
  area_code: z.string().nullable(),
  cell_code: z.string().nullable(),
  edge_code: z.string(),
  ts: timestamp,
  value: z.unknown(),
  quality: qualitySchema,
  source: z.string(),
  // El DTO lo declara `serde_json::Value` (siempre presente), no un opcional como decia el tipo
  // anterior. Se acepta ausente por compatibilidad hacia atras, degradando a `{}`.
  metadata_json: looseRecord,
  expected_interval_ms: z.number().nullable(),
  // El DTO lo declara `String`, no `Option<String>`: el tipo anterior lo marcaba opcional.
  tag_status: z.string(),
});

export const tagHistorySchema = z.object({
  ts: timestamp,
  site_code: z.string(),
  edge_code: z.string(),
  tag_code: z.string(),
  value: z.unknown(),
  quality_status: z.string(),
});

export const opsEventSchema = z.object({
  id: z.number(),
  ts: timestamp,
  severity: z.string(),
  event_type: z.string(),
  site_code: z.string(),
  edge_code: z.string().nullable(),
  connection_id: z.string().nullable(),
  device_code: z.string().nullable(),
  tag_code: z.string().nullable(),
  config_hash: z.string().nullable(),
  op_id: z.string().nullable(),
  message: z.string(),
  payload_json: looseRecord,
});

export const contextOptionSchema = z.object({
  code: z.string(),
  name: z.string(),
});

export type EdgeCurrent = z.infer<typeof edgeCurrentSchema>;
export type DeviceCurrent = z.infer<typeof deviceCurrentSchema>;
export type TagCurrent = z.infer<typeof tagCurrentSchema>;
export type TagHistory = z.infer<typeof tagHistorySchema>;
export type OpsEvent = z.infer<typeof opsEventSchema>;
export type ContextOption = z.infer<typeof contextOptionSchema>;
