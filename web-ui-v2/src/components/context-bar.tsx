import { useQuery } from "@tanstack/react-query";
import { fetchTagsCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { useTranslation } from "react-i18next";

export function ContextBar() {
  const { t } = useTranslation();
  const { site, setSite } = useOperationalContextStore();
  // No dedicated "list of sites" endpoint exists today (verified against api.rs). Deriving
  // the real, currently-reporting site list from tag data instead of a hardcoded/free-text
  // field is what actually fixes the "Site is fixed text" complaint.
  const allTags = useQuery({ queryKey: ["all-sites-probe"], queryFn: () => fetchTagsCurrent(1000) });
  const sites = Array.from(new Set((allTags.data ?? []).map((t) => t.site_code))).sort();

  return (
    <Select value={site} onValueChange={setSite}>
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
  );
}
