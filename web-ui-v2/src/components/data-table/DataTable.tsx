import type { JSX } from "react";
import { useTranslation } from "react-i18next";
import { type Table } from "@tanstack/react-table";
import { Download } from "lucide-react";
import { getNumberLocale, type DataTableProps } from "./types";
import { exportToCsv } from "./utils/exportCsv";
import { useDataTableInstance } from "./hooks/useDataTableInstance";
import { DataTableContext, DataTableContent } from "./DataTableContent";
import { DataTableLoading } from "./DataTableLoading";
import { DataTableError } from "./DataTableError";
import { DataTableToolbar, DataTableFilterChip } from "./DataTableToolbar";
import { DataTableViewOptions } from "./DataTableViewOptions";
import { DataTableSavedViews } from "./DataTableSavedViews";
import { DataTableSearch } from "./DataTableSearch";
import { DataTablePagination } from "./DataTablePagination";
import { TABLES_NS } from "./i18n";

export function DataTable<T extends object>(props: DataTableProps<T>): JSX.Element {
  const {
    data,
    columns,
    totalRows,
    loading = false,
    error = false,
    serverState,
    serverHandlers,
    maxHeight,
    locale = getNumberLocale(),
    exportable,
    exportFilename = "export",
    showSearch = true,
    showViewOptions = true,
    emptyState,
    tableId,
  } = props;
  const { t } = useTranslation(TABLES_NS);
  const boolTrue = t("boolean.true");
  const boolFalse = t("boolean.false");
  const { table, tanstackColumns, density, setDensity } = useDataTableInstance(props);

  return (
    <DataTableContext
      value={{
        table: table as unknown as Table<object>,
        maxHeight,
        density,
        setDensity,
        emptyState,
      }}
    >
      <div className="flex flex-col gap-2">
        <DataTableToolbar>
          {showSearch && (
            <DataTableSearch
              value={serverState.globalFilter}
              onChange={serverHandlers.onGlobalFilterChange}
            />
          )}
          <DataTableFilterChip />
          {showViewOptions && (
            <DataTableViewOptions density={density} onDensityChange={setDensity} />
          )}
          {tableId && <DataTableSavedViews tableId={tableId} />}
          {exportable && (
            <button
              type="button"
              className="inline-flex items-center gap-1.5 rounded-md border border-input bg-background px-3 py-1.5 text-sm transition-colors hover:bg-accent"
              onClick={() => {
                exportToCsv(exportFilename, columns, data, locale, {
                  true: boolTrue,
                  false: boolFalse,
                });
              }}
            >
              <Download className="size-4" />
              {t("toolbar.exportCsv")}
            </button>
          )}
        </DataTableToolbar>

        <div className="rounded-md border">
          {loading ? (
            <DataTableLoading columnCount={tanstackColumns.length} />
          ) : error ? (
            <DataTableError />
          ) : (
            <DataTableContent />
          )}
        </div>

        <DataTablePagination
          serverState={serverState}
          serverHandlers={serverHandlers}
          totalRows={totalRows}
        />
      </div>
    </DataTableContext>
  );
}
