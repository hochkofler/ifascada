"use client";

import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchAreas, fetchCells, fetchEdgesCurrent, fetchLines } from "@/lib/api";
import { useOperationalContextStore } from "@/store/context-store";

function isOnline(status: string) {
  return String(status || "").toLowerCase() === "online";
}

export function ContextBar() {
  const { site, line, area, cell, edge, setSite, setLine, setArea, setCell, setEdge } =
    useOperationalContextStore();

  const lines = useQuery({ queryKey: ["ctxb-lines", site], queryFn: () => fetchLines(site) });
  const areas = useQuery({
    queryKey: ["ctxb-areas", site, line],
    queryFn: () => fetchAreas(site, line || undefined),
  });
  const cells = useQuery({
    queryKey: ["ctxb-cells", site, line, area],
    queryFn: () => fetchCells(site, line || undefined, area || undefined),
  });

  const edges = useQuery({
    queryKey: ["ctxb-edges", site, line, area, cell],
    queryFn: () => fetchEdgesCurrent(200, { site, line: line || undefined, area: area || undefined, cell: cell || undefined }),
  });

  const edgeOptions = useMemo(
    () => [...new Set((edges.data ?? []).map((e) => e.edge_code))].sort(),
    [edges.data]
  );
  const edgeRows = edges.data ?? [];
  const onlineEdges = edgeRows.filter((e) => isOnline(e.status)).length;
  const anyOnline = onlineEdges > 0;
  const hasSelection = Boolean(line || area || cell || edge);

  return (
    <section className="context-bar">
      <div className="context-breadcrumb">
        <span className={`lamp ${anyOnline ? "on" : "off"}`} />
        <span className="mono">{site || "*"}</span>
        <span className="crumb-sep">/</span>
        <span className={`lamp ${line ? "on" : "off"}`} />
        <span className="mono">{line || "*"}</span>
        <span className="crumb-sep">/</span>
        <span className={`lamp ${area ? "on" : "off"}`} />
        <span className="mono">{area || "*"}</span>
        <span className="crumb-sep">/</span>
        <span className={`lamp ${cell ? "on" : "off"}`} />
        <span className="mono">{cell || "*"}</span>
        <span className="crumb-sep">/</span>
        <span className={`lamp ${edge ? "on" : "off"}`} />
        <span className="mono">{edge || "*"}</span>
        <span className="context-summary">
          edges online {onlineEdges}/{edgeRows.length}
        </span>
        {hasSelection && (
          <button className="ctx-reset" onClick={() => { setLine(""); setArea(""); setCell(""); setEdge(""); }}>
            clear
          </button>
        )}
      </div>
      <div className="context-grid">
        <label>
          <span>Site</span>
          <input value={site} onChange={(e) => setSite(e.target.value)} />
        </label>
        <label>
          <span>Line</span>
          <select value={line} onChange={(e) => setLine(e.target.value)}>
            <option value="">All</option>
            {(lines.data ?? []).map((x) => (
              <option key={x.code} value={x.code}>{x.code}</option>
            ))}
          </select>
        </label>
        <label>
          <span>Area</span>
          <select value={area} onChange={(e) => setArea(e.target.value)}>
            <option value="">All</option>
            {(areas.data ?? []).map((x) => (
              <option key={x.code} value={x.code}>{x.code}</option>
            ))}
          </select>
        </label>
        <label>
          <span>Cell</span>
          <select value={cell} onChange={(e) => setCell(e.target.value)}>
            <option value="">All</option>
            {(cells.data ?? []).map((x) => (
              <option key={x.code} value={x.code}>{x.code}</option>
            ))}
          </select>
        </label>
        <label>
          <span>Edge</span>
          <select value={edge} onChange={(e) => setEdge(e.target.value)}>
            <option value="">All</option>
            {edgeOptions.map((x) => (
              <option key={x} value={x}>{x}</option>
            ))}
          </select>
        </label>
      </div>
    </section>
  );
}
