import type { JSX } from "react";
import { useTranslation } from "react-i18next";
import { StatusBadge, type StatusTone } from "@/components/status-badge";
import type { EdgeCurrent } from "@/lib/api-client";
import { ONLINE_STATUSES } from "@/lib/connectivity";

/**
 * Deriva el tono del conteo. Cero edges NO es una falla: es "todavia no se sabe" (la lista aun
 * no cargo, o el filtro no matchea nada), y pintarlo de rojo seria una alarma falsa.
 */
function toneFor(online: number, total: number): StatusTone {
  if (total === 0) return "neutral";
  if (online === total) return "ok";
  if (online === 0) return "bad";
  return "warn";
}

export function EdgesOnlineBadge({ edges }: { edges: EdgeCurrent[] }): JSX.Element {
  const { t } = useTranslation();
  const online = edges.filter((e) =>
    ONLINE_STATUSES.has(String(e.status || "").toLowerCase())
  ).length;
  return (
    <StatusBadge tone={toneFor(online, edges.length)} title={t("live.edgesOnline")}>
      {online}/{edges.length}
    </StatusBadge>
  );
}
