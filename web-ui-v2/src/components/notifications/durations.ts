import type { MessageLevel } from "./types";

/**
 * Display duration per level (ms). Single source of truth: to make every toast
 * last longer, change it here. Errors stay on screen the longest because the
 * user usually needs to read the reference/correlation id before it disappears.
 */
export const DURATION_BY_LEVEL: Record<MessageLevel, number> = {
  success: 4000,
  info: 4000,
  warning: 6000,
  error: 8000,
};
