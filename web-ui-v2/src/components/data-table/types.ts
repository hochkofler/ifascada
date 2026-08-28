import type { ColumnFiltersState, SortingState } from "@tanstack/react-table";
import type React from "react";
import { ApiError } from "@/lib/api-error";
import { getAuthHeader } from "@/lib/api-client";

// ---------------------------------------------------------------------------
// Minimal local stand-ins for `@ifahub/types` and `@ifahub/api-client`.
//
// The vendored DataTable system (see components/hooks in this directory) was written against
// two other ifahub libs this project doesn't have: `@ifahub/types` (shared domain enums/DocumentRef
// + number/date formatters) and `@ifahub/api-client` (an authenticated fetch wrapper). Vendoring
// either wholesale would pull in SAP-document-specific types and auth/session plumbing this
// project doesn't have (no login yet -- see `useCan` in `@/lib/use-can`). Only the pieces the
// DataTable system actually imports are reproduced below, trimmed to what's used here.
// ---------------------------------------------------------------------------

// `enum` is real (non-erasable) TS syntax and this project's tsconfig sets `erasableSyntaxOnly`
// (Task 2), so these are modeled as a const object + a derived union type instead -- same
// call-site shape (`ColumnDisplayType.String`, usable as a type) as `@ifahub/types`'s enums.

/** Cell data types a `ColumnDefinition` can render/format/filter as. */
export const ColumnDisplayType = {
  String: "string",
  Number: "number",
  Currency: "currency",
  Percentage: "percentage",
  Date: "date",
  DateTime: "datetime",
  Boolean: "boolean",
} as const;
export type ColumnDisplayType = (typeof ColumnDisplayType)[keyof typeof ColumnDisplayType];

/** UI control a column filter renders as. */
export const FilterType = {
  Text: "text",
  Select: "select",
  Number: "number",
  Date: "date",
} as const;
export type FilterType = (typeof FilterType)[keyof typeof FilterType];

/**
 * Reference to a document a cell can link to (see `linkTo` on `ColumnDefinition` and
 * `DetailLinkCell`). `@ifahub/types`'s version ties `type` to a SAP-specific `DocumentType`
 * enum; this project has no such domain yet, so `type` is just an opaque string the app's own
 * `DocumentRouteResolver` interprets.
 */
export interface DocumentRef {
  type: string;
  entry: number;
  docNum?: number;
}

/** Locale used for number formatting. Fixed for now -- no runtime format config exists yet. */
export function getNumberLocale(): string {
  return "en-US";
}

const DATE_LOCALE = "en-GB";
const numberFormatCache = new Map<string, Intl.NumberFormat>();

function numberFormatter(variant: string, options: Intl.NumberFormatOptions): Intl.NumberFormat {
  const key = `${getNumberLocale()}|${variant}`;
  let formatter = numberFormatCache.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat(getNumberLocale(), options);
    numberFormatCache.set(key, formatter);
  }
  return formatter;
}

/** Quantities/stock/units. Up to 6 decimals, no forced trailing zeros: `100` -> `"100"`. */
export function formatQuantity(value: number): string {
  return Number.isNaN(value)
    ? ""
    : numberFormatter("qty", { maximumFractionDigits: 6 }).format(value);
}

/** Amounts/prices. Always 2 decimals: `1234.5` -> `"1,234.50"`. */
export function formatAmount(value: number): string {
  return Number.isNaN(value)
    ? ""
    : numberFormatter("amount", { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(
        value
      );
}

export type DateInput = string | number | Date | null | undefined;

function parseDateInput(value: DateInput): { date: Date; dateOnly: boolean } | null {
  if (value === null || value === undefined || value === "") return null;
  const dateOnly = typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);
  const date = value instanceof Date ? value : new Date(value);
  return Number.isNaN(date.getTime()) ? null : { date, dateOnly };
}

const NUMERIC_DATE_OPTS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
};

/** Date only, `en-GB` (`DD/MM/YYYY`). `""` if empty; the raw string if unparseable. */
export function formatDate(value: DateInput): string {
  const parsed = parseDateInput(value);
  if (!parsed) return typeof value === "string" ? value : "";
  // Date-only strings (YYYY-MM-DD) parse as UTC midnight; formatting in the local TZ would shift
  // the day, so that case is formatted in UTC.
  return new Intl.DateTimeFormat(
    DATE_LOCALE,
    parsed.dateOnly ? { ...NUMERIC_DATE_OPTS, timeZone: "UTC" } : NUMERIC_DATE_OPTS
  ).format(parsed.date);
}

/** Date + time (24h), `en-GB`. */
export function formatDateTime(value: DateInput): string {
  const parsed = parseDateInput(value);
  if (!parsed) return typeof value === "string" ? value : "";
  return new Intl.DateTimeFormat(DATE_LOCALE, {
    ...NUMERIC_DATE_OPTS,
    hour: "2-digit",
    minute: "2-digit",
  }).format(parsed.date);
}

/** Minimal stand-in for `@ifahub/api-client`'s `ApiClient` -- only the methods `useTableLayouts`
 *  actually calls (no upload/blob/download support). */
export interface ApiClient {
  get: <T>(
    path: string,
    params?: Record<string, string | number | boolean | undefined>,
    signal?: AbortSignal
  ) => Promise<T>;
  post: <T>(path: string, body?: unknown) => Promise<T>;
  put: <T>(path: string, body?: unknown) => Promise<T>;
  patch: <T>(path: string, body?: unknown) => Promise<T>;
  delete: <T>(path: string) => Promise<T>;
}

async function apiRequest<T>(
  method: string,
  path: string,
  body?: unknown,
  params?: Record<string, string | number | boolean | undefined>,
  signal?: AbortSignal
): Promise<T> {
  const url = new URL(`/api${path}`, window.location.origin);
  if (params) {
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
  }
  const response = await fetch(url, {
    method,
    headers: {
      // Este fetch se saltaba `getAuthHeader()`, el unico punto donde se inyectara la
      // autorizacion cuando exista login: habria quedado sin el header ese dia.
      ...getAuthHeader(),
      ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
    signal,
  });
  if (!response.ok) {
    // ApiError, no Error generico: asi `notify` puede sacarle el status y el motivo, igual que
    // con el resto de las llamadas de la app.
    const text = await response.text().catch(() => "");
    throw new ApiError(response.status, text || `${method} ${path}`);
  }
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  return text ? (JSON.parse(text) as T) : (undefined as T);
}

/**
 * Minimal stand-in for `@ifahub/api-client`'s `useApiClient` -- this project has no auth/session
 * wiring yet (see `useCan` in `@/lib/use-can`), so there's no token to attach and no 401-retry to
 * orchestrate. Requests go through Vite's `/api` proxy (see `vite.config.ts`) to the
 * central-server backend. Swap for the real client once this project has one.
 */
export function useApiClient(): ApiClient {
  return {
    get: (path, params, signal) => apiRequest("GET", path, undefined, params, signal),
    post: (path, body) => apiRequest("POST", path, body),
    put: (path, body) => apiRequest("PUT", path, body),
    patch: (path, body) => apiRequest("PATCH", path, body),
    delete: (path) => apiRequest("DELETE", path),
  };
}

export interface FilterOption {
  label: string;
  value: string;
}

/** Densidad de filas de la grilla (personalización de vista, estilo Fiori "Compact"). */
export type TableDensity = "comfortable" | "compact";

/** Disposición de columnas que una vista guardada puede capturar además de los
 *  filtros/orden/búsqueda: orden de columnas, visibilidad y densidad. Todo opcional
 *  y retrocompatible (las vistas viejas no lo traen). */
export interface TableLayout {
  columnOrder?: string[];
  columnVisibility?: Record<string, boolean>;
  density?: TableDensity;
}

/** Contenido del estado vacío de la grilla (ícono por defecto + título/descripción/CTA). */
export interface EmptyStateConfig {
  title?: string;
  description?: string;
  /** Acción primaria opcional (CTA), p.ej. un botón "Crear el primero". */
  action?: React.ReactNode;
}

export interface ColumnDefinition<T> {
  accessorKey: keyof T;
  header: string;
  type: ColumnDisplayType;
  sortable?: boolean;
  /**
   * ¿Filtrable? Default: true (lo controla `defaultFilterable` de la tabla). El
   * tipo de filtro se deriva de `type`. Poné `false` en columnas sin filtro
   * server-side (el endpoint debe mapear el filtro o el control no tiene efecto).
   */
  filterable?: boolean;
  /** Sobrescribe el tipo de filtro; por defecto se deriva de `type`. */
  filterType?: FilterType;
  filterOptions?: FilterOption[];
  pinned?: "left" | "right";
  visible?: boolean;
  width?: number;
  cell?: (value: unknown, row: T) => React.ReactNode;
  /**
   * When set, the cell renders as a link to a document's detail view. Return the
   * document reference, or null for rows without a target (e.g. no related document).
   */
  linkTo?: (row: T) => DocumentRef | null;
}

export interface ServerState {
  page: number;
  pageSize: number;
  sorting: SortingState;
  filters: ColumnFiltersState;
  globalFilter: string;
}

export interface ServerHandlers {
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  onSortingChange: (sorting: SortingState) => void;
  onFiltersChange: (filters: ColumnFiltersState) => void;
  onGlobalFilterChange: (value: string) => void;
}

export interface UseDataTableOptions {
  initialPageSize?: number;
  initialSorting?: SortingState;
}

export interface DataTableProps<T extends object> {
  data: T[];
  columns: ColumnDefinition<T>[];
  exportable?: boolean;
  exportFilename?: string;
  totalRows: number;
  loading?: boolean;
  error?: boolean;
  serverState: ServerState;
  serverHandlers: ServerHandlers;
  selectable?: boolean;
  maxHeight?: string;
  locale?: string;
  showSearch?: boolean;
  /** Densidad inicial de filas. Default: "compact" (vista densa, estilo Fiori). El usuario la cambia desde "Vista". */
  density?: TableDensity;
  /** Muestra el menú "Vista" (densidad + visibilidad de columnas). Default: true. */
  showViewOptions?: boolean;
  /** Personaliza el estado vacío (título/descripción/CTA). Sin esto usa el default con ícono. */
  emptyState?: EmptyStateConfig;
  /** Id estable de la grilla. Si se pasa, habilita el menú "Vistas" (presets en localStorage). */
  tableId?: string;
  /** Filtrado por columna habilitado por defecto (derivado del `type`). Default: true. Opt-out por columna con `filterable:false`. */
  defaultFilterable?: boolean;
  rowActions?: (row: T) => React.ReactNode;
  /**
   * Filas hijas de una fila, o `undefined` si no tiene. Pasarlo habilita la expansion
   * (chevron por fila + indentacion por profundidad). Sin esta prop, nada cambia.
   *
   * Extension de ifascada sobre el DataTable de libs/tables, que no tiene expansion. Es
   * aditiva y se apoya en `getExpandedRowModel` de TanStack: vale proponerla upstream.
   */
  getSubRows?: (row: T) => T[] | undefined;
  /**
   * Identidad estable de fila. OBLIGATORIA si se usa `getSubRows`: el id por defecto de
   * TanStack es el indice del array, asi que en una grilla que refresca sola lo que el usuario
   * tenia expandido se cerraria en cada refetch -- o se quedaria abierto sobre otra fila.
   */
  getRowId?: (row: T) => string;
  onSelectionChange?: (rows: T[]) => void;
}

export interface DataTableRootProps<T extends object> {
  data: T[];
  columns: ColumnDefinition<T>[];
  totalRows: number;
  serverState: ServerState;
  serverHandlers: ServerHandlers;
  selectable?: boolean;
  locale?: string;
  maxHeight?: string;
  onSelectionChange?: (rows: T[]) => void;
  /** Filtrado por columna habilitado por defecto (derivado del `type`). Default: true. */
  defaultFilterable?: boolean;
  children: React.ReactNode;
}
