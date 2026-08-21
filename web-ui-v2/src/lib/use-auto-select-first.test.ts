import { describe, it, expect, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useAutoSelectFirst } from "./use-auto-select-first";

describe("useAutoSelectFirst", () => {
  it("corrects a stale/invalid current value to the first real option once options load", () => {
    const setCurrent = vi.fn();
    type Props = { options: string[] | undefined; current: string };
    const { rerender } = renderHook(
      ({ options, current }: Props) => useAutoSelectFirst(options, current, setCurrent),
      { initialProps: { options: undefined, current: "plant-a" } as Props }
    );
    expect(setCurrent).not.toHaveBeenCalled();

    rerender({ options: ["plant-b", "plant-c"], current: "plant-a" });
    expect(setCurrent).toHaveBeenCalledWith("plant-b");
  });

  it("does not touch a current value that is already among the real options", () => {
    const setCurrent = vi.fn();
    renderHook(() => useAutoSelectFirst(["plant-a", "plant-b"], "plant-a", setCurrent));
    expect(setCurrent).not.toHaveBeenCalled();
  });

  it("does nothing while the options list is empty or not yet loaded", () => {
    const setCurrent = vi.fn();
    renderHook(() => useAutoSelectFirst([], "plant-a", setCurrent));
    expect(setCurrent).not.toHaveBeenCalled();
  });
});
