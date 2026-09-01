import { describe, it, expect } from "vitest";
import { buildConnectionRows, lampFromConnection } from "./connection-rows";
import type { ConnectionCurrent } from "@/lib/api-client";

function conn(over: Partial<ConnectionCurrent> = {}): ConnectionCurrent {
  return {
    site_code: "plant-a",
    line_code: null,
    area_code: null,
    cell_code: null,
    edge_code: "e1",
    connection_id: "c1",
    state: "connected",
    severity: "info",
    last_change_at: "2026-08-28T15:00:00Z",
    message: "",
    ...over,
  };
}

describe("lampFromConnection", () => {
  it("info + connected es sano", () => {
    expect(lampFromConnection(conn())).toBe("good");
  });

  it("error es falla", () => {
    expect(lampFromConnection(conn({ severity: "error", state: "failed" }))).toBe("bad");
  });

  it("warn es advertencia", () => {
    expect(lampFromConnection(conn({ severity: "warn" }))).toBe("warn");
  });

  // Ante la duda no se pinta de sano algo que podria estar fallando.
  it("una severidad desconocida cae en advertencia, no en sano", () => {
    expect(lampFromConnection(conn({ severity: "vaya-uno-a-saber" }))).toBe("warn");
  });

  // `state` es texto libre del edge-agent y varia entre drivers: info con un estado que no es
  // `connected` no alcanza para declararla sana.
  it("info con un estado que no es connected no es sano", () => {
    expect(lampFromConnection(conn({ severity: "info", state: "reconnecting" }))).toBe("warn");
  });
});

describe("buildConnectionRows", () => {
  // Caso real de produccion: `conn-protocol-1` aparece bajo dos edges distintos. Si el id no
  // incluyera el edge, las dos filas colisionarian.
  it("da ids distintos al mismo connection_id bajo edges distintos", () => {
    const rows = buildConnectionRows([
      conn({ edge_code: "e1", connection_id: "compartida" }),
      conn({ edge_code: "e2", connection_id: "compartida" }),
    ]);
    expect(new Set(rows.map((r) => r.id)).size).toBe(2);
  });

  // Esta pantalla se abre porque algo falla: lo que falla va arriba.
  it("ordena las fallidas primero, despues las advertencias, despues las sanas", () => {
    const rows = buildConnectionRows([
      conn({ connection_id: "sana" }),
      conn({ connection_id: "rota", severity: "error", state: "failed" }),
      conn({ connection_id: "dudosa", severity: "warn" }),
    ]);
    expect(rows.map((r) => r.connectionId)).toEqual(["rota", "dudosa", "sana"]);
  });

  it("conserva el mensaje del backend, que es el motivo de la falla", () => {
    const rows = buildConnectionRows([conn({ severity: "error", message: "puerto ocupado" })]);
    expect(rows[0].message).toBe("puerto ocupado");
  });

  it("tolera la lista vacia", () => {
    expect(buildConnectionRows([])).toEqual([]);
  });
});
