# Live Page Connectivity Redesign — Design

**Status:** Approved by user in chat (2026-08-26), pending written spec review.
**Scope:** `web-ui-v2`'s Live page, `ContextBar`, `EdgeDiagnosticsPanel`, and a new shared date/time formatting utility. This is "Group B" of a larger feedback set (see conversation); Groups A (navbar shell) and C (History page) are separate, later specs.

## Background

After the first production deploy of `web-ui-v2` (tag `webuiv2-v0.1.0`), the user reviewed it against the still-running legacy `web-ui` (Next.js, port 3001) and flagged real regressions and missing functionality:

- The "edges online" badge always reads a wrong/zero count even when edges are genuinely connected (a live-reproduced instance of an already-known `central-server` status-vocabulary inconsistency — see `docs/finding-mqtt-client-stale-session-detection.md` and the SDD ledger's Task 8/9/11 notes).
- No Line/Area/Cell/Edge filter selectors exist (only Site) — `web-ui`'s `ContextBar` has always had these.
- No device-level connectivity visualization (the "lamp" — green/amber/red dot per device) that `web-ui`'s Live page has.
- No timestamp shown next to live tag values.
- Telemetry values and the reset action are mixed into the same view; the user wants connectivity separated from telemetry detail.
- All dates/times use the browser's local `toLocaleString()`/`toLocaleTimeString()`, which is unregionalized — depends on each operator's own machine clock/timezone rather than a single, consistent time.

The user confirmed (in chat, via explicit choices) that this is a real redesign, not a small patch, and asked to reintroduce `web-ui`'s SSE-based live-update architecture (not just add missing UI on top of `web-ui-v2`'s current 2.5s-poll approach).

## Global Constraints (from the original web-ui-v2 spec, still binding)

- `web-ui` (Next.js, port 3001) is never modified. This work is entirely inside `web-ui-v2`.
- No `central-server` domain-model changes — everything needed already exists: `/api/stream/events` (SSE), `/api/devices/current`, `/api/context/{lines,areas,cells}`, `/api/edges/current`, `/api/tags/current`. Confirmed by direct read of `crates/central-server/src/api.rs`.
- Spanish default via the existing i18next setup (`web-ui-v2/src/lib/i18n.ts`, `locales/es.ts`/`en.ts`).
- Vendored `@ifahub/ui`/`@ifahub/tables` primitives are the UI toolkit; no new component libraries.

## 1. Data architecture: SSE dual-pipeline + device layer

Port `web-ui`'s SSE client (`web-ui/lib/sse.ts`'s `subscribeSse`) into `web-ui-v2/src/lib/sse.ts`, connecting to the existing `GET /api/stream/events?site=&line=&area=&cell=&edge=&tag=&exclude_raw=&replay=` endpoint via the browser's native `EventSource`. Two pipelines, matching `web-ui/app/live/page.tsx`'s existing pattern:

- **Grid pipeline**: batches incoming tag events every ~120ms and patches the in-memory tag/device/edge state (no `setState` per event — avoids render thrashing on a busy site).
- **Selected-item pipeline**: when a device/edge is selected (diagnostics panel open), a second `subscribeSse` filtered to that item's tag(s) feeds the panel's live telemetry list directly, independent of the grid's batching cadence.

**Polling is not replaced by SSE — it stays exactly as `web-ui-v2` already runs it, `refetchInterval: 2500` (2.5s) on `fetchEdgesCurrent`/`fetchDevicesCurrent`/`fetchTagsCurrent`, matching `web-ui`'s own live page (same 2500ms value).** This is `web-ui`'s real, already-proven pattern: the poll is the reliable base state (self-healing if the browser tab was backgrounded, the SSE connection silently dropped, or an event was missed), and SSE is an additive low-latency patch on top of it — never the sole source of truth. No separate "SSE-down" detection/reconnect-timer mechanism is needed as a result; the next 2.5s poll always corrects any drift. The edge staleness threshold (`edgeConnected`'s cutoff, §2) is 45 seconds, matching `central-server`'s own `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT` (already confirmed during this session's earlier investigation) — `web-ui`'s `NEXT_PUBLIC_EDGE_STALE_SECS` env var is Next.js-specific plumbing that doesn't carry over; hardcode 45 as a named constant in the new stack instead of reintroducing an env var for it.

**New**: `fetchDevicesCurrent(limit, filter)` + `DeviceCurrent` type, added to `web-ui-v2/src/lib/api-client.ts`, calling the already-existing `GET /api/devices/current` — same shape as `web-ui/lib/api.ts`'s existing, already-proven version (`site_code/line_code/area_code/cell_code/edge_code/device_code/connection_id/state/severity/reason/tags_connected/tags_stale/tags_disconnected/last_change_at/last_seen_at`).

## 2. Connectivity model: three-tier lamp

Port `web-ui/app/live/page.tsx`'s `edgeConnected()` and `lampFromDeviceState()` verbatim (adjusted only for the new stack's types/imports):

```ts
function edgeConnected(edge: EdgeCurrent | undefined): boolean {
  if (!edge) return false;
  const status = String(edge.status || "").toLowerCase();
  const okState = status === "ok" || status === "online"; // <- both literals, on purpose
  return okState && ageSecsFromIso(edge.last_seen_at) <= EDGE_STALE_AFTER_SECS; // = 45, see §1
}

function lampFromDeviceState(device: DeviceCurrent | undefined, edgeConn: boolean): "good" | "warn" | "bad" {
  if (!edgeConn) return "bad";
  const state = String(device?.state || "").toLowerCase();
  if (state === "connected") return "good";
  if (state === "stale") return "warn";
  if (state === "disconnected") return "bad";
  return "warn";
}
```

The `status === "ok" || status === "online"` check is also the fix for the wrong-badge-count bug: `central-server` writes `"online"` from the telemetry-ingest path and `"ok"` from the health-heartbeat path into the same `edge_current_state.status` column (a real backend inconsistency, already documented, not something this frontend-only redesign fixes at the source) — checking both literals, exactly like `web-ui` already does, makes the frontend correct regardless of which literal happens to be live in the column at read time. `EdgesOnlineBadge`'s `ONLINE_STATUSES` set (`web-ui-v2/src/components/live/edges-online-badge.tsx`) changes from `new Set(["online"])` to this same two-literal check.

Render as a colored dot (reuse the vendored `Badge`/a small new `ConnectivityDot` component — CSS, no new dependency) next to each device/edge row, with a `title` tooltip showing the raw `state`/`status` for operators who want the detail (matches `web-ui`'s `title={\`device_state: ${state}\`}` pattern).

## 3. ContextBar: real Line/Area/Cell/Edge selectors

`web-ui-v2/src/store/context-store.ts` already has `line`/`area`/`cell`/`edge` state (Task 7 built the full store; only the UI was Site-only). Extend `web-ui-v2/src/components/context-bar.tsx` to add:

- `fetchLines(site)`, `fetchAreas(site, line?)`, `fetchCells(site, line?, area?)` — new functions in `api-client.ts`, calling the already-existing `/api/context/lines|areas|cells` routes (same query-param cascade `web-ui/lib/api.ts` already uses).
- Real `<Select>` (vendored, matching the Site selector's existing pattern from Task 9) for each of Line/Area/Cell/Edge. Options cascade by parent selection (`fetchAreas(site, line)` only returns areas under that line, etc. — the backend routes already scope this way). `web-ui`'s own `ContextBar` does NOT explicitly clear child selections when a parent changes (e.g. picking a different Line while an Area from the old Line is still selected leaves a stale, now-invalid Area value in the store) — this redesign explicitly fixes that gap rather than porting it: changing Site clears Line/Area/Cell/Edge; changing Line clears Area/Cell/Edge; changing Area clears Cell/Edge; changing Cell clears Edge. Implement via a `useEffect` per level watching its parent's value (matching the already-proven `useAutoSelectFirst` hook's shape from `web-ui-v2/src/lib/use-auto-select-first.ts`), not inline in each `onValueChange`.
- A breadcrumb row with a connectivity-style dot per level (lit if a value is selected at that level) and a "clear filters" button, matching `web-ui`'s `context-breadcrumb`/`ctx-reset` — implemented with vendored primitives instead of hand-rolled CSS classes.
- The edges-online badge (`0/2`-style) moves into this breadcrumb row, using the fixed two-literal check from §2.

## 4. Live page: connectivity dashboard, not a telemetry grid

The main Live view becomes a device/edge-centric list: each row is one device (grouped under its edge), showing device_code, edge_code, the lamp, and `last_seen_at` (formatted via the new date utility, §6) — no per-tag values in this view. Clicking a row opens `EdgeDiagnosticsPanel` (§5) scoped to that device's edge.

Edges with zero devices (or devices with zero tags) still render, with an explicit "no devices reporting" state — don't silently drop them, since an edge that stopped reporting entirely is exactly the failure mode operators need to see.

## 5. EdgeDiagnosticsPanel: gains a live telemetry section

The existing panel (`web-ui-v2/src/components/live/edge-diagnostics-panel.tsx` — events list + reset button, from the earlier plan's Task 14) gains a new section: the selected edge's tags, each showing `tag_code`, current `value`, `quality.status`, and `ts` (formatted, §6) — fed by the selected-item SSE pipeline (§1), with the existing `fetchTagsCurrent(1, {edge, tag})`-style initial load as the pre-SSE-connect fallback. Reset and the events list are unchanged in behavior, just visually adjacent to the new telemetry section instead of the reset button living on the main Live grid.

## 6. Regionalized date/time formatting

New `web-ui-v2/src/lib/datetime.ts`:

```ts
const SERVER_TIME_ZONE = "America/La_Paz"; // matches the real deployment (.154, Bolivia)

export function formatServerDateTime(iso: string): string {
  return new Intl.DateTimeFormat("es-BO", {
    timeZone: SERVER_TIME_ZONE,
    dateStyle: "short",
    timeStyle: "medium",
  }).format(new Date(iso));
}

export function formatServerTime(iso: string): string {
  return new Intl.DateTimeFormat("es-BO", {
    timeZone: SERVER_TIME_ZONE,
    timeStyle: "medium",
  }).format(new Date(iso));
}
```

Every raw `new Date(x).toLocaleString()`/`toLocaleTimeString()` introduced by this redesign (Live rows, diagnostics panel telemetry, breadcrumb) uses these instead — this makes every operator's screen show the same wall-clock time regardless of that machine's own OS locale/timezone settings, which is the actual bug (not just cosmetic formatting). `SERVER_TIME_ZONE` is a named constant specifically so a future multi-site deployment in a different timezone is a one-line change, not a search-and-replace.

Out of scope for this spec: retrofitting History's existing timestamp column to use this utility too — that belongs to Group C's own spec, though this utility is written to be reused there directly.

## Testing

- `datetime.ts`: unit tests pinning known ISO inputs to known `America/La_Paz`-formatted outputs (deterministic, no reliance on the test runner's own machine timezone).
- `lampFromDeviceState`/`edgeConnected`: unit tests covering all state combinations (ported/adapted from `web-ui`'s own logic, which has no existing test file — new coverage, not a port of existing tests).
- `EdgesOnlineBadge`: existing tests updated for the two-literal check (`"ok"` counts as online now, in addition to `"online"`).
- SSE client (`lib/sse.ts`): unit test the batching/dedup logic with a fake `EventSource` (matches the vendored `useDataTableInstance` test pattern of mocking at a narrow boundary) — not a real browser SSE connection.
- Live page, ContextBar, diagnostics panel: Playwright verification against the real local stack (`docker-compose.scada.yml` + `edge-sim.yml`), per this project's established pattern — real SSE events observed end-to-end, not just component-level tests.

## Out of scope (explicitly, for this spec)

- Navbar/shell redesign (collapsible nav, active-section indicator) — separate spec (Group A).
- History page's own filter/selection/date-range work — separate spec (Group C).
- Any `central-server` backend change (the status-vocabulary inconsistency's root fix) — tracked separately in the existing finding doc, not this spec's job.
- Cutover from `web-ui` v1 to `web-ui-v2` — explicitly future, separate decision per the original rewrite spec.
