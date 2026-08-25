import { ColumnDisplayType, type ColumnDefinition } from "@/components/data-table/types";
import type { TagHistory } from "@/lib/api-client";
import { parseValueWithUnit } from "@/lib/value-formatting";

/**
 * A `TagHistory` row plus the fields the History page derives client-side:
 * - `unit`: split out of the compound raw `value` (see value-formatting.ts) so it can be its
 *   own column, independent of the `value` column showing just the number.
 * - `rowKey`: a stable identity for a row that does NOT depend on its position within the
 *   current page's slice (unlike TanStack's default row id) -- see selection.ts. Needed so a
 *   shift-click range-select survives pagination.
 *
 * `value` itself is left untouched (still the raw, possibly-compound value) so print payloads
 * that need the original value (see print-selected-button.tsx) still get it verbatim.
 */
export type HistoryRow = TagHistory & { unit: string | null; rowKey: string };

export function toHistoryRow(row: TagHistory, index: number): HistoryRow {
  const parsed = parseValueWithUnit(row.value);
  return {
    ...row,
    unit: parsed?.unit ?? "-",
    rowKey: `${row.tag_code}-${row.ts}-${String(index)}`,
  };
}

/**
 * Column definitions for the History table, in the vendored DataTable system's own
 * `ColumnDefinition<T>` shape (accessorKey/header/type/cell) -- NOT TanStack's `createColumnHelper`
 * (the brief's illustrative sketch used that, but it doesn't match what `<DataTable>` actually
 * accepts as `columns`; confirmed against data-table/types.ts and data-table/data-table.test.tsx).
 *
 * `tag_code`/`site_code`/`edge_code` are deliberately omitted (spec: the History page is always
 * scoped to one already-selected tag, so repeating it per row is noise). `unit` is a column of
 * its own, separate from `value`.
 */
export const historyColumns: ColumnDefinition<HistoryRow>[] = [
  {
    accessorKey: "ts",
    header: "Timestamp",
    type: ColumnDisplayType.String,
    cell: (value) => new Date(String(value)).toLocaleString(),
  },
  {
    accessorKey: "value",
    header: "Value",
    type: ColumnDisplayType.Number,
    cell: (value) => {
      const parsed = parseValueWithUnit(value);
      return parsed ? String(parsed.number) : "-";
    },
  },
  {
    accessorKey: "unit",
    header: "Unit",
    type: ColumnDisplayType.String,
  },
  {
    accessorKey: "quality_status",
    header: "Quality",
    type: ColumnDisplayType.String,
  },
];
