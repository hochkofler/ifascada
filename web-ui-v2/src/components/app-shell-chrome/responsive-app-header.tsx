import type { ReactElement, ReactNode } from "react";
import { SidebarTrigger } from "@/components/ui/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { AppBreadcrumb, AppRouteLabel } from "./app-breadcrumb";
import { MobileMoreMenu } from "./mobile-more-menu";
import type { NavModule } from "./nav";
import { type ResponsiveHeaderTier, useResponsiveHeaderTier } from "./responsive-header-tier";

/**
 * Cosechado de libs/app-shell/src/responsive-app-header.tsx de ifahub. Se quitaron dos slots que
 * dependen de identidad OIDC (`UserMenu`, `AppSwitcher`) y el disparador del command palette,
 * que se difiere: con dos destinos, un ⌘K no aporta nada -- se suma cuando haya paginas que
 * justifiquen buscarlas.
 *
 * `getResponsiveActionPlacement` sigue listando "command" y "apps": el render ya tolera slots
 * ausentes (`actions[key] == null ? null : ...`), asi que la tabla de ubicacion queda intacta y
 * lista para cuando esas acciones existan.
 */
type HeaderActionKey = "command" | "notifications" | "apps" | "theme";

export interface ResponsiveHeaderViewProps {
  tier: ResponsiveHeaderTier;
  sidebarTrigger?: ReactNode;
  routeContext?: ReactNode;
  breadcrumb?: ReactNode;
  command?: ReactNode;
  notifications?: ReactNode;
  appSwitcher?: ReactNode;
  theme?: ReactNode;
}

export interface ResponsiveAppHeaderProps {
  navModules: readonly NavModule[];
  headerActions?: ReactNode;
}

export function getResponsiveActionPlacement(tier: ResponsiveHeaderTier): {
  inline: readonly HeaderActionKey[];
  more: readonly HeaderActionKey[];
} {
  if (tier === "mobile") {
    return { inline: [], more: ["command", "notifications", "apps", "theme"] };
  }
  if (tier === "tablet") {
    return { inline: ["command", "notifications"], more: ["apps", "theme"] };
  }
  return { inline: ["command", "notifications", "apps", "theme"], more: [] };
}

const HEADER_CLASS =
  "sticky top-0 z-40 flex h-14 min-w-0 shrink-0 items-center gap-2 border-b bg-background/95 px-2 backdrop-blur supports-[backdrop-filter]:bg-background/60 sm:px-4";

export function ResponsiveHeaderView(props: ResponsiveHeaderViewProps): ReactElement {
  const { tier, sidebarTrigger, routeContext, breadcrumb } = props;
  const placement = getResponsiveActionPlacement(tier);
  const actions: Record<HeaderActionKey, ReactNode> = {
    command: props.command,
    notifications: props.notifications,
    apps: props.appSwitcher,
    theme: props.theme,
  };
  const more = Object.fromEntries(placement.more.map((key) => [key, actions[key]])) as Partial<
    Record<HeaderActionKey, ReactNode>
  >;

  return (
    <header className={HEADER_CLASS}>
      {sidebarTrigger}
      <div className="min-w-0 flex-1">{tier === "mobile" ? routeContext : breadcrumb}</div>
      <div className="flex shrink-0 items-center gap-1 md:gap-2">
        {placement.inline.map((key) =>
          actions[key] == null ? null : (
            <div key={key} data-header-action={key} className="shrink-0">
              {actions[key]}
            </div>
          )
        )}
        {placement.more.length > 0 && (
          <MobileMoreMenu
            command={more.command}
            notifications={more.notifications}
            appSwitcher={more.apps}
            theme={more.theme}
          />
        )}
      </div>
    </header>
  );
}

export function ResponsiveAppHeader({
  navModules,
  headerActions,
}: ResponsiveAppHeaderProps): ReactElement {
  const tier = useResponsiveHeaderTier();
  return (
    <ResponsiveHeaderView
      tier={tier}
      sidebarTrigger={<SidebarTrigger className="-ml-1 shrink-0" />}
      routeContext={<AppRouteLabel navModules={navModules} />}
      breadcrumb={<AppBreadcrumb navModules={navModules} />}
      notifications={headerActions}
      theme={<ThemeToggle />}
    />
  );
}
