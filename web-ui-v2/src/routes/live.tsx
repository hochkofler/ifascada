import { createFileRoute } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchEdgesCurrent, fetchTagsCurrent, type EdgeCurrent, type TagCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { ContextBar } from "@/components/context-bar";
import { EdgesOnlineBadge } from "@/components/live/edges-online-badge";
import { EdgeDiagnosticsPanel } from "@/components/live/edge-diagnostics-panel";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useTranslation } from "react-i18next";

// Mirrors EdgesOnlineBadge's own online-status formula (see that component's doc comment,
// verified against postgres.rs's edge_current_state upsert): anything not "online" is treated
// as disconnected/stale for the purpose of offering diagnostics.
const ONLINE_STATUSES = new Set(["online"]);

function isEdgeDisconnected(edge: EdgeCurrent): boolean {
  return !ONLINE_STATUSES.has(String(edge.status || "").toLowerCase());
}

export const Route = createFileRoute("/live")({
  component: LivePage,
});

type DeviceGroup = {
  key: string;
  deviceCode: string;
  edgeCode: string;
  tags: TagCurrent[];
};

function groupTagsByDevice(tags: TagCurrent[]): DeviceGroup[] {
  const m = new Map<string, DeviceGroup>();
  for (const t of tags) {
    const key = `${t.site_code}|${t.line_code ?? "-"}|${t.area_code ?? "-"}|${t.cell_code ?? "-"}|${t.device_code}`;
    if (!m.has(key)) {
      m.set(key, { key, deviceCode: t.device_code, edgeCode: t.edge_code, tags: [] });
    }
    m.get(key)!.tags.push(t);
  }
  return Array.from(m.values()).sort((a, b) => a.deviceCode.localeCompare(b.deviceCode));
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined) return "-";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function LivePage() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = {
    site,
    line: line || undefined,
    area: area || undefined,
    cell: cell || undefined,
    edge: edge || undefined,
  };
  const edges = useQuery({
    queryKey: ["live-edges", filter],
    queryFn: () => fetchEdgesCurrent(200, filter),
    refetchInterval: 2500,
  });
  const tags = useQuery({
    queryKey: ["live-tags", filter],
    queryFn: () => fetchTagsCurrent(1000, filter),
    refetchInterval: 2500,
  });

  const groups = useMemo(() => groupTagsByDevice(tags.data ?? []), [tags.data]);

  const [diagnosticsEdge, setDiagnosticsEdge] = useState<{ edgeCode: string; site: string } | null>(null);

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center gap-4">
        <ContextBar />
        <EdgesOnlineBadge edges={edges.data ?? []} />
      </div>
      <h1 className="text-lg font-semibold">{t("live.title")}</h1>
      {(edges.data ?? []).length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">{t("live.edgesCardTitle")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1">
            {(edges.data ?? []).map((e) => {
              const disconnected = isEdgeDisconnected(e);
              return (
                <div
                  key={`${e.site_code}|${e.edge_code}`}
                  role={disconnected ? "button" : undefined}
                  tabIndex={disconnected ? 0 : undefined}
                  onClick={
                    disconnected
                      ? () => setDiagnosticsEdge({ edgeCode: e.edge_code, site: e.site_code })
                      : undefined
                  }
                  onKeyDown={
                    disconnected
                      ? (ev) => {
                          if (ev.key === "Enter" || ev.key === " ") {
                            ev.preventDefault();
                            setDiagnosticsEdge({ edgeCode: e.edge_code, site: e.site_code });
                          }
                        }
                      : undefined
                  }
                  className={`flex items-center justify-between gap-2 rounded px-2 py-1 font-mono text-xs ${
                    disconnected ? "cursor-pointer hover:bg-accent" : ""
                  }`}
                >
                  <span>{e.edge_code}</span>
                  <span className="text-muted-foreground">{e.last_seen_at ?? "-"}</span>
                  <Badge variant={disconnected ? "destructive" : "default"}>{e.status}</Badge>
                </div>
              );
            })}
          </CardContent>
        </Card>
      )}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {groups.map((g) => (
          <Card key={g.key}>
            <CardHeader>
              <CardTitle className="flex items-center justify-between gap-2 font-mono text-sm">
                <span>{g.deviceCode}</span>
                <span className="text-xs text-muted-foreground">{g.edgeCode}</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-1">
              {g.tags.map((tg) => (
                <div key={tg.tag_code} className="flex items-center justify-between gap-2 font-mono text-xs">
                  <span className="truncate">{tg.tag_code}</span>
                  <span className="truncate" title={formatValue(tg.value)}>
                    {formatValue(tg.value)}
                  </span>
                  <Badge variant={String(tg.quality?.status ?? "").toLowerCase() === "good" ? "default" : "outline"}>
                    {tg.quality?.status ?? t("live.qualityUnknown")}
                  </Badge>
                </div>
              ))}
            </CardContent>
          </Card>
        ))}
        {groups.length === 0 && <p className="text-sm text-muted-foreground">{t("live.noData")}</p>}
      </div>
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
