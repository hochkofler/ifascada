import type { TableSearch } from "./tableSearch";
import type { TableLayout } from "../types";

/** Bump when the payload shape changes incompatibly; older/newer payloads then fall back to defaults. */
export const LAYOUT_PAYLOAD_VERSION = 1;

/** Opaque snapshot stored per view: the URL search (filters/sort/query) + the column layout. */
export interface LayoutPayload {
  version: number;
  search: TableSearch;
  layout?: TableLayout;
}

/** Builds the payload persisted for a view: current search (minus the transient page) + layout. */
export function buildPayload(search: TableSearch, layout?: TableLayout): LayoutPayload {
  const viewSearch: TableSearch = { ...search };
  delete viewSearch.page;
  return { version: LAYOUT_PAYLOAD_VERSION, search: viewSearch, ...(layout ? { layout } : {}) };
}

/** Defensive parse of a payload coming from the server or storage (never throws). */
export function parsePayload(raw: unknown): LayoutPayload {
  const o = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  const version = typeof o.version === "number" ? o.version : 0;
  const search = o.search && typeof o.search === "object" ? (o.search as TableSearch) : {};
  const layout = o.layout && typeof o.layout === "object" ? (o.layout as TableLayout) : undefined;
  return { version, search, layout };
}

/** True when the code knows how to apply this payload (its version is not newer than ours). */
export function isApplicable(payload: LayoutPayload): boolean {
  return payload.version > 0 && payload.version <= LAYOUT_PAYLOAD_VERSION;
}

/** True when a search carries no user intent (so an auto-applied default won't clobber a deep link). */
export function isSearchEmpty(search: TableSearch): boolean {
  return !search.q && !search.sort && !(search.filters && search.filters.length > 0);
}

/**
 * Sanitizes a stored layout against the columns that exist NOW, so a renamed or removed column
 * can never break the grid: unknown column ids are dropped from the order, columns missing from
 * the stored order are appended at the end (a new column stays visible), and visibility keys for
 * columns that no longer exist are discarded.
 */
export function sanitizeLayout(
  layout: TableLayout | undefined,
  leafColumnIds: string[]
): TableLayout | undefined {
  if (!layout) return undefined;
  const known = new Set(leafColumnIds);
  const result: TableLayout = {};

  if (layout.columnOrder) {
    const kept = layout.columnOrder.filter((id) => known.has(id));
    const keptSet = new Set(kept);
    const appended = leafColumnIds.filter((id) => !keptSet.has(id));
    result.columnOrder = [...kept, ...appended];
  }

  if (layout.columnVisibility) {
    const vis: Record<string, boolean> = {};
    for (const id of leafColumnIds) {
      const visible = layout.columnVisibility[id];
      if (visible !== undefined) vis[id] = visible;
    }
    result.columnVisibility = vis;
  }

  if (layout.density) result.density = layout.density;

  return result;
}
