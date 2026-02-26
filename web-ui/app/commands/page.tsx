"use client";

import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { fetchEdgesCurrent, postEdgeReset } from "@/lib/api";
import { useOperationalContextStore } from "@/store/context-store";

export default function CommandsPage() {
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const [operator, setOperator] = useState("operator.local");
  const [reason, setReason] = useState("manual reset from HMI");
  const [lastMsg, setLastMsg] = useState<string>("-");
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

  const edges = useQuery({
    queryKey: ["cmd-edges-reset", filter],
    queryFn: () => fetchEdgesCurrent(200, filter),
  });

  const resetMutation = useMutation({
    mutationFn: async (args: { siteCode: string; edgeCode: string }) =>
      postEdgeReset(args.siteCode, args.edgeCode, {
        operator,
        reason,
        request_id: `rst-${Date.now()}`,
      }),
    onSuccess: (res, vars) => {
      setLastMsg(
        `Reset enviado a ${vars.edgeCode} (${res.topic}) ${new Date().toLocaleTimeString()}`
      );
    },
    onError: (err, vars) => {
      setLastMsg(
        `Error enviando reset a ${vars.edgeCode}: ${err instanceof Error ? err.message : "unknown"}`
      );
    },
  });

  return (
    <>
      <h2>Commands</h2>
      <section className="card">
        <h3>Edge Reset</h3>
        <p style={{ color: "var(--muted)" }}>
          Envía comando MQTT <span className="mono">control/reset</span> vía central API.
        </p>
        <div style={{ display: "grid", gap: 8, gridTemplateColumns: "1fr 2fr" }}>
          <label className="mono">Operator</label>
          <input value={operator} onChange={(e) => setOperator(e.target.value)} />
          <label className="mono">Reason</label>
          <input value={reason} onChange={(e) => setReason(e.target.value)} />
        </div>
        <p className="mono" style={{ marginTop: 10, color: "var(--muted)" }}>
          {lastMsg}
        </p>
      </section>

      <section className="card" style={{ marginTop: 12 }}>
        <h3>Edges in Scope</h3>
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th>Site</th>
                <th>Edge</th>
                <th>Status</th>
                <th>Last Seen</th>
                <th style={{ width: 160 }}>Action</th>
              </tr>
            </thead>
            <tbody>
              {(edges.data ?? []).map((e) => (
                <tr key={`${e.site_code}:${e.edge_code}`}>
                  <td className="mono">{e.site_code}</td>
                  <td className="mono">{e.edge_code}</td>
                  <td>{e.status}</td>
                  <td>{new Date(e.last_seen_at).toLocaleString()}</td>
                  <td>
                    <button
                      onClick={() => resetMutation.mutate({ siteCode: e.site_code, edgeCode: e.edge_code })}
                      disabled={resetMutation.isPending}
                    >
                      Reset Edge
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}
