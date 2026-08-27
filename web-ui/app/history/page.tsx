"use client";

import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchTagHistory, fetchTagsCurrent, postEdgeAction, type TagHistory } from "@/lib/api";
import { useOperationalContextStore } from "@/store/context-store";
import { useHmiStore } from "@/store/hmi-store";
import { formatProcessValue } from "@/lib/hmi-value";
import { useAutoSelectFirstTag } from "@/lib/use-auto-select-tag";

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

// Upper bound on how much history we pull per tag selection/date-range in one request. The date
// range (when set) is applied server-side (WHERE ts >= from AND ts <= to, before LIMIT), so this
// no longer bounds how far back a filter can reach -- it's just a safety cap on how many matching
// rows come back in one page-load, generous enough for interactive use without pulling unbounded
// history for a tag that's been reporting for months. Pagination over the returned set is still
// client-side.
const HISTORY_FETCH_LIMIT = 2000;

// Stable row identity independent of which page it's currently displayed on -- needed so a
// selection made on one page survives navigating to another (selection is keyed by this, not by
// position within the currently-rendered page).
function rowKey(r: { ts: string; tag_code: string }, indexInFullSet: number) {
  return `${indexInFullSet}-${r.ts}-${r.tag_code}`;
}

export default function HistoryPage() {
  const { selectedTag, setSelectedTag } = useHmiStore();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const [pageSize, setPageSize] = useState(50);
  const [page, setPage] = useState(1);
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  // Keyed by rowKey(), not by position-on-page: this is what makes a selection survive both
  // paging and date-range changes, since the same historical row always maps to the same key.
  const [selectedRows, setSelectedRows] = useState<Record<string, TagHistory>>({});
  const [printing, setPrinting] = useState(false);
  const [printMsg, setPrintMsg] = useState<string>("");

  const filter = { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined };
  const tags = useQuery({ queryKey: ["history-tags", filter], queryFn: () => fetchTagsCurrent(500, filter) });
  useAutoSelectFirstTag(tags.data, selectedTag, setSelectedTag);
  // Converted to RFC3339/UTC for the API -- a date filter never hides tags in the "Tag" selector
  // above (that list comes from fetchTagsCurrent, an unrelated query), only the rows fetched here
  // for the already-selected tag.
  const dateRange = {
    from: dateFrom ? new Date(dateFrom).toISOString() : undefined,
    to: dateTo ? new Date(dateTo).toISOString() : undefined,
  };
  const history = useQuery({
    queryKey: ["history-events", selectedTag, HISTORY_FETCH_LIMIT, dateRange.from, dateRange.to],
    queryFn: () => fetchTagHistory(selectedTag, HISTORY_FETCH_LIMIT, 0, dateRange),
    enabled: Boolean(selectedTag),
  });

  // Already filtered by the server when a date range is set -- just paginate client-side.
  const filteredRows = useMemo(() => history.data ?? [], [history.data]);
  const pageCount = Math.max(1, Math.ceil(filteredRows.length / pageSize));
  const clampedPage = Math.min(page, pageCount);
  const rows = useMemo(
    () => filteredRows.slice((clampedPage - 1) * pageSize, clampedPage * pageSize),
    [filteredRows, clampedPage, pageSize]
  );
  const hasNext = clampedPage < pageCount;
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
      Object.values(selectedRows).sort(
        (a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime()
      ),
    [selectedRows]
  );
  // Only a tag change invalidates a selection -- rows from a different tag are meaningless to
  // keep around. Changing the page or the date filter must NOT clear it: that's the whole point
  // of keying selection by rowKey() instead of by page position.
  useEffect(() => {
    setSelectedRows({});
  }, [selectedTag]);
  useEffect(() => {
    setPage(1);
  }, [selectedTag, dateFrom, dateTo, pageSize]);

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
            measurement_device_id: selectedTagObj.device_code,
            measurement_device_name: selectedTagObj.device_code,
            max_items: Math.max(500, selectedItems.length + 10),
            only_positive: false,
            trigger: {
              tag_id: selectedTagObj.tag_code,
              device_id: selectedTagObj.device_code,
              device_name: selectedTagObj.device_code,
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
      payload.measurement_device_id = selectedTagObj.device_code;
      payload.measurement_device_name = selectedTagObj.device_code;
      payload.trigger = {
        tag_id: selectedTagObj.tag_code,
        device_id: selectedTagObj.device_code,
        device_name: selectedTagObj.device_code,
      };
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
            <select value={String(pageSize)} onChange={(e) => setPageSize(Number.parseInt(e.target.value, 10))}>
              <option value="25">25</option>
              <option value="50">50</option>
              <option value="100">100</option>
            </select>
          </label>
          <div className="mono muted-inline">rows: {filteredRows.length}</div>
          <div className="mono muted-inline">selected: {selectedItems.length}</div>
          <button
            disabled={printing || !printCommandPayload || selectedItems.length === 0}
            onClick={executePrintSelected}
          >
            {printing ? "Printing..." : "Print Selected Weights"}
          </button>
          {selectedItems.length > 0 ? (
            <button type="button" onClick={() => setSelectedRows({})}>
              Clear selection
            </button>
          ) : null}
        </div>
        <div className="control-row">
          <label>
            <span>From</span>
            <input
              type="datetime-local"
              value={dateFrom}
              onChange={(e) => setDateFrom(e.target.value)}
            />
          </label>
          <label>
            <span>To</span>
            <input
              type="datetime-local"
              value={dateTo}
              onChange={(e) => setDateTo(e.target.value)}
            />
          </label>
          {dateFrom || dateTo ? (
            <button type="button" onClick={() => { setDateFrom(""); setDateTo(""); }}>
              Clear dates
            </button>
          ) : null}
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
              {rows.map((r, idxOnPage) => {
                const indexInFullSet = (clampedPage - 1) * pageSize + idxOnPage;
                const key = rowKey(r, indexInFullSet);
                return (
                  <tr key={key}>
                    <td>
                      <input
                        type="checkbox"
                        checked={Boolean(selectedRows[key])}
                        onChange={(e) =>
                          setSelectedRows((prev) => {
                            const next = { ...prev };
                            if (e.target.checked) {
                              next[key] = r;
                            } else {
                              delete next[key];
                            }
                            return next;
                          })
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
                );
              })}
            </tbody>
          </table>
        </div>
        <div className="pagination-row">
          <button disabled={clampedPage <= 1} onClick={() => setPage((p) => Math.max(1, p - 1))}>Prev</button>
          <span className="mono">Page {clampedPage} of {pageCount}</span>
          <button disabled={!hasNext} onClick={() => setPage((p) => p + 1)}>Next</button>
        </div>
      </section>
    </>
  );
}
