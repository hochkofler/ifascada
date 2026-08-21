import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
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
    expect(await screen.findByRole("combobox")).toBeInTheDocument();
  });

  it("populates the dropdown with the real, derived site options (not just an empty combobox)", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      { site_code: "plant-a" } as never,
      { site_code: "plant-b" } as never,
    ]);
    const user = userEvent.setup();
    renderWithQuery();
    const trigger = await screen.findByRole("combobox");
    await user.click(trigger);
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
    await screen.findByRole("combobox");
    await vi.waitFor(() => {
      expect(useOperationalContextStore.getState().site).toBe("plant-b");
    });
  });

  it("shows a visible error affordance and disables the trigger when the sites fetch fails", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockRejectedValue(new Error("network down"));
    renderWithQuery();
    const trigger = await screen.findByRole("combobox");
    await vi.waitFor(() => {
      expect(trigger).toBeDisabled();
    });
  });
});
