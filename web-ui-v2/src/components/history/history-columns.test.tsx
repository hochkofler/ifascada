import { describe, it, expect } from "vitest";
import { historyColumns } from "./history-columns";

// NOTE: the brief's illustrative sketch for this file used TanStack's `createColumnHelper`
// (producing `ColumnDef`s with an `id`). The vendored DataTable's real `columns` prop is
// `ColumnDefinition<T>[]` (see data-table/types.ts: `accessorKey`/`header`/`type`/`cell`), which
// has no `id` field -- confirmed against data-table/data-table.test.tsx's own usage. This test
// keeps the brief's `c.id ?? accessorKey` fallback (still correct: `id` is just always absent).
describe("historyColumns", () => {
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
