import { createFileRoute } from "@tanstack/react-router";
import { useMemo, useState, useEffect, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { subscribeSse, type RtEvent } from "@/lib/sse";
import { fetchEdgesCurrent, fetchDevicesCurrent, type EdgeCurrent, type DeviceCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { edgeConnected, lampFromDeviceState } from "@/lib/connectivity";
import { formatServerDateTime } from "@/lib/datetime";
import { ContextBar } from "@/components/context-bar";
import { EdgesOnlineBadge } from "@/components/live/edges-online-badge";
import { EdgeDiagnosticsPanel } from "@/components/live/edge-diagnostics-panel";
import { ConnectivityDot } from "@/components/live/connectivity-dot";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { useTranslation } from "react-i18next";

export const Route = createFileRoute("/live")({
  component: LivePage,
});

type DeviceRow = {
  key: string;
  device: DeviceCurrent;
  edge: EdgeCurrent | undefined;
  lamp: "good" | "warn" | "bad";
};

function buildDeviceRows(devices: DeviceCurrent[], edges: EdgeCurrent[]): DeviceRow[] {
  const edgeByCode = new Map(edges.map((e) => [e.edge_code, e]));
  return devices
    .map((d) => {
      const edge = edgeByCode.get(d.edge_code);
      const conn = edgeConnected(edge);
      return { key: `${d.edge_code}|${d.device_code}`, device: d, edge, lamp: lampFromDeviceState(d, conn) };
    })
    .sort((a, b) => a.device.device_code.localeCompare(b.device.device_code));
}

export function LivePage() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = {
    site,
    line: line || undefined,
    area: area || undefined,
    cell: cell || undefined,
    edge: edge || undefined,
  };
  const edgesQuery = useQuery({
    queryKey: ["live-edges", filter],
    queryFn: () => fetchEdgesCurrent(200, filter),
    refetchInterval: 2500,
  });
  const devicesQuery = useQuery({
    queryKey: ["live-devices", filter],
    queryFn: () => fetchDevicesCurrent(1000, filter),
    refetchInterval: 2500,
  });

  const queryClient = useQueryClient();
  const pendingRef = useRef<Map<string, RtEvent>>(new Map());
  const lastInvalidateAtRef = useRef(0);

  useEffect(() => {
    const unsubscribe = subscribeSse(
      (evt) => {
        const payload = evt.payload as { tag_id?: string; device_id?: string } | undefined;
        const key = payload?.device_id ?? payload?.tag_id;
        if (!key) return;
        pendingRef.current.set(key, evt);
      },
      { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined, excludeRaw: true }
    );
    const flush = setInterval(() => {
      if (pendingRef.current.size === 0) return;
      pendingRef.current.clear();
      // Throttle to at most one SSE-triggered refetch round per second. Without this,
      // a continuous telemetry stream (the real edge-sim fleet publishes roughly one
      // event every 25ms) keeps this 120ms tick's pendingRef non-empty essentially
      // always, turning an intended "occasional nudge on top of the 2.5s poll" into a
      // refetch storm that exhausts central-server's rate limiter (confirmed live
      // during Task 9 verification: ~120ms actual request cadence instead of ~2.5s).
      const now = Date.now();
      if (now - lastInvalidateAtRef.current < 1000) return;
      lastInvalidateAtRef.current = now;
      // A real-time nudge: invalidate so the next poll tick (already running every 2.5s) fires
      // sooner instead of waiting out the full interval. This deliberately does NOT hand-patch
      // individual device/edge objects in the cache -- reusing the same fetchDevicesCurrent/
      // fetchEdgesCurrent path that already normalizes and shapes this data keeps there being
      // exactly one code path that produces what the grid renders, matching the spec's "poll
      // stays authoritative" decision instead of maintaining a second, divergence-prone copy.
      queryClient.invalidateQueries({ queryKey: ["live-edges", filter] });
      queryClient.invalidateQueries({ queryKey: ["live-devices", filter] });
    }, 120);
    return () => {
      clearInterval(flush);
      unsubscribe();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [site, line, area, cell, edge]);

  const rows = useMemo(
    () => buildDeviceRows(devicesQuery.data ?? [], edgesQuery.data ?? []),
    [devicesQuery.data, edgesQuery.data]
  );

  const [diagnosticsEdge, setDiagnosticsEdge] = useState<{ edgeCode: string; site: string } | null>(null);

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center gap-4">
        <ContextBar />
        <EdgesOnlineBadge edges={edgesQuery.data ?? []} />
      </div>
      <h1 className="text-lg font-semibold">{t("live.title")}</h1>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">{t("live.devicesCardTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1">
          {rows.map((r) => (
            <div
              key={r.key}
              role="button"
              tabIndex={0}
              onClick={() => setDiagnosticsEdge({ edgeCode: r.device.edge_code, site: r.device.site_code })}
              onKeyDown={(ev) => {
                if (ev.key === "Enter" || ev.key === " ") {
                  ev.preventDefault();
                  setDiagnosticsEdge({ edgeCode: r.device.edge_code, site: r.device.site_code });
                }
              }}
              className="flex cursor-pointer items-center gap-3 rounded px-2 py-1 font-mono text-xs hover:bg-accent"
            >
              <ConnectivityDot state={r.lamp} title={t("live.deviceStateTooltip", { state: r.device.state || t("live.qualityUnknown") })} />
              <span className="min-w-0 flex-1 truncate">{r.device.device_code}</span>
              <span className="text-muted-foreground">{r.device.edge_code}</span>
              <span className="text-muted-foreground">
                {r.device.last_seen_at ? formatServerDateTime(r.device.last_seen_at) : "-"}
              </span>
            </div>
          ))}
          {rows.length === 0 && <p className="text-sm text-muted-foreground">{t("live.noDevices")}</p>}
        </CardContent>
      </Card>
      {diagnosticsEdge && (
        <EdgeDiagnosticsPanel
          edgeCode={diagnosticsEdge.edgeCode}
          site={diagnosticsEdge.site}
          open={diagnosticsEdge !== null}
          onOpenChange={(nextOpen) => {
            if (!nextOpen) setDiagnosticsEdge(null);
          }}
        />
      )}
    </div>
  );
}
