import { useState, useMemo, type Dispatch, type SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import {
  useReactTable,
  getCoreRowModel,
  type Table,
  type RowSelectionState,
  type ColumnDef,
  type Updater,
  type PaginationState,
  type VisibilityState,
} from "@tanstack/react-table";
import { Checkbox } from "@/components/ui/checkbox";
import { getNumberLocale } from "../types";
import type { DataTableProps, TableDensity } from "../types";
import { buildColumns } from "../utils/buildColumns";
import { initialColumnVisibility } from "../utils/columnVisibility";
import { TABLES_NS } from "../i18n";

export interface DataTableInstance<T extends object> {
  table: Table<T>;
  tanstackColumns: ColumnDef<T>[];
  density: TableDensity;
  setDensity: Dispatch<SetStateAction<TableDensity>>;
}

/**
 * Builds the TanStack table instance plus its derived columns and state for {@link DataTable}.
 * Extracted from the component (behavior-preserving) so the component body stays within the size
 * budget while the wiring lives in one place.
 */
export function useDataTableInstance<T extends object>(
  props: DataTableProps<T>
): DataTableInstance<T> {
  const {
    data,
    columns,
    totalRows,
    serverState,
    serverHandlers,
    selectable = false,
    locale = getNumberLocale(),
    rowActions,
    onSelectionChange,
    density: initialDensity,
    defaultFilterable = true,
  } = props;
  const { t } = useTranslation(TABLES_NS);
  const boolTrue = t("boolean.true");
  const boolFalse = t("boolean.false");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [density, setDensity] = useState<TableDensity>(initialDensity ?? "compact");
  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>(() =>
    initialColumnVisibility(columns)
  );

  const pinnedLeft = useMemo(
    () => columns.flatMap((c) => (c.pinned === "left" ? [String(c.accessorKey)] : [])),
    [columns]
  );

  const tanstackColumns = useMemo<ColumnDef<T>[]>(() => {
    const cols = buildColumns<T>(columns, locale, defaultFilterable, {
      true: boolTrue,
      false: boolFalse,
    });

    if (selectable) {
      cols.unshift({
        id: "__select",
        enableSorting: false,
        enableHiding: false,
        enableResizing: false,
        size: 40,
        header: ({ table }) => (
          <Checkbox
            checked={table.getIsAllPageRowsSelected()}
            onCheckedChange={(v) => {
              table.toggleAllPageRowsSelected(v === true);
            }}
            aria-label={t("toolbar.selectAll")}
          />
        ),
        cell: ({ row }) => (
          <Checkbox
            checked={row.getIsSelected()}
            onCheckedChange={(v) => {
              row.toggleSelected(v === true);
            }}
            aria-label={t("toolbar.selectRow")}
          />
        ),
      } satisfies ColumnDef<T>);
    }

    if (rowActions) {
      cols.push({
        id: "__actions",
        enableSorting: false,
        enableHiding: false,
        enableResizing: false,
        size: 80,
        header: "",
        cell: ({ row }) => <div className="flex justify-end">{rowActions(row.original)}</div>,
      } satisfies ColumnDef<T>);
    }

    return cols;
  }, [columns, selectable, rowActions, locale, defaultFilterable, t, boolTrue, boolFalse]);

  const handlePaginationChange = (updater: Updater<PaginationState>) => {
    const current: PaginationState = {
      pageIndex: serverState.page,
      pageSize: serverState.pageSize,
    };
    const next = typeof updater === "function" ? updater(current) : updater;
    if (next.pageIndex !== current.pageIndex) {
      serverHandlers.onPageChange(next.pageIndex);
    }
    if (next.pageSize !== current.pageSize) {
      serverHandlers.onPageSizeChange(next.pageSize);
    }
  };

  const table = useReactTable<T>({
    data,
    columns: tanstackColumns,
    state: {
      sorting: serverState.sorting,
      columnFilters: serverState.filters,
      globalFilter: serverState.globalFilter,
      pagination: {
        pageIndex: serverState.page,
        pageSize: serverState.pageSize,
      },
      rowSelection,
      columnVisibility,
      columnPinning: { left: pinnedLeft, right: [] },
    },
    onColumnVisibilityChange: setColumnVisibility,
    onSortingChange: (updater) => {
      const next = typeof updater === "function" ? updater(serverState.sorting) : updater;
      serverHandlers.onSortingChange(next);
    },
    onColumnFiltersChange: (updater) => {
      const next = typeof updater === "function" ? updater(serverState.filters) : updater;
      serverHandlers.onFiltersChange(next);
    },
    onGlobalFilterChange: (updater: string | ((old: string) => string)) => {
      const next = typeof updater === "function" ? updater(serverState.globalFilter) : updater;
      serverHandlers.onGlobalFilterChange(next);
    },
    onPaginationChange: handlePaginationChange,
    onRowSelectionChange: (updater) => {
      setRowSelection((prev) => {
        const next = typeof updater === "function" ? updater(prev) : updater;
        if (onSelectionChange) {
          const selected: T[] = [];
          for (const key of Object.keys(next)) {
            if (next[key] !== true) continue;
            const item = data[Number(key)];
            if (item !== undefined) selected.push(item);
          }
          onSelectionChange(selected);
        }
        return next;
      });
    },
    manualPagination: true,
    manualSorting: true,
    manualFiltering: true,
    // Resize de columnas en vivo (arrastrar el borde del header). Tamaño mín. para no colapsar.
    enableColumnResizing: true,
    columnResizeMode: "onChange",
    defaultColumn: { minSize: 60 },
    pageCount: totalRows === 0 ? 0 : Math.ceil(totalRows / serverState.pageSize),
    getCoreRowModel: getCoreRowModel(),
  });

  return { table, tanstackColumns, density, setDensity };
}
