import type React from "react";
import type { JSX } from "react";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useDataTableContext } from "./DataTableContent";
import { TABLES_NS } from "./i18n";

interface DataTableToolbarProps {
  children: React.ReactNode;
}

export function DataTableToolbar({ children }: DataTableToolbarProps): JSX.Element {
  return <div className="flex flex-wrap items-center gap-2 py-3">{children}</div>;
}

export function DataTableFilterChip(): JSX.Element | null {
  const { t } = useTranslation(TABLES_NS);
  const { table } = useDataTableContext();
  const count = table.getState().columnFilters.length;
  if (count === 0) return null;

  return (
    <button
      type="button"
      onClick={() => {
        table.resetColumnFilters();
      }}
      className="flex items-center gap-1 rounded-full border border-primary/25 bg-primary/15 px-2 py-1 text-[11px] text-primary transition-colors hover:bg-primary/20"
    >
      <X className="size-2.5" />
      {t("filters.activeChip", { count })}
    </button>
  );
}
