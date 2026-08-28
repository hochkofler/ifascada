import type { JSX } from "react";
import { cn } from "@/lib/utils";

export type ConnectivityState = "good" | "warn" | "bad";

/**
 * The "lamp" from web-ui's Live page -- a colored indicator, not a Badge with text, since
 * operators scan a dense grid of these at a glance.
 *
 * Dos correcciones sobre la version anterior:
 *
 * 1. Usaba `bg-emerald-500` / `bg-amber-500` / `bg-red-500`, paleta cruda de Tailwind, mientras
 *    su propio comentario afirmaba que se tematizaba desde globals.css -- lo cual no era cierto.
 *    Ahora sale de --success / --warning / --destructive, que ya tenian su variante on-dark
 *    calculada y no los usaba nadie.
 *
 * 2. El color era el UNICO portador de significado (WCAG 1.4.1). El `title` no llega de forma
 *    confiable a un lector de pantalla, y no ayuda en absoluto a quien no distingue verde de
 *    rojo -- que en una sala de control es exactamente el caso que no se puede fallar. Ahora
 *    cada estado tiene ademas su propia forma (circulo / rombo / cuadrado) y un nombre accesible.
 */
const STATE_CLASS: Record<ConnectivityState, string> = {
  good: "rounded-full bg-success",
  warn: "rotate-45 rounded-[2px] bg-warning",
  bad: "rounded-[2px] bg-destructive",
};

export function ConnectivityDot({
  state,
  title,
}: {
  state: ConnectivityState;
  title?: string;
}): JSX.Element {
  return (
    <span
      data-testid="connectivity-dot"
      data-state={state}
      title={title}
      // Sin `title` el punto es decorativo (el estado se lee en otra parte de la fila); con
      // `title`, es la unica fuente del estado y tiene que tener nombre accesible.
      role={title === undefined ? undefined : "img"}
      aria-label={title}
      aria-hidden={title === undefined ? true : undefined}
      className={cn("inline-block h-2.5 w-2.5 shrink-0", STATE_CLASS[state])}
    />
  );
}
