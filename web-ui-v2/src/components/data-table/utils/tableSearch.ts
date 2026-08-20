import type { SortingState } from "@tanstack/react-table";
import type { ServerState, UseDataTableOptions } from "../types";

/**
 * Forma del estado de grilla en la URL (search params). Todo opcional: una URL
 * sin params es válida (estado por defecto). `filters` va como dato estructurado
 * (TanStack Router serializa arrays/objetos nativamente).
 */
export interface TableSearch {
  page?: number; // 0-based; se omite si 0 (default)
  size?: number;
  sort?: string; // "id:desc" | "id:asc"
  q?: string; // globalFilter
  filters?: { id: string; value: unknown }[];
}

function toInt(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isInteger(value)) return value;
  if (typeof value === "string" && /^-?\d+$/.test(value)) return Number(value);
  return undefined;
}

/**
 * `validateSearch` de las rutas de lista: narrowing defensivo del search crudo a
 * `TableSearch`. Descarta valores inválidos en vez de romper la navegación.
 */
export function validateTableSearch(input: Record<string, unknown>): TableSearch {
  const out: TableSearch = {};
  const page = toInt(input.page);
  if (page !== undefined && page > 0) out.page = page; // 0 = default → se omite
  const size = toInt(input.size);
  if (size !== undefined && size > 0) out.size = size;
  if (typeof input.sort === "string" && input.sort) out.sort = input.sort;
  if (typeof input.q === "string" && input.q) out.q = input.q;
  if (Array.isArray(input.filters)) {
    const filters = input.filters.filter(
      (f): f is { id: string; value: unknown } =>
        typeof f === "object" && f !== null && typeof (f as { id?: unknown }).id === "string"
    );
    if (filters.length) out.filters = filters;
  }
  return out;
}

/** "id:desc" → [{ id, desc:true }]. Vacío/undefined → []. */
export function parseSort(sort: string | undefined): SortingState {
  if (!sort) return [];
  const [id, dir] = sort.split(":");
  if (!id) return [];
  return [{ id, desc: dir === "desc" }];
}

/** Primer sort → "id:desc"/"id:asc". Sin sort → undefined. */
export function formatSort(sorting: SortingState): string | undefined {
  const first = sorting[0];
  return first ? `${first.id}:${first.desc ? "desc" : "asc"}` : undefined;
}

/** Mapea el search de la URL a ServerState, aplicando defaults cuando falta. */
export function searchToServerState(
  search: TableSearch,
  defaults: UseDataTableOptions = {}
): ServerState {
  return {
    page: search.page ?? 0,
    pageSize: search.size ?? defaults.initialPageSize ?? 25,
    sorting: search.sort ? parseSort(search.sort) : (defaults.initialSorting ?? []),
    filters: search.filters ?? [],
    globalFilter: search.q ?? "",
  };
}
