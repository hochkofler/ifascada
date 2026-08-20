import { useState, type JSX } from "react";
import { Bookmark, Building2, Plus, Star, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useGridViews } from "./hooks/useGridViews";
import { TABLES_NS } from "./i18n";

const run = (p: Promise<unknown>): void => void p.catch(() => undefined);

/**
 * Menú "Vistas" de la toolbar. Lista la vista por defecto de la organización + las vistas
 * personales del usuario (aplicar, fijar como mi predeterminada ★, eliminar) y permite guardar
 * la vista actual (filtros/orden/búsqueda + orden/visibilidad/densidad de columnas). Los
 * superadmin además pueden fijar la vista actual como estándar de la organización. Server-side
 * por `tableId`. Se habilita pasando `tableId` al DataTable.
 */
export function DataTableSavedViews({ tableId }: { tableId: string }): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  const views = useGridViews(tableId);
  const [name, setName] = useState("");
  const org = views.orgDefault;

  const handleSave = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    run(views.saveCurrent(trimmed));
    setName("");
  };

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-input bg-background px-3 py-1.5 text-sm transition-colors hover:bg-accent"
          aria-label={t("savedViews.triggerAria")}
        >
          <Bookmark className="size-4" />
          {t("savedViews.trigger")}
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 p-2">
        {org && (
          <div className="mb-2 border-b pb-2">
            <p className="px-1 pb-1 text-xs font-medium text-muted-foreground">
              {t("savedViews.organization")}
            </p>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => {
                  views.applyView(org);
                }}
                className="flex flex-1 items-center gap-1.5 truncate rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
              >
                <Building2 className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="truncate">{org.name || t("savedViews.orgViewFallback")}</span>
              </button>
              {views.canManageOrg && (
                <button
                  type="button"
                  onClick={() => {
                    run(views.removeOrgDefault());
                  }}
                  aria-label={t("savedViews.removeOrgAria")}
                  className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-destructive"
                >
                  <Trash2 className="size-3.5" />
                </button>
              )}
            </div>
          </div>
        )}

        <p className="px-1 pb-1 text-xs font-medium text-muted-foreground">
          {t("savedViews.mine")}
        </p>
        {views.mine.length === 0 ? (
          <p className="px-1 py-2 text-sm text-muted-foreground">{t("savedViews.empty")}</p>
        ) : (
          <ul className="max-h-56 space-y-0.5 overflow-y-auto">
            {views.mine.map((view) => {
              const isDefault = view.id === views.myDefaultId;
              return (
                <li key={view.id} className="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={() => {
                      views.applyView(view);
                    }}
                    className="flex-1 truncate rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
                  >
                    {view.name}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      run(views.setMyDefault(view.id, !isDefault));
                    }}
                    aria-label={
                      isDefault
                        ? t("savedViews.unsetDefaultAria", { name: view.name })
                        : t("savedViews.setDefaultAria", { name: view.name })
                    }
                    className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent"
                  >
                    <Star className={`size-3.5 ${isDefault ? "fill-primary text-primary" : ""}`} />
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      run(views.deleteView(view.id));
                    }}
                    aria-label={t("savedViews.deleteAria", { name: view.name })}
                    className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-destructive"
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </li>
              );
            })}
          </ul>
        )}

        <div className="mt-2 flex items-center gap-1 border-t pt-2">
          <Input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSave();
            }}
            placeholder={t("savedViews.namePlaceholder")}
            className="h-8 text-sm"
          />
          <Button
            size="sm"
            className="h-8 shrink-0"
            disabled={!name.trim()}
            onClick={handleSave}
            aria-label={t("savedViews.saveAria")}
          >
            <Plus className="size-4" />
          </Button>
        </div>

        {views.canManageOrg && (
          <button
            type="button"
            onClick={() => {
              run(views.saveOrgDefaultFromCurrent());
            }}
            className="mt-2 flex w-full items-center justify-center gap-1.5 rounded border border-dashed border-input px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent"
          >
            <Building2 className="size-3.5" />
            {t("savedViews.setOrgDefault")}
          </button>
        )}
      </PopoverContent>
    </Popover>
  );
}
