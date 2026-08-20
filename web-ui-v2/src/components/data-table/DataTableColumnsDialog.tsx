import type { JSX } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useDataTableContext } from "./DataTableContent";
import { moveColumn } from "./utils/moveColumn";
import { TABLES_NS } from "./i18n";

interface DataTableColumnsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Diálogo "Personalizar columnas" (estilo Fiori): mostrar/ocultar y reordenar
 * columnas con flechas ↑/↓ (accesible, sin dependencias de drag). La visibilidad
 * y el orden los maneja TanStack Table; las columnas internas (selección,
 * acciones) quedan fijas (enableHiding=false → fuera de la lista y no se cruzan).
 */
export function DataTableColumnsDialog({
  open,
  onOpenChange,
}: DataTableColumnsDialogProps): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  const { table } = useDataTableContext();
  const allLeaf = table.getAllLeafColumns();
  const columns = allLeaf.filter((column) => column.getCanHide());
  const fixedIds = new Set(allLeaf.flatMap((column) => (column.getCanHide() ? [] : [column.id])));

  function move(id: string, direction: "up" | "down") {
    const state = table.getState().columnOrder;
    const order = state.length ? state : allLeaf.map((c) => c.id);
    table.setColumnOrder(moveColumn(order, id, direction, fixedIds));
  }

  function reset() {
    table.resetColumnOrder();
    table.setColumnVisibility({});
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("columnsDialog.title")}</DialogTitle>
          <DialogDescription>{t("columnsDialog.description")}</DialogDescription>
        </DialogHeader>

        <ul className="max-h-80 space-y-0.5 overflow-y-auto py-1">
          {columns.map((column, index) => {
            const label = column.columnDef.meta?.label ?? column.id;
            return (
              <li
                key={column.id}
                className="flex items-center gap-2 rounded-md px-1 py-1 hover:bg-accent"
              >
                <Checkbox
                  checked={column.getIsVisible()}
                  onCheckedChange={(v) => {
                    column.toggleVisibility(v === true);
                  }}
                  aria-label={t("columnsDialog.showColumnAria", { label })}
                />
                <span className="flex-1 truncate text-sm">{label}</span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  disabled={index === 0}
                  onClick={() => {
                    move(column.id, "up");
                  }}
                  aria-label={t("columnsDialog.moveUpAria", { label })}
                >
                  <ChevronUp className="size-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  disabled={index === columns.length - 1}
                  onClick={() => {
                    move(column.id, "down");
                  }}
                  aria-label={t("columnsDialog.moveDownAria", { label })}
                >
                  <ChevronDown className="size-4" />
                </Button>
              </li>
            );
          })}
        </ul>

        <DialogFooter>
          <Button variant="outline" onClick={reset}>
            {t("columnsDialog.reset")}
          </Button>
          <Button
            onClick={() => {
              onOpenChange(false);
            }}
          >
            {t("columnsDialog.done")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
