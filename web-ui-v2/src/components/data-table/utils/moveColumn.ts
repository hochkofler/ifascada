/**
 * Mueve un id una posición arriba/abajo en el array de orden de columnas, sin
 * cruzar columnas fijas (selección/acciones). Devuelve el orden original (sin
 * mutar) si no puede moverse: borde del array o vecino fijo.
 */
export function moveColumn(
  order: string[],
  id: string,
  direction: "up" | "down",
  fixedIds: ReadonlySet<string> = new Set()
): string[] {
  const idx = order.indexOf(id);
  const swap = direction === "up" ? idx - 1 : idx + 1;
  const swapId = order[swap];
  if (idx === -1 || swapId === undefined || fixedIds.has(swapId)) return order;
  const next = [...order];
  next[idx] = swapId;
  next[swap] = id;
  return next;
}
