import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useApiClient } from "../types";
import { useCan } from "@/lib/use-can";
import { parsePayload, type LayoutPayload } from "../utils/layoutPayload";

/** A layout row as consumed by the UI (payload parsed defensively). */
export interface GridLayout {
  id: string;
  name: string;
  isDefault: boolean;
  payload: LayoutPayload;
}

interface GridLayoutsData {
  orgDefault: GridLayout | null;
  mine: GridLayout[];
  myDefaultId: string | null;
}

const EMPTY: GridLayoutsData = { orgDefault: null, mine: [], myDefaultId: null };

const keys = {
  grid: (tableId: string) => ["table-layouts", tableId] as const,
};

function parseRow(raw: unknown): GridLayout | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  if (typeof o.id !== "string" || typeof o.name !== "string") return null;
  return {
    id: o.id,
    name: o.name,
    isDefault: o.isDefault === true,
    payload: parsePayload(o.payload),
  };
}

function parseResponse(data: unknown): GridLayoutsData {
  const o = data && typeof data === "object" ? (data as Record<string, unknown>) : {};
  const mine = Array.isArray(o.mine)
    ? o.mine.map(parseRow).filter((r): r is GridLayout => r !== null)
    : [];
  return {
    orgDefault: parseRow(o.orgDefault),
    mine,
    myDefaultId: typeof o.myDefaultId === "string" ? o.myDefaultId : null,
  };
}

export interface UseTableLayoutsResult extends GridLayoutsData {
  isLoading: boolean;
  canManageOrg: boolean;
  saveMine: (name: string, payload: LayoutPayload) => Promise<void>;
  setMyDefault: (id: string, isDefault: boolean) => Promise<void>;
  removeMine: (id: string) => Promise<void>;
  saveOrgDefault: (payload: LayoutPayload) => Promise<void>;
  removeOrgDefault: () => Promise<void>;
}

/**
 * Server-backed grid layouts (identity DB via `/table-layouts`): the org default + the user's
 * personal layouts. Reads are cached by `tableId`; every mutation invalidates that key.
 */
export function useTableLayouts(tableId: string): UseTableLayoutsResult {
  const api = useApiClient();
  const queryClient = useQueryClient();
  const canManageOrg = useCan("tableLayouts.manageOrgDefault");
  const orgPath = `/table-layouts/org-default?tableId=${encodeURIComponent(tableId)}`;

  const query = useQuery({
    queryKey: keys.grid(tableId),
    staleTime: 60_000,
    placeholderData: keepPreviousData,
    queryFn: async ({ signal }) =>
      parseResponse(await api.get<unknown>("/table-layouts", { tableId }, signal)),
  });

  const invalidate = () => queryClient.invalidateQueries({ queryKey: keys.grid(tableId) });

  const saveMineM = useMutation({
    mutationFn: (v: { name: string; payload: LayoutPayload }) =>
      api.post("/table-layouts", { tableId, name: v.name, payload: v.payload }),
    onSuccess: () => void invalidate(),
  });
  const setDefaultM = useMutation({
    mutationFn: (v: { id: string; isDefault: boolean }) =>
      api.patch(`/table-layouts/${v.id}/default`, { isDefault: v.isDefault }),
    onSuccess: () => void invalidate(),
  });
  const removeMineM = useMutation({
    mutationFn: (id: string) => api.delete(`/table-layouts/${id}`),
    onSuccess: () => void invalidate(),
  });
  const saveOrgM = useMutation({
    mutationFn: (payload: LayoutPayload) => api.put(orgPath, { payload }),
    onSuccess: () => void invalidate(),
  });
  const removeOrgM = useMutation({
    mutationFn: () => api.delete(orgPath),
    onSuccess: () => void invalidate(),
  });

  const data = query.data ?? EMPTY;

  return {
    ...data,
    isLoading: query.isLoading,
    canManageOrg,
    saveMine: async (name, payload) => {
      await saveMineM.mutateAsync({ name, payload });
    },
    setMyDefault: async (id, isDefault) => {
      await setDefaultM.mutateAsync({ id, isDefault });
    },
    removeMine: async (id) => {
      await removeMineM.mutateAsync(id);
    },
    saveOrgDefault: async (payload) => {
      await saveOrgM.mutateAsync(payload);
    },
    removeOrgDefault: async () => {
      await removeOrgM.mutateAsync();
    },
  };
}
