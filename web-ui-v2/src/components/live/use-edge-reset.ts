import { useEffect, useRef, useState } from "react";
import { fetchEdgesCurrent } from "@/lib/api-client";
import { resetEdge } from "@/lib/edge-actions";
import { notify } from "@/components/notifications";

export type ResetState =
  "idle" | "sent" | "confirmed-recovered" | "error" | "timed-out-no-recovery";

// Polling schedule for post-reset recovery confirmation. Kept as named constants so the
// test's wall-clock timeout (edge-diagnostics-panel.test.tsx) has one place to stay in sync
// with if this ever changes.
const RESET_POLL_ATTEMPTS = 15;
const RESET_POLL_INTERVAL_MS = 2000;

export interface EdgeReset {
  resetState: ResetState;
  runReset: () => void;
}

/**
 * El flujo de reset de un edge: comando, confirmacion de recuperacion por sondeo, y el estado
 * que el panel muestra. Extraido de EdgeDiagnosticsPanel porque el componente pasaba el limite
 * de 150 lineas, y porque este es el unico lugar del flujo que sabe QUE fallo.
 *
 * Los `catch` de este flujo antes eran `catch { setResetState("error") }` -- sin siquiera ligar
 * el error. El operador que reseteaba un edge y fallaba veia la palabra "error" y nada mas: ni
 * status HTTP, ni motivo, ni nada que pasarle a Sistemas. Ahora cada fallo va tambien por
 * `notify.apiError`, que levanta el toast y lo deja en el log de sesion (la campana) con lo que
 * `ApiError` traiga.
 */
export function useEdgeReset(edgeCode: string, site: string): EdgeReset {
  const [resetState, setResetState] = useState<ResetState>("idle");

  // Guards setState calls made after this component has unmounted (e.g. the Sheet is closed
  // mid-poll) or after the target edge has changed out from under an in-flight poll loop --
  // without this, a stale poll from a previous edge/mount could still call setResetState later.
  const aliveForRef = useRef<string | null>(null);

  // Reset feedback is specific to one edge -- if the panel is switched directly from one
  // disconnected edge's row to another's (without closing in between, see live.tsx), stale
  // feedback text from the previous edge's reset attempt shouldn't linger for the new one.
  useEffect(() => {
    aliveForRef.current = edgeCode;
    setResetState("idle");
    return () => {
      if (aliveForRef.current === edgeCode) aliveForRef.current = null;
    };
  }, [edgeCode]);

  async function handleReset(): Promise<void> {
    const forEdge = edgeCode;
    const stillRelevant = () => aliveForRef.current === forEdge;
    setResetState("sent");
    let lastSeenBefore: string | undefined;
    try {
      // The pre-reset probe is inside this try too: it's a real network call made while
      // diagnosing a network problem, so it can plausibly reject. If it did while it lived
      // outside this block, the rejection was unhandled and resetState stayed stuck at "sent"
      // forever (disabled={resetState === "sent"} never clears) with nothing actually sent.
      const before = await fetchEdgesCurrent(1, { edge: edgeCode });
      lastSeenBefore = before[0]?.last_seen_at;
      await resetEdge({
        site_code: site,
        edge_code: edgeCode,
        reason: "manual reset from diagnostics panel",
      });
    } catch (error) {
      notify.apiError(error, "live.diagnostics.error", { source: `edge.reset.${edgeCode}` });
      if (stillRelevant()) setResetState("error");
      return;
    }
    // Poll for real recovery evidence -- the initial accepted:true only means the MQTT publish
    // succeeded, not that the edge came back (see lib/edge-actions.ts's resetEdge doc comment,
    // Task 13's finding). Give it a bounded window matching this project's established
    // health-poll pattern (scripts/lib/DeployDockerService.ps1's Test-ServiceHealthy).
    try {
      // This loop makes RESET_POLL_ATTEMPTS real network calls in a row, right after
      // commanding a reset on a network that's already having problems -- much more likely
      // to hit a rejected fetch than the single pre-reset probe above. Without this try/catch,
      // a rejection here is unhandled and resetState stays stuck at "sent" forever, same
      // failure mode the pre-reset probe was already fixed for.
      for (let attempt = 0; attempt < RESET_POLL_ATTEMPTS; attempt++) {
        await new Promise((r) => setTimeout(r, RESET_POLL_INTERVAL_MS));
        if (!stillRelevant()) return;
        const after = await fetchEdgesCurrent(1, { edge: edgeCode });
        if (after[0]?.last_seen_at && after[0].last_seen_at !== lastSeenBefore) {
          if (stillRelevant()) setResetState("confirmed-recovered");
          return;
        }
      }
    } catch (error) {
      // El comando ya salio; lo que fallo es la confirmacion. Se distingue en el log de sesion
      // con otro `source`, porque operativamente no es lo mismo: el edge puede haberse
      // recuperado igual.
      notify.apiError(error, "live.diagnostics.error", {
        source: `edge.reset.confirm.${edgeCode}`,
      });
      if (stillRelevant()) setResetState("error");
      return;
    }
    if (stillRelevant()) setResetState("timed-out-no-recovery");
  }

  return {
    resetState,
    runReset: () => {
      void handleReset();
    },
  };
}
