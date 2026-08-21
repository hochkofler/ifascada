import { create } from "zustand";

type OperationalContextStore = {
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

export const useOperationalContextStore = create<OperationalContextStore>((set) => ({
  site: "plant-a",
  line: "",
  area: "",
  cell: "",
  edge: "",
  setSite: (site) => set({ site }),
  setLine: (line) => set({ line }),
  setArea: (area) => set({ area }),
  setCell: (cell) => set({ cell }),
  setEdge: (edge) => set({ edge }),
}));
