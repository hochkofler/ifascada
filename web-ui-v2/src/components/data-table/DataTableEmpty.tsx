import type { ReactNode, JSX } from "react";
import { Inbox } from "lucide-react";
import { useTranslation } from "react-i18next";
import { TABLES_NS } from "./i18n";
import type { EmptyStateConfig } from "./types";

export type DataTableEmptyProps = EmptyStateConfig & {
  /** Override del ícono por defecto (Inbox). */
  icon?: ReactNode;
};

/**
 * Estado vacío de la grilla: ícono + título y, opcionalmente, una descripción
 * y una acción primaria (CTA, p.ej. "Crear el primero"). El consumidor las
 * pasa vía la prop `emptyState` del DataTable.
 */
export function DataTableEmpty({
  title,
  description,
  action,
  icon,
}: DataTableEmptyProps): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  return (
    <div className="flex flex-col items-center justify-center gap-1 py-16 text-center text-muted-foreground">
      {icon ?? <Inbox className="mb-2 size-10 opacity-40" />}
      <p className="text-sm font-medium text-foreground/80">{title ?? t("empty.title")}</p>
      {description && <p className="max-w-sm text-sm">{description}</p>}
      {action && <div className="mt-3">{action}</div>}
    </div>
  );
}
