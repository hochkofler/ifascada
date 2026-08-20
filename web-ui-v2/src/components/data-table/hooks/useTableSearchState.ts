import { useMemo } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { SortingState } from "@tanstack/react-table";
import type { ServerHandlers, ServerState, UseDataTableOptions } from "../types";
import { type TableSearch, formatSort, searchToServerState } from "../utils/tableSearch";

/**
 * Variante de useDataTable respaldada por la URL (search params): el estado de
 * la grilla (filtros/orden/página/búsqueda) vive en la URL → deep-linking, links
 * compartibles, refresh que preserva, y back/forward correcto. Mismo contrato
 * `{ serverState, serverHandlers }` que useDataTable, así que es drop-in.
 *
 * La ruta debe declarar `validateSearch: validateTableSearch`. Los cambios usan
 * `replace: true` para no inundar el historial con cada tecla/cambio.
 */
export function useTableSearchState(options: UseDataTableOptions = {}): {
  serverState: ServerState;
  serverHandlers: ServerHandlers;
} {
  const search: TableSearch = useSearch({ strict: false });
  const navigate = useNavigate();
  const { initialPageSize, initialSorting } = options;

  const serverState = useMemo(
    () => searchToServerState(search, { initialPageSize, initialSorting }),
    [search, initialPageSize, initialSorting]
  );

  const serverHandlers = useMemo<ServerHandlers>(() => {
    const update = (patch: Partial<TableSearch>) => {
      void navigate({
        to: ".",
        replace: true,
        search: (prev: TableSearch) => {
          const next: TableSearch = { ...prev, ...patch };
          // Limpiar claves en su valor por defecto para mantener la URL corta.
          if (!next.page) delete next.page;
          if (!next.q) delete next.q;
          if (!next.sort) delete next.sort;
          if (!next.filters || next.filters.length === 0) delete next.filters;
          return next;
        },
      });
    };

    return {
      onPageChange: (page) => {
        update({ page });
      },
      onPageSizeChange: (size) => {
        update({ size, page: 0 });
      },
      onSortingChange: (sorting: SortingState) => {
        update({ sort: formatSort(sorting), page: 0 });
      },
      onFiltersChange: (filters) => {
        update({ filters, page: 0 });
      },
      onGlobalFilterChange: (value) => {
        update({ q: value, page: 0 });
      },
    };
  }, [navigate]);

  return { serverState, serverHandlers };
}
