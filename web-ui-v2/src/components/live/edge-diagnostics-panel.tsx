import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  fetchEdgeEvents,
  fetchTagsCurrent,
  type OpsEvent,
  type TagCurrent,
} from "@/lib/api-client";
import { subscribeSse } from "@/lib/sse";
import { formatServerDateTime, formatServerTime } from "@/lib/datetime";
import { notify } from "@/components/notifications";
import { useEdgeReset } from "@/components/live/use-edge-reset";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

export function EdgeDiagnosticsPanel({
  edgeCode,
  site,
  open,
  onOpenChange,
}: {
  edgeCode: string;
  site: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  // El flujo de reset (comando + confirmacion por sondeo + estado) vive en use-edge-reset.ts:
  // es donde se sabe QUE fallo, y por eso es donde se llama a notify.
  const { resetState, runReset } = useEdgeReset(edgeCode, site);
  const [events, setEvents] = useState<OpsEvent[]>([]);
  const [eventsError, setEventsError] = useState(false);
  const [tags, setTags] = useState<TagCurrent[]>([]);

  // Plain fetch-on-open rather than react-query's useQuery: this panel can be mounted and
  // unit-tested standalone (see edge-diagnostics-panel.test.tsx), without a QueryClientProvider
  // ancestor. In the real app it's mounted under live.tsx, itself under main.tsx's top-level
  // QueryClientProvider, so this doesn't lose any caching that mattered here -- the event list
  // is only ever needed fresh, on panel open.
  //
  // Nota: al no pasar por react-query, estos fetch tampoco pasan por el `queryCache.onError`
  // global de lib/query-client.ts. Por eso registran su fallo a mano.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setEventsError(false);
    fetchEdgeEvents(edgeCode)
      .then((data) => {
        if (!cancelled) setEvents(data);
      })
      .catch((error: unknown) => {
        // La UI ya muestra su propio estado inline; esto ademas lo deja en el log de sesion,
        // sin toast, para que el fallo no desaparezca al cerrar el panel.
        notify.logApiError(error, {
          titleKey: "notifications:log.autoTitle.query",
          source: `edge.events.${edgeCode}`,
        });
        if (!cancelled) setEventsError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [open, edgeCode]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const load = () => {
      fetchTagsCurrent(200, { edge: edgeCode })
        .then((data) => {
          if (!cancelled) setTags(data);
        })
        .catch((error: unknown) => {
          // Telemetry fetch failures don't block the rest of the panel (reset, events) --
          // an empty list just falls through to the "no telemetry" empty state. Pero que no
          // bloquee no significa que deba perderse sin rastro: va al log, sin toast.
          notify.logApiError(error, {
            titleKey: "notifications:log.autoTitle.query",
            source: `edge.telemetry.${edgeCode}`,
          });
        });
    };
    load();
    const interval = setInterval(load, 2500);
    const lastSseLoadAt = { current: 0 };
    const unsubscribeSse = subscribeSse(
      () => {
        // Throttle to at most one SSE-triggered load per second -- without this, a
        // continuous per-edge telemetry stream (the real edge-sim fleet publishes
        // roughly one event every 25ms per edge across its 5 tags) calls load() on
        // every single message, far exceeding the existing 2.5s poll and adding to the
        // refetch storm that competes with the long-lived SSE EventSource connections for
        // the browser's small per-origin HTTP/1.1 connection pool (confirmed live during
        // Task 9 verification, producing failed/stuck requests and an empty grid).
        const now = Date.now();
        if (now - lastSseLoadAt.current < 1000) return;
        lastSseLoadAt.current = now;
        load();
      },
      { edge: edgeCode, excludeRaw: true }
    );
    return () => {
      cancelled = true;
      clearInterval(interval);
      unsubscribeSse();
    };
  }, [open, edgeCode]);

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent>
        <SheetHeader>
          <SheetTitle>{edgeCode}</SheetTitle>
        </SheetHeader>
        <div className="flex flex-col gap-3 px-4">
          <Button onClick={runReset} disabled={resetState === "sent"}>
            {t("live.diagnostics.reset")}
          </Button>
          {resetState === "sent" && (
            <p className="text-sm text-muted-foreground">{t("live.diagnostics.sent")}</p>
          )}
          {resetState === "confirmed-recovered" && (
            <p className="text-sm text-success">{t("live.diagnostics.confirmedRecovered")}</p>
          )}
          {resetState === "timed-out-no-recovery" && (
            <p className="text-sm text-warning">{t("live.diagnostics.timedOutNoRecovery")}</p>
          )}
          {resetState === "error" && (
            <p className="text-sm text-destructive">{t("live.diagnostics.error")}</p>
          )}

          <h3 className="mt-2 text-sm font-medium">{t("live.diagnostics.telemetry")}</h3>
          {tags.length === 0 && (
            <p className="text-sm text-muted-foreground">{t("live.diagnostics.noTelemetry")}</p>
          )}
          <ul className="flex flex-col gap-1 text-xs">
            {tags.map((tg) => (
              <li
                key={tg.tag_code}
                className="flex items-center justify-between gap-2 border-b py-1 font-mono"
              >
                <span className="truncate">{tg.tag_code}</span>
                <span>{String(tg.value)}</span>
                <span className="text-muted-foreground">{tg.quality?.status ?? "-"}</span>
                <span className="text-muted-foreground">
                  {tg.ts ? formatServerTime(tg.ts) : "-"}
                </span>
              </li>
            ))}
          </ul>

          <h3 className="mt-2 text-sm font-medium">{t("live.diagnostics.recentEvents")}</h3>
          {eventsError && (
            <p className="text-sm text-destructive">{t("live.diagnostics.eventsError")}</p>
          )}
          {!eventsError && events.length === 0 && (
            <p className="text-sm text-muted-foreground">{t("live.diagnostics.noEvents")}</p>
          )}
          <ul className="flex flex-col gap-1 overflow-y-auto text-xs">
            {events.map((e) => (
              <li key={e.id} className="border-b py-1 font-mono">
                <span className="text-muted-foreground">{formatServerDateTime(e.ts)}</span> [
                {e.severity}] {e.event_type} - {e.message}
              </li>
            ))}
          </ul>
        </div>
      </SheetContent>
    </Sheet>
  );
}
