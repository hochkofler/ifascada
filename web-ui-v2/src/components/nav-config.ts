import { Activity } from "lucide-react";
import type { NavModule } from "@/components/app-shell-chrome/nav";

/**
 * Modelo de navegacion de ifascada.
 *
 * Las dos paginas cuelgan de un unico modulo "Operacion" en vez de ser dos items sueltos: es la
 * forma que el `NavModuleItem` cosechado espera (un modulo agrupa `subItems`), y ademas le da al
 * breadcrumb una jerarquia real -- "Operacion / En vivo" en lugar de un solo nivel.
 *
 * Los `labelKey` son claves i18n, no texto: `resolveNavLabel` las resuelve. La union `NavLabel`
 * es un XOR tipado -- o `labelKey` (traducible) o `label` (texto que viene del backend en
 * runtime), nunca ambos.
 *
 * Cuando aparezcan alarmas, configuracion de tags o reportes, se suman como modulos hermanos y
 * el `vistaCode` de cada uno queda listo para el dia que exista autenticacion.
 */
export const NAV_MODULES: readonly NavModule[] = [
  {
    labelKey: "nav.operations",
    icon: Activity,
    disabled: false,
    subItems: [
      { labelKey: "nav.live", to: "/live" },
      { labelKey: "nav.history", to: "/history" },
      { labelKey: "nav.connections", to: "/connections" },
    ],
  },
];
