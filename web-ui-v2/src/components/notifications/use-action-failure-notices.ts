/**
 * Avisa en la aplicación cuando una acción pedida a un edge falla.
 *
 * Vive en el shell y no en una página, para que el aviso llegue estés donde estés. Toda la
 * decisión de QUÉ avisar es pura y está en `action-failure-notifier.ts`, con sus pruebas; acá
 * solo queda el sondeo y el efecto.
 *
 * Por qué esta señal y no el silencio de un edge: un edge callado solo significa un problema
 * en los que reportan de forma continua, y hoy el sistema no distingue esa modalidad. Que
 * alguien pida algo y falle significa lo mismo en todos los casos.
 */
import { useEffect, useRef } from "react";
import { fetchFailedActions } from "@/lib/api-client";
import { notify } from "./notify";
import { decide, initialState, type NotifierState } from "./action-failure-notifier";

/** Los fallos son raros: no hace falta el ritmo de 2,5 s que usa la vista En vivo. */
export const POLL_MS = 10_000;

const SOURCE = "ops.actionFailed";

export function useActionFailureNotices(pollMs: number = POLL_MS): void {
  const state = useRef<NotifierState>(initialState);

  useEffect(() => {
    let cancelled = false;

    async function check(): Promise<void> {
      let events;
      try {
        events = await fetchFailedActions();
      } catch {
        // Un sondeo fallido no se avisa: el cliente de la API ya registra los errores de red
        // en el log de sesión, y un canal que se queja de sí mismo es ruido.
        return;
      }
      if (cancelled) return;

      const { toNotify, collapsed, next } = decide(events, state.current, Date.now());
      state.current = next;

      if (collapsed > 0) {
        notify.warning("notifications.actionFailed.collapsed", {
          params: { count: collapsed },
          source: SOURCE,
        });
        return;
      }

      for (const e of toNotify) {
        notify.warning("notifications.actionFailed.title", {
          params: { edge: e.edge_code ?? "?" },
          description: e.message,
          source: SOURCE,
        });
      }
    }

    void check();
    const timer = setInterval(() => void check(), pollMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [pollMs]);
}
