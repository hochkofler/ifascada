import { Badge } from "@/components/ui/badge";
import type { EdgeCurrent } from "@/lib/api-client";
import { ONLINE_STATUSES } from "@/lib/connectivity";
import { useTranslation } from "react-i18next";

export function EdgesOnlineBadge({ edges }: { edges: EdgeCurrent[] }) {
  const { t } = useTranslation();
  const online = edges.filter((e) => ONLINE_STATUSES.has(String(e.status || "").toLowerCase())).length;
  return (
    <Badge title={t("live.edgesOnline")}>
      {online}/{edges.length}
    </Badge>
  );
}
