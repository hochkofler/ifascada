"use client";

import { useQuery } from "@tanstack/react-query";
import { fetchTagHistory, fetchTagsCurrent } from "@/lib/api";
import { useHmiStore } from "@/store/hmi-store";
import { TrendChart } from "@/components/trend-chart";
import { useOperationalContextStore } from "@/store/context-store";
import { formatProcessValue } from "@/lib/hmi-value";
import { useAutoSelectFirstTag } from "@/lib/use-auto-select-tag";

export default function TrendsPage() {
  const { selectedTag, setSelectedTag } = useHmiStore();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const tags = useQuery({ queryKey: ["trend-tags", filter], queryFn: () => fetchTagsCurrent(200, filter) });
  useAutoSelectFirstTag(tags.data, selectedTag, setSelectedTag);
  const history = useQuery({
    queryKey: ["tag-history", selectedTag],
    queryFn: () => fetchTagHistory(selectedTag, 300),
    enabled: Boolean(selectedTag),
  });

  return (
    <>
      <h2>Tag Trends</h2>
      <div className="card" style={{ marginBottom: 12 }}>
        <h3>Tag Selection (Context Scoped)</h3>
        <select value={selectedTag} onChange={(e) => setSelectedTag(e.target.value)} style={{ width: "100%", marginBottom: 8 }}>
          {(tags.data ?? []).map((t) => (
            <option key={t.tag_code} value={t.tag_code}>
              {t.tag_code} ({t.device_code})
            </option>
          ))}
        </select>
        <div className="nest-tags">
          {(tags.data ?? []).slice(0, 40).map((t) => (
            <button
              key={t.tag_code}
              className={`tag-box ${selectedTag === t.tag_code ? "active" : ""}`}
              onClick={() => setSelectedTag(t.tag_code)}
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
      </div>
      <section className="card">
        <h3>History Chart</h3>
        <TrendChart data={history.data ?? []} />
      </section>
    </>
  );
}
