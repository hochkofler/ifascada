export function parseCompound(value: unknown): { value: string; unit?: string; raw?: string } | null {
  if (value == null) return null;
  if (typeof value === "string") {
    try {
      const parsed = JSON.parse(value) as { value?: unknown; unit?: unknown; raw?: unknown };
      if (parsed && parsed.value !== undefined) {
        return {
          value: String(parsed.value),
          unit: parsed.unit !== undefined ? String(parsed.unit) : undefined,
          raw: parsed.raw !== undefined ? String(parsed.raw) : undefined,
        };
      }
    } catch {
      return { value };
    }
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return { value: String(value) };
  }
  return null;
}

export function formatProcessValue(value: unknown): string {
  const c = parseCompound(value);
  if (!c) return "-";
  if (c.unit) return `${c.value} ${c.unit}`;
  return c.value;
}
