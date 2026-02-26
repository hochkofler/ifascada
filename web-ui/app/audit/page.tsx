"use client";

import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { fetchConnectionsCurrent, fetchOperationalEvents, OperationalEvent } from "@/lib/api";
import { subscribeOpsSse } from "@/lib/sse";
import { useOperationalContextStore } from "@/store/context-store";

const SEVERITY_OPTIONS = ["", "info", "warn", "error"];
const EVENT_TYPE_BASE_OPTIONS = [
  "",
  "connection.connecting",
  "connection.connected",
  "connection.reconnected",
  "connection.disconnected",
  "connection.failed",
  "edge.status.changed",
  "edge.status.recovered",
  "device.status.connected",
  "device.status.stale",
  "device.status.disconnected",
  "device.connection.connected",
  "device.connection.error",
  "tag.quality.bad",
  "tag.quality.recovered",
  "config.apply.accepted",
  "config.apply.rejected",
  "edge.reset.accepted",
  "edge.reset.rejected",
];

export default function AuditPage() {
  const { site, edge } = useOperationalContextStore();
  const [q, setQ] = useState("");
  const [severity, setSeverity] = useState("");
  const [eventType, setEventType] = useState("");
  const [device, setDevice] = useState("");
  const [tag, setTag] = useState("");
  const [rowsLive, setRowsLive] = useState<OperationalEvent[]>([]);
  const filter = useMemo(
    () => ({
      site,
      edge: edge || undefined,
      device: device || undefined,
      severity: severity || undefined,
      event_type: eventType || undefined,
      tag: tag || undefined,
      q: q || undefined,
    }),
    [site, edge, device, severity, eventType, tag, q]
  );
  const rows = useQuery({
    queryKey: ["ops-events-audit", filter],
    queryFn: () => fetchOperationalEvents(200, filter),
  });
  const conns = useQuery({
    queryKey: ["ops-connections-current", site, edge],
    queryFn: () => fetchConnectionsCurrent(200, { site, edge: edge || undefined }),
  });

  useEffect(() => {
    setRowsLive(rows.data ?? []);
  }, [rows.data]);

  useEffect(() => {
    return subscribeOpsSse(
      (evt) => {
        setRowsLive((prev) => {
          if (prev.some((p) => p.id === evt.id)) return prev;
          return [evt, ...prev].slice(0, 300);
        });
      },
      {
        site,
        edge: edge || undefined,
        device: device || undefined,
        tag: tag || undefined,
        severity: severity || undefined,
        event_type: eventType || undefined,
        q: q || undefined,
        replay: false,
      }
    );
  }, [site, edge, device, tag, severity, eventType, q]);

  const eventTypeOptions = useMemo(() => {
    const set = new Set<string>();
    for (const t of EVENT_TYPE_BASE_OPTIONS) {
      if (t) set.add(t);
    }
    for (const r of rowsLive) set.add(r.event_type);
    return ["", ...Array.from(set).sort()];
  }, [rowsLive]);

  return (
    <>
      <h2>Observability</h2>
      <section className="card">
        <h3>Connection Current State</h3>
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 90 }}>Site</th>
                <th style={{ width: 120 }}>Edge</th>
                <th style={{ width: 220 }}>Connection</th>
                <th style={{ width: 120 }}>State</th>
                <th style={{ width: 90 }}>Sev</th>
                <th style={{ width: 180 }}>Last Change</th>
                <th>Message</th>
              </tr>
            </thead>
            <tbody>
              {(conns.data ?? []).map((c) => (
                <tr key={`${c.site_code}:${c.edge_code}:${c.connection_id}`}>
                  <td className="mono">{c.site_code}</td>
                  <td className="mono">{c.edge_code}</td>
                  <td className="mono">{c.connection_id}</td>
                  <td>{c.state}</td>
                  <td>{c.severity}</td>
                  <td className="mono">{new Date(c.last_change_at).toLocaleString()}</td>
                  <td title={c.message}>{c.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="card" style={{ marginTop: 12 }}>
        <h3>Operational Events</h3>
        <p style={{ color: "var(--muted)" }}>
          Eventos de cambios de estado (edge/tag/config/comandos) persistidos en central.
        </p>
        <div style={{ display: "grid", gridTemplateColumns: "2fr 1fr 1fr 1fr 1fr", gap: 8 }}>
          <input
            placeholder="buscar libre (type, message, edge, tag)"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
          <select value={severity} onChange={(e) => setSeverity(e.target.value)}>
            {SEVERITY_OPTIONS.map((opt) => (
              <option key={opt || "all"} value={opt}>
                {opt || "severity: all"}
              </option>
            ))}
          </select>
          <select value={eventType} onChange={(e) => setEventType(e.target.value)}>
            {eventTypeOptions.map((opt) => (
              <option key={opt || "all"} value={opt}>
                {opt || "event_type: all"}
              </option>
            ))}
          </select>
          <input placeholder="device_code (contains)" value={device} onChange={(e) => setDevice(e.target.value)} />
          <input placeholder="tag_code (contains)" value={tag} onChange={(e) => setTag(e.target.value)} />
        </div>
        <div className="table-scroll" style={{ marginTop: 10 }}>
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 180 }}>Ts</th>
                <th style={{ width: 70 }}>Sev</th>
                <th style={{ width: 220 }}>Type</th>
                <th style={{ width: 90 }}>Site</th>
                <th style={{ width: 110 }}>Edge</th>
                <th style={{ width: 120 }}>Device</th>
                <th style={{ width: 110 }}>Tag</th>
                <th>Message</th>
              </tr>
            </thead>
            <tbody>
              {rowsLive.map((r) => (
                <tr key={r.id}>
                  <td className="mono">{new Date(r.ts).toLocaleString()}</td>
                  <td>{r.severity}</td>
                  <td className="mono">{r.event_type}</td>
                  <td className="mono">{r.site_code}</td>
                  <td className="mono">{r.edge_code || "-"}</td>
                  <td className="mono">{r.device_code || "-"}</td>
                  <td className="mono">{r.tag_code || "-"}</td>
                  <td title={JSON.stringify(r.payload_json)}>{r.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}
