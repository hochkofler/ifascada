import type { TFunction } from "i18next";
import { ConnectivityDot } from "@/components/live/connectivity-dot";
import { formatServerDateTime } from "@/lib/datetime";
import { ColumnDisplayType, type ColumnDefinition } from "@/components/data-table/types";
import type { LiveRow } from "./live-rows";

/**
 * Columnas de la grilla en vivo. Device y tag comparten columnas pero muestran cosas distintas,
 * asi que cada celda ramifica por `row.kind`. El texto ya viene resuelto en la fila (ver
 * live-rows.ts): aca solo se decide como se ve.
 */
export function getLiveColumns(t: TFunction): ColumnDefinition<LiveRow>[] {
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
          title={t("live.deviceStateTooltip", {
            state: row.quality || t("live.qualityUnknown"),
          })}
        />
      ),
    },
    {
      accessorKey: "code",
      header: t("live.columns.code"),
      type: ColumnDisplayType.String,
      cell: (_value, row) => (
        <span
          className={
            row.kind === "device"
              ? "font-mono text-xs font-medium"
              : "font-mono text-xs text-muted-foreground"
          }
        >
          {row.code}
        </span>
      ),
    },
    {
      accessorKey: "edge",
      header: t("live.edge"),
      type: ColumnDisplayType.String,
      width: 140,
      cell: (_value, row) => <span className="font-mono text-xs">{row.edge}</span>,
    },
    {
      accessorKey: "detail",
      header: t("live.columns.detail"),
      type: ColumnDisplayType.String,
      cell: (_value, row) => (
        <span
          className={
            row.kind === "tag" ? "font-mono text-xs tabular-nums" : "text-xs text-muted-foreground"
          }
        >
          {row.kind === "edge" ? t("live.noDevicesForEdge") : row.detail}
        </span>
      ),
    },
    {
      accessorKey: "quality",
      header: t("live.columns.quality"),
      type: ColumnDisplayType.String,
      width: 120,
      cell: (_value, row) => <span className="text-xs">{row.quality || "-"}</span>,
    },
    {
      accessorKey: "lastSeen",
      header: t("live.lastSeen"),
      type: ColumnDisplayType.String,
      width: 180,
      // En una fila de tag, `lastSeen` vacio significa "el mismo instante que el dispositivo"
      // (ver live-rows.ts), no "sin dato": se deja la celda en blanco para que se lea como
      // heredada. El "-" queda solo para cuando de verdad no hay fecha.
      cell: (_value, row) =>
        row.kind === "tag" && !row.lastSeen ? null : (
          <span className="font-mono text-xs text-muted-foreground">
            {row.lastSeen ? formatServerDateTime(row.lastSeen) : "-"}
          </span>
        ),
    },
  ];
}
