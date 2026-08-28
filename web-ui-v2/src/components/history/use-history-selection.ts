import { useEffect, useState } from "react";
import type { HistoryRow } from "./history-columns";
import { applySelectionClick } from "./selection";

export interface HistorySelection {
  selected: Map<string, HistoryRow>;
  lastClickedKey: string | null;
  handleSelectClick: (row: HistoryRow, shiftKey: boolean) => void;
}

function rowKeyOf(row: HistoryRow): string {
  return row.rowKey;
}

/**
 * Estado de seleccion de filas del historico, extraido de HistoryPage porque el componente pasaba
 * el limite de 150 lineas.
 *
 * La regla que encapsula: cambiar de tag invalida la seleccion (filas de otro tag no significan
 * nada), pero el filtro "Valor > x" y la paginacion NO la tocan -- que es justamente para lo que
 * la seleccion se indexa por `HistoryRow.rowKey` y no por posicion en la pagina (ver selection.ts).
 */
export function useHistorySelection(
  selectedTag: string,
  filteredRows: readonly HistoryRow[]
): HistorySelection {
  const [selected, setSelected] = useState<Map<string, HistoryRow>>(new Map());
  const [lastClickedKey, setLastClickedKey] = useState<string | null>(null);

  useEffect(() => {
    setSelected(new Map());
    setLastClickedKey(null);
  }, [selectedTag]);

  function handleSelectClick(row: HistoryRow, shiftKey: boolean): void {
    setSelected((prev) =>
      applySelectionClick(prev, [...filteredRows], rowKeyOf, row, lastClickedKey, shiftKey)
    );
    setLastClickedKey(row.rowKey);
  }

  return { selected, lastClickedKey, handleSelectClick };
}
