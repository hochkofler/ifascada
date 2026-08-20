import { useState, useMemo, type JSX } from "react";
import { useTranslation } from "react-i18next";
import {
  useReactTable,
  getCoreRowModel,
  type Table,
  type RowSelectionState,
  type ColumnDef,
  type Updater,
  type PaginationState,
} from "@tanstack/react-table";
import { Checkbox } from "@/components/ui/checkbox";
import { getNumberLocale } from "./types";
import { DataTableContext } from "./DataTableContent";
import { buildColumns } from "./utils/buildColumns";
import { TABLES_NS } from "./i18n";
import type { DataTableRootProps } from "./types";

export function DataTableRoot<T extends object>({
  data,
  columns,
  totalRows,
  serverState,
  serverHandlers,
  selectable = false,
  locale = getNumberLocale(),
  maxHeight,
  onSelectionChange,
  children,
  defaultFilterable = true,
}: DataTableRootProps<T>): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  const boolTrue = t("boolean.true");
  const boolFalse = t("boolean.false");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});

  const tanstackColumns = useMemo<ColumnDef<T>[]>(() => {
    const cols = buildColumns<T>(columns, locale, defaultFilterable, {
      true: boolTrue,
      false: boolFalse,
    });
    if (selectable) {
      cols.unshift({
        id: "__select",
        enableSorting: false,
        size: 40,
        header: ({ table }) => (
          <Checkbox
            aria-label={t("toolbar.selectAll")}
            checked={table.getIsAllPageRowsSelected()}
            onCheckedChange={(v) => {
              table.toggleAllPageRowsSelected(v === true);
            }}
          />
        ),
        cell: ({ row }) => (
          <Checkbox
            aria-label={t("toolbar.selectRow")}
            checked={row.getIsSelected()}
            onCheckedChange={(v) => {
              row.toggleSelected(v === true);
            }}
          />
        ),
      } satisfies ColumnDef<T>);
    }
    return cols;
  }, [columns, selectable, locale, defaultFilterable, t, boolTrue, boolFalse]);

  const handlePaginationChange = (updater: Updater<PaginationState>) => {
    const current: PaginationState = {
      pageIndex: serverState.page,
      pageSize: serverState.pageSize,
    };
    const next = typeof updater === "function" ? updater(current) : updater;
    if (next.pageIndex !== current.pageIndex) serverHandlers.onPageChange(next.pageIndex);
    if (next.pageSize !== current.pageSize) serverHandlers.onPageSizeChange(next.pageSize);
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
    },
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
    pageCount: totalRows === 0 ? 0 : Math.ceil(totalRows / serverState.pageSize),
    getCoreRowModel: getCoreRowModel(),
  });

  return (
    <DataTableContext value={{ table: table as unknown as Table<object>, maxHeight }}>
      {children}
    </DataTableContext>
  );
}
