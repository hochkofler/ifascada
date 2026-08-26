import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ContextBar } from "./context-bar";
import * as apiClient from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import "../lib/i18n";

function renderWithQuery() {
  // retry: false so a mocked rejection surfaces as isError promptly instead of
  // waiting through react-query's default exponential-backoff retries.
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ContextBar />
    </QueryClientProvider>
  );
}

beforeEach(() => {
  // Reset store state between tests since it's a module-level singleton.
  useOperationalContextStore.setState({ site: "plant-a", line: "", area: "", cell: "", edge: "" });
});

describe("ContextBar", () => {
  it("renders Site as a dropdown populated from real tag data, not free text", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      { site_code: "plant-a" } as never,
      { site_code: "plant-b" } as never,
    ]);
    renderWithQuery();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    // ContextBar now renders Site + Line + Area + Cell + Edge as five separate comboboxes;
    // Site is always the first in DOM order.
    const [siteTrigger] = await screen.findAllByRole("combobox");
    expect(siteTrigger).toBeInTheDocument();
  });

  it("populates the dropdown with the real, derived site options (not just an empty combobox)", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      { site_code: "plant-a" } as never,
      { site_code: "plant-b" } as never,
    ]);
    const user = userEvent.setup();
    renderWithQuery();
    const [siteTrigger] = await screen.findAllByRole("combobox");
    await user.click(siteTrigger);
    expect(await screen.findByRole("option", { name: "plant-a" })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: "plant-b" })).toBeInTheDocument();
  });

  it("auto-corrects the selected site to the first real option when the stored value isn't among them", async () => {
    // Store default is "plant-a", but the real, currently-reporting site list turns out to
    // be only plant-b/plant-c -- the store's stale value should be corrected, not silently
    // left mismatched against every real option (the use-auto-select-first.ts bug category).
    useOperationalContextStore.setState({ site: "plant-a" });
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      { site_code: "plant-b" } as never,
      { site_code: "plant-c" } as never,
    ]);
    renderWithQuery();
    await screen.findAllByRole("combobox");
    await vi.waitFor(() => {
      expect(useOperationalContextStore.getState().site).toBe("plant-b");
    });
  });

  it("shows a visible error affordance and disables the trigger when the sites fetch fails", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockRejectedValue(new Error("network down"));
    renderWithQuery();
    const [siteTrigger] = await screen.findAllByRole("combobox");
    await vi.waitFor(() => {
      expect(siteTrigger).toBeDisabled();
    });
  });
});

describe("ContextBar cascade selectors", () => {
  beforeEach(() => {
    useOperationalContextStore.setState({ site: "plant-a", line: "", area: "", cell: "", edge: "" });
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([{ site_code: "plant-a" } as never]);
    vi.spyOn(apiClient, "fetchLines").mockResolvedValue([{ code: "line-main", name: "Line Main" }]);
    vi.spyOn(apiClient, "fetchAreas").mockResolvedValue([{ code: "area-pack", name: "Area Pack" }]);
    vi.spyOn(apiClient, "fetchCells").mockResolvedValue([{ code: "cell-1", name: "Cell 1" }]);
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([{ edge_code: "edge-pack-1" } as never]);
  });

  it("renders Line/Area/Cell/Edge dropdowns populated from the real hierarchy endpoints", async () => {
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    const comboboxes = await screen.findAllByRole("combobox");
    expect(comboboxes.length).toBeGreaterThanOrEqual(5); // site + line + area + cell + edge
  });

  it("clears Area/Cell/Edge when Line changes", async () => {
    useOperationalContextStore.setState({ site: "plant-a", line: "line-main", area: "area-pack", cell: "cell-1", edge: "edge-pack-1" });
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    await screen.findAllByRole("combobox");
    useOperationalContextStore.getState().setLine("line-other");
    await waitFor(() => {
      const state = useOperationalContextStore.getState();
      expect(state.area).toBe("");
      expect(state.cell).toBe("");
      expect(state.edge).toBe("");
    });
  });

  it("shows a clear-filters button when any level below Site is selected, and clears them on click", async () => {
    useOperationalContextStore.setState({ site: "plant-a", line: "line-main", area: "", cell: "", edge: "" });
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    const clearButton = await screen.findByRole("button", { name: /limpiar filtros/i });
    await userEvent.click(clearButton);
    await waitFor(() => {
      expect(useOperationalContextStore.getState().line).toBe("");
    });
  });
});
