import { expect, test } from "vitest";
import { buildBreadcrumbs, currentRouteLabel } from "./app-breadcrumb";
import type { NavModule } from "./nav";

const icon = (() => null) as unknown as NavModule["icon"];

test("compact route context uses only the final resolved crumb", () => {
  expect(
    currentRouteLabel([
      { label: "Manufactura", to: "/manufacturing" },
      { label: "WMS móvil", to: "/manufacturing/wms" },
    ])
  ).toBe("WMS móvil");
  expect(currentRouteLabel([])).toBe(undefined);
});

test("breadcrumb de ejecución muestra nombre runtime y acción de ruta", () => {
  const modules: NavModule[] = [
    {
      labelKey: "nav.operaciones",
      icon,
      disabled: false,
      subItems: [
        {
          label: "Ingreso de cajas por fabricación",
          to: "/operations/ingreso-cajas/new",
        },
      ],
    },
  ];

  expect(
    buildBreadcrumbs(
      modules,
      "/operations/ingreso-cajas/new",
      [{ label: "Ejecutar operación", to: "/operations/ingreso-cajas/new" }],
      (key) => `translated:${key}`
    )
  ).toEqual([
    { label: "translated:nav.operaciones", to: "/operations/ingreso-cajas/new" },
    { label: "Ingreso de cajas por fabricación", to: "/operations/ingreso-cajas/new" },
    { label: "Ejecutar operación", to: "/operations/ingreso-cajas/new" },
  ]);
});

test("breadcrumb estático conserva el comportamiento previo sin duplicar el subitem", () => {
  const modules: NavModule[] = [
    {
      labelKey: "nav.comprasCxp",
      icon,
      disabled: false,
      subItems: [{ labelKey: "nav.sub.solicitudesCompra", to: "/purchasing/requests" }],
    },
  ];

  expect(
    buildBreadcrumbs(
      modules,
      "/purchasing/requests",
      [{ label: "Solicitudes", to: "/purchasing/requests" }],
      (key) => `translated:${key}`
    )
  ).toEqual([
    { label: "translated:nav.comprasCxp", to: "/purchasing/requests" },
    { label: "Solicitudes", to: "/purchasing/requests" },
  ]);
});

test("breadcrumb recursivo incluye todos los grupos y conserva crumbs de la ruta", () => {
  const modules: NavModule[] = [
    {
      labelKey: "nav.administracion",
      icon,
      disabled: false,
      subItems: [
        {
          labelKey: "nav.sub.systemConfiguration",
          children: [{ label: "WMS", to: "/admin/system-configuration/wms" }],
        },
      ],
    },
  ];
  expect(
    buildBreadcrumbs(
      modules,
      "/admin/system-configuration/wms/edit",
      [
        { label: "Editar módulo", to: "/admin/system-configuration/wms/edit" },
        { label: "Detalle adicional", to: "/admin/system-configuration/wms/edit/detail" },
      ],
      (key) => `translated:${key}`
    )
  ).toEqual([
    { label: "translated:nav.administracion", to: "/admin/system-configuration/wms" },
    { label: "translated:nav.sub.systemConfiguration", to: "/admin/system-configuration/wms" },
    { label: "WMS", to: "/admin/system-configuration/wms" },
    { label: "Editar módulo", to: "/admin/system-configuration/wms/edit" },
    { label: "Detalle adicional", to: "/admin/system-configuration/wms/edit/detail" },
  ]);
});
