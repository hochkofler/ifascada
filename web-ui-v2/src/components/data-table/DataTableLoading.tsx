import type { JSX } from "react";

interface DataTableLoadingProps {
  columnCount?: number;
  rowCount?: number;
}

// Decorative loading placeholder: hidden from assistive tech (aria-hidden) and built from plain
// divs, so it exposes no empty table cells/controls and no focusable targets to screen readers.
export function DataTableLoading({
  columnCount = 5,
  rowCount = 10,
}: DataTableLoadingProps): JSX.Element {
  return (
    <div className="w-full overflow-x-auto text-sm" aria-hidden="true">
      <div className="flex gap-3 border-b p-3">
        {Array.from({ length: columnCount }).map((_, i) => (
          <div key={i} className="h-4 w-24 animate-pulse rounded bg-muted" />
        ))}
      </div>
      {Array.from({ length: rowCount }).map((_, rowIdx) => (
        <div key={rowIdx} className="flex gap-3 border-b p-3">
          {Array.from({ length: columnCount }).map((_, colIdx) => (
            <div key={colIdx} className="h-4 flex-1 animate-pulse rounded bg-muted" />
          ))}
        </div>
      ))}
    </div>
  );
}
