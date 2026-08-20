import { getNumberLocale } from "../types";
import type { ColumnDefinition } from "../types";
import { formatCellValue, type BooleanCellLabels } from "../formatters";

function escapeCsvCell(value: string): string {
  if (value.includes(",") || value.includes('"') || value.includes("\n")) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

export function exportToCsv<T extends object>(
  filename: string,
  columns: ColumnDefinition<T>[],
  rows: T[],
  locale = getNumberLocale(),
  booleanLabels?: BooleanCellLabels
): void {
  const visibleColumns = columns.filter((col) => col.visible !== false && !col.cell);

  const header = visibleColumns.map((col) => escapeCsvCell(col.header)).join(",");

  const body = rows
    .map((row) =>
      visibleColumns
        .map((col) => {
          const value = row[col.accessorKey] as unknown;
          return escapeCsvCell(formatCellValue(value, col.type, locale, booleanLabels));
        })
        .join(",")
    )
    .join("\n");

  const csv = `${header}\n${body}`;
  const blob = new Blob(["﻿" + csv], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${filename}.csv`;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}
