import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as apiClient from "@/lib/api-client";
import * as sse from "@/lib/sse";
import { useOperationalContextStore } from "@/store/context-store";
import "../lib/i18n";

// live.tsx is a route component (createFileRoute) -- test the underlying LivePage logic via a
// minimal standalone render matching the pattern already used for other route-adjacent tests
// in this codebase (see app-shell.test.tsx's router-free RouterProvider setup) is unnecessary
// here since LivePage itself has no router-specific behavior; import and render it directly.
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
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    // Suppress React 19 Suspense warning during render, as the component suspends
    // while queries load (expected) but the effect still runs and sets up SSE before
    // the queries complete.
    const consoleError = vi.spyOn(console, "error").mockImplementation((...args) => {
      if (args[0]?.toString().includes("suspended inside an `act` scope")) {
        return;
      }
      console.error(...args);
    });
    try {
      render(
        <QueryClientProvider client={qc}>
          <LivePage />
        </QueryClientProvider>
      );
    } finally {
      consoleError.mockRestore();
    }
    // The effect runs during mount even if the component suspends, so subscribeSse
    // should have been called by now
    await waitFor(() => {
      expect(sse.subscribeSse).toHaveBeenCalled();
    });
    expect(sseHandler).toBeTypeOf("function");
  });
});
