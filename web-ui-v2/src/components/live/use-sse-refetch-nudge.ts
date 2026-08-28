import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { subscribeSse, type RtEvent } from "@/lib/sse";
import type { LiveFilter } from "@/lib/api-client";

/**
 * Escucha el stream SSE y, como mucho una vez por segundo, invalida las queries indicadas para
 * que el proximo tick del poll (que ya corre cada 2500 ms) dispare antes en vez de esperar el
 * intervalo completo.
 *
 * Extraido de LivePage porque el componente pasaba el limite de 150 lineas. La logica no cambia.
 */
export function useSseRefetchNudge(filter: LiveFilter, queryKeys: readonly string[]): void {
  const queryClient = useQueryClient();
  const pendingRef = useRef<Map<string, RtEvent>>(new Map());
  const lastInvalidateAtRef = useRef(0);
  const { site, line, area, cell, edge } = filter;

  useEffect(() => {
    const unsubscribe = subscribeSse(
      (evt) => {
        const payload = evt.payload as { tag_id?: string; device_id?: string } | undefined;
        const key = payload?.device_id ?? payload?.tag_id;
        if (!key) return;
        pendingRef.current.set(key, evt);
      },
      { site, line, area, cell, edge, excludeRaw: true }
    );
    const flush = setInterval(() => {
      if (pendingRef.current.size === 0) return;
      // Throttle to at most one SSE-triggered refetch round per second. Without this,
      // a continuous telemetry stream (the real edge-sim fleet publishes roughly one
      // event every 25ms) keeps this 120ms tick's pendingRef non-empty essentially
      // always, turning an intended "occasional nudge on top of the 2.5s poll" into a
      // refetch storm that competes with two long-lived SSE EventSource connections for
      // the browser's small per-origin HTTP/1.1 connection pool (confirmed live during
      // Task 9 verification: ~120ms actual request cadence instead of ~2.5s, producing
      // failed/stuck requests and an empty grid).
      const now = Date.now();
      if (now - lastInvalidateAtRef.current < 1000) return;
      pendingRef.current.clear();
      lastInvalidateAtRef.current = now;
      // A real-time nudge: invalidate so the next poll tick (already running every 2.5s) fires
      // sooner instead of waiting out the full interval. This deliberately does NOT hand-patch
      // individual device/edge objects in the cache -- reusing the same fetchDevicesCurrent/
      // fetchEdgesCurrent path that already normalizes and shapes this data keeps there being
      // exactly one code path that produces what the grid renders, matching the spec's "poll
      // stays authoritative" decision instead of maintaining a second, divergence-prone copy.
      for (const key of queryKeys) {
        queryClient.invalidateQueries({ queryKey: [key, filter] });
      }
    }, 120);
    return () => {
      clearInterval(flush);
      unsubscribe();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [site, line, area, cell, edge]);
}
