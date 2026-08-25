import { describe, it, expect } from "vitest";
import { parseValueWithUnit, numericValue, formatValueWithUnit } from "./value-formatting";

describe("parseValueWithUnit", () => {
  it("splits a compound value string into number and unit", () => {
    expect(parseValueWithUnit("330 g")).toEqual({ number: 330, unit: "g" });
  });

  it("handles a plain number with no unit", () => {
    expect(parseValueWithUnit(42)).toEqual({ number: 42, unit: null });
  });

  it("handles a negative decimal with a unit", () => {
    expect(parseValueWithUnit("-8.05238 g")).toEqual({ number: -8.05238, unit: "g" });
  });

  it("handles a plain numeric string with no unit", () => {
    expect(parseValueWithUnit("42")).toEqual({ number: 42, unit: null });
  });

  it("returns null for unparseable strings", () => {
    expect(parseValueWithUnit("not a number")).toBeNull();
  });

  it("returns null for null/undefined/objects", () => {
    expect(parseValueWithUnit(null)).toBeNull();
    expect(parseValueWithUnit(undefined)).toBeNull();
    expect(parseValueWithUnit({ foo: "bar" })).toBeNull();
  });

  // Real tag values coming out of central-server for weighing-scale tags are compound,
  // JSON-encoded strings (see web-ui/lib/hmi-value.ts's parseCompound) -- not the plain
  // "NUMBER UNIT" space-joined string the brief's own tests above use. Both must parse.
  it("parses a JSON-compound value string (real scale-tag format)", () => {
    expect(parseValueWithUnit('{"value":330,"unit":"g"}')).toEqual({ number: 330, unit: "g" });
  });

  it("parses a JSON-compound value string with no unit", () => {
    expect(parseValueWithUnit('{"value":330}')).toEqual({ number: 330, unit: null });
  });

  it("falls back gracefully when JSON-looking string doesn't parse", () => {
    expect(parseValueWithUnit("{not json")).toBeNull();
  });
});

describe("numericValue", () => {
  it("extracts just the number for filtering purposes", () => {
    expect(numericValue("100 mg")).toBe(100);
    expect(numericValue(-5)).toBe(-5);
    expect(numericValue("not a number")).toBeNull();
  });

  it("extracts the number out of a JSON-compound value", () => {
    expect(numericValue('{"value":7.5,"unit":"kg"}')).toBe(7.5);
  });
});

describe("formatValueWithUnit", () => {
  it("joins the number and unit for display", () => {
    expect(formatValueWithUnit("330 g")).toBe("330 g");
    expect(formatValueWithUnit('{"value":7.5,"unit":"kg"}')).toBe("7.5 kg");
  });

  it("shows just the number when there is no unit", () => {
    expect(formatValueWithUnit(42)).toBe("42");
  });

  it("falls back to '-' for an unparseable value", () => {
    expect(formatValueWithUnit("garbage")).toBe("-");
    expect(formatValueWithUnit(null)).toBe("-");
  });
});
