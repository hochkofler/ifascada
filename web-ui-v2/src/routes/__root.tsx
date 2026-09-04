import { createRootRoute } from "@tanstack/react-router";
import { AppShell } from "@/components/app-shell-chrome/app-shell";
import { IfaScadaLogo } from "@/components/brand/ifascada-logo";
import { MessageLogDrawer, useActionFailureNotices } from "@/components/notifications";
import { NAV_MODULES } from "@/components/nav-config";

/**
 * `staticData.breadcrumb` es el contrato que lee `buildBreadcrumbs`: el breadcrumb esta dirigido
 * por la jerarquia de rutas, NO por heuristica sobre el pathname (ADR-0014 de ifahub). Cada ruta
 * que deba aparecer como nivel declara su etiqueta aca.
 */
declare module "@tanstack/react-router" {
  interface StaticDataRouteOption {
    breadcrumb?: string;
  }
}

function RootLayout() {
  // En el shell y no en una pagina: un aviso de accion fallida tiene que llegar estes
  // donde estes, no solo con la vista En vivo abierta.
  useActionFailureNotices();

  return (
    <AppShell
      branding={{ logo: <IfaScadaLogo /> }}
      navModules={NAV_MODULES}
      headerActions={<MessageLogDrawer />}
    />
  );
}

export const Route = createRootRoute({ component: RootLayout });
