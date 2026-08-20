import { useState, type JSX } from "react";
import { Settings2, SlidersHorizontal } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useDataTableContext } from "./DataTableContent";
import { DataTableColumnsDialog } from "./DataTableColumnsDialog";
import { TABLES_NS } from "./i18n";
import type { TableDensity } from "./types";

interface DataTableViewOptionsProps {
  density: TableDensity;
  onDensityChange: (density: TableDensity) => void;
}

/**
 * Menú "Vista" de la grilla: densidad de filas + acceso a "Personalizar
 * columnas…" (diálogo con visibilidad y reorden, estilo Fiori). La visibilidad
 * y el orden los maneja TanStack Table.
 */
export function DataTableViewOptions({
  density,
  onDensityChange,
}: DataTableViewOptionsProps): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  const { table } = useDataTableContext();
  const hasHideableColumns = table.getAllLeafColumns().some((column) => column.getCanHide());
  const [columnsOpen, setColumnsOpen] = useState(false);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded-md border border-input bg-background px-3 py-1.5 text-sm transition-colors hover:bg-accent"
            aria-label={t("viewOptions.triggerAria")}
          >
            <SlidersHorizontal className="size-4" />
            {t("viewOptions.trigger")}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-52">
          <DropdownMenuLabel>{t("viewOptions.density")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={density}
            onValueChange={(value) => {
              onDensityChange(value as TableDensity);
            }}
          >
            <DropdownMenuRadioItem value="comfortable">
              {t("viewOptions.comfortable")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="compact">
              {t("viewOptions.compact")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>

          {hasHideableColumns && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onSelect={() => {
                  setColumnsOpen(true);
                }}
              >
                <Settings2 className="size-4" />
                {t("viewOptions.customizeColumns")}
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <DataTableColumnsDialog open={columnsOpen} onOpenChange={setColumnsOpen} />
    </>
  );
}
