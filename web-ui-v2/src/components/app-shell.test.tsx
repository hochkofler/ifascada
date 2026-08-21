import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { RouterProvider, createRouter, createRootRoute, createRoute } from "@tanstack/react-router";
import { AppShell } from "./app-shell";
import "../lib/i18n";

describe("AppShell", () => {
  it("renders links to Live and History, and nothing else", async () => {
    const rootRoute = createRootRoute({ component: AppShell });
    const liveRoute = createRoute({ getParentRoute: () => rootRoute, path: "/live", component: () => <div>live</div> });
    const routeTree = rootRoute.addChildren([liveRoute]);
    const router = createRouter({ routeTree, history: undefined });
    render(<RouterProvider router={router} />);
    // The router resolves its initial match asynchronously, so wait for the
    // sidebar (rendered by the root component regardless of which child
    // route matches) rather than asserting synchronously after render.
    expect(await screen.findByText("En vivo")).toBeInTheDocument();
    expect(await screen.findByText("Histórico")).toBeInTheDocument();
    expect(screen.queryByText(/overview|trends|commands|audit/i)).not.toBeInTheDocument();
  });
});
