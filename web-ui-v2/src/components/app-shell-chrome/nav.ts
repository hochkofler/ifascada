import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { hasPermission, type PermissionCode } from "@/lib/permissions";

export type NavLabel = { labelKey: string; label?: never } | { label: string; labelKey?: never };

export type NavLink = NavLabel & {
  to: string;
  vistaCode?: PermissionCode | readonly PermissionCode[];
  /** Badge opcional (p.ej. contador de pendientes) junto al label. */
  badge?: ReactNode;
};

export type NavGroup = NavLabel & {
  vistaCode?: PermissionCode | readonly PermissionCode[];
  /** Badge opcional (p.ej. contador de pendientes) junto al label. */
  badge?: ReactNode;
  children: readonly NavNode[];
};

export type NavNode = NavGroup | NavLink;

/** Compatibility name for existing two-level navigation callers. */
export type NavSubItem = NavLink;

export type NavModule = NavLabel & {
  icon: LucideIcon;
  disabled: boolean;
  /**
   * Vista requerida para ver el módulo. Sin vistaCode, no se filtra por permiso.
   * Array = OR: basta con tener una de las vistas listadas.
   */
  vistaCode?: PermissionCode | readonly PermissionCode[];
  subItems: readonly NavNode[];
  /** Badge opcional (p.ej. contador) junto al label del módulo. */
  badge?: ReactNode;
};

export function resolveNavLabel(item: NavLabel, translate: (key: string) => string): string {
  return item.label ?? translate(item.labelKey);
}

/**
 * Filtra los módulos visibles en el nav, con política de menor conocimiento
 * (ver docs/decisions/0009): un usuario solo ve lo que existe y puede usar.
 *
 * - `disabled` (no construido / en desarrollo): se OCULTA a todos. No se muestra
 *   griseado: lo que no existe todavía no se anuncia. El entry queda en el código
 *   como roadmap, invisible hasta construirse.
 * - con `vistaCode`: visible solo si el usuario tiene el permiso (array = OR).
 *   Así, quien solo hace solicitudes de compra no ve siquiera el módulo de ventas.
 *
 * Delega en hasPermission (única fuente de la semántica de authz, incl. el wildcard
 * "*" = superadmin) para honrar el wildcard igual que el resto de call-sites
 * (useCan, <Can>, <RequireVista>, el guard del backend).
 */
export function visibleNavModules(
  modules: readonly NavModule[],
  // readonly string[] (not PermissionCode[]): usePermissions() returns the raw string[] from
  // the Authentik claim and the auth engine is not touched. PermissionCode ⊆ string: nothing is lost.
  permissions: readonly string[]
): NavModule[] {
  return modules
    .filter((m) => !m.disabled && (!m.vistaCode || hasPermission(permissions, m.vistaCode)))
    .map((m) => ({
      ...m,
      subItems: visibleNavNodes(m.subItems, permissions),
    }));
}

export function isNavGroup(node: NavNode): node is NavGroup {
  return "children" in node;
}

export function isNavLink(node: NavNode): node is NavLink {
  return !isNavGroup(node);
}

function visibleNavNode(node: NavNode, permissions: readonly string[]): NavNode | null {
  if (node.vistaCode && !hasPermission(permissions, node.vistaCode)) return null;
  if (!isNavGroup(node)) return { ...node };

  const children = visibleNavNodes(node.children, permissions);
  return children.length === 0 ? null : { ...node, children };
}

export function visibleNavNodes(
  nodes: readonly NavNode[],
  permissions: readonly string[]
): NavNode[] {
  return nodes.flatMap((node) => {
    const visible = visibleNavNode(node, permissions);
    return visible ? [visible] : [];
  });
}

function routeMatches(pathname: string, to: string): boolean {
  return pathname === to || pathname.startsWith(`${to}/`);
}

function nodeLabelDiscriminant(node: NavLabel): string {
  return node.label ?? node.labelKey;
}

function findActivePath(node: NavNode, pathname: string): { path: string[]; to: string } | null {
  if (!isNavGroup(node)) {
    return routeMatches(pathname, node.to)
      ? { path: [nodeLabelDiscriminant(node), node.to], to: node.to }
      : null;
  }

  const matches = node.children
    .map((child) => findActivePath(child, pathname))
    .filter((match): match is { path: string[]; to: string } => match !== null)
    .sort((left, right) => right.to.length - left.to.length);
  const best = matches[0];
  return best ? { path: [nodeLabelDiscriminant(node), ...best.path], to: best.to } : null;
}

export function activeNavPath(node: NavNode, pathname: string): string[] {
  return findActivePath(node, pathname)?.path ?? [];
}

/** Ruta de la hoja más específica que corresponde al pathname actual. */
export function activeNavTarget(nodes: readonly NavNode[], pathname: string): string | undefined {
  return nodes
    .map((node) => findActivePath(node, pathname))
    .filter((match): match is { path: string[]; to: string } => match !== null)
    .sort((left, right) => right.to.length - left.to.length)[0]?.to;
}

/** Un módulo navegable del command palette: el label del módulo es el grupo, los subítems sus comandos. */
export type NavCommand = NavLabel & {
  key: string;
  icon: LucideIcon;
  items: (NavLabel & { to: string; searchText: string })[];
};

/**
 * Identidad estable para renderizar un módulo de navegación.
 * No depende del texto traducido y distingue módulos con el mismo label
 * cuando apuntan a destinos diferentes.
 */
export function navModuleKey(module: NavModule): string {
  return [
    module.labelKey === undefined ? "label" : `labelKey:${module.labelKey}`,
    ...collectNavRoutes(module.subItems),
  ].join("|");
}

function collectNavRoutes(nodes: readonly NavNode[]): string[] {
  return nodes.flatMap((node) => (isNavGroup(node) ? collectNavRoutes(node.children) : [node.to]));
}

function copyNavLabel(item: NavLabel): NavLabel {
  return item.label === undefined ? { labelKey: item.labelKey } : { label: item.label };
}

/**
 * Deriva los comandos de navegación del command palette desde los módulos del nav.
 * Reutiliza visibleNavModules (mismo filtrado por permisos que el sidebar) y omite
 * los módulos sin subítems (no aportan destino navegable). El orden se preserva.
 */
export function navCommands(
  modules: readonly NavModule[],
  permissions: readonly string[],
  translate: (key: string) => string = (key) => key
): NavCommand[] {
  return visibleNavModules(modules, permissions).flatMap((m) => {
    const items = flattenNavLinks(m.subItems, [copyNavLabel(m)], translate);
    return items.length > 0
      ? [{ ...copyNavLabel(m), key: navModuleKey(m), icon: m.icon, items }]
      : [];
  });
}

function flattenNavLinks(
  nodes: readonly NavNode[],
  ancestors: readonly NavLabel[],
  translate: (key: string) => string
): (NavLabel & { to: string; searchText: string })[] {
  return nodes.flatMap((node) => {
    if (isNavGroup(node)) {
      return flattenNavLinks(node.children, [...ancestors, copyNavLabel(node)], translate);
    }
    const labels = [...ancestors, copyNavLabel(node)];
    return [
      {
        ...copyNavLabel(node),
        to: node.to,
        searchText: labels.map((label) => resolveNavLabel(label, translate)).join(" "),
      },
    ];
  });
}
