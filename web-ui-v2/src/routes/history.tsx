import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { fetchTagHistory, fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { useAutoSelectFirst } from "@/lib/use-auto-select-first";
import { numericValue } from "@/lib/value-formatting";
import { historyColumns, toHistoryRow, type HistoryRow } from "@/components/history/history-columns";
import { applySelectionClick } from "@/components/history/selection";
import { PrintSelectedButton } from "@/components/history/print-selected-button";
import { DataTable } from "@/components/data-table/DataTable";
import { ColumnDisplayType, type ColumnDefinition, type ServerState, type ServerHandlers } from "@/components/data-table/types";
import { ContextBar } from "@/components/context-bar";
import { Input } from "@/components/ui/input";

// Upper bound on how much history is pulled per tag selection -- matches web-ui's History page
// (see web-ui/app/history/page.tsx). The API has no date-range parameter, so this is effectively
// "how far back an interactive session can reach" for a single tag.
const HISTORY_FETCH_LIMIT = 2000;

export const Route = createFileRoute("/history")({
  component: HistoryPage,
});

function rowKeyOf(row: HistoryRow): string {
  return row.rowKey;
}

function HistoryPage() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const [selectedTag, setSelectedTag] = useState("");
  const [minValue, setMinValue] = useState(0);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(25);
  const [selected, setSelected] = useState<Map<string, HistoryRow>>(new Map());
  const [lastClickedKey, setLastClickedKey] = useState<string | null>(null);

  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const tags = useQuery({ queryKey: ["history-tags", filter], queryFn: () => fetchTagsCurrent(500, filter) });
  const tagCodes = useMemo(() => (tags.data ?? []).map((tg) => tg.tag_code), [tags.data]);
  useAutoSelectFirst(tagCodes, selectedTag, setSelectedTag);

  const history = useQuery({
    queryKey: ["history-events", selectedTag, HISTORY_FETCH_LIMIT],
    queryFn: () => fetchTagHistory(selectedTag, HISTORY_FETCH_LIMIT, 0),
    enabled: Boolean(selectedTag),
  });

  // rowKey is derived from each row's position in the RAW fetched set (before the Value>x
  // filter), so it stays stable as `minValue` changes -- only a different tag selection (a
  // different underlying fetch) changes it.
  const allRows = useMemo(
    () => (history.data ?? []).map((r, i) => toHistoryRow(r, i)),
    [history.data]
  );
  const filteredRows = useMemo(
    () =>
      allRows.filter((r) => {
        const n = numericValue(r.value);
        return n !== null && n > minValue;
      }),
    [allRows, minValue]
  );

  const pageCount = Math.max(1, Math.ceil(filteredRows.length / pageSize));
  const clampedPage = Math.min(page, pageCount - 1);
  const currentPageRows = useMemo(
    () => filteredRows.slice(clampedPage * pageSize, (clampedPage + 1) * pageSize),
    [filteredRows, clampedPage, pageSize]
  );

  // A tag switch invalidates any prior selection (rows from a different tag are meaningless to
  // keep around); the Value>x filter and pagination must NOT clear it -- that's the whole point
  // of keying selection by HistoryRow.rowKey instead of by page position (see selection.ts).
  useEffect(() => {
    setSelected(new Map());
    setLastClickedKey(null);
  }, [selectedTag]);
  useEffect(() => {
    setPage(0);
  }, [selectedTag, minValue, pageSize]);

  function handleSelectClick(row: HistoryRow, shiftKey: boolean) {
    setSelected((prev) => applySelectionClick(prev, filteredRows, rowKeyOf, row, lastClickedKey, shiftKey));
    setLastClickedKey(row.rowKey);
  }

  // Prepended to historyColumns. Deliberately NOT the vendored DataTable's native
  // `selectable`/`onSelectionChange` checkbox column -- see selection.ts for why (its
  // TanStack-default row id is only unique within one page, and its Radix Checkbox swallows
  // `shiftKey`). This is a plain column with a native <input type="checkbox">, giving direct
  // access to the click event and full control over the stable-keyed selection state.
  const columns: ColumnDefinition<HistoryRow>[] = useMemo(
    () => [
      {
        accessorKey: "rowKey",
        header: "",
        type: ColumnDisplayType.String,
        width: 32,
        cell: (_value, row) => (
          <input
            type="checkbox"
            checked={selected.has(row.rowKey)}
            onChange={() => {
              /* selection logic runs in onClick, which fires first and carries shiftKey */
            }}
            onClick={(e) => {
              e.stopPropagation();
              handleSelectClick(row, e.shiftKey);
            }}
            aria-label={t("history.selectedCount")}
          />
        ),
      },
      ...historyColumns,
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [selected, lastClickedKey, filteredRows]
  );

  const serverState: ServerState = { page: clampedPage, pageSize, sorting: [], filters: [], globalFilter: "" };
  const serverHandlers: ServerHandlers = {
    onPageChange: setPage,
    onPageSizeChange: (size) => {
      setPageSize(size);
    },
    onSortingChange: () => {
      /* no sortable columns on this page */
    },
    onFiltersChange: () => {
      /* per-column filtering is disabled (defaultFilterable=false) -- only Value>x applies */
    },
    onGlobalFilterChange: () => {
      /* global search is disabled (showSearch=false) -- not wired to manual filtering */
    },
  };

  const selectedRows = useMemo(() => Array.from(selected.values()), [selected]);
  const selectedTagObj = useMemo(
    () => (tags.data ?? []).find((tg) => tg.tag_code === selectedTag),
    [tags.data, selectedTag]
  );

  return (
    <div className="p-4 space-y-4">
      <ContextBar />
      <h1 className="text-lg font-semibold">{t("history.title")}</h1>
      <div className="flex flex-wrap items-end gap-4">
        <label className="flex flex-col gap-1 text-sm">
          <span>{t("history.tag")}</span>
          <select
            className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"
            value={selectedTag}
            onChange={(e) => {
              setSelectedTag(e.target.value);
            }}
          >
            {(tags.data ?? []).map((tg) => (
              <option key={tg.tag_code} value={tg.tag_code}>
                {tg.tag_code} ({tg.device_code})
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span>{t("history.valueFilter")}</span>
          <Input
            type="number"
            className="w-28"
            value={minValue}
            onChange={(e) => {
              setMinValue(Number.parseFloat(e.target.value) || 0);
            }}
          />
        </label>
        <span className="text-sm text-muted-foreground">
          {t("history.selectedCount")}: {selectedRows.length}
        </span>
        <PrintSelectedButton selectedRows={selectedRows} tag={selectedTagObj} fallbackSite={site} />
      </div>
      <DataTable
        data={currentPageRows}
        columns={columns}
        totalRows={filteredRows.length}
        loading={history.isLoading && Boolean(selectedTag)}
        error={history.isError}
        serverState={serverState}
        serverHandlers={serverHandlers}
        showSearch={false}
        defaultFilterable={false}
      />
    </div>
  );
}
