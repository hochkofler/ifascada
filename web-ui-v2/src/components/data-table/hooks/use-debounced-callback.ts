import { useCallback, useEffect, useRef } from "react";

/**
 * Returns a stable, debounced version of `fn`: each call resets a timer, and only
 * the last call within `delayMs` actually runs. The latest `fn` is always invoked,
 * so callers don't need to memoize it.
 *
 * Debouncing lives in the event handler (not an effect), which is the idiomatic place
 * for it — see the React docs on "You Might Not Need an Effect".
 */
export function useDebouncedCallback<Args extends unknown[]>(
  fn: (...args: Args) => void,
  delayMs: number
): (...args: Args) => void {
  const fnRef = useRef(fn);
  useEffect(() => {
    fnRef.current = fn;
  }, [fn]);

  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  return useCallback(
    (...args: Args) => {
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        fnRef.current(...args);
      }, delayMs);
    },
    [delayMs]
  );
}
