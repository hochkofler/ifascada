import { describe, it, expect } from "vitest";
import { formatServerDateTime, formatServerTime, SERVER_TIME_ZONE } from "./datetime";

describe("SERVER_TIME_ZONE", () => {
  it("is America/La_Paz", () => {
    expect(SERVER_TIME_ZONE).toBe("America/La_Paz");
  });
});

describe("formatServerDateTime", () => {
  it("formats a UTC ISO timestamp in America/La_Paz (UTC-4, no DST)", () => {
    // 2026-08-26T18:31:13.144564Z UTC -> 14:31 in America/La_Paz (UTC-4)
    const result = formatServerDateTime("2026-08-26T18:31:13.144564Z");
    expect(result).toContain("14:31");
    expect(result).toMatch(/26[/-]8|8[/-]26|ago/i); // date portion present in some locale-valid form
  });
});

describe("formatServerTime", () => {
  it("formats just the time portion in America/La_Paz", () => {
    const result = formatServerTime("2026-08-26T18:31:13.144564Z");
    expect(result).toContain("14:31");
    expect(result).not.toMatch(/2026/); // no date portion
  });

  it("handles midnight UTC correctly (crosses to previous day locally)", () => {
    // 2026-08-26T02:00:00Z UTC -> 2026-08-25 22:00 in America/La_Paz (UTC-4)
    const result = formatServerTime("2026-08-26T02:00:00Z");
    expect(result).toContain("22:00");
  });
});
