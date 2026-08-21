import { useEffect } from "react";

/**
 * Keeps a controlled selection in sync with reality: whenever the real list of options
 * loads (or changes -- e.g. a filter narrows it), and the currently selected value isn't
 * one of them, this picks the first available option instead.
 *
 * Generic sibling of web-ui/lib/use-auto-select-tag.ts's useAutoSelectFirstTag, which fixed
 * the exact same bug category (a stored value that doesn't match any real option) for tag
 * selection: without this, a stale/default selection (e.g. ContextBar's store-default
 * `site: "plant-a"` when the real site list turns out to be `["plant-b"]`) renders a
 * <Select value={...}> whose value matches no <SelectItem>, so anything gated on that
 * value silently mismatches reality with no visible signal.
 */
export function useAutoSelectFirst(
  options: string[] | undefined,
  current: string,
  setCurrent: (value: string) => void
) {
  useEffect(() => {
    if (!options || options.length === 0) return;
    const stillValid = options.includes(current);
    if (!stillValid) {
      setCurrent(options[0]);
    }
  }, [options, current, setCurrent]);
}
