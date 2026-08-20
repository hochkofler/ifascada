"use client";

import { create } from "zustand";

type HmiStore = {
  selectedTag: string;
  setSelectedTag: (tag: string) => void;
};

export const useHmiStore = create<HmiStore>((set) => ({
  // Intentionally empty, not a placeholder tag code: "tag_hr_0" (an old demo/example tag that
  // doesn't exist in real data) used to live here, and every page reading selectedTag renders a
  // <select value={selectedTag}> whose value never matches any real <option>. The browser then
  // shows the first real tag as visually selected while React's own state stays stuck on the
  // stale value, so the history query fires for a tag that has no data -- "looks selected, no
  // data" until the user manually fires a real onChange by picking something else and back.
  // useAutoSelectFirstTag() (lib/use-auto-select-tag.ts) is what actually assigns a valid tag
  // once the tags list loads; this default just has to not collide with a real tag_code.
  selectedTag: "",
  setSelectedTag: (selectedTag) => set({ selectedTag }),
}));
