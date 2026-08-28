import { Fragment, type ReactElement } from "react";
import { Link, useMatches, useRouterState } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { isNavGroup, resolveNavLabel, type NavModule, type NavNode } from "./nav";

function hasBreadcrumb(data: unknown): data is { breadcrumb: string } {
  return (
    typeof data === "object" &&
    data !== null &&
    "breadcrumb" in data &&
    typeof (data as Record<string, unknown>).breadcrumb === "string"
  );
}

export interface Crumb {
  label: string;
  to: string;
}

interface ResolvedNavCrumb extends Crumb {
  runtime: boolean;
}

export function currentRouteLabel(crumbs: readonly Crumb[]): string | undefined {
  return crumbs.at(-1)?.label;
}

export function buildBreadcrumbs(
  navModules: readonly NavModule[],
  pathname: string,
  routeCrumbs: readonly Crumb[],
  translate: (key: string) => string
): Crumb[] {
  const candidates = navModules.flatMap((module) => {
    const path = findNodePath(module.subItems, pathname, translate);
    return path
      ? [
          {
            label: resolveNavLabel(module, translate),
            to: path.at(-1)?.to ?? path[0]?.to ?? "",
            path,
          },
        ]
      : [];
  });
  const current = candidates.sort((left, right) => right.path.length - left.path.length)[0];
  if (!current) return [...routeCrumbs];
  const navCrumbs = [
    current.label ? { label: current.label, to: current.to } : null,
    ...current.path.map(({ label, to }) => ({ label, to })),
  ].filter((crumb): crumb is Crumb => crumb !== null);
  const terminal = current.path.at(-1);
  const routeTail = routeCrumbs;
  const visibleNavCrumbs = terminal?.runtime ? navCrumbs : navCrumbs.slice(0, -1);
  return [...visibleNavCrumbs, ...routeTail];
}

function findNodePath(
  nodes: readonly NavNode[],
  pathname: string,
  translate: (key: string) => string
): ResolvedNavCrumb[] | null {
  const matches = nodes.flatMap((node) => {
    if (isNavGroup(node)) {
      const childPath = findNodePath(node.children, pathname, translate);
      return childPath
        ? [
            [
              {
                label: resolveNavLabel(node, translate),
                to: childPath.at(-1)?.to ?? "",
                runtime: node.label !== undefined,
              },
              ...childPath,
            ],
          ]
        : [];
    }
    return pathname === node.to || pathname.startsWith(`${node.to}/`)
      ? [
          [
            {
              label: resolveNavLabel(node, translate),
              to: node.to,
              runtime: node.label !== undefined,
            },
          ],
        ]
      : [];
  });
  return (
    matches.sort(
      (left, right) => (right.at(-1)?.to.length ?? 0) - (left.at(-1)?.to.length ?? 0)
    )[0] ?? null
  );
}

function useResolvedCrumbs(navModules: readonly NavModule[]): Crumb[] {
  const { t } = useTranslation();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const matches = useMatches();

  // Un crumb por cada ruta de la cadena que declara breadcrumb, enlazado a su ruta real.
  const routeCrumbs: Crumb[] = [];
  for (const m of matches) {
    if (hasBreadcrumb(m.staticData)) {
      routeCrumbs.push({ label: m.staticData.breadcrumb, to: m.pathname });
    }
  }

  return buildBreadcrumbs(navModules, pathname, routeCrumbs, t);
}

export function AppRouteLabel({
  navModules,
}: {
  navModules: readonly NavModule[];
}): ReactElement | null {
  const crumbs = useResolvedCrumbs(navModules);
  const label = currentRouteLabel(crumbs);
  if (label === undefined) return null;
  return <span className="block truncate text-sm font-medium">{label}</span>;
}

/**
 * Breadcrumb dirigido por la jerarquía de rutas (no heurístico): recorre la
 * cadena de matches de TanStack Router y arma un crumb por cada ruta que declara
 * `staticData.breadcrumb`, enlazándolo a su `pathname` real. Antepone el módulo
 * del nav (que es un grupo, no una ruta) enlazándolo a su "home" (1ª sub-ruta).
 * El último crumb es la página actual (no enlazada).
 *
 * Convención para que aparezca un nivel intermedio NAVEGABLE: ese nivel debe ser
 * una ruta (layout route con `<Outlet/>`) que declare `staticData.breadcrumb`. Un
 * `index.tsx` matchea solo exacto y no representa un nivel intermedio. Ver
 * docs/decisions/0014-breadcrumbs-dirigidos-por-ruta.md.
 */
export function AppBreadcrumb({
  navModules,
}: {
  navModules: readonly NavModule[];
}): ReactElement | null {
  const crumbs = useResolvedCrumbs(navModules);

  if (crumbs.length === 0) return null;

  return (
    <Breadcrumb>
      <BreadcrumbList>
        {crumbs.map((crumb, i) => {
          const isLast = i === crumbs.length - 1;
          return (
            <Fragment key={`${crumb.to}-${i}`}>
              <BreadcrumbItem>
                {isLast ? (
                  <BreadcrumbPage>{crumb.label}</BreadcrumbPage>
                ) : (
                  <BreadcrumbLink asChild>
                    <Link to={crumb.to}>{crumb.label}</Link>
                  </BreadcrumbLink>
                )}
              </BreadcrumbItem>
              {!isLast && <BreadcrumbSeparator />}
            </Fragment>
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );
}
