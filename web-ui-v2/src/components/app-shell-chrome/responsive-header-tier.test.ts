import { expect, test } from "vitest";
import { getResponsiveHeaderTier } from "./responsive-header-tier";

test("maps viewport widths to the approved responsive tiers", () => {
  expect(getResponsiveHeaderTier(320)).toBe("mobile");
  expect(getResponsiveHeaderTier(767)).toBe("mobile");
  expect(getResponsiveHeaderTier(768)).toBe("tablet");
  expect(getResponsiveHeaderTier(1023)).toBe("tablet");
  expect(getResponsiveHeaderTier(1024)).toBe("desktop");
});
