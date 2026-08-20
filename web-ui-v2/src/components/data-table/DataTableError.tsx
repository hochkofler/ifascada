import type { JSX } from "react";
import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { TABLES_NS } from "./i18n";

export function DataTableError(): JSX.Element {
  const { t } = useTranslation(TABLES_NS);
  return (
    <div className="flex flex-col items-center justify-center py-16 text-destructive">
      <AlertCircle className="mb-3 size-10 opacity-60" />
      <p className="text-sm font-medium">{t("error.title")}</p>
      <p className="mt-1 text-xs text-muted-foreground">{t("error.description")}</p>
    </div>
  );
}
