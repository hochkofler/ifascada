import { expect, test } from "vitest";
import {
  activeNavPath,
  activeNavTarget,
  navCommands,
  resolveNavLabel,
  visibleNavModules,
  type NavGroup,
  type NavModule,
} from "./nav";

const icon = (() => null) as unknown as NavModule["icon"];

function mod(partial: Partial<NavModule> & { labelKey: string }): NavModule {
  return { icon, disabled: false, subItems: [], ...partial };
}

const labels = (mods: NavModule[]) => mods.map((m) => m.labelKey);

test("módulo habilitado sin vistaCode siempre visible", () => {
  expect(labels(visibleNavModules([mod({ labelKey: "a" })], []))).toEqual(["a"]);
});

test("módulo con vistaCode se oculta sin el permiso", () => {
  const mods = [mod({ labelKey: "ventas", vistaCode: "sales.view" })];
  expect(labels(visibleNavModules(mods, []))).toEqual([]);
});

test("módulo con vistaCode visible con el permiso", () => {
  const mods = [mod({ labelKey: "ventas", vistaCode: "sales.view" })];
  expect(labels(visibleNavModules(mods, ["sales.view"]))).toEqual(["ventas"]);
});

test("módulo disabled (no construido) se oculta a todos, incluso al superadmin", () => {
  const mods = [mod({ labelKey: "rrhh", disabled: true })];
  expect(labels(visibleNavModules(mods, []))).toEqual([]);
  expect(labels(visibleNavModules(mods, ["*"]))).toEqual([]);
});

test("menor conocimiento: con solo purchasing.view no se ven ventas ni reportes", () => {
  const mods = [
    mod({ labelKey: "compras", vistaCode: "purchasing.view" }),
    mod({ labelKey: "ventas", vistaCode: "sales.view" }),
    mod({ labelKey: "reportes", vistaCode: "reports.view" }),
  ];
  expect(labels(visibleNavModules(mods, ["purchasing.view"]))).toEqual(["compras"]);
});

test("array vacío de módulos devuelve vacío", () => {
  expect(visibleNavModules([], ["sales.view"])).toEqual([]);
});

test("con varios módulos filtra los sin permiso y los disabled, y preserva el orden", () => {
  const mods = [
    mod({ labelKey: "a" }),
    mod({ labelKey: "ventas", vistaCode: "sales.view" }),
    mod({ labelKey: "compras", vistaCode: "purchasing.view" }),
    mod({ labelKey: "rrhh", disabled: true }),
  ];
  // compras: oculto (sin permiso); rrhh: oculto (disabled). Quedan a y ventas, en orden.
  expect(labels(visibleNavModules(mods, ["sales.view"]))).toEqual(["a", "ventas"]);
});

test("vistaCode array OR — visible si tiene el primer código del array", () => {
  const mods = [
    mod({
      labelKey: "gestion",
      vistaCode: ["support.tickets.manage", "support.tickets.delete"] as const,
    }),
  ];
  expect(labels(visibleNavModules(mods, ["support.tickets.manage"]))).toEqual(["gestion"]);
});

test("vistaCode array OR — visible si tiene el segundo código del array", () => {
  const mods = [
    mod({
      labelKey: "gestion",
      vistaCode: ["support.tickets.manage", "support.tickets.delete"] as const,
    }),
  ];
  expect(labels(visibleNavModules(mods, ["support.tickets.delete"]))).toEqual(["gestion"]);
});

test("vistaCode array OR — oculto si no tiene ninguno de los códigos del array", () => {
  const mods = [
    mod({
      labelKey: "gestion",
      vistaCode: ["support.tickets.manage", "support.tickets.delete"] as const,
    }),
  ];
  expect(labels(visibleNavModules(mods, ["support.tickets.view"]))).toEqual([]);
});

test("wildcard '*' (superadmin) ve un módulo con vistaCode simple", () => {
  const mods = [mod({ labelKey: "ventas", vistaCode: "sales.view" })];
  expect(labels(visibleNavModules(mods, ["*"]))).toEqual(["ventas"]);
});

test("wildcard '*' (superadmin) ve un módulo con vistaCode array", () => {
  const mods = [
    mod({
      labelKey: "gestion",
      vistaCode: ["support.tickets.manage", "support.tickets.delete"] as const,
    }),
  ];
  expect(labels(visibleNavModules(mods, ["*"]))).toEqual(["gestion"]);
});

test("wildcard '*' (superadmin) ve todos los módulos con vistaCode", () => {
  const mods = [
    mod({ labelKey: "a" }),
    mod({ labelKey: "ventas", vistaCode: "sales.view" }),
    mod({ labelKey: "compras", vistaCode: "purchasing.view" }),
  ];
  expect(labels(visibleNavModules(mods, ["*"]))).toEqual(["a", "ventas", "compras"]);
});

test("navCommands: omite módulos sin subítems (no aportan destino navegable)", () => {
  const mods = [
    mod({ labelKey: "compras", subItems: [{ labelKey: "compras.sdc", to: "/x" }] }),
    mod({ labelKey: "vacio", subItems: [] }),
  ];
  expect(navCommands(mods, []).map((c) => c.labelKey)).toEqual(["compras"]);
});

test("navCommands: respeta permisos (reutiliza visibleNavModules)", () => {
  const mods = [
    mod({
      labelKey: "ventas",
      vistaCode: "sales.view",
      subItems: [{ labelKey: "ventas.inst", to: "/v" }],
    }),
  ];
  expect(navCommands(mods, [])).toEqual([]);
  expect(navCommands(mods, ["sales.view"]).map((c) => c.labelKey)).toEqual(["ventas"]);
});

test("navCommands: aplana los subítems con su ruta y preserva el orden", () => {
  const mods = [
    mod({
      labelKey: "compras",
      subItems: [
        { labelKey: "compras.sdc", to: "/purchasing/purchase-requests" },
        { labelKey: "compras.oc", to: "/purchasing/purchase-orders" },
      ],
    }),
  ];
  expect(navCommands(mods, [])).toEqual([
    {
      key: "labelKey:compras|/purchasing/purchase-requests|/purchasing/purchase-orders",
      labelKey: "compras",
      icon,
      items: [
        {
          labelKey: "compras.sdc",
          to: "/purchasing/purchase-requests",
          searchText: "compras compras.sdc",
        },
        {
          labelKey: "compras.oc",
          to: "/purchasing/purchase-orders",
          searchText: "compras compras.oc",
        },
      ],
    },
  ]);
});

test("resolveNavLabel traduce labels estáticos y preserva labels runtime", () => {
  const translatedKeys: string[] = [];
  const translate = (key: string) => {
    translatedKeys.push(key);
    return `translated:${key}`;
  };

  expect(resolveNavLabel({ labelKey: "nav.comprasCxp" }, translate)).toBe(
    "translated:nav.comprasCxp"
  );
  expect(resolveNavLabel({ label: "Ingreso de cajas" }, translate)).toBe("Ingreso de cajas");
  expect(translatedKeys).toEqual(["nav.comprasCxp"]);
});

test("navCommands preserva labels runtime y estáticos sin traducirlos", () => {
  const runtimeModule: NavModule = {
    labelKey: "nav.operaciones",
    icon,
    disabled: false,
    subItems: [{ label: "Salida especial", to: "/operations/salida/new" }],
  };

  expect(navCommands([runtimeModule], [])).toEqual([
    {
      key: "labelKey:nav.operaciones|/operations/salida/new",
      labelKey: "nav.operaciones",
      icon,
      items: [
        {
          label: "Salida especial",
          to: "/operations/salida/new",
          searchText: "nav.operaciones Salida especial",
        },
      ],
    },
  ]);
});

test("navCommands genera identidades distintas para labels iguales con destinos distintos", () => {
  const modules = [
    mod({
      labelKey: "nav.duplicado",
      subItems: [{ labelKey: "nav.primero", to: "/first" }],
    }),
    mod({
      labelKey: "nav.duplicado",
      subItems: [{ labelKey: "nav.segundo", to: "/second" }],
    }),
  ];

  expect(navCommands(modules, []).map((command) => command.key)).toEqual([
    "labelKey:nav.duplicado|/first",
    "labelKey:nav.duplicado|/second",
  ]);
});

test("la identidad usa el discriminante original y no el label traducido", () => {
  const module = mod({
    labelKey: "nav.inventario",
    subItems: [{ labelKey: "nav.sub.ingreso", to: "/inventory/receipts" }],
  });

  expect(resolveNavLabel(module, () => "Inventario")).toBe("Inventario");
  expect(resolveNavLabel(module, () => "Inventory")).toBe("Inventory");
  expect(navCommands([module], [])[0]?.key).toBe("labelKey:nav.inventario|/inventory/receipts");
});

test("NavModule rechaza label y labelKey simultáneos en el límite TypeScript", () => {
  const invalidModule = {
    label: "Operaciones",
    labelKey: "nav.operaciones",
    icon,
    disabled: false,
    subItems: [],
  };

  // @ts-expect-error El contrato discriminado permite exactamente una fuente de label.
  const _navModule: NavModule = invalidModule;
  expect(_navModule).toBeTruthy();
});

function group(
  labelKey: string,
  children: NavGroup["children"],
  vistaCode?: NavGroup["vistaCode"]
): NavGroup {
  return { labelKey, children, ...(vistaCode ? { vistaCode } : {}) };
}

test("filtra recursivamente un árbol de cuatro niveles y elimina grupos vacíos", () => {
  const modules = [
    mod({
      labelKey: "admin",
      subItems: [
        group("settings", [
          group("frontend", [{ label: "Visible", to: "/visible", vistaCode: "inventory.view" }]),
          group("wms", [{ label: "Hidden", to: "/hidden", vistaCode: "reports.view" }]),
        ]),
      ],
    }),
  ];

  expect(visibleNavModules(modules, ["inventory.view"])[0]?.subItems).toEqual([
    group("settings", [
      group("frontend", [{ label: "Visible", to: "/visible", vistaCode: "inventory.view" }]),
    ]),
  ]);
});

test("wildcard preserves every recursive descendant", () => {
  const modules = [
    mod({
      labelKey: "admin",
      vistaCode: "admin.access",
      subItems: [group("settings", [{ label: "WMS", to: "/wms", vistaCode: "inventory.view" }])],
    }),
  ];

  expect(navCommands(modules, ["*"])[0]?.items[0]?.to).toBe("/wms");
});

test("activeNavPath uses segment boundaries and returns the most specific leaf", () => {
  const tree: NavGroup = {
    labelKey: "root",
    children: [
      group("short", [{ label: "Short", to: "/admin/config" }]),
      group("deep", [{ label: "Deep", to: "/admin/config/wms" }]),
    ],
  };

  expect(activeNavPath(tree, "/admin/config/wms/edit")).toEqual([
    "root",
    "deep",
    "Deep",
    "/admin/config/wms",
  ]);
  expect(activeNavPath(tree, "/admin/configuration")).toEqual([]);
});

test("activeNavTarget marks only the most specific matching leaf", () => {
  const nodes: NavGroup["children"] = [
    { label: "Overview", to: "/admin/system-configuration" },
    { label: "WMS", to: "/admin/system-configuration/wms" },
  ];

  expect(activeNavTarget(nodes, "/admin/system-configuration/wms")).toBe(
    "/admin/system-configuration/wms"
  );
  expect(activeNavTarget(nodes, "/admin/system-configuration/wms/edit")).toBe(
    "/admin/system-configuration/wms"
  );
});

test("navCommands includes every ancestor label in the searchable chain", () => {
  const modules = [
    mod({
      labelKey: "admin",
      subItems: [group("settings", [{ label: "Frontend", to: "/frontend" }])],
    }),
  ];

  expect(navCommands(modules, [])[0]?.items[0]?.searchText).toBe("admin settings Frontend");
  expect(navCommands(modules, [], (key) => `translated:${key}`)[0]?.items[0]?.searchText).toBe(
    "translated:admin translated:settings Frontend"
  );
});
