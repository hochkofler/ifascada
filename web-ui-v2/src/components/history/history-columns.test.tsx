import { describe, it, expect } from "vitest";
import i18n from "@/lib/i18n";
import { getHistoryColumns, toHistoryRows, type HistoryRow } from "./history-columns";
import type { TagHistory } from "@/lib/api-client";

// NOTE: the brief's illustrative sketch for this file used TanStack's `createColumnHelper`
// (producing `ColumnDef`s with an `id`). The vendored DataTable's real `columns` prop is
// `ColumnDefinition<T>[]` (see data-table/types.ts: `accessorKey`/`header`/`type`/`cell`), which
// has no `id` field -- confirmed against data-table/data-table.test.tsx's own usage. This test
// keeps the brief's `c.id ?? accessorKey` fallback (still correct: `id` is just always absent).
describe("getHistoryColumns", () => {
  const historyColumns = getHistoryColumns(i18n.t);

  it("does not include tag_code, site_code, or edge_code columns", () => {
    const ids = historyColumns.map((c) => c.accessorKey);
    expect(ids).not.toContain("tag_code");
    expect(ids).not.toContain("site_code");
    expect(ids).not.toContain("edge_code");
  });

  it("includes a unit column separate from the raw value column", () => {
    const ids = historyColumns.map((c) => c.accessorKey);
    expect(ids).toContain("unit");
    expect(ids).toContain("value");
  });

  it("translates all four column headers (default language is es)", () => {
    const headers = Object.fromEntries(historyColumns.map((c) => [c.accessorKey, c.header]));
    expect(headers.ts).toBe("Fecha y hora");
    expect(headers.value).toBe("Valor");
    expect(headers.unit).toBe("Unidad");
    expect(headers.quality_status).toBe("Calidad");
  });

  it("formats the value cell using the numeric part only (unit lives in its own column)", () => {
    const valueCol = historyColumns.find((c) => c.accessorKey === "value");
    expect(valueCol?.cell?.('{"value":330,"unit":"g"}', {} as never)).toBe("330");
  });

  it("falls back to '-' for an unparseable value", () => {
    const valueCol = historyColumns.find((c) => c.accessorKey === "value");
    expect(valueCol?.cell?.("garbage", {} as never)).toBe("-");
  });

  it("formats the timestamp as a locale string", () => {
    const tsCol = historyColumns.find((c) => c.accessorKey === "ts");
    expect(typeof tsCol?.cell?.("2026-08-25T18:07:30.000Z", {} as never)).toBe("string");
  });
});

function historyRow(overrides: Partial<TagHistory>): TagHistory {
  return {
    ts: "2026-08-25T18:00:00.000Z",
    site_code: "plant-a",
    edge_code: "edge-1",
    tag_code: "tag-1",
    value: 10,
    quality_status: "Good",
    ...overrides,
  };
}

function keyOf(rows: HistoryRow[], tagCode: string, ts: string): string {
  const row = rows.find((r) => r.tag_code === tagCode && r.ts === ts);
  if (!row) throw new Error(`no row for ${tagCode}/${ts}`);
  return row.rowKey;
}

// Regression tests for the finding: rowKey used to be `${tag_code}-${ts}-${arrayIndex}`, which
// renumbers every row after a background refetch shifts array positions (react-query's default
// staleTime: 0 / refetchOnWindowFocus: true, set in main.tsx). Keying on an ordinal among rows
// sharing the same `ts` instead is refetch-stable, since tag_code+ts don't change.
describe("toHistoryRows rowKey derivation", () => {
  it("gives two rows sharing the same ts distinct, stable ordinal-based keys", () => {
    const sameTs = "2026-08-25T18:00:00.000Z";
    const rows = toHistoryRows([
      historyRow({ tag_code: "tag-a", ts: sameTs, value: 1 }),
      historyRow({ tag_code: "tag-a", ts: sameTs, value: 2 }),
    ]);
    expect(rows[0].rowKey).not.toBe(rows[1].rowKey);
    expect(rows[0].rowKey).toBe("tag-a-2026-08-25T18:00:00.000Z-0");
    expect(rows[1].rowKey).toBe("tag-a-2026-08-25T18:00:00.000Z-1");
  });

  it("keeps every existing row's key unchanged when a refetch inserts a new row earlier in the array", () => {
    const raw: TagHistory[] = [
      historyRow({ tag_code: "tag-a", ts: "2026-08-25T18:00:00.000Z", value: 1 }),
      historyRow({ tag_code: "tag-a", ts: "2026-08-25T18:00:01.000Z", value: 2 }),
      historyRow({ tag_code: "tag-a", ts: "2026-08-25T18:00:02.000Z", value: 3 }),
    ];
    const before = toHistoryRows(raw);
    const keysBefore = new Map(before.map((r) => [`${r.tag_code}|${r.ts}`, r.rowKey]));

    // Simulate a background refetch: a brand-new sample (newer ts) lands at the FRONT of the
    // array, the shape react-query's cache would hand back after a poll picks up new data.
    const afterRefetch: TagHistory[] = [
      historyRow({ tag_code: "tag-a", ts: "2026-08-25T18:00:03.000Z", value: 4 }), // new
      ...raw,
    ];
    const after = toHistoryRows(afterRefetch);

    for (const [k, keyBefore] of keysBefore) {
      const [tagCode, ts] = k.split("|");
      expect(keyOf(after, tagCode, ts)).toBe(keyBefore);
    }
  });

  it("keeps existing same-ts rows' relative ordinals unchanged when an unrelated (different-ts) row is inserted earlier", () => {
    const sameTs = "2026-08-25T18:00:00.000Z";
    const raw: TagHistory[] = [
      historyRow({ tag_code: "tag-a", ts: sameTs, value: 1 }),
      historyRow({ tag_code: "tag-a", ts: sameTs, value: 2 }),
    ];
    const before = toHistoryRows(raw);
    const key0Before = before[0].rowKey;
    const key1Before = before[1].rowKey;

    const afterRefetch: TagHistory[] = [
      historyRow({ tag_code: "tag-b", ts: "2026-08-25T17:59:00.000Z", value: 99 }), // new, different ts
      ...raw,
    ];
    const after = toHistoryRows(afterRefetch);
    const sameTsRowsAfter = after.filter((r) => r.tag_code === "tag-a" && r.ts === sameTs);

    expect(sameTsRowsAfter[0].rowKey).toBe(key0Before);
    expect(sameTsRowsAfter[1].rowKey).toBe(key1Before);
  });
});
