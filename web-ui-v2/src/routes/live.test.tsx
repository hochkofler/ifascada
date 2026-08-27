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
