import type React from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { ColumnDisplayType, type FilterType, getNumberLocale } from "../types";
import type { ColumnDefinition, FilterOption } from "../types";
import { formatCellValue, type BooleanCellLabels } from "../formatters";
import { resolveColumnFilter } from "./columnFilter";
import { DetailLinkCell } from "../DetailLinkCell";

declare module "@tanstack/react-table" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData, TValue> {
    filterable?: boolean;
    filterType?: FilterType;
    filterOptions?: FilterOption[];
    label?: string;
  }
}

export function buildColumns<T extends object>(
  definitions: ColumnDefinition<T>[],
  locale = getNumberLocale(),
  defaultFilterable = true,
  booleanLabels?: BooleanCellLabels
): ColumnDef<T>[] {
  return definitions.map((def) => {
    const filter = resolveColumnFilter(def, defaultFilterable);
    const col: ColumnDef<T> = {
      id: String(def.accessorKey),
      accessorKey: def.accessorKey as string,
      header: def.header,
      enableSorting: def.sortable ?? false,
      enableColumnFilter: filter.enabled,
      size: def.width,
      meta: {
        label: def.header,
        filterable: filter.enabled,
        filterType: filter.filterType,
        filterOptions: filter.filterOptions,
      },
      cell: ({ getValue, row }) => {
        const value = getValue();
        let content: React.ReactNode;
        if (def.cell) {
          content = def.cell(value, row.original);
        } else if (def.type === ColumnDisplayType.Boolean) {
          // Divergencia deliberada respecto de libs/tables/src/utils/buildColumns.tsx de ifahub,
          // que aca usa `bg-green-50 / text-green-700` y `bg-gray-50 / text-gray-600`: paleta
          // cruda de Tailwind, sin variante on-dark. Es el mismo defecto que la auditoria de
          // ifahub marca en otro lado ("instituciones usa dark:bg-blue-950 crudo en badges"),
          // y en modo oscuro pinta un verde muy claro sobre una tarjeta oscura. Con tokens
          // funciona en los dos temas. Vale proponerlo upstream.
          content = (
            <span
              className={
                value
                  ? "inline-flex items-center rounded-md border border-success/30 bg-success/15 px-2 py-1 text-xs font-medium text-success"
                  : "inline-flex items-center rounded-md border border-border bg-muted px-2 py-1 text-xs font-medium text-muted-foreground"
              }
            >
              {value ? booleanLabels?.true : booleanLabels?.false}
            </span>
          );
        } else {
          content = <span>{formatCellValue(value, def.type, locale, booleanLabels)}</span>;
        }

        const docRef = def.linkTo?.(row.original) ?? null;
        if (docRef) {
          return (
            <DetailLinkCell
              docRef={docRef}
              label={`${def.header} ${formatCellValue(value, def.type, locale, booleanLabels)}`.trim()}
            >
              {content}
            </DetailLinkCell>
          );
        }
        return content;
      },
    };
    return col;
  });
}
