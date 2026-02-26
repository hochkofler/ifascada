"use client";

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchTagHistory, fetchTagsCurrent } from "@/lib/api";
import { useOperationalContextStore } from "@/store/context-store";
import { useHmiStore } from "@/store/hmi-store";
import { TrendAccumChart } from "@/components/trend-accum-chart";
import { formatProcessValue, parseCompound } from "@/lib/hmi-value";

function toNumeric(v: unknown): number {
  if (typeof v === "number") return v;
  const c = parseCompound(v);
  if (!c) return 0;
  const n = Number.parseFloat(c.value);
  return Number.isFinite(n) ? n : 0;
}

function bucketTs(iso: string, bucketSec: number): string {
  const d = new Date(iso);
  const t = Math.floor(d.getTime() / 1000);
  const b = Math.floor(t / bucketSec) * bucketSec;
  return new Date(b * 1000).toISOString();
}

export default function TrendsAccumPage() {
  const { selectedTag, setSelectedTag } = useHmiStore();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const [limit, setLimit] = useState(800);
  const [bucketSec, setBucketSec] = useState(10);

  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const tags = useQuery({ queryKey: ["accum-tags", filter], queryFn: () => fetchTagsCurrent(500, filter) });
  const history = useQuery({
    queryKey: ["accum-history", selectedTag, limit],
    queryFn: () => fetchTagHistory(selectedTag, limit),
    enabled: Boolean(selectedTag),
  });

  const points = useMemo(() => {
    const rows = [...(history.data ?? [])].reverse();
    const m = new Map<string, { sum: number; count: number }>();
    for (const r of rows) {
      const k = bucketTs(r.ts, bucketSec);
      const curr = m.get(k) ?? { sum: 0, count: 0 };
      curr.sum += toNumeric(r.value);
      curr.count += 1;
      m.set(k, curr);
    }
    let acc = 0;
    return Array.from(m.entries())
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([ts, x]) => {
        const avg = x.count > 0 ? x.sum / x.count : 0;
        acc += avg;
        return { ts, value: avg, accum: acc };
      });
  }, [history.data, bucketSec]);

  return (
    <>
      <h2>Accumulated Trends</h2>
      <section className="card" style={{ marginBottom: 12 }}>
        <h3>Tag Selection (Context Scoped)</h3>
        <div className="nest-tags">
          {(tags.data ?? []).map((t) => (
            <button
              key={t.tag_code}
              className={`tag-box ${selectedTag === t.tag_code ? "active" : ""}`}
              onClick={() => setSelectedTag(t.tag_code)}
              title={`${t.site_code}/${t.line_code || "-"}/${t.area_code || "-"}/${t.cell_code || "-"} / ${t.device_code}`}
            >
              <div className="tag-box-head">
                <span className={`lamp ${String(t.quality?.status || "").toLowerCase() === "good" ? "on" : "off"}`} />
                <span className="mono">{t.tag_code}</span>
              </div>
              <div className="mono tag-box-value">{formatProcessValue(t.value)}</div>
              <div className="tag-box-meta">
                <span>{t.device_code}</span>
                <span>{new Date(t.ts).toLocaleTimeString()}</span>
              </div>
            </button>
          ))}
        </div>
      </section>

      <section className="card">
        <h3>Accumulated Chart</h3>
        <div className="control-row">
          <label>
            <span>Samples</span>
            <select value={String(limit)} onChange={(e) => setLimit(Number.parseInt(e.target.value, 10))}>
              <option value="300">300</option>
              <option value="800">800</option>
              <option value="1500">1500</option>
            </select>
          </label>
          <label>
            <span>Bucket</span>
            <select value={String(bucketSec)} onChange={(e) => setBucketSec(Number.parseInt(e.target.value, 10))}>
              <option value="1">1s</option>
              <option value="5">5s</option>
              <option value="10">10s</option>
              <option value="30">30s</option>
              <option value="60">60s</option>
            </select>
          </label>
          <div className="mono muted-inline">tag: {selectedTag || "-"}</div>
        </div>
        <TrendAccumChart data={points} />
      </section>
    </>
  );
}
