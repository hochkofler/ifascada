"use client";

import { useQuery } from "@tanstack/react-query";
import { fetchEdgesCurrent, fetchTagsCurrent } from "@/lib/api";
import { useOperationalContextStore } from "@/store/context-store";

export default function OverviewPage() {
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const edges = useQuery({ queryKey: ["edges-current-ov", filter], queryFn: () => fetchEdgesCurrent(20, filter) });
  const tags = useQuery({ queryKey: ["tags-current-ov", filter], queryFn: () => fetchTagsCurrent(50, filter) });

  const edgeCount = edges.data?.length ?? 0;
  const tagCount = tags.data?.length ?? 0;
  const badCount =
    tags.data?.filter((t) => String(t.quality?.status || "").toLowerCase() !== "good").length ?? 0;

  return (
    <>
      <h2>Overview</h2>
      <div className="grid">
        <section className="card" style={{ gridColumn: "span 4" }}>
          <h3>Edges Online</h3>
          <div className="kpi">{edgeCount}</div>
        </section>
        <section className="card" style={{ gridColumn: "span 4" }}>
          <h3>Tags Current</h3>
          <div className="kpi">{tagCount}</div>
        </section>
        <section className="card" style={{ gridColumn: "span 4" }}>
          <h3>Bad Quality</h3>
          <div className="kpi">{badCount}</div>
        </section>
      </div>
      <section className="card" style={{ marginTop: 12 }}>
        <h3>Operational Scope</h3>
        <p style={{ color: "var(--muted)" }}>
          Focus daily operation on <span className="mono">Live</span> (device and tag status by area). Use{" "}
          <span className="mono">Trends</span> for history and analysis. Edge is diagnostic and optional as a filter.
        </p>
      </section>
    </>
  );
}
