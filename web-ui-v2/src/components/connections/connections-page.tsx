import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { ColumnFiltersState } from "@tanstack/react-table";
import { useTranslation } from "react-i18next";
import { fetchConnectionsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { ContextBar } from "@/components/context-bar";
import { DataTable } from "@/components/data-table/DataTable";
import type { ServerState, ServerHandlers } from "@/components/data-table/types";
import { buildConnectionRows, connectionRowId } from "./connection-rows";
import { getConnectionColumns } from "./connections-columns";

/**
 * Conexiones: la capa de transporte entre el edge y sus dispositivos.
 *
 * El endpoint `/api/connections/current` existia en el backend desde siempre y el frontend nunca
 * lo llamaba, asi que las conexiones en `failed` no se veian en ninguna parte de la app. Un
 * operador veia dispositivos en `stale` sin manera de enterarse de que lo que se cayo era el
 * puerto que los transporta.
 *
 * Es una vista propia y no una columna de la grilla en vivo a proposito: en los datos reales el
 * enlace device -> conexion esta incompleto (de 40 dispositivos, 14 traen `connection_id` y solo
 * 5 apuntan a una conexion que existe), asi que colgarla de ahi mostraria vacio o, peor, algo
 * enganoso. Aca no depende de ese enlace.
 */
const CONNECTIONS_POLL_MS = 5000;

export function ConnectionsPage() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = {
    site,
    line: line || undefined,
    area: area || undefined,
    cell: cell || undefined,
    edge: edge || undefined,
  };

  // Mas lento que la grilla en vivo (2500 ms): el estado de una conexion cambia con transiciones
  // reales del enlace, no con cada muestra de telemetria.
  const connectionsQuery = useQuery({
    queryKey: ["connections", filter],
    queryFn: () => fetchConnectionsCurrent(200, filter),
    refetchInterval: CONNECTIONS_POLL_MS,
  });

  const [filters, setFilters] = useState<ColumnFiltersState>([]);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(25);

  const allRows = useMemo(
    () => buildConnectionRows(connectionsQuery.data ?? []),
    [connectionsQuery.data]
  );
  const rows = useMemo(() => {
    const active = filters.filter((f) => String(f.value ?? "").trim() !== "");
    if (active.length === 0) return allRows;
    return allRows.filter((row) =>
      active.every((f) => {
        const needle = String(f.value ?? "")
          .trim()
          .toLowerCase();
        const field = (row as unknown as Record<string, unknown>)[f.id];
        return String(field ?? "")
          .toLowerCase()
          .includes(needle);
      })
    );
  }, [allRows, filters]);

  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const clampedPage = Math.min(page, pageCount - 1);
  const pageRows = useMemo(
    () => rows.slice(clampedPage * pageSize, (clampedPage + 1) * pageSize),
    [rows, clampedPage, pageSize]
  );

  const columns = useMemo(() => getConnectionColumns(t), [t]);

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
      <ContextBar />
      <h1 className="text-lg font-semibold">{t("connections.title")}</h1>
      <DataTable
        data={pageRows}
        columns={columns}
        totalRows={rows.length}
        loading={connectionsQuery.isPending}
        error={connectionsQuery.isError}
        serverState={serverState}
        serverHandlers={serverHandlers}
        showSearch={false}
        getRowId={connectionRowId}
        emptyState={{ title: t("connections.empty") }}
      />
    </div>
  );
}
