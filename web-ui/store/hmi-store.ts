"use client";

import { create } from "zustand";

type HmiStore = {
  selectedTag: string;
  setSelectedTag: (tag: string) => void;
};

export const useHmiStore = create<HmiStore>((set) => ({
  selectedTag: "tag_hr_0",
  setSelectedTag: (selectedTag) => set({ selectedTag }),
}));
