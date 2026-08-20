import { useEffect, useRef } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useDataTableContext } from "../DataTableContent";
import { useTableLayouts, type GridLayout } from "./useTableLayouts";
import type { TableSearch } from "../utils/tableSearch";
import type { TableLayout } from "../types";
import { buildPayload, isApplicable, isSearchEmpty, sanitizeLayout } from "../utils/layoutPayload";

export interface GridViews {
  orgDefault: GridLayout | null;
  mine: GridLayout[];
  myDefaultId: string | null;
  canManageOrg: boolean;
  applyView: (row: GridLayout) => void;
  saveCurrent: (name: string) => Promise<void>;
  setMyDefault: (id: string, isDefault: boolean) => Promise<void>;
  deleteView: (id: string) => Promise<void>;
  saveOrgDefaultFromCurrent: () => Promise<void>;
  removeOrgDefault: () => Promise<void>;
}

/**
 * Orchestrates the "Vistas" menu: server layouts (useTableLayouts) + the live table state
 * (context) + the URL search (router). Also auto-applies the user's default (else the org
 * default) once per grid on mount — layout always, search only when the URL carries no intent
 * (so a deep-linked/filtered URL is never clobbered).
 */
export function useGridViews(tableId: string): GridViews {
  const layouts = useTableLayouts(tableId);
  const { table, density, setDensity } = useDataTableContext();
  const search: TableSearch = useSearch({ strict: false });
  const navigate = useNavigate();
  const autoAppliedFor = useRef<string | null>(null);

  function captureLayout(): TableLayout {
    const state = table.getState();
    return { columnOrder: state.columnOrder, columnVisibility: state.columnVisibility, density };
  }

  function applyLayout(layout: TableLayout | undefined): void {
    const leafIds = table.getAllLeafColumns().map((c) => c.id);
    const safe = sanitizeLayout(layout, leafIds);
    if (!safe) return;
    if (safe.columnOrder) table.setColumnOrder(safe.columnOrder);
    if (safe.columnVisibility) table.setColumnVisibility(safe.columnVisibility);
    if (safe.density && setDensity) setDensity(safe.density);
  }

  function applySearch(target: TableSearch): void {
    void navigate({ to: ".", replace: true, search: () => ({ ...target }) });
  }

  function applyView(row: GridLayout): void {
    if (!isApplicable(row.payload)) return;
    applyLayout(row.payload.layout);
    applySearch(row.payload.search);
  }

  useEffect(() => {
    if (layouts.isLoading || autoAppliedFor.current === tableId) return;
    autoAppliedFor.current = tableId;
    const mineDefault = layouts.mine.find((l) => l.id === layouts.myDefaultId) ?? null;
    const def = mineDefault ?? layouts.orgDefault;
    if (!def || !isApplicable(def.payload)) return;
    applyLayout(def.payload.layout);
    if (isSearchEmpty(search)) applySearch(def.payload.search);
    // Run once per grid when data first resolves; guarded by the ref, so search/table refs
    // deliberately stay out of the deps (re-applying on every keystroke would fight the user).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tableId, layouts.isLoading, layouts.myDefaultId, layouts.orgDefault]);

  return {
    orgDefault: layouts.orgDefault,
    mine: layouts.mine,
    myDefaultId: layouts.myDefaultId,
    canManageOrg: layouts.canManageOrg,
    applyView,
    saveCurrent: (name) => layouts.saveMine(name.trim(), buildPayload(search, captureLayout())),
    setMyDefault: layouts.setMyDefault,
    deleteView: layouts.removeMine,
    saveOrgDefaultFromCurrent: () => layouts.saveOrgDefault(buildPayload(search, captureLayout())),
    removeOrgDefault: layouts.removeOrgDefault,
  };
}
