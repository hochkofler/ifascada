/**
 * Every date/time shown in this app must read the same wall-clock time regardless of which
 * operator's machine (and its own OS locale/timezone settings) is looking at the screen --
 * `new Date(x).toLocaleString()` (what the app being replaced does everywhere) depends on the
 * browser's own timezone, which is the actual bug, not just a formatting nicety. Pin to the
 * real deployment's timezone instead. A future multi-site deployment in a different timezone
 * is then a one-line change here, not a search-and-replace across every page.
 */
export const SERVER_TIME_ZONE = "America/La_Paz";

export function formatServerDateTime(iso: string): string {
  return new Intl.DateTimeFormat("es-BO", {
    timeZone: SERVER_TIME_ZONE,
    year: "2-digit",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(iso));
}

export function formatServerTime(iso: string): string {
  return new Intl.DateTimeFormat("es-BO", {
    timeZone: SERVER_TIME_ZONE,
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(iso));
}
