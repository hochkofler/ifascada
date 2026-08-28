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
    expect(screen.getByTestId("connectivity-dot")).toHaveAttribute(
      "title",
      "device_state: disconnected"
    );
  });

  // WCAG 1.4.1: el color no puede ser el unico portador de significado. `title` no llega de
  // forma confiable a un lector de pantalla ni ayuda a quien no distingue verde de rojo.
  it("exposes the state to assistive tech as an accessible name", () => {
    render(<ConnectivityDot state="bad" title="device_state: disconnected" />);
    const dot = screen.getByRole("img", { name: "device_state: disconnected" });
    expect(dot).toBeInTheDocument();
  });

  // Segunda via, no cromatica: cada estado tiene su propia forma.
  it("gives each state a distinct shape, not just a distinct color", () => {
    const { rerender } = render(<ConnectivityDot state="good" />);
    const shapeOf = () => screen.getByTestId("connectivity-dot").className;
    const good = shapeOf();
    rerender(<ConnectivityDot state="warn" />);
    const warn = shapeOf();
    rerender(<ConnectivityDot state="bad" />);
    const bad = shapeOf();
    expect(new Set([good, warn, bad]).size).toBe(3);
  });

  // Ningun color crudo de la paleta de Tailwind: solo tokens semanticos del tema.
  it("styles itself from theme tokens, never raw palette colors", () => {
    render(<ConnectivityDot state="good" />);
    expect(screen.getByTestId("connectivity-dot").className).not.toMatch(
      /(bg|text|ring|border)-(emerald|amber|red|green|gray|slate|zinc)-\d{2,3}/
    );
  });
});
