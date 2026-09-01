import type { TFunction } from "i18next";
import { ConnectivityDot } from "@/components/live/connectivity-dot";
import { formatServerDateTime } from "@/lib/datetime";
import { ColumnDisplayType, type ColumnDefinition } from "@/components/data-table/types";
import type { ConnectionRow } from "./connection-rows";

export function getConnectionColumns(t: TFunction): ColumnDefinition<ConnectionRow>[] {
  return [
    {
      accessorKey: "lamp",
      header: "",
      type: ColumnDisplayType.String,
      width: 44,
      filterable: false,
      sortable: false,
      cell: (_value, row) => (
        <ConnectivityDot
          state={row.lamp}
          title={t("connections.stateTooltip", { state: row.state })}
        />
      ),
    },
    {
      accessorKey: "connectionId",
      header: t("connections.columns.connection"),
      type: ColumnDisplayType.String,
      cell: (_value, row) => (
        <span className="font-mono text-xs font-medium">{row.connectionId}</span>
      ),
    },
    {
      accessorKey: "edge",
      header: t("live.edge"),
      type: ColumnDisplayType.String,
      width: 160,
      cell: (_value, row) => <span className="font-mono text-xs">{row.edge}</span>,
    },
    {
      accessorKey: "state",
      header: t("connections.columns.state"),
      type: ColumnDisplayType.String,
      width: 140,
      cell: (_value, row) => <span className="text-xs">{row.state}</span>,
    },
    {
      accessorKey: "severity",
      header: t("connections.columns.severity"),
      type: ColumnDisplayType.String,
      width: 110,
      cell: (_value, row) => <span className="text-xs">{row.severity}</span>,
    },
    {
      // La columna que justifica la pantalla: cuando una conexion falla, aca esta el motivo.
      accessorKey: "message",
      header: t("connections.columns.message"),
      type: ColumnDisplayType.String,
      cell: (_value, row) => (
        <span className="text-xs text-muted-foreground">{row.message || "-"}</span>
      ),
    },
    {
      accessorKey: "lastChange",
      header: t("connections.columns.lastChange"),
      type: ColumnDisplayType.String,
      width: 180,
      cell: (_value, row) => (
        <span className="font-mono text-xs text-muted-foreground">
          {row.lastChange ? formatServerDateTime(row.lastChange) : "-"}
        </span>
      ),
    },
  ];
}
