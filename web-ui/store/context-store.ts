"use client";

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

type OperationalContextState = {
  site: string;
  line: string;
  area: string;
  cell: string;
  edge: string;
  setSite: (v: string) => void;
  setLine: (v: string) => void;
  setArea: (v: string) => void;
  setCell: (v: string) => void;
  setEdge: (v: string) => void;
};

export const useOperationalContextStore = create<OperationalContextState>()(
  persist(
    (set) => ({
      site: "plant-a",
      line: "",
      area: "",
      cell: "",
      edge: "",
      setSite: (site) => set(() => ({ site, line: "", area: "", cell: "", edge: "" })),
      setLine: (line) => set(() => ({ line, area: "", cell: "", edge: "" })),
      setArea: (area) => set(() => ({ area, cell: "", edge: "" })),
      setCell: (cell) => set(() => ({ cell, edge: "" })),
      setEdge: (edge) => set(() => ({ edge })),
    }),
    {
      name: "hmi.operational-context.v1",
      storage: createJSONStorage(() => localStorage),
    }
  )
);
