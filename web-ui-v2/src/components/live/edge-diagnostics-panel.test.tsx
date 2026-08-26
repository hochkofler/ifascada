import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EdgeDiagnosticsPanel } from "./edge-diagnostics-panel";
import * as edgeActions from "@/lib/edge-actions";
import * as apiClient from "@/lib/api-client";
import "../../lib/i18n";

describe("EdgeDiagnosticsPanel reset action", () => {
  it("shows confirmed-recovered feedback once last_seen_at actually advances after reset", async () => {
    vi.spyOn(edgeActions, "resetEdge").mockResolvedValue({ accepted: true, topic: "x", request_id: null });
    vi.spyOn(apiClient, "fetchEdgesCurrent")
      .mockResolvedValueOnce([{ last_seen_at: "2026-08-20T10:00:00Z" } as never]) // before
      .mockResolvedValueOnce([{ last_seen_at: "2026-08-20T10:00:05Z" } as never]); // after, advanced
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    // The real implementation's first poll attempt only fires after one RESET_POLL_INTERVAL_MS
    // (2000ms) wait -- @testing-library's default waitFor timeout (1000ms) is shorter than that,
    // so it's raised here to comfortably clear one real poll interval plus test overhead. The
    // outer test timeout (3rd arg to `it`) is raised to match.
    await waitFor(() => expect(screen.getByText(/reset confirmado/i)).toBeInTheDocument(), { timeout: 8000 });
  }, 10000);

  it("shows a no-recovery-confirmed warning when accepted:true but last_seen_at never advances", async () => {
    vi.spyOn(edgeActions, "resetEdge").mockResolvedValue({ accepted: true, topic: "x", request_id: null });
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([{ last_seen_at: "2026-08-20T10:00:00Z" } as never]);
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(
      () => expect(screen.getByText(/no confirm[oó] recuperaci[oó]n/i)).toBeInTheDocument(),
      { timeout: 35000 }
    );
    // The outer test timeout (3rd arg to `it`) defaults to 5000ms, far shorter than the inner
    // waitFor's own 35000ms budget above -- raised to match so the full real 15x2000ms poll
    // schedule (Step 3) actually gets to run to its "timed-out-no-recovery" conclusion instead
    // of the whole test being killed first.
  }, 40000);

  it("shows an error state when the reset call itself fails (not just an unconfirmed recovery)", async () => {
    vi.spyOn(edgeActions, "resetEdge").mockRejectedValue(new Error("network error"));
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([{ last_seen_at: "2026-08-20T10:00:00Z" } as never]);
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(() => expect(screen.getByText(/error al enviar/i)).toBeInTheDocument());
  });

  // Regression test for the finding: the pre-reset `fetchEdgesCurrent` probe used to sit
  // outside handleReset's try block. Since it's a real network call made while diagnosing a
  // network problem, it can plausibly reject -- and when it did, that was an unhandled promise
  // rejection with no error state shown, leaving disabled={resetState === "sent"} stuck forever
  // even though resetEdge itself was never actually called.
  it("shows an error state (not a permanently stuck 'sent' state) when the pre-reset probe itself rejects", async () => {
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockRejectedValue(new Error("network error"));
    // spyOn (without restoring between tests) returns the SAME persistent spy across this whole
    // describe block, so its call history accumulates from earlier tests -- clear it here so
    // "not called" below asserts on this test's click only.
    const resetEdgeSpy = vi.spyOn(edgeActions, "resetEdge");
    resetEdgeSpy.mockClear();
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-pack-1" site="plant-a" open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /reset/i }));
    await waitFor(() => expect(screen.getByText(/error al enviar/i)).toBeInTheDocument());
    expect(screen.getByRole("button", { name: /reset/i })).not.toBeDisabled();
    expect(resetEdgeSpy).not.toHaveBeenCalled();
  });
});
