import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { Suspense } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as apiClient from "@/lib/api-client";
import * as sse from "@/lib/sse";
import { useOperationalContextStore } from "@/store/context-store";
import "../lib/i18n";

// live.tsx is a route component (createFileRoute) with autoCodeSplitting enabled, which wraps
// component: LivePage in a lazy boundary. Route.options.component! is therefore a lazy component,
// not the underlying component function, and suspends before the effect runs. We import LivePage
// directly to bypass this lazy wrapper and test the component logic itself, using a Suspense
// boundary to handle suspension from useQuery.
import { LivePage } from "./live";

describe("Live page SSE patching", () => {
  let sseHandler: ((evt: sse.RtEvent) => void) | undefined;

  beforeEach(() => {
    useOperationalContextStore.setState({ site: "plant-a", line: "", area: "", cell: "", edge: "" });
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([
      { edge_code: "edge-mix-1", site_code: "plant-a", status: "online", last_seen_at: new Date().toISOString() } as never,
    ]);
    vi.spyOn(apiClient, "fetchDevicesCurrent").mockResolvedValue([
      { edge_code: "edge-mix-1", device_code: "dev-mix-1", site_code: "plant-a", state: "connected" } as never,
    ]);
    vi.spyOn(sse, "subscribeSse").mockImplementation((onMessage) => {
      sseHandler = onMessage;
      return () => {};
    });
  });

  it("subscribes to SSE on mount alongside the existing poll", async () => {
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <Suspense fallback={<div>Loading...</div>}>
          <LivePage />
        </Suspense>
      </QueryClientProvider>
    );
    // The effect runs during mount. Suspense boundary handles suspension from useQuery,
    // allowing the effect to set up SSE subscription before queries complete.
    await waitFor(() => {
      expect(sse.subscribeSse).toHaveBeenCalled();
    });
    // Also verify the device text appears once component finishes loading
    await screen.findByText("dev-mix-1");
    expect(sseHandler).toBeTypeOf("function");
  });
});

describe("Live page SSE refetch throttling", () => {
  it("does not invalidate queries more than once per second even under a burst of SSE events", async () => {
    vi.useFakeTimers();
    let sseHandler: ((evt: sse.RtEvent) => void) | undefined;
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([]);
    vi.spyOn(apiClient, "fetchDevicesCurrent").mockResolvedValue([]);
    vi.spyOn(sse, "subscribeSse").mockImplementation((onMessage) => {
      sseHandler = onMessage;
      return () => {};
    });
    const qc = new QueryClient();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    render(
      <QueryClientProvider client={qc}>
        <Suspense fallback={<div>Loading...</div>}>
          <LivePage />
        </Suspense>
      </QueryClientProvider>
    );
    await vi.waitFor(() => expect(sseHandler).toBeTypeOf("function"));

    // Simulate the real edge-sim load: an SSE event every ~25ms for 3 seconds --
    // far faster than the 120ms flush tick, matching the burst this finding was based on.
    for (let elapsedMs = 0; elapsedMs < 3000; elapsedMs += 25) {
      sseHandler!({ event_type: "telemetry", site: "plant-a", agent: "edge-mix-1", payload: { device_id: "dev-mix-1" }, published_at: new Date().toISOString() });
      await vi.advanceTimersByTimeAsync(25);
    }

    // Over 3 seconds, a once-per-second throttle allows at most 3-4 invalidation rounds
    // (2 queries invalidated per round: live-edges, live-devices) -- not the ~25 rounds
    // a 120ms-tick-driven invalidation would produce.
    expect(invalidateSpy.mock.calls.length).toBeLessThanOrEqual(8);
    vi.useRealTimers();
  });
});
