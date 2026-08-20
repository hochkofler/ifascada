import { ColumnDisplayType, FilterType } from "../types";
import type { ColumnDefinition, FilterOption } from "../types";

/** Opciones por defecto para columnas booleanas sin opciones explícitas. */
export const BOOLEAN_FILTER_OPTIONS: FilterOption[] = [
  { label: "Sí", value: "true" },
  { label: "No", value: "false" },
];

/** Deriva el tipo de filtro (la UI) a partir del tipo de dato de la columna. */
export function deriveFilterType(displayType: ColumnDisplayType): FilterType {
  switch (displayType) {
    case ColumnDisplayType.Number:
    case ColumnDisplayType.Currency:
    case ColumnDisplayType.Percentage:
      return FilterType.Number;
    case ColumnDisplayType.Date:
    case ColumnDisplayType.DateTime:
      return FilterType.Date;
    case ColumnDisplayType.Boolean:
      return FilterType.Select;
    default:
      return FilterType.Text;
  }
}

export interface ResolvedColumnFilter {
  enabled: boolean;
  filterType: FilterType;
  filterOptions?: FilterOption[];
}

/**
 * Resuelve el filtrado de una columna desde su definición: el filtro se deriva
 * del tipo de dato y está habilitado por defecto (configurable con
 * `defaultEnabled`); se puede forzar por columna con `filterable`. Las columnas
 * booleanas sin opciones reciben Sí/No automáticamente.
 *
 * El filtrado es server-side: una columna filtrable cuyo endpoint no mapea el
 * filtro mostraría un control sin efecto → marcala `filterable: false` (o usá
 * `defaultEnabled: false` a nivel tabla y habilitá las soportadas).
 */
export function resolveColumnFilter<T>(
  def: ColumnDefinition<T>,
  defaultEnabled = true
): ResolvedColumnFilter {
  const filterType = def.filterType ?? deriveFilterType(def.type);
  let filterOptions = def.filterOptions;
  if (
    filterType === FilterType.Select &&
    !filterOptions &&
    def.type === ColumnDisplayType.Boolean
  ) {
    filterOptions = BOOLEAN_FILTER_OPTIONS;
  }
  return { enabled: def.filterable ?? defaultEnabled, filterType, filterOptions };
}
