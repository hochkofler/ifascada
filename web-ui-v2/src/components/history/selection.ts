/**
 * Shift-click range multi-select, built on top of a stable per-row key rather than the
 * vendored DataTable's native `selectable`/`onSelectionChange` mechanism.
 *
 * Why not the native mechanism: `useDataTableInstance` keys TanStack's `rowSelection` state by
 * position within the `data` array passed to the table for the CURRENT render (its default
 * `getRowId`, and the vendored component doesn't accept a custom one). Since `<DataTable>`'s
 * pagination is "manual" (the consumer must slice `data` to just the current page -- see
 * DataTablePagination/useDataTableInstance's `manualPagination: true`), that positional key is
 * only unique *within one page*. Row 0 on page 1 and row 0 on page 2 would collide, corrupting
 * a selection that's supposed to survive a shift-click spanning a page boundary (the exact
 * scenario this task's Step 11 verification exercises). Also, the native checkbox column's
 * `onCheckedChange` (Radix `Checkbox`) is a boolean-only callback with no access to the raw
 * click event's `shiftKey`.
 *
 * This module owns selection independently, keyed by `rowKey(row)` (History's `HistoryRow.rowKey`,
 * stable across pagination) and rendered through a plain, custom column with a native
 * `<input type="checkbox">` (see history.tsx) -- so the click event's `shiftKey` is directly
 * available, no Radix in the way.
 */

export function applySelectionClick<T>(
  selected: Map<string, T>,
  rows: T[],
  rowKey: (row: T) => string,
  clickedRow: T,
  lastClickedKey: string | null,
  shiftKey: boolean
): Map<string, T> {
  const next = new Map(selected);
  const clickedKey = rowKey(clickedRow);

  if (shiftKey && lastClickedKey !== null) {
    const lastIndex = rows.findIndex((r) => rowKey(r) === lastClickedKey);
    const clickedIndex = rows.findIndex((r) => rowKey(r) === clickedKey);
    if (lastIndex !== -1 && clickedIndex !== -1) {
      const [start, end] = lastIndex <= clickedIndex ? [lastIndex, clickedIndex] : [clickedIndex, lastIndex];
      for (let i = start; i <= end; i++) {
        const row = rows[i];
        next.set(rowKey(row), row);
      }
      return next;
    }
    // Anchor (or the clicked row itself) isn't in the current row set -- fall through to a
    // plain toggle rather than silently doing nothing.
  }

  if (next.has(clickedKey)) next.delete(clickedKey);
  else next.set(clickedKey, clickedRow);
  return next;
}
