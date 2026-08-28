import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { ColumnFiltersState } from "@tanstack/react-table";
import { useTranslation } from "react-i18next";
import { fetchEdgesCurrent, fetchDevicesCurrent, fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { ContextBar } from "@/components/context-bar";
import { EdgesOnlineBadge } from "@/components/live/edges-online-badge";
import { EdgeDiagnosticsPanel } from "@/components/live/edge-diagnostics-panel";
import { useSseRefetchNudge } from "@/components/live/use-sse-refetch-nudge";
import {
  buildLiveRows,
  filterLiveRows,
  liveRowId,
  liveSubRows,
  type LiveRow,
} from "@/components/live/live-rows";
import { getLiveColumns } from "@/components/live/live-columns";
import { DataTable } from "@/components/data-table/DataTable";
import type { ServerState, ServerHandlers } from "@/components/data-table/types";
import { Button } from "@/components/ui/button";

/**
 * El poll de 2500 ms es la fuente de verdad; SSE solo adelanta el siguiente tick.
 *
 * No es una eleccion de conveniencia: un edge que se cae NO genera ningun evento, y su estado
 * `disconnected` lo deriva el servidor de `now - last_seen_at > 45s`. SSE puede contar lo que
 * pasa, no lo que dejo de pasar, asi que una vista puramente SSE mostraria un edge caido como
 * online para siempre. Ver docs/central-persistence-architecture.md, seccion de derivacion por
 * heartbeat: "el frontend debe refrescar los endpoints current periodicamente, no solo SSE".
 */
const LIVE_POLL_MS = 2500;

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
    refetchInterval: LIVE_POLL_MS,
  });
  const devicesQuery = useQuery({
    queryKey: ["live-devices", filter],
    queryFn: () => fetchDevicesCurrent(1000, filter),
    refetchInterval: LIVE_POLL_MS,
  });
  // Los tags se traen de una sola vez con el mismo filtro de contexto y se agrupan por device en
  // el cliente, en vez de una consulta por device al expandir: expandir es instantaneo, no hay
  // spinners por fila, y no se multiplican peticiones sobre el pool de conexiones HTTP/1.1 que
  // ya compite con las conexiones SSE de larga vida.
  const tagsQuery = useQuery({
    queryKey: ["live-tags", filter],
    queryFn: () => fetchTagsCurrent(2000, filter),
    refetchInterval: LIVE_POLL_MS,
  });

  useSseRefetchNudge(filter, ["live-edges", "live-devices", "live-tags"]);

  const [filters, setFilters] = useState<ColumnFiltersState>([]);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(25);
  const [diagnosticsEdge, setDiagnosticsEdge] = useState<{ edgeCode: string; site: string } | null>(
    null
  );

  const allRows = useMemo(
    () => buildLiveRows(devicesQuery.data ?? [], edgesQuery.data ?? [], tagsQuery.data ?? []),
    [devicesQuery.data, edgesQuery.data, tagsQuery.data]
  );
  const rows = useMemo(() => filterLiveRows(allRows, filters), [allRows, filters]);

  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const clampedPage = Math.min(page, pageCount - 1);
  const pageRows = useMemo(
    () => rows.slice(clampedPage * pageSize, (clampedPage + 1) * pageSize),
    [rows, clampedPage, pageSize]
  );

  const columns = useMemo(() => getLiveColumns(t), [t]);

  const serverState: ServerState = {
    page: clampedPage,
    pageSize,
    sorting: [],
    filters,
    globalFilter: "",
  };
  const serverHandlers: ServerHandlers = {
    onPageChange: setPage,
    onPageSizeChange: (size) => {
      setPageSize(size);
      setPage(0);
    },
    onSortingChange: () => undefined,
    onFiltersChange: (next) => {
      setFilters(next);
      setPage(0);
    },
    onGlobalFilterChange: () => undefined,
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-4">
        <ContextBar />
        <EdgesOnlineBadge edges={edgesQuery.data ?? []} />
      </div>
      <h1 className="text-lg font-semibold">{t("live.title")}</h1>
      <DataTable<LiveRow>
        data={pageRows}
        columns={columns}
        totalRows={rows.length}
        loading={devicesQuery.isPending || edgesQuery.isPending}
        error={devicesQuery.isError || edgesQuery.isError}
        serverState={serverState}
        serverHandlers={serverHandlers}
        showSearch={false}
        getSubRows={liveSubRows}
        getRowId={liveRowId}
        emptyState={{ title: t("live.noDevices") }}
        rowActions={(row) =>
          row.kind !== "tag" ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setDiagnosticsEdge({ edgeCode: row.edgeCode, site: row.site });
              }}
            >
              {t("live.columns.diagnostics")}
            </Button>
          ) : null
        }
      />
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
