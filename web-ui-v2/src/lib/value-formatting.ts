/**
 * Unit-aware value display for the History page. Port of web-ui/lib/hmi-value.ts's
 * `parseCompound`/`formatProcessValue` (which handle the JSON-compound value strings
 * real weighing-scale tags produce, e.g. `'{"value":330,"unit":"g"}'`), extended with a
 * `parseValueWithUnit`/`numericValue` pair that also accepts the simpler "NUMBER UNIT"
 * space-joined string format so both real value shapes parse.
 */

export type ParsedValue = { number: number; unit: string | null };

const PLAIN_VALUE_RE = /^(-?\d+(?:\.\d+)?)\s*([a-zA-Z%]+)?$/;

function parseJsonCompound(trimmed: string): ParsedValue | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return null;
  const obj = parsed as { value?: unknown; unit?: unknown };
  const n = Number(obj.value);
  if (obj.value === undefined || Number.isNaN(n)) return null;
  return { number: n, unit: obj.unit !== undefined && obj.unit !== null ? String(obj.unit) : null };
}

/** Splits a raw tag value into `{ number, unit }`, or `null` if it can't be parsed as one. */
export function parseValueWithUnit(value: unknown): ParsedValue | null {
  if (typeof value === "number") return Number.isNaN(value) ? null : { number: value, unit: null };
  if (typeof value !== "string") return null;

  const trimmed = value.trim();
  if (trimmed.startsWith("{")) return parseJsonCompound(trimmed);

  const match = trimmed.match(PLAIN_VALUE_RE);
  if (!match) return null;
  return { number: Number.parseFloat(match[1]), unit: match[2] ?? null };
}

/** Extracts just the numeric part of a raw tag value, for filtering/sorting purposes. */
export function numericValue(value: unknown): number | null {
  return parseValueWithUnit(value)?.number ?? null;
}

/** Formats a raw tag value as `"330 g"` (or just `"330"` with no unit), `"-"` if unparseable. */
export function formatValueWithUnit(value: unknown): string {
  const parsed = parseValueWithUnit(value);
  if (!parsed) return "-";
  return parsed.unit ? `${parsed.number} ${parsed.unit}` : String(parsed.number);
}
