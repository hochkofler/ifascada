import { describe, it, expect } from "vitest";
import { applySelectionClick } from "./selection";

type Row = { key: string };
const rows: Row[] = [{ key: "a" }, { key: "b" }, { key: "c" }, { key: "d" }, { key: "e" }];
const rowKey = (r: Row) => r.key;

describe("applySelectionClick", () => {
  it("toggles a single row on when clicked without shift", () => {
    const next = applySelectionClick(new Map(), rows, rowKey, rows[1], null, false);
    expect(Array.from(next.keys())).toEqual(["b"]);
  });

  it("toggles a selected row back off when clicked again without shift", () => {
    const selected = new Map([["b", rows[1]]]);
    const next = applySelectionClick(selected, rows, rowKey, rows[1], "b", false);
    expect(next.has("b")).toBe(false);
  });

  it("selects a forward range on shift-click", () => {
    const selected = new Map([["a", rows[0]]]);
    const next = applySelectionClick(selected, rows, rowKey, rows[3], "a", true);
    expect(Array.from(next.keys()).sort()).toEqual(["a", "b", "c", "d"]);
  });

  it("selects a backward range on shift-click (last-clicked after the new click)", () => {
    const selected = new Map([["d", rows[3]]]);
    const next = applySelectionClick(selected, rows, rowKey, rows[1], "d", true);
    expect(Array.from(next.keys()).sort()).toEqual(["b", "c", "d"]);
  });

  it("adds the range to any pre-existing selection rather than replacing it", () => {
    const selected = new Map([["e", rows[4]]]);
    const next = applySelectionClick(selected, rows, rowKey, rows[1], "a", true);
    // "a" isn't in `rows` selection state itself but acts as the anchor; range a..b selected,
    // plus the pre-existing "e".
    expect(Array.from(next.keys()).sort()).toEqual(["a", "b", "e"]);
  });

  it("falls back to a plain toggle when shift-clicked with no prior anchor", () => {
    const next = applySelectionClick(new Map(), rows, rowKey, rows[2], null, true);
    expect(Array.from(next.keys())).toEqual(["c"]);
  });

  it("falls back to a plain toggle when the anchor row is no longer in the row set", () => {
    const next = applySelectionClick(new Map(), rows, rowKey, rows[2], "not-a-real-key", true);
    expect(Array.from(next.keys())).toEqual(["c"]);
  });

  it("still toggles a row that isn't found in the row set (plain click doesn't need an index)", () => {
    const missing: Row = { key: "z" };
    const next = applySelectionClick(new Map(), rows, rowKey, missing, null, false);
    expect(Array.from(next.keys())).toEqual(["z"]);
  });
});
