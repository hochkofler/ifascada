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
});
