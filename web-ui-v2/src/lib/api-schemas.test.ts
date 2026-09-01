import { describe, it, expect } from "vitest";
import {
  edgeCurrentSchema,
  deviceCurrentSchema,
  tagCurrentSchema,
  tagHistorySchema,
  opsEventSchema,
  contextOptionSchema,
} from "./api-schemas";

/**
 * Los casos de abajo estan derivados de los DTO reales de crates/central-server/src/api.rs, no
 * de los tipos que estaban escritos a mano en api-client.ts. Escribirlos contra la fuente de
 * verdad expuso tres desajustes que el `as Promise<T>` anterior no podia detectar:
 *
 *   - `tag_status` estaba tipado opcional; el DTO lo declara `String`, siempre presente.
 *   - `metadata_json` estaba tipado opcional; el DTO lo declara `serde_json::Value`, siempre
 *     presente y NO necesariamente un objeto.
 *   - `quality` estaba tipado `{ status?, reason? }`; el DTO lo declara `serde_json::Value`
 *     arbitrario. El codigo consumidor ya lo leia con `?.`, senal de que nadie confiaba en ese
 *     tipo.
 */

const tagCurrent = {
  tag_code: "tag_cc_in_bala11_21_weight",
  device_code: "CC-IN-BALA11-21",
  site_code: "plant-a",
  line_code: null,
  area_code: null,
  cell_code: null,
  edge_code: "lcc01",
  ts: "2026-08-26T18:31:13.144564Z",
  value: 0.10002,
  quality: { status: "good" },
  source: "modbus",
  metadata_json: { automations: [] },
  expected_interval_ms: 1000,
  tag_status: "connected",
};

describe("tagCurrentSchema", () => {
  it("acepta la forma real que emite TagCurrentDto", () => {
    expect(tagCurrentSchema.parse(tagCurrent)).toMatchObject({ tag_code: tagCurrent.tag_code });
  });

  it("rechaza un payload al que le falta un campo requerido por el DTO", () => {
    const { edge_code: _omitted, ...withoutEdge } = tagCurrent;
    expect(() => tagCurrentSchema.parse(withoutEdge)).toThrow();
  });

  // `quality` es serde_json::Value: el backend puede mandar null, un string o un objeto con
  // otras claves. Nada de eso debe tumbar la grilla entera.
  it.each([null, "good", 42, []])("tolera quality = %o sin romper", (quality) => {
    const parsed = tagCurrentSchema.parse({ ...tagCurrent, quality });
    expect(parsed.quality).toEqual({});
  });

  it("conserva las claves conocidas de quality cuando SI viene como objeto", () => {
    const parsed = tagCurrentSchema.parse({
      ...tagCurrent,
      quality: { status: "stale", reason: "timeout" },
    });
    expect(parsed.quality).toMatchObject({ status: "stale", reason: "timeout" });
  });

  // Mismo criterio para metadata_json, que alimenta el flujo de impresion.
  it("degrada metadata_json a objeto vacio si no viene como objeto", () => {
    expect(tagCurrentSchema.parse({ ...tagCurrent, metadata_json: null }).metadata_json).toEqual(
      {}
    );
  });

  it("acepta expected_interval_ms nulo, como declara Option<i64>", () => {
    expect(
      tagCurrentSchema.parse({ ...tagCurrent, expected_interval_ms: null }).expected_interval_ms
    ).toBeNull();
  });
});

describe("edgeCurrentSchema", () => {
  const edge = {
    site_code: "plant-a",
    line_code: null,
    area_code: null,
    cell_code: null,
    edge_code: "lcc01",
    status: "online",
    last_seen_at: "2026-08-26T18:30:59.174986Z",
    outbox_depth: 0,
    outbox_oldest_secs: null,
    action_metrics: {},
  };

  it("acepta la forma real que emite EdgeCurrentDto", () => {
    expect(edgeCurrentSchema.parse(edge).edge_code).toBe("lcc01");
  });

  it("rechaza outbox_depth ausente (el DTO lo declara i64, no Option)", () => {
    const { outbox_depth: _omitted, ...withoutDepth } = edge;
    expect(() => edgeCurrentSchema.parse(withoutDepth)).toThrow();
  });
});

describe("deviceCurrentSchema", () => {
  it("acepta la forma real que emite DeviceCurrentDto", () => {
    const parsed = deviceCurrentSchema.parse({
      site_code: "plant-a",
      line_code: null,
      area_code: null,
      cell_code: null,
      edge_code: "lcc01",
      device_code: "CC-IN-BALA11-21",
      connection_id: null,
      state: "connected",
      severity: "info",
      reason: null,
      tags_connected: 1,
      tags_stale: 0,
      tags_disconnected: 0,
      last_change_at: "2026-08-26T18:31:13Z",
      last_seen_at: "2026-08-26T18:31:13Z",
    });
    expect(parsed.device_code).toBe("CC-IN-BALA11-21");
  });
});

describe("tagHistorySchema / opsEventSchema / contextOptionSchema", () => {
  it("tagHistorySchema acepta la forma de TagHistoryDto", () => {
    expect(
      tagHistorySchema.parse({
        ts: "2026-08-26T18:31:13Z",
        site_code: "plant-a",
        edge_code: "lcc01",
        tag_code: "t1",
        value: "+ 15.2500 g",
        quality_status: "good",
      }).quality_status
    ).toBe("good");
  });

  it("opsEventSchema acepta la forma de OperationalEventDto", () => {
    expect(
      opsEventSchema.parse({
        id: 1,
        ts: "2026-08-26T18:31:13Z",
        severity: "warn",
        event_type: "edge.disconnected",
        site_code: "plant-a",
        edge_code: "lcc01",
        connection_id: null,
        device_code: null,
        tag_code: null,
        config_hash: null,
        op_id: null,
        message: "sin heartbeat",
        payload_json: {},
      }).id
    ).toBe(1);
  });

  it("contextOptionSchema acepta la forma de ContextOptionDto", () => {
    expect(contextOptionSchema.parse({ code: "line-main", name: "Line Main" }).name).toBe(
      "Line Main"
    );
  });
});

describe("deviceCurrentSchema · last_seen_at nullable", () => {
  const base = {
    site_code: "plant-a",
    line_code: null,
    area_code: null,
    cell_code: null,
    edge_code: "e1",
    device_code: "d1",
    connection_id: null,
    state: "connected",
    severity: "info",
    reason: null,
    tags_connected: 0,
    tags_stale: 0,
    tags_disconnected: 0,
    last_change_at: "2026-08-28T15:00:00Z",
  };

  // El backend deriva last_seen_at del MAX(ts) de los tags del dispositivo. Un dispositivo sin
  // tags devuelve null, y si el esquema no lo aceptara el .parse() vaciaria la grilla entera.
  it("acepta last_seen_at null, como devuelve un dispositivo sin tags", () => {
    expect(deviceCurrentSchema.parse({ ...base, last_seen_at: null }).last_seen_at).toBeNull();
  });

  it("sigue aceptando una fecha cuando el dispositivo si tiene tags", () => {
    const ts = "2026-08-28T15:04:00Z";
    expect(deviceCurrentSchema.parse({ ...base, last_seen_at: ts }).last_seen_at).toBe(ts);
  });

  it("rechaza un last_seen_at ausente: el backend siempre emite la clave", () => {
    expect(() => deviceCurrentSchema.parse(base)).toThrow();
  });
});
