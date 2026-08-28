import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as apiClient from "@/lib/api-client";
import * as sse from "@/lib/sse";
import { useOperationalContextStore } from "@/store/context-store";
import "../lib/i18n";

// LivePage lives in components/live/live-page.tsx (not routes/live.tsx, which is just the
// createFileRoute wiring) so that TanStack Router's autoCodeSplitting can code-split the
// route without a named `LivePage` export defeating it. Importing it directly from its real
// module here means this test exercises the same component the app renders, with no lazy-route
// boundary to work around.
import { LivePage } from "@/components/live/live-page";

describe("Live page SSE patching", () => {
  let sseHandler: ((evt: sse.RtEvent) => void) | undefined;

  beforeEach(() => {
    useOperationalContextStore.setState({
      site: "plant-a",
      line: "",
      area: "",
      cell: "",
      edge: "",
    });
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([
      {
        edge_code: "edge-mix-1",
        site_code: "plant-a",
        status: "online",
        last_seen_at: new Date().toISOString(),
      } as never,
    ]);
    vi.spyOn(apiClient, "fetchDevicesCurrent").mockResolvedValue([
      {
        edge_code: "edge-mix-1",
        device_code: "dev-mix-1",
        site_code: "plant-a",
        state: "connected",
      } as never,
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
        <LivePage />
      </QueryClientProvider>
    );
    // The effect runs during mount, before the queries resolve.
    await waitFor(() => {
      expect(sse.subscribeSse).toHaveBeenCalled();
    });
    // Also verify the device text appears once component finishes loading
    await screen.findByText("dev-mix-1");
    expect(sseHandler).toBeTypeOf("function");
  });
});

describe("Live page SSE refetch throttling", () => {
  beforeEach(() => {
    useOperationalContextStore.setState({
      site: "plant-a",
      line: "",
      area: "",
      cell: "",
      edge: "",
    });
  });

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
        <LivePage />
      </QueryClientProvider>
    );
    await vi.waitFor(() => expect(sseHandler).toBeTypeOf("function"));

    // Simulate the real edge-sim load: an SSE event every ~25ms for 3 seconds --
    // far faster than the 120ms flush tick, matching the burst this finding was based on.
    for (let elapsedMs = 0; elapsedMs < 3000; elapsedMs += 25) {
      sseHandler!({
        event_type: "telemetry",
        site: "plant-a",
        agent: "edge-mix-1",
        payload: { device_id: "dev-mix-1" },
        published_at: new Date().toISOString(),
      });
      await vi.advanceTimersByTimeAsync(25);
    }

    // Over 3 seconds, a once-per-second throttle allows at most 3-4 invalidation ROUNDS --
    // not the ~25 rounds a 120ms-tick-driven invalidation would produce. Se mide en rondas y no
    // en llamadas crudas a proposito: cada ronda invalida una query por clave, asi que contar
    // llamadas obligaria a retocar este numero cada vez que la pagina suma una query (paso al
    // agregar live-tags), escondiendo si lo que cambio fue el throttle o solo el divisor.
    const KEYS_PER_ROUND = 3; // live-edges, live-devices, live-tags
    const rounds = invalidateSpy.mock.calls.length / KEYS_PER_ROUND;
    expect(rounds).toBeLessThanOrEqual(4);
    vi.useRealTimers();
  });
});

describe("Live page edges with no matching devices", () => {
  beforeEach(() => {
    useOperationalContextStore.setState({
      site: "plant-a",
      line: "",
      area: "",
      cell: "",
      edge: "",
    });
    vi.spyOn(sse, "subscribeSse").mockImplementation(() => () => {});
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([]);
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
  });

  it("renders an edge with zero devices and opens its diagnostics panel on click", async () => {
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([
      {
        edge_code: "edge-silent-1",
        site_code: "plant-a",
        status: "online",
        last_seen_at: new Date().toISOString(),
      } as never,
    ]);
    vi.spyOn(apiClient, "fetchDevicesCurrent").mockResolvedValue([]);
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <LivePage />
      </QueryClientProvider>
    );

    // El diagnostico se abre desde su propia accion, no haciendo clic en la fila: en la grilla
    // nueva el clic en la fila es para expandir sus tags. El requisito no cambia -- Reset tiene
    // que seguir siendo alcanzable para un edge sin ningun device.
    await screen.findByText("edge-silent-1");
    await userEvent.click(await screen.findByRole("button", { name: /diagn/i }));

    // The diagnostics panel (with its Reset button) must be reachable for an edge that has
    // no device rows to click through -- otherwise Reset is unreachable for exactly the edges
    // that need it most (an edge that stopped reporting entirely).
    expect(await screen.findByRole("button", { name: /reset/i })).toBeInTheDocument();
  });
});
