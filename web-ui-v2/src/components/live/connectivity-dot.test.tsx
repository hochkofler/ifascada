import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ConnectivityDot } from "./connectivity-dot";

describe("ConnectivityDot", () => {
  it("renders with the good/warn/bad state as a data attribute for styling", () => {
    render(<ConnectivityDot state="good" />);
    expect(screen.getByTestId("connectivity-dot")).toHaveAttribute("data-state", "good");
  });

  it("passes through a title for the tooltip", () => {
    render(<ConnectivityDot state="bad" title="device_state: disconnected" />);
    expect(screen.getByTestId("connectivity-dot")).toHaveAttribute("title", "device_state: disconnected");
  });
});
