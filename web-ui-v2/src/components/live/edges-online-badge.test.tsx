import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { EdgesOnlineBadge } from "./edges-online-badge";
import type { EdgeCurrent } from "@/lib/api-client";
import "../../lib/i18n";

const onlineEdge = { status: "online", edge_code: "e1" } as EdgeCurrent;
const okEdge = { status: "ok", edge_code: "e2" } as EdgeCurrent;
const offlineEdge = { status: "disconnected", edge_code: "e3" } as EdgeCurrent;

describe("EdgesOnlineBadge", () => {
  it("counts edges with status 'online' OR 'ok' in the numerator (fixes the always-wrong count)", () => {
    render(<EdgesOnlineBadge edges={[onlineEdge, okEdge, offlineEdge]} />);
    expect(screen.getByText("2/3")).toBeInTheDocument();
  });

  it("shows 0/0 with no edges rather than crashing", () => {
    render(<EdgesOnlineBadge edges={[]} />);
    expect(screen.getByText("0/0")).toBeInTheDocument();
  });

  // El estado sano NO puede pintarse con el rojo de marca (variant "default" = bg-primary):
  // en una pantalla de planta eso se lee como alarma. El tono se deriva del conteo.
  it("uses the ok tone when every edge is online", () => {
    render(<EdgesOnlineBadge edges={[onlineEdge, okEdge]} />);
    expect(screen.getByText("2/2")).toHaveAttribute("data-tone", "ok");
  });

  it("uses the warn tone when only some edges are online", () => {
    render(<EdgesOnlineBadge edges={[onlineEdge, offlineEdge]} />);
    expect(screen.getByText("1/2")).toHaveAttribute("data-tone", "warn");
  });

  it("uses the bad tone when edges are reporting but none is online", () => {
    render(<EdgesOnlineBadge edges={[offlineEdge]} />);
    expect(screen.getByText("0/1")).toHaveAttribute("data-tone", "bad");
  });

  // Cero edges no es una falla: es "todavia no se sabe". Pintarlo de rojo seria una alarma falsa.
  it("uses the neutral tone when there are no edges at all", () => {
    render(<EdgesOnlineBadge edges={[]} />);
    expect(screen.getByText("0/0")).toHaveAttribute("data-tone", "neutral");
  });
});
