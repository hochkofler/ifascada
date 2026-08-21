import { useQuery } from "@tanstack/react-query";
import { fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { useAutoSelectFirst } from "@/lib/use-auto-select-first";
import { useTranslation } from "react-i18next";

export function ContextBar() {
  const { t } = useTranslation();
  const { site, setSite } = useOperationalContextStore();
  // No dedicated "list of sites" endpoint exists today (verified against api.rs). Deriving
  // the real, currently-reporting site list from tag data instead of a hardcoded/free-text
  // field is what actually fixes the "Site is fixed text" complaint.
  const allTags = useQuery({ queryKey: ["all-sites-probe"], queryFn: () => fetchTagsCurrent(1000) });
  const sites = Array.from(new Set((allTags.data ?? []).map((t) => t.site_code))).sort();

  // The store defaults site to "plant-a" with no way to know in advance whether that's a
  // real, currently-reporting site. Once the real list loads, correct a stored value that
  // doesn't match any of them instead of leaving a <Select> whose value matches no option.
  useAutoSelectFirst(sites, site, setSite);

  return (
    <div className="flex items-center gap-2">
      <Select value={site} onValueChange={setSite} disabled={allTags.isError}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.site")} />
        </SelectTrigger>
        <SelectContent>
          {sites.map((s) => (
            <SelectItem key={s} value={s}>
              {s}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {allTags.isError && <span className="text-xs text-destructive">{t("live.siteError")}</span>}
    </div>
  );
}
