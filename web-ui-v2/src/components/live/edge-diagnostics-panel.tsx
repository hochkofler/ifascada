import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { resetEdge } from "@/lib/edge-actions";
import { fetchEdgesCurrent, fetchEdgeEvents, fetchTagsCurrent, type OpsEvent, type TagCurrent } from "@/lib/api-client";
import { subscribeSse } from "@/lib/sse";
import { formatServerDateTime, formatServerTime } from "@/lib/datetime";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

type ResetState = "idle" | "sent" | "confirmed-recovered" | "error" | "timed-out-no-recovery";

// Polling schedule for post-reset recovery confirmation. Kept as named constants so the
// test's wall-clock timeout (edge-diagnostics-panel.test.tsx) has one place to stay in sync
// with if this ever changes.
const RESET_POLL_ATTEMPTS = 15;
const RESET_POLL_INTERVAL_MS = 2000;

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
  const [resetState, setResetState] = useState<ResetState>("idle");
  const [events, setEvents] = useState<OpsEvent[]>([]);
  const [eventsError, setEventsError] = useState(false);
  const [tags, setTags] = useState<TagCurrent[]>([]);

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

  // Plain fetch-on-open rather than react-query's useQuery: this panel can be mounted and
  // unit-tested standalone (see edge-diagnostics-panel.test.tsx), without a QueryClientProvider
  // ancestor. In the real app it's mounted under live.tsx, itself under main.tsx's top-level
  // QueryClientProvider, so this doesn't lose any caching that mattered here -- the event list
  // is only ever needed fresh, on panel open.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setEventsError(false);
    fetchEdgeEvents(edgeCode)
      .then((data) => {
        if (!cancelled) setEvents(data);
      })
      .catch(() => {
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
      fetchTagsCurrent(200, { edge: edgeCode }).then((data) => {
        if (!cancelled) setTags(data);
      }).catch(() => {
        // Telemetry fetch failures don't block the rest of the panel (reset, events) --
        // an empty list just falls through to the "no telemetry" empty state.
      });
    };
    load();
    const interval = setInterval(load, 2500);
    const lastSseLoadAt = { current: 0 };
    const unsubscribeSse = subscribeSse(() => {
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
    }, { edge: edgeCode, excludeRaw: true });
    return () => {
      cancelled = true;
      clearInterval(interval);
      unsubscribeSse();
    };
  }, [open, edgeCode]);

  async function handleReset() {
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
      await resetEdge({ site_code: site, edge_code: edgeCode, reason: "manual reset from diagnostics panel" });
    } catch {
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
    } catch {
      if (stillRelevant()) setResetState("error");
      return;
    }
    if (stillRelevant()) setResetState("timed-out-no-recovery");
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent>
        <SheetHeader>
          <SheetTitle>{edgeCode}</SheetTitle>
        </SheetHeader>
        <div className="flex flex-col gap-3 px-4">
          <Button
            onClick={() => {
              void handleReset();
            }}
            disabled={resetState === "sent"}
          >
            {t("live.diagnostics.reset")}
          </Button>
          {resetState === "sent" && (
            <p className="text-sm text-muted-foreground">{t("live.diagnostics.sent")}</p>
          )}
          {resetState === "confirmed-recovered" && (
            <p className="text-sm text-emerald-600">{t("live.diagnostics.confirmedRecovered")}</p>
          )}
          {resetState === "timed-out-no-recovery" && (
            <p className="text-sm text-amber-600">{t("live.diagnostics.timedOutNoRecovery")}</p>
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
              <li key={tg.tag_code} className="flex items-center justify-between gap-2 border-b py-1 font-mono">
                <span className="truncate">{tg.tag_code}</span>
                <span>{String(tg.value)}</span>
                <span className="text-muted-foreground">{tg.quality?.status ?? "-"}</span>
                <span className="text-muted-foreground">{tg.ts ? formatServerTime(tg.ts) : "-"}</span>
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
                <span className="text-muted-foreground">{formatServerDateTime(e.ts)}</span> [{e.severity}]{" "}
                {e.event_type} - {e.message}
              </li>
            ))}
          </ul>
        </div>
      </SheetContent>
    </Sheet>
  );
}
