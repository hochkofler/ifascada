/**
 * Decide qué avisar cuando una acción pedida a un edge falla.
 *
 * La señal son los `operational_events` con `event_type = 'action.command.rejected'`: alguien
 * pidió algo y no se pudo. Sirve en cualquier modalidad de edge — a diferencia del silencio,
 * que solo significa un problema en los edges que reportan de forma continua.
 *
 * Todo el riesgo de esta función es el ruido. La automatización de impresión de lcc01 dispara
 * en CADA pesada: con la impresora caída eso son decenas de fallos por hora, y un canal que
 * avisa decenas de veces se silencia el primer día. Por eso se agrupa por causa y no por
 * evento.
 */
import type { OpsEvent } from "@/lib/api-schemas";

/** Cuánto se calla una causa ya avisada antes de poder volver a avisar. */
export const SUPPRESSION_WINDOW_MS = 15 * 60 * 1000;

/** Cuántas causas distintas se muestran una por una antes de colapsarlas en un conteo. */
export const MAX_INDIVIDUAL_PER_CYCLE = 3;

export type NotifierState = {
  /** `null` mientras no se haya mirado nunca. */
  lastId: number | null;
  /** Por causa: hasta cuándo permanece callada (epoch ms). */
  quietUntil: Record<string, number>;
};

export type NotifierDecision = {
  /** Causas a mostrar individualmente. */
  toNotify: OpsEvent[];
  /** Cuántas causas se colapsaron por exceder el tope; 0 si no hubo colapso. */
  collapsed: number;
  next: NotifierState;
};

export const initialState: NotifierState = { lastId: null, quietUntil: {} };

/** La identidad de una causa: el mismo fallo en el mismo edge es una sola cosa. */
export function causeKey(event: OpsEvent): string {
  return `${event.edge_code ?? "?"}|${event.message}`;
}

export function decide(
  events: OpsEvent[],
  state: NotifierState,
  now: number
): NotifierDecision {
  // Las ventanas vencidas se descartan acá: el estado vive mientras la pestaña esté
  // abierta, y sin esto una sesión larga con muchas causas distintas crecería sin techo.
  const quietUntil: Record<string, number> = {};
  for (const [key, until] of Object.entries(state.quietUntil)) {
    if (until > now) quietUntil[key] = until;
  }

  const lastId = state.lastId;
  const maxId = events.reduce((max, e) => Math.max(max, e.id), lastId ?? 0);

  // Primera mirada: se fija la marca y no se dice nada. Avisar acá sería reproducir todo el
  // historial de fallos como si acabara de ocurrir.
  if (lastId === null) {
    return { toNotify: [], collapsed: 0, next: { lastId: maxId, quietUntil } };
  }

  // Un representante por causa, el más reciente: cincuenta fallos idénticos son una sola
  // cosa que contar, no cincuenta.
  const byCause = new Map<string, OpsEvent>();
  for (const e of events) {
    if (e.id <= lastId) continue;
    const key = causeKey(e);
    const chosen = byCause.get(key);
    if (chosen === undefined || e.id > chosen.id) byCause.set(key, e);
  }

  const candidates = [...byCause.entries()]
    .filter(([key]) => quietUntil[key] === undefined)
    .map(([, e]) => e);

  // Se callan todas las causas elegidas, se muestren una por una o colapsadas: en ambos
  // casos ya se avisó de ellas.
  for (const e of candidates) quietUntil[causeKey(e)] = now + SUPPRESSION_WINDOW_MS;

  const next: NotifierState = { lastId: maxId, quietUntil };

  return candidates.length > MAX_INDIVIDUAL_PER_CYCLE
    ? { toNotify: [], collapsed: candidates.length, next }
    : { toNotify: candidates, collapsed: 0, next };
}
