import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { EdgesOnlineBadge } from "./edges-online-badge";
import type { EdgeCurrent } from "@/lib/api-client";
import "../../lib/i18n";

// Verified against crates/central-server/src/persistence/postgres.rs:642-648
// (insert_telemetry's edge_current_state upsert hardcodes status = 'online')
// and web-ui/components/context-bar.tsx:8-9 (the already-shipped, verified-correct
// `status.toLowerCase() === "online"` formula). "ok" is a health-path literal that
// lands in the same column via a separate, already-documented backend inconsistency
// -- not the value this counter should key on.
const onlineEdge = { status: "online", edge_code: "e1" } as EdgeCurrent;
const offlineEdge = { status: "disconnected", edge_code: "e2" } as EdgeCurrent;

describe("EdgesOnlineBadge", () => {
  it("counts only edges with an online status in the numerator", () => {
    render(<EdgesOnlineBadge edges={[onlineEdge, offlineEdge]} />);
    expect(screen.getByText("1/2")).toBeInTheDocument();
  });

  it("shows 0/0 with no edges rather than crashing", () => {
    render(<EdgesOnlineBadge edges={[]} />);
    expect(screen.getByText("0/0")).toBeInTheDocument();
  });

  it("does not count a status of 'ok' as online (health-path literal, not the telemetry-path literal)", () => {
    const okStatusEdge = { status: "ok", edge_code: "e3" } as EdgeCurrent;
    render(<EdgesOnlineBadge edges={[okStatusEdge, offlineEdge]} />);
    expect(screen.getByText("0/2")).toBeInTheDocument();
  });
});
