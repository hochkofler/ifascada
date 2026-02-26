"use client";

import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchTagHistory, fetchTagsCurrent, postEdgeAction } from "@/lib/api";
import { useOperationalContextStore } from "@/store/context-store";
import { useHmiStore } from "@/store/hmi-store";
import { formatProcessValue } from "@/lib/hmi-value";

type DeviceCommandAction = {
  action_type: string;
  payload: Record<string, unknown>;
};

function extractActions(meta: Record<string, unknown> | undefined): DeviceCommandAction[] {
  const autos = meta?.automations;
  if (!Array.isArray(autos)) return [];
  const out: DeviceCommandAction[] = [];
  for (const a of autos) {
    if (!a || typeof a !== "object") continue;
    const obj = a as Record<string, unknown>;
    const enabled = obj.enabled;
    if (enabled === false) continue;
    const actions = Array.isArray(obj.actions) ? obj.actions : obj.action ? [obj.action] : [];
    for (const act of actions) {
      if (!act || typeof act !== "object") continue;
      const actObj = act as Record<string, unknown>;
      const actionType = String(actObj.action_type || "");
      const payload = (actObj.payload && typeof actObj.payload === "object")
        ? (actObj.payload as Record<string, unknown>)
        : {};
      out.push({ action_type: actionType, payload });
    }
  }
  return out;
}

function findPrintDeviceCommand(meta: Record<string, unknown> | undefined): Record<string, unknown> | null {
  const actions = extractActions(meta);
  for (const a of actions) {
    if (a.action_type !== "device.command") continue;
    const cmd = String(a.payload.command || "").toLowerCase();
    if (cmd === "print" || cmd === "print.escpos") return a.payload;
  }
  return null;
}

function findPrintPersistAction(meta: Record<string, unknown> | undefined): DeviceCommandAction | null {
  const actions = extractActions(meta);
  for (const a of actions) {
    if (a.action_type === "print.persist") return a;
  }
  return null;
}

export default function HistoryPage() {
  const { selectedTag, setSelectedTag } = useHmiStore();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const [pageSize, setPageSize] = useState(50);
  const [page, setPage] = useState(1);
  const [selectedRows, setSelectedRows] = useState<Record<string, boolean>>({});
  const [printing, setPrinting] = useState(false);
  const [printMsg, setPrintMsg] = useState<string>("");
  const offset = (page - 1) * pageSize;

  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const tags = useQuery({ queryKey: ["history-tags", filter], queryFn: () => fetchTagsCurrent(500, filter) });
  const history = useQuery({
    queryKey: ["history-events", selectedTag, pageSize, page],
    queryFn: () => fetchTagHistory(selectedTag, pageSize, offset),
    enabled: Boolean(selectedTag),
  });

  const rows = useMemo(() => history.data ?? [], [history.data]);
  const hasNext = rows.length === pageSize;
  const selectedTagObj = useMemo(
    () => (tags.data ?? []).find((t) => t.tag_code === selectedTag),
    [tags.data, selectedTag]
  );
  const printCommandPayload = useMemo(
    () => findPrintDeviceCommand(selectedTagObj?.metadata_json),
    [selectedTagObj]
  );
  const printPersistAction = useMemo(
    () => findPrintPersistAction(selectedTagObj?.metadata_json),
    [selectedTagObj]
  );
  const selectedItems = useMemo(
    () =>
      rows
        .filter((r, idx) => selectedRows[`${r.ts}-${r.tag_code}-${idx}`])
        .sort((a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime()),
    [rows, selectedRows]
  );
  useEffect(() => {
    setSelectedRows({});
  }, [selectedTag, page, pageSize]);

  async function executePrintSelected() {
    if (!selectedTagObj) {
      setPrintMsg("No tag selected.");
      return;
    }
    if (!printCommandPayload) {
      setPrintMsg("Selected tag has no print automation (device.command print).");
      return;
    }
    if (selectedItems.length === 0) {
      setPrintMsg("Select at least one historical row.");
      return;
    }
    setPrinting(true);
    setPrintMsg("");
    try {
      const bufferId = `ui:${selectedTagObj.tag_code}:${Date.now()}`;
      for (const r of selectedItems) {
        await postEdgeAction(
          selectedTagObj.site_code || site,
          selectedTagObj.edge_code,
          "buffer.weights.accumulate",
          {
            buffer_id: bufferId,
            max_items: Math.max(500, selectedItems.length + 10),
            only_positive: false,
            trigger: {
              tag_id: selectedTagObj.tag_code,
              value: r.value,
              timestamp: r.ts,
            },
          },
          { source: "web-ui", target: "edge" }
        );
      }

      const payload = JSON.parse(JSON.stringify(printCommandPayload)) as Record<string, unknown>;
      const argsRaw = payload.args;
      const args =
        argsRaw && typeof argsRaw === "object"
          ? { ...(argsRaw as Record<string, unknown>) }
          : {};
      args.mode = "from_buffer";
      args.buffer_id = bufferId;
      args.clear_after_print = true;
      payload.args = args;
      if (!payload.command) payload.command = "print";

      await postEdgeAction(
        selectedTagObj.site_code || site,
        selectedTagObj.edge_code,
        "device.command",
        payload,
        { source: "web-ui", target: "edge" }
      );

      if (printPersistAction) {
        await postEdgeAction(
          selectedTagObj.site_code || site,
          selectedTagObj.edge_code,
          "print.persist",
          {
            ...(printPersistAction.payload || {}),
            buffer_id: bufferId,
            selected_count: selectedItems.length,
            tag_code: selectedTagObj.tag_code,
          },
          { source: "web-ui", target: "central" }
        );
      }

      setPrintMsg(`Print command sent. samples=${selectedItems.length} buffer=${bufferId}`);
    } catch (e) {
      setPrintMsg(`Print failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setPrinting(false);
    }
  }

  return (
    <>
      <h2>Historical Query</h2>
      <section className="card" style={{ marginBottom: 12 }}>
        <h3>Tag + Query Scope</h3>
        <div className="control-row">
          <label style={{ minWidth: 300 }}>
            <span>Tag</span>
            <select
              value={selectedTag}
              onChange={(e) => {
                setSelectedTag(e.target.value);
                setPage(1);
              }}
            >
              {(tags.data ?? []).map((t) => (
                <option key={t.tag_code} value={t.tag_code}>
                  {t.tag_code} ({t.device_code})
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Page Size</span>
            <select value={String(pageSize)} onChange={(e) => { setPageSize(Number.parseInt(e.target.value, 10)); setPage(1); }}>
              <option value="25">25</option>
              <option value="50">50</option>
              <option value="100">100</option>
            </select>
          </label>
          <div className="mono muted-inline">rows: {rows.length}</div>
          <div className="mono muted-inline">selected: {selectedItems.length}</div>
          <button
            disabled={printing || !printCommandPayload || selectedItems.length === 0}
            onClick={executePrintSelected}
          >
            {printing ? "Printing..." : "Print Selected Weights"}
          </button>
        </div>
        {printCommandPayload ? (
          <div className="mono muted-inline">print automation: detected</div>
        ) : (
          <div className="mono muted-inline">print automation: not detected for selected tag</div>
        )}
        {printMsg ? <div className="mono">{printMsg}</div> : null}
      </section>

      <section className="card">
        <h3>Event History</h3>
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 40 }}></th>
                <th style={{ width: 220 }}>Timestamp</th>
                <th style={{ width: 130 }}>Site</th>
                <th style={{ width: 130 }}>Edge</th>
                <th style={{ width: 200 }}>Tag</th>
                <th style={{ width: 220 }}>Value</th>
                <th style={{ width: 110 }}>Quality</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, idx) => (
                <tr key={`${r.ts}-${r.tag_code}-${idx}`}>
                  <td>
                    <input
                      type="checkbox"
                      checked={Boolean(selectedRows[`${r.ts}-${r.tag_code}-${idx}`])}
                      onChange={(e) =>
                        setSelectedRows((prev) => ({
                          ...prev,
                          [`${r.ts}-${r.tag_code}-${idx}`]: e.target.checked,
                        }))
                      }
                    />
                  </td>
                  <td className="mono">{new Date(r.ts).toLocaleString()}</td>
                  <td className="mono">{r.site_code}</td>
                  <td className="mono">{r.edge_code}</td>
                  <td className="mono">{r.tag_code}</td>
                  <td className="mono">{formatProcessValue(r.value)}</td>
                  <td>{r.quality_status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="pagination-row">
          <button disabled={page <= 1} onClick={() => setPage((p) => Math.max(1, p - 1))}>Prev</button>
          <span className="mono">Page {page}</span>
          <button disabled={!hasNext} onClick={() => setPage((p) => p + 1)}>Next</button>
        </div>
      </section>
    </>
  );
}
