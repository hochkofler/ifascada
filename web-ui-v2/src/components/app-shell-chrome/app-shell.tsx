import type { ReactElement, ReactNode } from "react";
import { Outlet } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { AppSidebar } from "./app-sidebar";
import { ResponsiveAppHeader } from "./responsive-app-header";
import type { NavModule } from "./nav";

/**
 * Cosechado de libs/app-shell/src/app-shell.tsx de ifahub. Lo que se quito, todo por depender de
 * una identidad que esta app no tiene: `RequireAuth` (y su `authFallback`), `SessionExpiredDialog`
 * y `appSwitcher`. Tambien el `CommandPalette` y su atajo ⌘K, que se difieren hasta que haya mas
 * de dos destinos que buscar.
 *
 * Lo que se conservo tal cual y es lo que importa: `SidebarProvider defaultOpen={false}` (el
 * sidebar arranca colapsado), el header sticky con breadcrumb, el slot `topBanner`, y el
 * `<main className="min-w-0 flex-1 p-3 sm:p-6">` -- ese `min-w-0` es el que evita que una tabla
 * ancha empuje el layout y genere scroll horizontal de pagina.
 */
export interface AppShellBranding {
  /** Nodo del logo (ej. `<IfaScadaLogo/>`). */
  logo?: ReactNode;
  /** Nombre de la app bajo el logo (visible con el sidebar expandido). */
  appName?: string;
}

export interface AppShellProps {
  branding: AppShellBranding;
  navModules: readonly NavModule[];
  /** Slot: acciones a la derecha del header (la campana de mensajes, por ejemplo). */
  headerActions?: ReactNode;
  /** Slot: banner global entre el header y el main (avisos del sistema). */
  topBanner?: ReactNode;
}

export function AppShell({
  branding,
  navModules,
  headerActions,
  topBanner,
}: AppShellProps): ReactElement {
  return (
    <SidebarProvider defaultOpen={false}>
      <AppSidebar branding={branding} navModules={navModules} />
      <SidebarInset>
        <ResponsiveAppHeader navModules={navModules} headerActions={headerActions} />
        {topBanner}
        <main className="min-w-0 flex-1 p-3 sm:p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
