"use client";

import { useEffect } from "react";

type WithTagCode = { tag_code: string };

/**
 * Keeps a <select value={selectedTag}> in sync with reality: whenever the available tags list
 * loads (or changes -- e.g. a hierarchy filter narrows it), and the currently selected tag isn't
 * one of them, this picks the first available tag instead.
 *
 * Without this, a stale/empty selectedTag renders a <select> whose value matches no <option>.
 * The browser shows the first real option as visually selected while React's controlled value
 * stays on the stale one, so anything gated on `enabled: Boolean(selectedTag)` either never
 * fires or fires for a tag with no data -- "looks selected, no data shows" until the user
 * manually fires a real onChange by picking something else and back.
 */
export function useAutoSelectFirstTag(
  tags: WithTagCode[] | undefined,
  selectedTag: string,
  setSelectedTag: (tag: string) => void
) {
  useEffect(() => {
    if (!tags || tags.length === 0) return;
    const stillValid = tags.some((t) => t.tag_code === selectedTag);
    if (!stillValid) {
      setSelectedTag(tags[0].tag_code);
    }
  }, [tags, selectedTag, setSelectedTag]);
}
