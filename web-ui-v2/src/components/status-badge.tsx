import type { JSX, ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

/**
 * Badge de estado operativo, propio de ifascada.
 *
 * NO se agregan variantes al `Badge` de components/ui: ese primitivo es copia de
 * libs/ui/src/ui/badge.tsx de ifahub, y ifahub tampoco tiene variantes semanticas ahi. Este
 * componente compone el primitivo desde afuera, que es el mismo patron que usa ifahub en
 * apps/ifa-web/src/components/sap/status-badge.tsx: el primitivo queda intacto y la semantica
 * de dominio vive en codigo de la app.
 *
 * Por que existe: sin el, todo estado caia en `variant="default"` = `bg-primary`, o sea el rojo
 * de marca IFA. En una pantalla de planta eso invierte la lectura -- el operador ve alarma
 * donde el sistema dice "todo bien". Los tokens --success/--warning/--info ya estaban definidos
 * en globals.css (con su variante on-dark calculada) y no los usaba nadie.
 */
export type StatusTone = "ok" | "warn" | "bad" | "neutral";

const TONE_CLASS: Record<StatusTone, string> = {
  ok: "border-success/30 bg-success/15 text-success",
  warn: "border-warning/30 bg-warning/15 text-warning",
  bad: "border-destructive/30 bg-destructive/15 text-destructive",
  neutral: "border-border bg-muted text-muted-foreground",
};

export function StatusBadge({
  tone,
  children,
  className,
  title,
}: {
  tone: StatusTone;
  children: ReactNode;
  className?: string;
  title?: string;
}): JSX.Element {
  return (
    <Badge
      variant="outline"
      title={title}
      // `data-tone` sigue la convencion de los primitivos (data-slot / data-variant): permite
      // seleccionar por atributo sin clases magicas, y hace el tono verificable en un test.
      data-tone={tone}
      className={cn("tabular-nums", TONE_CLASS[tone], className)}
    >
      {children}
    </Badge>
  );
}
