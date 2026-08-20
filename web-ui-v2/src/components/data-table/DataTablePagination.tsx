import type { JSX } from "react";
import { ChevronFirst, ChevronLast, ChevronLeft, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatQuantity } from "./types";
import { TABLES_NS } from "./i18n";
import type { ServerHandlers, ServerState } from "./types";

const PAGE_SIZE_OPTIONS = [10, 25, 50, 100] as const;

interface DataTablePaginationProps {
  serverState: ServerState;
  serverHandlers: ServerHandlers;
  totalRows: number;
}

export function DataTablePagination({
  serverState,
  serverHandlers,
  totalRows,
}: DataTablePaginationProps): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  const { page, pageSize } = serverState;
  const pageCount = Math.max(1, Math.ceil(totalRows / pageSize));
  const from = totalRows === 0 ? 0 : page * pageSize + 1;
  const to = Math.min((page + 1) * pageSize, totalRows);

  const btnClass =
    "inline-flex size-8 items-center justify-center rounded-md border border-input text-sm transition-colors hover:bg-accent disabled:pointer-events-none disabled:opacity-40";

  return (
    <div className="flex flex-wrap items-center justify-between gap-4 px-1 py-3">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <span>{t("pagination.rowsPerPage")}</span>
        <select
          className="h-8 rounded-md border border-input bg-background px-2 text-sm"
          value={pageSize}
          onChange={(e) => {
            serverHandlers.onPageSizeChange(Number(e.target.value));
          }}
        >
          {PAGE_SIZE_OPTIONS.map((size) => (
            <option key={size} value={size}>
              {size}
            </option>
          ))}
        </select>
      </div>

      <p className="text-sm text-muted-foreground">
        {totalRows === 0
          ? t("pagination.noResults")
          : t("pagination.showing", { from, to, total: formatQuantity(totalRows) })}
      </p>

      <div className="flex items-center gap-1">
        <button
          type="button"
          className={btnClass}
          onClick={() => {
            serverHandlers.onPageChange(0);
          }}
          disabled={page === 0}
          aria-label={t("pagination.firstPage")}
        >
          <ChevronFirst className="size-4" />
        </button>
        <button
          type="button"
          className={btnClass}
          onClick={() => {
            serverHandlers.onPageChange(page - 1);
          }}
          disabled={page === 0}
          aria-label={t("pagination.previousPage")}
        >
          <ChevronLeft className="size-4" />
        </button>
        <span className="px-2 text-sm">
          {t("pagination.pageOf", { page: page + 1, count: pageCount })}
        </span>
        <button
          type="button"
          className={btnClass}
          onClick={() => {
            serverHandlers.onPageChange(page + 1);
          }}
          disabled={page >= pageCount - 1}
          aria-label={t("pagination.nextPage")}
        >
          <ChevronRight className="size-4" />
        </button>
        <button
          type="button"
          className={btnClass}
          onClick={() => {
            serverHandlers.onPageChange(pageCount - 1);
          }}
          disabled={page >= pageCount - 1}
          aria-label={t("pagination.lastPage")}
        >
          <ChevronLast className="size-4" />
        </button>
      </div>
    </div>
  );
}
