import type { JSX } from "react";
import { cn } from "@/lib/utils";

/**
 * Marca de ifascada, construida sobre el lockup que ya existe en la familia IFA.
 *
 * No se invento iconografia: el chevron de tres capas es el mismo de `IfaHubLogo` en
 * libs/ui/src/brand de ifahub -- es la marca corporativa de IFA, compartida entre productos -- y
 * el wordmark sigue el mismo patron (`Ifa` en color de texto + el nombre del producto en el rojo
 * de marca). Lo que cambia es el producto, no la identidad.
 *
 * Los colores salen de tokens (`fill-primary`, `fill-ifa-coral`, `fill-ifa-red-deep`), asi que
 * NO hace falta una prop `variant="dark"`: cambian solos con la clase `.dark`. Es la misma
 * decision que documenta el spec del brand refresh de ifahub, y la razon por la que el SVG del
 * kit no se copia tal cual (usa hex inline, que viola la regla de "cero CSS custom").
 */
export function IfaScadaMark({
  variant = "full",
  className,
}: {
  variant?: "full" | "single";
  className?: string;
}): JSX.Element {
  if (variant === "single") {
    return (
      <svg
        viewBox="22 20 30 24"
        role="img"
        aria-label="ifascada"
        className={cn("h-6 w-auto", className)}
      >
        <path d="M24 22 L36 22 L50 32 L36 42 L24 42 L38 32 Z" className="fill-primary" />
      </svg>
    );
  }
  return (
    <svg
      viewBox="12 20 50 24"
      role="img"
      aria-label="ifascada"
      className={cn("h-6 w-auto", className)}
    >
      <path d="M14 22 L26 22 L40 32 L26 42 L14 42 L28 32 Z" className="fill-ifa-coral" />
      <path d="M24 22 L36 22 L50 32 L36 42 L24 42 L38 32 Z" className="fill-primary" />
      <path d="M34 22 L46 22 L60 32 L46 42 L34 42 L48 32 Z" className="fill-ifa-red-deep" />
    </svg>
  );
}

/**
 * Lockup responsive. Expandido: chevron de tres capas + wordmark en Sora. Dentro de un Sidebar
 * colapsado (`group-data-[collapsible=icon]`) el chevron triple no entra en el riel de iconos,
 * asi que cambia a un chevron simple y suelta el wordmark.
 */
export function IfaScadaLogo({ className }: { className?: string }): JSX.Element {
  return (
    <span className={cn("inline-flex items-center gap-2.5", className)}>
      <IfaScadaMark className="group-data-[collapsible=icon]:hidden" />
      <IfaScadaMark variant="single" className="hidden h-5 group-data-[collapsible=icon]:block" />
      <span className="font-sora text-lg font-semibold leading-none tracking-[-0.03em] text-foreground group-data-[collapsible=icon]:hidden">
        Ifa<span className="font-bold text-primary">Scada</span>
      </span>
    </span>
  );
}
