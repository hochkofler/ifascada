"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  DeviceCurrent,
  EdgeCurrent,
  fetchDevicesCurrent,
  fetchEdgesCurrent,
  fetchTagHistory,
  fetchTagsCurrent,
  postEdgeAction,
  TagCurrent,
} from "@/lib/api";
import { TrendChart } from "@/components/trend-chart";
import { subscribeOpsSse, subscribeSse } from "@/lib/sse";
import { formatProcessValue } from "@/lib/hmi-value";
import { useOperationalContextStore } from "@/store/context-store";

type Point = { ts: string; value: unknown; quality_status: string };
type LatencySummary = {
  sendToSseP50: number | null;
  sendToSseP95: number | null;
  sseToRenderP50: number | null;
  sseToRenderP95: number | null;
  samples: number;
};

type EdgeActionMetric = {
  action: string;
  received: number;
  accepted: number;
  failed: number;
};

type GridEvent = {
  tag_id: string;
  value: unknown;
  quality?: { status?: string; reason?: string };
  timestamp: string;
  published_at: string;
  event_type: string;
};

type SelectedTagEvent = {
  value: unknown;
  quality?: { status?: string; reason?: string };
  timestamp: string;
  published_at: string;
  event_type: string;
  received_at_ms: number;
};

const DEBUG_LIVE = process.env.NEXT_PUBLIC_LIVE_DEBUG === "true";

function nowMs() {
  return Date.now();
}

function ageSecsFromIso(ts: string) {
  const t = new Date(ts).getTime();
  if (Number.isNaN(t)) return Number.POSITIVE_INFINITY;
  return Math.max(0, Math.floor((nowMs() - t) / 1000));
}

function percentile(values: number[], p: number): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.max(0, Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[idx];
}

function upsertPointByTs(prev: Point[], incoming: Point, limit = 50): Point[] {
  const idx = prev.findIndex((p) => p.ts === incoming.ts);
  if (idx >= 0) {
    const sameValue = JSON.stringify(prev[idx].value) === JSON.stringify(incoming.value);
    const sameQuality = prev[idx].quality_status === incoming.quality_status;
    if (sameValue && sameQuality) return prev;
    const next = [...prev];
    next[idx] = incoming;
    return next;
  }
  return [incoming, ...prev].slice(0, limit);
}

function edgeConnected(edge: EdgeCurrent | undefined) {
  if (!edge) return false;
  const status = String(edge.status || "").toLowerCase();
  const staleSecs = Number(process.env.NEXT_PUBLIC_EDGE_STALE_SECS || 45);
  const okState = status === "ok" || status === "online";
  return okState && ageSecsFromIso(edge.last_seen_at) <= staleSecs;
}

function lampFromDeviceState(device: DeviceCurrent | undefined, edgeConn: boolean): string {
  if (!edgeConn) return "bad";
  const state = String(device?.state || "").toLowerCase();
  if (state === "connected") return "good";
  if (state === "stale") return "warn";
  if (state === "disconnected") return "bad";
  return "warn";
}

function edgeActionMetrics(edge: EdgeCurrent | undefined): EdgeActionMetric[] {
  if (!edge?.action_metrics || typeof edge.action_metrics !== "object") return [];
  const out: EdgeActionMetric[] = [];
  for (const [action, raw] of Object.entries(edge.action_metrics)) {
    const rec = Number(raw?.received_total ?? 0);
    const ok = Number(raw?.accepted_total ?? 0);
    const fail = Number(raw?.failed_total ?? 0);
    if (rec <= 0 && ok <= 0 && fail <= 0) continue;
    out.push({ action, received: rec, accepted: ok, failed: fail });
  }
  return out.sort((a, b) => b.received - a.received || b.failed - a.failed);
}

export default function LivePage() {
  const queryClient = useQueryClient();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = useMemo(
    () => ({
      site,
      line: line || undefined,
      area: area || undefined,
      cell: cell || undefined,
      edge: edge || undefined,
    }),
    [site, line, area, cell, edge]
  );

  const tagsQuery = useQuery({
    queryKey: ["live-tags-hierarchy", filter],
    queryFn: () => fetchTagsCurrent(1000, filter),
    refetchInterval: 2500,
  });
  const edgesQuery = useQuery({
    queryKey: ["live-edges-hierarchy", filter],
    queryFn: () => fetchEdgesCurrent(200, filter),
    refetchInterval: 2500,
  });
  const devicesQuery = useQuery({
    queryKey: ["live-devices-hierarchy", filter],
    queryFn: () => fetchDevicesCurrent(1000, filter),
    refetchInterval: 2500,
  });

  const [tags, setTags] = useState<TagCurrent[]>([]);
  const [selectedTag, setSelectedTag] = useState<string>("");
  const [points, setPoints] = useState<Point[]>([]);
  const [lastEvent, setLastEvent] = useState("-");
  const [actionMsg, setActionMsg] = useState<string>("");
  const [latency, setLatency] = useState<LatencySummary>({
    sendToSseP50: null,
    sendToSseP95: null,
    sseToRenderP50: null,
    sseToRenderP95: null,
    samples: 0,
  });

  const pendingGridEvents = useRef<GridEvent[]>([]);
  const pendingSelectedEvents = useRef<SelectedTagEvent[]>([]);
  const lastSelectedSseAtMs = useRef<number>(0);
  const sendToSseMsRef = useRef<number[]>([]);
  const sseToRenderMsRef = useRef<number[]>([]);

  useEffect(() => {
    const data = tagsQuery.data ?? [];
    setTags(data);
    // Keep selected tag stable across refetches; only auto-pick on first load.
    if (!selectedTag && data.length > 0) {
      setSelectedTag(data[0]?.tag_code ?? "");
    }
  }, [tagsQuery.data, selectedTag]);

  const selectedTagObj = useMemo(
    () => tags.find((t) => t.tag_code === selectedTag),
    [tags, selectedTag]
  );
  const selectedEdgeObj = useMemo(
    () => (selectedTagObj ? edgesQuery.data?.find((e) => e.edge_code === selectedTagObj.edge_code) : undefined),
    [selectedTagObj, edgesQuery.data]
  );
  const selectedEdgeActionMetrics = useMemo(
    () => edgeActionMetrics(selectedEdgeObj),
    [selectedEdgeObj]
  );

  const hist = useQuery({
    queryKey: ["live-hier-history", selectedTag],
    queryFn: () => fetchTagHistory(selectedTag, 50),
    enabled: selectedTag.length > 0,
  });

  useEffect(() => {
    if (!hist.data) return;
    setPoints(hist.data.map((h) => ({ ts: h.ts, value: h.value, quality_status: h.quality_status })));
    sendToSseMsRef.current = [];
    sseToRenderMsRef.current = [];
    setLatency({
      sendToSseP50: null,
      sendToSseP95: null,
      sseToRenderP50: null,
      sseToRenderP95: null,
      samples: 0,
    });
  }, [hist.data]);

  // Safety net: if selected tag value is updating in the live grid,
  // keep trend/table in sync even if selected-tag SSE stream drops.
  useEffect(() => {
    if (!selectedTagObj) return;
    // Avoid double-rendering same sample via fallback when selected-tag SSE is healthy.
    if (Date.now() - lastSelectedSseAtMs.current < 2000) return;
    const ts = selectedTagObj.ts;
    if (!ts) return;
    setPoints((prev) => {
      const quality = String(selectedTagObj.quality?.status || "Unknown");
      const incoming: Point = { ts, value: selectedTagObj.value, quality_status: quality };
      const next = upsertPointByTs(prev, incoming, 50);
      if (DEBUG_LIVE) {
        console.debug("[live:fallback] upsert point", {
          tag: selectedTagObj.tag_code,
          ts,
          total: next.length,
        });
      }
      return next;
    });
  }, [selectedTagObj]);

  // Pipeline 1: keep grid tag values updated for the active context.
  useEffect(() => {
    const unsubscribe = subscribeSse(
      (evt) => {
        const payload = evt.payload as
          | { tag_id?: string; value?: unknown; quality?: { status?: string; reason?: string }; timestamp?: string }
          | undefined;
        if (!payload?.tag_id) return;
        pendingGridEvents.current.push({
          tag_id: payload.tag_id,
          value: payload.value ?? null,
          quality: payload.quality,
          timestamp: payload.timestamp ?? new Date().toISOString(),
          published_at: evt.published_at,
          event_type: evt.event_type,
        });
      },
      {
        site,
        line: line || undefined,
        area: area || undefined,
        cell: cell || undefined,
        edge: edge || undefined,
        excludeRaw: true,
        replay: false,
      }
    );

    const timer = setInterval(() => {
      const batch = pendingGridEvents.current.splice(0, pendingGridEvents.current.length);
      if (batch.length === 0) return;

      const latestByTag = new Map<string, { value: unknown; quality?: { status?: string; reason?: string }; timestamp: string }>();
      for (const e of batch) {
        latestByTag.set(e.tag_id, {
          value: e.value,
          quality: e.quality,
          timestamp: e.timestamp,
        });
      }

      const last = batch[batch.length - 1];
      setLastEvent(`${last.event_type} @ ${new Date(last.published_at).toLocaleTimeString()}`);
      setTags((prev) =>
        prev.map((t) => {
          const upd = latestByTag.get(t.tag_code);
          if (!upd) return t;
          return {
            ...t,
            ts: upd.timestamp,
            value: upd.value,
            quality: upd.quality ?? t.quality,
          };
        })
      );
    }, 120);

    return () => {
      clearInterval(timer);
      unsubscribe();
    };
  }, [site, line, area, cell, edge]);

  // Pipeline 2: dedicated stream for selected tag history/trend.
  useEffect(() => {
    if (!selectedTag) return;
    const selectedEdgeForStream = selectedTagObj?.edge_code || edge || undefined;

    const unsubscribe = subscribeSse(
      (evt) => {
        const payload = evt.payload as
          | { tag_id?: string; value?: unknown; quality?: { status?: string; reason?: string }; timestamp?: string }
          | undefined;
        if (!payload?.tag_id || payload.tag_id !== selectedTag) return;

        pendingSelectedEvents.current.push({
          value: payload.value ?? null,
          quality: payload.quality,
          timestamp: payload.timestamp ?? new Date().toISOString(),
          published_at: evt.published_at,
          event_type: evt.event_type,
          received_at_ms: evt.received_at_ms ?? Date.now(),
        });
        lastSelectedSseAtMs.current = Date.now();
        if (DEBUG_LIVE) {
          console.debug("[live:selected-sse] recv", {
            tag: selectedTag,
            payload_ts: payload.timestamp,
            published_at: evt.published_at,
          });
        }
      },
      {
        site,
        line: line || undefined,
        area: area || undefined,
        cell: cell || undefined,
        edge: selectedEdgeForStream,
        tag: selectedTag,
        excludeRaw: true,
        replay: false,
      }
    );

    const timer = setInterval(() => {
      const batch = pendingSelectedEvents.current.splice(0, pendingSelectedEvents.current.length);
      if (batch.length === 0) return;

      const latest = batch[batch.length - 1];
      setLastEvent(`${latest.event_type} @ ${new Date(latest.published_at).toLocaleTimeString()}`);

      for (const e of batch) {
        const srcTs = new Date(e.timestamp).getTime();
        if (Number.isNaN(srcTs)) continue;
        const sendToSse = e.received_at_ms - srcTs;
        if (sendToSse >= 0 && sendToSse < 60000) sendToSseMsRef.current.push(sendToSse);
      }
      if (sendToSseMsRef.current.length > 300) {
        sendToSseMsRef.current = sendToSseMsRef.current.slice(-300);
      }

      setPoints((prev) => {
        let next = [...prev];
        for (const e of batch) {
          next = upsertPointByTs(
            next,
            {
            ts: e.timestamp,
            value: e.value,
            quality_status: e.quality?.status ?? "Unknown",
            },
            50
          );
        }
        if (DEBUG_LIVE) {
          console.debug("[live:selected-sse] batch applied", {
            tag: selectedTag,
            batch: batch.length,
            topTs: next[0]?.ts,
            total: next.length,
          });
        }
        return next;
      });

      const recvMs = latest.received_at_ms;
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          const renderMs = Date.now() - recvMs;
          if (renderMs >= 0 && renderMs < 60000) sseToRenderMsRef.current.push(renderMs);
          if (sseToRenderMsRef.current.length > 300) {
            sseToRenderMsRef.current = sseToRenderMsRef.current.slice(-300);
          }
          setLatency({
            sendToSseP50: percentile(sendToSseMsRef.current, 50),
            sendToSseP95: percentile(sendToSseMsRef.current, 95),
            sseToRenderP50: percentile(sseToRenderMsRef.current, 50),
            sseToRenderP95: percentile(sseToRenderMsRef.current, 95),
            samples: Math.min(sendToSseMsRef.current.length, sseToRenderMsRef.current.length),
          });
        });
      });
    }, 80);

    return () => {
      clearInterval(timer);
      unsubscribe();
    };
  }, [site, line, area, cell, edge, selectedTag, selectedTagObj?.edge_code]);

  useEffect(() => {
    return subscribeOpsSse(
      (evt) => {
        const et = String(evt.event_type || "");
        if (!et.startsWith("device.status.") && !et.startsWith("device.connection.")) return;
        setLastEvent(`${evt.event_type} @ ${new Date(evt.ts).toLocaleTimeString()}`);
        queryClient.invalidateQueries({ queryKey: ["live-devices-hierarchy", filter] });
      },
      {
        site,
        edge: edge || undefined,
        replay: false,
      }
    );
  }, [site, edge, filter, queryClient]);

  const grouped = useMemo(() => {
    type DeviceGroup = {
      key: string;
      deviceCode: string;
      site: string;
      line: string;
      area: string;
      cell: string;
      edge: string;
      tags: TagCurrent[];
    };

    const m = new Map<string, DeviceGroup>();

    for (const d of devicesQuery.data ?? []) {
      const siteCode = d.site_code || site || "-";
      const lineCode = d.line_code || "-";
      const areaCode = d.area_code || "-";
      const cellCode = d.cell_code || "-";
      const edgeCode = d.edge_code || "-";
      const key = `${siteCode}|${lineCode}|${areaCode}|${cellCode}|${d.device_code}`;
      if (!m.has(key)) {
        m.set(key, {
          key,
          deviceCode: d.device_code,
          site: siteCode,
          line: lineCode,
          area: areaCode,
          cell: cellCode,
          edge: edgeCode,
          tags: [],
        });
      }
    }

    for (const t of tags) {
      const siteCode = t.site_code || site || "-";
      const lineCode = t.line_code || "-";
      const areaCode = t.area_code || "-";
      const cellCode = t.cell_code || "-";
      const edgeCode = t.edge_code || "-";
      const key = `${siteCode}|${lineCode}|${areaCode}|${cellCode}|${t.device_code}`;
      if (!m.has(key)) {
        m.set(key, {
          key,
          deviceCode: t.device_code,
          site: siteCode,
          line: lineCode,
          area: areaCode,
          cell: cellCode,
          edge: edgeCode,
          tags: [],
        });
      }
      m.get(key)!.tags.push(t);
    }

    return Array.from(m.values()).sort((a, b) =>
      `${a.site}|${a.line}|${a.area}|${a.cell}|${a.deviceCode}`.localeCompare(
        `${b.site}|${b.line}|${b.area}|${b.cell}|${b.deviceCode}`
      )
    );
  }, [tags, site, devicesQuery.data]);

  const edgesByCode = useMemo(() => {
    const m = new Map<string, EdgeCurrent>();
    for (const e of edgesQuery.data ?? []) m.set(e.edge_code, e);
    return m;
  }, [edgesQuery.data]);

  const devicesByKey = useMemo(() => {
    const m = new Map<string, DeviceCurrent>();
    for (const d of devicesQuery.data ?? []) {
      const key = `${d.site_code}|${d.line_code || "-"}|${d.area_code || "-"}|${d.cell_code || "-"}|${d.device_code}`;
      m.set(key, d);
    }
    return m;
  }, [devicesQuery.data]);

  async function sendEscPosPrint() {
    if (!selectedTagObj) {
      setActionMsg("Select a tag first");
      return;
    }
    try {
      await postEdgeAction(
        selectedTagObj.site_code,
        selectedTagObj.edge_code,
        "print.escpos",
        {
          lines: [
            "IFA SCADA",
            `TAG: ${selectedTagObj.tag_code}`,
            `VALUE: ${formatProcessValue(selectedTagObj.value)}`,
            `TS: ${new Date(selectedTagObj.ts).toLocaleString()}`,
          ],
        },
        {
          request_id: `ui-print-${Date.now()}`,
          source: "web-ui",
          target: "edge",
        }
      );
      setActionMsg("Print action sent");
    } catch (e) {
      setActionMsg(`Print action failed: ${String(e)}`);
    }
  }

  return (
    <>
      <h2>Live Operations</h2>
      <p style={{ color: "var(--muted)" }}>Last runtime event: {lastEvent}</p>
      <p style={{ color: "var(--muted)" }}>
        Latency p50/p95 (ms): send-&gt;sse {latency.sendToSseP50 ?? "-"} / {latency.sendToSseP95 ?? "-"} | sse-&gt;render {latency.sseToRenderP50 ?? "-"} / {latency.sseToRenderP95 ?? "-"} (n={latency.samples})
      </p>

      <div className="live-nested">
        {grouped.map((g) => {
          const e = edgesByCode.get(g.edge);
          const edgeConn = edgeConnected(e);
          const d = devicesByKey.get(g.key);
          const lamp = lampFromDeviceState(d, edgeConn);
          return (
            <section key={g.key} className="nest-cell">
              <div className="nest-head">
                <span className={`lamp ${lamp}`} />
                <strong className="mono">
                  {g.site} / LINE {g.line} / AREA {g.area} / CELL {g.cell}
                </strong>
              </div>
              <div className="nest-device">
                <div className="nest-head">
                  <span className={`lamp ${lamp}`} />
                  <strong className="mono">DEVICE {g.deviceCode}</strong>
                  <span className="mono muted-inline">tags: {g.tags.length}</span>
                  <span className="mono muted-inline">edge: {g.edge}</span>
                  <span className="mono muted-inline">state: {String(d?.state || "unknown")}</span>
                </div>
                <div className="nest-tags">
                  {g.tags.map((t) => (
                    <button
                      key={t.tag_code}
                      className={`tag-box ${selectedTag === t.tag_code ? "active" : ""}`}
                      onClick={() => setSelectedTag(t.tag_code)}
                    >
                      <div className="tag-box-head">
                        <span className={`lamp ${lamp}`} title={`device_state: ${String(d?.state || "unknown")}`} />
                        <span className="mono">{t.tag_code}</span>
                      </div>
                      <div className="mono tag-box-value" title={formatProcessValue(t.value)}>
                        {formatProcessValue(t.value)}
                      </div>
                      <div className="tag-box-meta">
                        <span>q:{String(t.quality?.status || "Unknown")}</span>
                        <span>{new Date(t.ts).toLocaleTimeString()}</span>
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            </section>
          );
        })}
      </div>

      <section className="card" style={{ marginTop: 12 }}>
        <h3>Selected Tag Trend</h3>
        <p className="mono" style={{ marginTop: 0 }}>
          {selectedTag || "-"}
        </p>
        <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
          <button className="btn" onClick={sendEscPosPrint} disabled={!selectedTagObj}>
            Print ESC/POS
          </button>
          <span className="mono" style={{ color: "var(--muted)" }}>
            {actionMsg}
          </span>
        </div>
        <TrendChart
          key={`${selectedTag}-${points[0]?.ts ?? "none"}`}
          data={points.map((p) => ({
            ts: p.ts,
            site_code: site,
            edge_code: edge || "",
            tag_code: selectedTag || "",
            value: p.value,
            quality_status: p.quality_status,
          }))}
        />
      </section>

      <section className="card" style={{ marginTop: 12 }}>
        <h3>Edge Action Metrics</h3>
        <p className="mono" style={{ marginTop: 0 }}>
          edge: {selectedTagObj?.edge_code || "-"}
        </p>
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 280 }}>Action</th>
                <th style={{ width: 110 }}>Received</th>
                <th style={{ width: 110 }}>Accepted</th>
                <th style={{ width: 110 }}>Failed</th>
              </tr>
            </thead>
            <tbody>
              {selectedEdgeActionMetrics.length === 0 ? (
                <tr>
                  <td colSpan={4} style={{ color: "var(--muted)" }}>
                    No action metrics yet
                  </td>
                </tr>
              ) : (
                selectedEdgeActionMetrics.map((m) => (
                  <tr key={m.action}>
                    <td className="mono">{m.action}</td>
                    <td className="mono">{m.received}</td>
                    <td className="mono">{m.accepted}</td>
                    <td className="mono">{m.failed}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="card" style={{ marginTop: 12 }}>
        <h3>Last Values (Selected Tag)</h3>
        <p className="mono" style={{ marginTop: 0 }}>{selectedTag || "-"}</p>
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 180 }}>Ts</th>
                <th style={{ width: 280 }}>Value</th>
                <th style={{ width: 120 }}>Quality</th>
              </tr>
            </thead>
            <tbody>
              {points.slice(0, 50).map((p, idx) => (
                <tr key={`${p.ts}-${idx}`}>
                  <td className="mono">{new Date(p.ts).toLocaleTimeString()}</td>
                  <td className="mono" title={formatProcessValue(p.value)}>
                    {formatProcessValue(p.value)}
                  </td>
                  <td>{p.quality_status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}
