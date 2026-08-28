import type { ReactElement } from "react";
import { Link, useRouterState } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import {
  Sidebar,
  SidebarContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { ALL_PERMISSIONS } from "@/lib/permissions";
import { NavModuleItem } from "./nav-module-item";
import { navModuleKey, resolveNavLabel, visibleNavModules, type NavModule } from "./nav";
import type { AppShellBranding } from "./app-shell";

/**
 * Cosechado de libs/app-shell/src/app-sidebar.tsx de ifahub. Dos cosas se quitaron, ambas por
 * depender de una identidad que esta app no tiene:
 *
 *   - el item de "Cerrar sesion" (`useAuth().logout`),
 *   - `usePermissions()`, reemplazado por `ALL_PERMISSIONS` -- el comodin "*" deja pasar todo,
 *     y `visibleNavModules` queda intacto para el dia que haya login.
 *
 * El `SidebarFooter` desaparecio con el logout: el toggle de tema y la campana de mensajes ahora
 * viven en el header, que es donde ifahub los pone.
 */
export interface AppSidebarProps {
  branding: AppShellBranding;
  navModules: readonly NavModule[];
}

export function AppSidebar({ branding, navModules }: AppSidebarProps): ReactElement {
  const { t } = useTranslation();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const modules = visibleNavModules(navModules, ALL_PERMISSIONS);
  const { isMobile } = useSidebar();

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader className="p-3 group-data-[collapsible=icon]:p-2">
        <Link to="/">
          {branding.logo}
          {branding.appName && (
            <span className="mt-1 block text-xs font-medium text-muted-foreground group-data-[collapsible=icon]:hidden">
              {branding.appName}
            </span>
          )}
        </Link>
      </SidebarHeader>

      <SidebarContent>
        <SidebarMenu>
          {modules.map((module) => {
            const label = resolveNavLabel(module, t);
            if (module.disabled) {
              return (
                <SidebarMenuItem key={navModuleKey(module)}>
                  <SidebarMenuButton
                    tooltip={label}
                    disabled
                    size={isMobile ? "lg" : "default"}
                    className="pointer-events-none opacity-40"
                  >
                    <module.icon />
                    <span>{label}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            }
            return <NavModuleItem key={navModuleKey(module)} module={module} pathname={pathname} />;
          })}
        </SidebarMenu>
      </SidebarContent>
    </Sidebar>
  );
}
