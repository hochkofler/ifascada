import { describe, it, expect } from "vitest";
import { z } from "zod";
import {
  contextOptionSchema,
  deviceCurrentSchema,
  edgeCurrentSchema,
  opsEventSchema,
  tagCurrentSchema,
  tagHistorySchema,
} from "./api-schemas";
import edgesCurrent from "@/test/fixtures/edges-current.json";
import devicesCurrent from "@/test/fixtures/devices-current.json";
import tagsCurrent from "@/test/fixtures/tags-current.json";
import tagHistory from "@/test/fixtures/tag-history.json";
import opsEvents from "@/test/fixtures/ops-events.json";
import contextLines from "@/test/fixtures/context-lines.json";

/**
 * Test de contrato contra respuestas REALES del central-server.
 *
 * Los fixtures de src/test/fixtures/ no estan escritos a mano: son la salida literal de un
 * central-server corriendo en Docker (docker-compose.scada.yml, perfil `central`) con la base
 * sembrada y los edge-sim publicando telemetria por MQTT. Se capturaron con curl y se
 * versionaron para que este test no necesite un backend vivo para correr.
 *
 * Por que existe: api-schemas.ts se derivo de los DTO de crates/central-server/src/api.rs
 * leyendo el codigo Rust. Eso es mucho mejor que adivinar desde los tipos de TypeScript, pero
 * sigue siendo una lectura. Esto lo confronta con lo que el servidor de verdad emite -- que es
 * lo unico que decide si `.parse()` va a tirar en produccion.
 *
 * Si el backend cambia un DTO, este test se pone rojo y hay que recapturar los fixtures. Ese es
 * el punto: el desajuste se ve en CI y no en la pantalla de un operador.
 */
function expectParses<T>(schema: z.ZodType<T>, rows: unknown, label: string): T[] {
  const result = z.array(schema).safeParse(rows);
  if (!result.success) {
    throw new Error(
      `${label} no valida contra su esquema:\n${JSON.stringify(result.error.issues, null, 2)}`
    );
  }
  expect(result.data.length).toBeGreaterThan(0);
  return result.data;
}

/**
 * Los campos con `.catch()` (quality, metadata_json, action_metrics, payload_json) NO pueden
 * verificarse solo con "parsea": el `.catch` los degrada en silencio y el test pasa igual. De
 * hecho asi se colo un bug real -- `reason: null` hacia fallar todo `quality` y la calidad de
 * cada tag se degradaba a `{}`. Sobre esos campos hay que afirmar que el dato se PRESERVA.
 */
function expectPreserved(actual: unknown, original: unknown, label: string): void {
  expect(actual, `${label} se degrado silenciosamente por un .catch()`).toEqual(original);
}

describe("contrato con el central-server real", () => {
  it("edgeCurrentSchema valida GET /api/edges/current", () => {
    const parsed = expectParses(edgeCurrentSchema, edgesCurrent, "edges/current");
    parsed.forEach((row, i) => {
      expectPreserved(row.action_metrics, edgesCurrent[i].action_metrics, "action_metrics");
    });
  });

  it("deviceCurrentSchema valida GET /api/devices/current", () => {
    expectParses(deviceCurrentSchema, devicesCurrent, "devices/current");
  });

  it("tagCurrentSchema valida GET /api/tags/current", () => {
    const parsed = expectParses(tagCurrentSchema, tagsCurrent, "tags/current");
    parsed.forEach((row, i) => {
      expectPreserved(row.quality, tagsCurrent[i].quality, "quality");
      expectPreserved(row.metadata_json, tagsCurrent[i].metadata_json, "metadata_json");
    });
  });

  // La regresion concreta que esto pillo: el panel de diagnostico muestra `quality.status`, y
  // con el esquema anterior mostraba "-" para todos los tags.
  it("conserva quality.status del servidor real, no lo degrada a {}", () => {
    const parsed = expectParses(tagCurrentSchema, tagsCurrent, "tags/current");
    expect(parsed[0].quality.status).toBe(tagsCurrent[0].quality.status);
  });

  it("tagHistorySchema valida GET /api/tags/{tag}/history", () => {
    expectParses(tagHistorySchema, tagHistory, "tags/history");
  });

  it("opsEventSchema valida GET /api/ops/events", () => {
    const parsed = expectParses(opsEventSchema, opsEvents, "ops/events");
    parsed.forEach((row, i) => {
      expectPreserved(row.payload_json, opsEvents[i].payload_json, "payload_json");
    });
  });

  it("contextOptionSchema valida GET /api/context/lines", () => {
    expectParses(contextOptionSchema, contextLines, "context/lines");
  });
});
