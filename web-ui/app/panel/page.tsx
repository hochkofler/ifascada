"use client";

import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchTagHistory, fetchTagsCurrent } from "@/lib/api";
import { subscribeSse } from "@/lib/sse";
import { TrendChart } from "@/components/trend-chart";
import { PvTile } from "@/components/pv-tile";

type Point = { ts: string; value: unknown; quality_status: string };

export default function PanelPage() {
  const tags = useQuery({
    queryKey: ["panel-tags-current"],
    queryFn: () => fetchTagsCurrent(200, { site: "plant-a", edge: "edge-01" }),
  });
  const [rows, setRows] = useState(tags.data ?? []);
  const [selected, setSelected] = useState<string>("");
  const [points, setPoints] = useState<Point[]>([]);

  useEffect(() => {
    if (!tags.data) return;
    setRows(tags.data);
    if (!selected && tags.data.length > 0) setSelected(tags.data[0].tag_code);
  }, [tags.data, selected]);

  const history = useQuery({
    queryKey: ["panel-tag-history", selected],
    queryFn: () => fetchTagHistory(selected, 50),
    enabled: selected.length > 0,
  });

  useEffect(() => {
    if (!history.data) return;
    setPoints(history.data.map((h) => ({ ts: h.ts, value: h.value, quality_status: h.quality_status })));
  }, [history.data]);

  useEffect(() => {
    return subscribeSse((evt) => {
      const payload = evt.payload as
        | { tag_id?: string; value?: unknown; quality?: { status?: string }; timestamp?: string }
        | undefined;
      if (!payload?.tag_id) return;
      setRows((prev) =>
        prev.map((t) =>
          t.tag_code === payload.tag_id
            ? {
                ...t,
                ts: payload.timestamp ?? t.ts,
                value: payload.value ?? t.value,
                quality: payload.quality ?? t.quality,
              }
            : t
        )
      );
      if (payload.tag_id === selected) {
        setPoints((prev) => [
          {
            ts: payload.timestamp ?? new Date().toISOString(),
            value: payload.value ?? null,
            quality_status: payload.quality?.status ?? "Unknown",
          },
          ...prev,
        ].slice(0, 50));
      }
    }, { site: "plant-a", edge: "edge-01", excludeRaw: true, replay: false });
  }, [selected]);

  const sel = useMemo(() => rows.find((r) => r.tag_code === selected), [rows, selected]);

  return (
    <>
      <h2>Process Panel</h2>
      <div className="panel-layout">
        <section className="card">
          <h3>Variables</h3>
          <div className="pv-grid">
            {rows.map((t) => (
              <PvTile key={t.tag_code} tag={t} selected={selected === t.tag_code} onSelect={() => setSelected(t.tag_code)} />
            ))}
          </div>
        </section>
        <section className="card">
          <h3>Selected Trend</h3>
          <p className="mono" style={{ marginTop: 0 }}>{sel?.tag_code || "-"}</p>
          <TrendChart
            data={points.map((p) => ({
              ts: p.ts,
              site_code: sel?.site_code || "",
              edge_code: sel?.edge_code || "",
              tag_code: sel?.tag_code || "",
              value: p.value,
              quality_status: p.quality_status,
            }))}
          />
        </section>
      </div>
    </>
  );
}
