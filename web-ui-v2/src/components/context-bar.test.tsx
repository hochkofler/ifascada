import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ContextBar } from "./context-bar";
import * as apiClient from "@/lib/api-client";
import "../lib/i18n";

describe("ContextBar", () => {
  it("renders Site as a dropdown populated from real tag data, not free text", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      { site_code: "plant-a" } as never,
      { site_code: "plant-b" } as never,
    ]);
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(await screen.findByRole("combobox")).toBeInTheDocument();
  });
});
