import type { TFunction } from "i18next";
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

/**
 * Converts the raw fetched history rows into `HistoryRow`s, deriving each `rowKey` as an
 * ordinal AMONG ROWS SHARING THE SAME `ts` (`${tag_code}-${ts}-${ordinalWithinSameTs}`) rather
 * than the row's position in the whole array.
 *
 * Why not array position (the original scheme): it's stable across pagination/filtering (both
 * slice/filter the already-built array without changing what's already in it), but NOT stable
 * across a background refetch. main.tsx's `QueryClient` uses default options (`staleTime: 0`,
 * `refetchOnWindowFocus: true`), so a background refetch that returns even one new sample
 * anywhere in the result renumbers every subsequent row's array-position key. Since selection.ts
 * keys selection by `rowKey`, previously-selected rows would silently uncheck (their old key no
 * longer matches any row) while the displayed "selected" count still included them, and
 * re-clicking the same physical row under its new key could double-add it to the print buffer --
 * a duplicated weight on a real printed ticket (see print-selected-button.tsx).
 *
 * The ordinal-within-`ts` scheme fixes this: `tag_code` and `ts` don't change across a refetch
 * for a given real sample, and a row's ordinal only depends on the relative order of OTHER rows
 * that share its exact `ts` -- an unrelated row with a different `ts` being inserted anywhere
 * else in the array (the shape of a background refetch picking up new samples) does not shift
 * it.
 */
export function toHistoryRows(rows: TagHistory[]): HistoryRow[] {
  const ordinalByTs = new Map<string, number>();
  return rows.map((row) => {
    const ordinal = ordinalByTs.get(row.ts) ?? 0;
    ordinalByTs.set(row.ts, ordinal + 1);
    const parsed = parseValueWithUnit(row.value);
    return {
      ...row,
      unit: parsed?.unit ?? "-",
      rowKey: `${row.tag_code}-${row.ts}-${String(ordinal)}`,
    };
  });
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
 *
 * A function of `t` rather than a module-scope constant: headers need translated text, and
 * `useTranslation()` can't be called from module scope (it's not a component/hook context) --
 * call this from within the route component, which does have `t`.
 */
export function getHistoryColumns(t: TFunction): ColumnDefinition<HistoryRow>[] {
  return [
    {
      accessorKey: "ts",
      header: t("history.timestamp"),
      type: ColumnDisplayType.String,
      cell: (value) => new Date(String(value)).toLocaleString(),
    },
    {
      accessorKey: "value",
      header: t("history.value"),
      type: ColumnDisplayType.Number,
      cell: (value) => {
        const parsed = parseValueWithUnit(value);
        return parsed ? String(parsed.number) : "-";
      },
    },
    {
      accessorKey: "unit",
      header: t("history.unit"),
      type: ColumnDisplayType.String,
    },
    {
      accessorKey: "quality_status",
      header: t("history.quality"),
      type: ColumnDisplayType.String,
    },
  ];
}
