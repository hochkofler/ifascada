import { Badge } from "@/components/ui/badge";
import type { EdgeCurrent } from "@/lib/api-client";
import { useTranslation } from "react-i18next";

// Verified against crates/central-server/src/persistence/postgres.rs:642-648:
// insert_telemetry's edge_current_state upsert hardcodes status = 'online' as a
// literal SQL string on every telemetry message for a known edge -- this is the
// value edge_current_state.status overwhelmingly holds for an actively-reporting
// edge. insert_health's upsert (postgres.rs:681-700) writes "ok"/"degraded" (the
// edge-agent's own compute_health_status() literal) into the same column via a
// separate, already-documented backend inconsistency -- not something this
// counter should key on. This mirrors the old web-ui's already-shipped, verified
// formula (web-ui/components/context-bar.tsx:8-9): status.toLowerCase() === "online".
const ONLINE_STATUSES = new Set(["online"]);

export function EdgesOnlineBadge({ edges }: { edges: EdgeCurrent[] }) {
  const { t } = useTranslation();
  const online = edges.filter((e) => ONLINE_STATUSES.has(String(e.status || "").toLowerCase())).length;
  return (
    <Badge title={t("live.edgesOnline")}>
      {online}/{edges.length}
    </Badge>
  );
}
