import { describe, expect, it } from "vitest";
import type { OpsEvent } from "@/lib/api-schemas";
import {
  decide,
  initialState,
  MAX_INDIVIDUAL_PER_CYCLE,
  SUPPRESSION_WINDOW_MS,
  type NotifierState,
} from "./action-failure-notifier";

function event(id: number, edge: string, message: string): OpsEvent {
  return {
    id,
    ts: new Date(1_788_000_000_000 + id * 1000).toISOString(),
    severity: "warn",
    event_type: "action.command.rejected",
    site_code: "plant-a",
    edge_code: edge,
    connection_id: null,
    device_code: null,
    tag_code: null,
    config_hash: null,
    op_id: `req-${id}`,
    message,
    payload_json: {},
  } as OpsEvent;
}

const T0 = 1_788_000_000_000;
const SHARE_DENIED = "windows share print failed ... Acceso denegado.";

describe("action-failure-notifier", () => {
  /**
   * Sin esto, abrir la aplicación reproduciría todo el historial de fallos como si acabaran
   * de ocurrir. Es la forma más rápida de que alguien silencie el canal el primer día.
   */
  it("la primera mirada no avisa nada, solo fija la marca de agua", () => {
    const { toNotify, collapsed, next } = decide(
      [event(10, "lcc01", SHARE_DENIED), event(9, "lcc01", SHARE_DENIED)],
      initialState,
      T0
    );

    expect(toNotify).toHaveLength(0);
    expect(collapsed).toBe(0);
    expect(next.lastId).toBe(10);
  });

  it("un fallo nuevo después de la primera mirada sí avisa", () => {
    const seen: NotifierState = { lastId: 10, quietUntil: {} };
    const { toNotify, next } = decide([event(11, "lcc01", SHARE_DENIED)], seen, T0);

    expect(toNotify.map((e) => e.id)).toEqual([11]);
    expect(next.lastId).toBe(11);
  });

  it("no vuelve a avisar lo ya visto cuando se repite el sondeo", () => {
    const seen: NotifierState = { lastId: 11, quietUntil: {} };
    const { toNotify } = decide([event(11, "lcc01", SHARE_DENIED)], seen, T0);
    expect(toNotify).toHaveLength(0);
  });

  /**
   * El caso real: la automatización de impresión de lcc01 dispara en cada pesada, así que la
   * misma causa produce decenas de eventos por hora.
   */
  it("cincuenta fallos idénticos son un solo aviso", () => {
    const burst = Array.from({ length: 50 }, (_, i) => event(100 + i, "lcc01", SHARE_DENIED));
    const seen: NotifierState = { lastId: 99, quietUntil: {} };

    const { toNotify, collapsed } = decide(burst, seen, T0);

    expect(toNotify).toHaveLength(1);
    expect(collapsed).toBe(0);
  });

  it("la misma causa queda callada dentro de la ventana", () => {
    const seen: NotifierState = { lastId: 99, quietUntil: {} };
    const first = decide([event(100, "lcc01", SHARE_DENIED)], seen, T0);
    expect(first.toNotify).toHaveLength(1);

    const later = decide(
      [event(101, "lcc01", SHARE_DENIED)],
      first.next,
      T0 + SUPPRESSION_WINDOW_MS - 1000
    );
    expect(later.toNotify).toHaveLength(0);
  });

  /**
   * El recordatorio. Sin él, un problema que no se arregla se vuelve invisible tras el primer
   * aviso -- el error opuesto al spam, y igual de malo.
   */
  it("vuelve a avisar pasada la ventana si el problema sigue", () => {
    const seen: NotifierState = { lastId: 99, quietUntil: {} };
    const first = decide([event(100, "lcc01", SHARE_DENIED)], seen, T0);

    const later = decide(
      [event(200, "lcc01", SHARE_DENIED)],
      first.next,
      T0 + SUPPRESSION_WINDOW_MS + 1000
    );
    expect(later.toNotify.map((e) => e.id)).toEqual([200]);
  });

  it("distingue causas distintas en el mismo edge", () => {
    const seen: NotifierState = { lastId: 99, quietUntil: {} };
    const { toNotify } = decide(
      [event(100, "lcc01", SHARE_DENIED), event(101, "lcc01", "buffer 'x' is empty")],
      seen,
      T0
    );
    expect(toNotify).toHaveLength(2);
  });

  it("distingue el mismo fallo en edges distintos", () => {
    const seen: NotifierState = { lastId: 99, quietUntil: {} };
    const { toNotify } = decide(
      [event(100, "lcc01", SHARE_DENIED), event(101, "lcc02", SHARE_DENIED)],
      seen,
      T0
    );
    expect(toNotify).toHaveLength(2);
  });

  /** Muchas causas distintas de golpe tampoco deben producir una cascada de toasts. */
  it("colapsa en un conteo cuando hay demasiadas causas distintas", () => {
    const many = Array.from({ length: MAX_INDIVIDUAL_PER_CYCLE + 3 }, (_, i) =>
      event(100 + i, "lcc01", `fallo distinto ${i}`)
    );
    const seen: NotifierState = { lastId: 99, quietUntil: {} };

    const { toNotify, collapsed } = decide(many, seen, T0);

    expect(toNotify).toHaveLength(0);
    expect(collapsed).toBe(MAX_INDIVIDUAL_PER_CYCLE + 3);
  });

  it("una respuesta vacía no rompe ni mueve la marca hacia atrás", () => {
    const seen: NotifierState = { lastId: 42, quietUntil: {} };
    const { toNotify, collapsed, next } = decide([], seen, T0);

    expect(toNotify).toHaveLength(0);
    expect(collapsed).toBe(0);
    expect(next.lastId).toBe(42);
  });

  /**
   * El estado vive en memoria mientras la pestaña está abierta. Si nunca se limpiara, una
   * sesión larga con muchas causas distintas lo haría crecer sin techo.
   */
  it("olvida las causas cuya ventana ya venció", () => {
    const seen: NotifierState = { lastId: 99, quietUntil: {} };
    const first = decide([event(100, "lcc01", SHARE_DENIED)], seen, T0);
    expect(Object.keys(first.next.quietUntil)).toHaveLength(1);

    const muchLater = decide([], first.next, T0 + SUPPRESSION_WINDOW_MS * 3);
    expect(Object.keys(muchLater.next.quietUntil)).toHaveLength(0);
  });
});
