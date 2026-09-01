import type { ContextOption } from "@/lib/api-client";

/**
 * Deduplica opciones de contexto por `code`, conservando la primera.
 *
 * Hace falta porque el backend puede devolver dos filas con el MISMO codigo: en produccion
 * `/api/context/cells?site=plant-a` devuelve dos celdas `cell-main`, con nombres distintos
 * ("Cell Main" y "main"). Eso rompia React con "Encountered two children with the same key".
 *
 * Pero el problema de fondo no es la clave: el `value` de cada opcion ES el codigo, asi que dos
 * entradas con el mismo codigo son indistinguibles como filtro -- elegir una u otra produce
 * exactamente la misma consulta. La segunda no aporta nada y solo puede confundir.
 *
 * Esto es una degradacion elegante, no el arreglo real: que existan dos celdas con el mismo
 * codigo dentro de un mismo sitio parece un problema de datos o de sembrado que conviene mirar
 * en el backend.
 */
export function dedupeByCode(options: readonly ContextOption[]): ContextOption[] {
  const seen = new Set<string>();
  const out: ContextOption[] = [];
  for (const option of options) {
    if (seen.has(option.code)) continue;
    seen.add(option.code);
    out.push(option);
  }
  return out;
}
