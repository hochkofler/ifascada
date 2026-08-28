import { describe, it, expect } from "vitest";
import { buildLiveRows, filterLiveRows, liveSubRows, lampFromTag } from "./live-rows";
import type { DeviceCurrent, EdgeCurrent, TagCurrent } from "@/lib/api-client";

const NOW = Date.parse("2026-08-28T15:00:00Z");
const fresh = "2026-08-28T14:59:50Z"; // 10 s: dentro del umbral de 45 s
const stale = "2026-08-28T14:00:00Z"; // una hora: fuera del umbral

function edge(code: string, status = "online", lastSeen = fresh): EdgeCurrent {
  return {
    site_code: "plant-a",
    line_code: null,
    area_code: null,
    cell_code: null,
    edge_code: code,
    status,
    last_seen_at: lastSeen,
    outbox_depth: 0,
    outbox_oldest_secs: null,
    action_metrics: {},
  };
}

function device(edgeCode: string, code: string, state = "connected"): DeviceCurrent {
  return {
    site_code: "plant-a",
    line_code: null,
    area_code: null,
    cell_code: null,
    edge_code: edgeCode,
    device_code: code,
    connection_id: null,
    state,
    severity: "info",
    reason: null,
    tags_connected: 3,
    tags_stale: 1,
    tags_disconnected: 0,
    last_change_at: fresh,
    last_seen_at: fresh,
  };
}

function tag(edgeCode: string, deviceCode: string, code: string, quality = "Good"): TagCurrent {
  return {
    tag_code: code,
    device_code: deviceCode,
    site_code: "plant-a",
    line_code: null,
    area_code: null,
    cell_code: null,
    edge_code: edgeCode,
    ts: fresh,
    value: 42,
    quality: { status: quality, reason: null },
    source: "modbus",
    metadata_json: {},
    expected_interval_ms: null,
    tag_status: "connected",
  };
}

describe("buildLiveRows", () => {
  it("cuelga cada tag del device que lo reporta", () => {
    const rows = buildLiveRows(
      [device("e1", "d1")],
      [edge("e1")],
      [tag("e1", "d1", "t2"), tag("e1", "d1", "t1")],
      NOW
    );
    expect(rows).toHaveLength(1);
    expect(liveSubRows(rows[0])?.map((r) => r.code)).toEqual(["t1", "t2"]);
  });

  it("una fila de tag no tiene hijos", () => {
    const rows = buildLiveRows([device("e1", "d1")], [edge("e1")], [tag("e1", "d1", "t1")], NOW);
    const child = liveSubRows(rows[0])?.[0];
    expect(child && liveSubRows(child)).toBeUndefined();
  });

  // El mismo device_code aparece bajo dos edges distintos en los datos reales
  // (dev_scale_manual_1 en edge-01 y en edge-com-01). Si el id no incluyera el edge, las dos
  // filas colisionarian y expandir una abriria la otra.
  it("da ids distintos al mismo device_code bajo edges distintos", () => {
    const rows = buildLiveRows(
      [device("e1", "shared"), device("e2", "shared")],
      [edge("e1"), edge("e2")],
      [],
      NOW
    );
    const ids = rows.map((r) => r.id);
    expect(new Set(ids).size).toBe(2);
  });

  it("los ids son estables entre refetches: mismos datos, mismos ids", () => {
    const args = [[device("e1", "d1")], [edge("e1")], [tag("e1", "d1", "t1")]] as const;
    const first = buildLiveRows(args[0], args[1], args[2], NOW);
    const second = buildLiveRows(args[0], args[1], args[2], NOW + 2500);
    expect(second.map((r) => r.id)).toEqual(first.map((r) => r.id));
  });

  // Este es el caso que importa de verdad: entre un poll y el siguiente el backend puede
  // devolver los devices en otro orden. Con ids por indice, lo expandido saltaria de fila.
  it("los ids no dependen del orden en que llega el array", () => {
    const a = device("e1", "aaa");
    const b = device("e1", "bbb");
    const one = buildLiveRows([a, b], [edge("e1")], [], NOW).map((r) => r.id);
    const two = buildLiveRows([b, a], [edge("e1")], [], NOW).map((r) => r.id);
    expect(two).toEqual(one);
  });

  it("marca todo como caido cuando el edge esta fuera del umbral de heartbeat", () => {
    const rows = buildLiveRows(
      [device("e1", "d1")],
      [edge("e1", "online", stale)],
      [tag("e1", "d1", "t1", "Good")],
      NOW
    );
    expect(rows[0].lamp).toBe("bad");
    expect(liveSubRows(rows[0])?.[0].lamp).toBe("bad");
  });
});

describe("lampFromTag", () => {
  it.each([
    ["Good", "good"],
    ["Stale", "warn"],
    ["Bad", "bad"],
    ["", "warn"],
  ])("calidad %s -> %s", (quality, expected) => {
    expect(lampFromTag(tag("e1", "d1", "t1", quality), true)).toBe(expected);
  });

  it("el edge caido gana sobre cualquier calidad del tag", () => {
    expect(lampFromTag(tag("e1", "d1", "t1", "Good"), false)).toBe("bad");
  });
});

describe("campos de display", () => {
  it("la fila de device resume sus tags con los contadores del DTO", () => {
    const rows = buildLiveRows([device("e1", "d1")], [edge("e1")], [], NOW);
    expect(rows[0].detail).toBe("3 ok · 1 stale · 0 caidos");
  });

  it("la fila de tag muestra valor con unidad", () => {
    const rows = buildLiveRows([device("e1", "d1")], [edge("e1")], [tag("e1", "d1", "t1")], NOW);
    expect(liveSubRows(rows[0])?.[0].detail).toBe("42");
  });

  // El edge solo se muestra en la fila del device; los tags lo heredan visualmente del padre.
  it("la fila de tag deja la columna de edge vacia pero conserva edgeCode para las acciones", () => {
    const rows = buildLiveRows([device("e1", "d1")], [edge("e1")], [tag("e1", "d1", "t1")], NOW);
    const child = liveSubRows(rows[0])?.[0];
    expect(child?.edge).toBe("");
    expect(child?.edgeCode).toBe("e1");
  });
});

describe("filterLiveRows", () => {
  const rows = buildLiveRows(
    [device("e1", "bomba-01"), device("e2", "balanza-02")],
    [edge("e1"), edge("e2")],
    [
      tag("e1", "bomba-01", "presion"),
      tag("e1", "bomba-01", "caudal"),
      tag("e2", "balanza-02", "peso"),
    ],
    NOW
  );

  it("sin filtros activos devuelve todo", () => {
    expect(filterLiveRows(rows, [])).toHaveLength(2);
    expect(filterLiveRows(rows, [{ id: "code", value: "  " }])).toHaveLength(2);
  });

  it("si el device coincide, se muestra con TODOS sus tags", () => {
    const out = filterLiveRows(rows, [{ id: "code", value: "bomba" }]);
    expect(out).toHaveLength(1);
    expect(liveSubRows(out[0])).toHaveLength(2);
  });

  // Sin esta regla, filtrar por un tag esconderia el device que lo contiene y no se veria nada.
  it("si coincide un tag, se conserva su device -- con solo los tags que coinciden", () => {
    const out = filterLiveRows(rows, [{ id: "code", value: "caudal" }]);
    expect(out).toHaveLength(1);
    expect(out[0].code).toBe("bomba-01");
    expect(liveSubRows(out[0])?.map((r) => r.code)).toEqual(["caudal"]);
  });

  it("descarta el device cuando no coinciden ni el ni sus tags", () => {
    expect(filterLiveRows(rows, [{ id: "code", value: "inexistente" }])).toHaveLength(0);
  });

  it("filtra por edge, que es una columna solo de la fila de device", () => {
    const out = filterLiveRows(rows, [{ id: "edge", value: "e2" }]);
    expect(out).toHaveLength(1);
    expect(out[0].code).toBe("balanza-02");
  });

  it("combina varios filtros con AND", () => {
    expect(
      filterLiveRows(rows, [
        { id: "code", value: "bomba" },
        { id: "edge", value: "e2" },
      ])
    ).toHaveLength(0);
  });

  it("no muta el arreglo original al recortar los tags", () => {
    filterLiveRows(rows, [{ id: "code", value: "caudal" }]);
    expect(liveSubRows(rows[0])).toHaveLength(2);
  });
});

describe("edges sin devices", () => {
  // Un edge que dejo de reportar del todo es exactamente el modo de falla que el operador
  // necesita ver. Si no apareciera como fila, su Reset quedaria inalcanzable.
  it("aparece como fila propia cuando ningun device lo reporta", () => {
    const rows = buildLiveRows([], [edge("mudo")], [], NOW);
    expect(rows).toHaveLength(1);
    expect(rows[0].kind).toBe("edge");
    expect(rows[0].code).toBe("mudo");
    expect(rows[0].edgeCode).toBe("mudo");
    // La columna Edge queda vacia: el codigo de la fila ya ES el edge.
    expect(rows[0].edge).toBe("");
  });

  it("no se duplica cuando el edge SI tiene devices", () => {
    const rows = buildLiveRows([device("e1", "d1")], [edge("e1")], [], NOW);
    expect(rows).toHaveLength(1);
    expect(rows[0].kind).toBe("device");
  });

  it("un edge fuera del umbral de heartbeat se marca como caido", () => {
    const rows = buildLiveRows([], [edge("mudo", "online", stale)], [], NOW);
    expect(rows[0].lamp).toBe("bad");
  });

  it("convive con los devices de otros edges", () => {
    const rows = buildLiveRows([device("e1", "d1")], [edge("e1"), edge("mudo")], [], NOW);
    expect(rows.map((r) => r.kind)).toEqual(["device", "edge"]);
  });
});
