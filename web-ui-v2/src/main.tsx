import "./styles/globals.css";
import "./lib/i18n";
import "./components/app-shell-chrome/i18n";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./lib/query-client";
import { ErrorBoundary } from "./components/error-boundary";
import { ThemeProvider } from "./components/theme-provider";
import { Toaster } from "./components/ui/sonner";
import { routeTree } from "./routeTree.gen";

const router = createRouter({ routeTree });
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

/**
 * Composicion de providers cosechada de libs/app-shell/src/app-providers.tsx de ifahub, sin la
 * capa de autenticacion (esta app no tiene login todavia -- ver getAuthHeader en lib/api-client).
 * El orden importa: ErrorBoundary es el mas externo para que capture fallos de cualquier provider
 * de adentro, y ThemeProvider va antes que el Toaster porque sonner lee el tema con useTheme.
 */
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <ThemeProvider>
        <QueryClientProvider client={queryClient}>
          <RouterProvider router={router} />
          <Toaster richColors position="top-right" />
        </QueryClientProvider>
      </ThemeProvider>
    </ErrorBoundary>
  </StrictMode>
);
