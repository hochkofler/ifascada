import { useState, useContext, createContext, type CSSProperties, type JSX } from "react";
import { flexRender, type Table, type Column } from "@tanstack/react-table";
import { ChevronUp, ChevronDown, ChevronsUpDown, Filter } from "lucide-react";
import { useTranslation } from "react-i18next";
import { TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ColumnFilterInput } from "./DataTableFilterInput";
import { DataTableEmpty } from "./DataTableEmpty";
import { TABLES_NS } from "./i18n";
import type { EmptyStateConfig, TableDensity } from "./types";

export interface DataTableContextValue<T extends object = object> {
  table: Table<T>;
  maxHeight?: string;
  density?: TableDensity;
  setDensity?: (density: TableDensity) => void;
  emptyState?: EmptyStateConfig;
}

/** Clases por densidad: compacta reduce el alto del header y el padding vertical de las celdas. */
function densityClasses(density: TableDensity): { head: string; cell: string } {
  return density === "compact" ? { head: "h-8", cell: "px-2 py-1" } : { head: "", cell: "" };
}

/** Une clases condicionales (la primitiva Table ya hace tailwind-merge sobre `className`). */
function cx(...classes: (string | false | undefined)[]): string | undefined {
  const joined = classes.filter(Boolean).join(" ");
  return joined === "" ? undefined : joined;
}

export const DataTableContext = createContext<DataTableContextValue | null>(null);

export function useDataTableContext<T extends object = object>(): DataTableContextValue<T> {
  const ctx = useContext(DataTableContext) as DataTableContextValue<T> | null;
  if (!ctx) throw new Error("useDataTableContext debe usarse dentro de DataTable o DataTableRoot");
  return ctx;
}

function ColHeader({
  column,
  title,
  filterOpen,
  onToggleFilter,
}: {
  column: Column<object>;
  title: string;
  filterOpen: boolean;
  onToggleFilter: () => void;
}) {
  const { t } = useTranslation(TABLES_NS);
  const isFiltered = column.getIsFiltered();
  const filterColor =
    isFiltered || filterOpen
      ? "text-primary hover:text-primary/70"
      : "text-muted-foreground/30 hover:text-muted-foreground/70";

  return (
    <div className="flex items-center gap-1">
      {column.getCanSort() ? (
        <button
          type="button"
          className="flex items-center gap-1 text-left"
          onClick={column.getToggleSortingHandler()}
        >
          <span className="truncate">{title}</span>
          <span className="shrink-0 text-muted-foreground">
            {column.getIsSorted() === "asc" ? (
              <ChevronUp className="size-3.5" />
            ) : column.getIsSorted() === "desc" ? (
              <ChevronDown className="size-3.5" />
            ) : (
              <ChevronsUpDown className="size-3.5 opacity-40" />
            )}
          </span>
        </button>
      ) : (
        <span className="truncate">{title}</span>
      )}
      {column.getCanFilter() && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onToggleFilter();
          }}
          title={isFiltered ? t("content.filterActive") : t("content.filterColumn")}
          className={`shrink-0 rounded p-0.5 transition-colors ${filterColor}`}
        >
          <Filter className="size-2.5" />
        </button>
      )}
    </div>
  );
}

function stickyStyle(
  column: Column<object>,
  isHeader: boolean,
  isLastLeft: boolean
): CSSProperties {
  if (column.getIsPinned() !== "left") return {};
  return {
    position: "sticky",
    left: column.getStart("left"),
    zIndex: isHeader ? 20 : 1,
    boxShadow: isLastLeft ? "inset -1px 0 0 hsl(var(--border))" : undefined,
  };
}

export function DataTableContent(): JSX.Element {
  const { table, maxHeight, density = "compact", emptyState } = useDataTableContext();
  const densityCls = densityClasses(density);
  const [openFilterCols, setOpenFilterCols] = useState<Set<string>>(new Set());

  function toggleFilter(colId: string) {
    setOpenFilterCols((prev) => {
      const next = new Set(prev);
      if (next.has(colId)) next.delete(colId);
      else next.add(colId);
      return next;
    });
  }

  const headerGroups = table.getHeaderGroups();
  const allHeaders = headerGroups[0]?.headers ?? [];
  const rows = table.getRowModel().rows;
  const lastLeftId = table.getLeftLeafColumns().at(-1)?.id;

  return (
    <div className="overflow-auto" style={maxHeight ? { maxHeight } : undefined}>
      <table className="w-full caption-bottom text-sm">
        <TableHeader className="sticky top-0 z-10 bg-background shadow-sm">
          {headerGroups.map((headerGroup) => (
            <TableRow key={headerGroup.id}>
              {headerGroup.headers.map((header) => {
                const pinned = header.column.getIsPinned() === "left";
                return (
                  <TableHead
                    key={header.id}
                    style={{
                      width: header.getSize() !== 150 ? header.getSize() : undefined,
                      ...stickyStyle(header.column, true, lastLeftId === header.column.id),
                    }}
                    className={cx(pinned && "bg-background", densityCls.head, "relative")}
                  >
                    {header.isPlaceholder ? null : typeof header.column.columnDef.header ===
                      "string" ? (
                      <ColHeader
                        column={header.column}
                        title={header.column.columnDef.header}
                        filterOpen={openFilterCols.has(header.column.id)}
                        onToggleFilter={() => {
                          toggleFilter(header.column.id);
                        }}
                      />
                    ) : (
                      flexRender(header.column.columnDef.header, header.getContext())
                    )}
                    {header.column.getCanResize() && (
                      <div
                        onMouseDown={header.getResizeHandler()}
                        onTouchStart={header.getResizeHandler()}
                        onClick={(e) => {
                          e.stopPropagation();
                        }}
                        aria-hidden="true"
                        className={`absolute top-0 right-0 h-full w-1 cursor-col-resize touch-none select-none ${
                          header.column.getIsResizing()
                            ? "bg-primary"
                            : "bg-transparent hover:bg-primary/40"
                        }`}
                      />
                    )}
                  </TableHead>
                );
              })}
            </TableRow>
          ))}
          {openFilterCols.size > 0 && (
            <TableRow>
              {allHeaders.map((header) => {
                const pinned = header.column.getIsPinned() === "left";
                return (
                  <TableHead
                    key={`f-${header.id}`}
                    style={stickyStyle(header.column, true, lastLeftId === header.column.id)}
                    className={`py-1 px-2 border-t border-border/30 ${pinned ? "bg-background" : ""}`}
                  >
                    {openFilterCols.has(header.column.id) && header.column.getCanFilter() ? (
                      <ColumnFilterInput column={header.column} />
                    ) : null}
                  </TableHead>
                );
              })}
            </TableRow>
          )}
        </TableHeader>
        <TableBody>
          {rows.length === 0 ? (
            <TableRow>
              <TableCell colSpan={allHeaders.length || 1} className="p-0">
                <DataTableEmpty {...emptyState} />
              </TableCell>
            </TableRow>
          ) : (
            rows.map((row) => (
              <TableRow
                key={row.id}
                data-state={row.getIsSelected() ? "selected" : undefined}
                data-depth={row.depth > 0 ? row.depth : undefined}
                className={row.getIsSelected() ? "bg-muted/50" : undefined}
              >
                {row.getVisibleCells().map((cell, cellIndex) => {
                  const pinned = cell.column.getIsPinned() === "left";
                  // Indentacion de filas hijas: se aplica en la PRIMERA celda, no en la fila,
                  // para no romper la alineacion de las columnas siguientes ni el pinning.
                  const indent =
                    cellIndex === 0 && row.depth > 0 ? `${String(row.depth * 1.25)}rem` : undefined;
                  return (
                    <TableCell
                      key={cell.id}
                      style={{
                        width: cell.column.getSize() !== 150 ? cell.column.getSize() : undefined,
                        ...(indent === undefined ? {} : { paddingLeft: indent }),
                        ...stickyStyle(cell.column, false, lastLeftId === cell.column.id),
                      }}
                      className={cx(pinned && "bg-background", densityCls.cell)}
                    >
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </TableCell>
                  );
                })}
              </TableRow>
            ))
          )}
        </TableBody>
      </table>
    </div>
  );
}
