import type { VisibilityState } from "@tanstack/react-table";
import type { ColumnDefinition } from "../types";

/**
 * Visibilidad inicial de columnas para TanStack Table.
 * Solo las columnas marcadas con `visible: false` arrancan ocultas; el resto
 * queda visible por defecto (no se agregan al estado). El usuario luego ajusta
 * la visibilidad desde el menú "Vista".
 */
export function initialColumnVisibility<T>(columns: ColumnDefinition<T>[]): VisibilityState {
  const state: VisibilityState = {};
  for (const col of columns) {
    if (col.visible === false) state[String(col.accessorKey)] = false;
  }
  return state;
}
