# Live Page Connectivity Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild `web-ui-v2`'s Live page as a connectivity-first dashboard with real Line/Area/Cell/Edge filters, a three-tier device-level connectivity lamp, SSE-driven live updates layered on top of the existing poll, per-edge telemetry moved into the diagnostics panel, and server-timezone-regionalized timestamps everywhere.

**Architecture:** Port `web-ui` (Next.js v1)'s already-proven SSE client, device-connectivity model, and `ContextBar` filter cascade into `web-ui-v2`'s stack (Vite/TanStack/vendored `@ifahub` primitives/i18next), fixing two real bugs found along the way (the "online" vs "ok" status-vocabulary badge bug, and a stale-child-selection gap in the legacy `ContextBar` itself) rather than reproducing them.

**Tech Stack:** React 19, TanStack Router/Query, Zustand, vendored `@ifahub/ui` primitives (`Select`, `Badge`, `Card`, `Sheet`, `Button`), react-i18next, native browser `EventSource` for SSE, Vitest/RTL.

**Spec:** `docs/superpowers/specs/2026-08-26-live-page-connectivity-redesign-design.md`

## Global Constraints

- `web-ui` (Next.js, port 3001) is never modified by any task in this plan — this work is entirely inside `web-ui-v2`.
- No `central-server` changes — every endpoint this plan needs already exists: `GET /api/stream/events`, `GET /api/devices/current`, `GET /api/context/{lines,areas,cells}`, `GET /api/edges/current`, `GET /api/tags/current`.
- Spanish default via the existing `web-ui-v2/src/lib/i18n.ts`/`locales/es.ts`/`en.ts` — every new user-facing string gets a real key in both files, no hardcoded literals.
- Only vendored `@ifahub/ui` primitives — no new UI libraries.
- Edge staleness threshold: 45 seconds (matches `central-server`'s `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT`).
- Server timezone for all regionalized date/time display: `America/La_Paz`.
- Every page/component task must be verified against the real local stack (`docker-compose.scada.yml` --profile central --profile seed --profile v2 + `docker-compose.edge-sim.yml`, project name `ifascada`) before being marked done.

---

### Task 1: Regionalized date/time formatting utility

**Files:**
- Create: `web-ui-v2/src/lib/datetime.ts`
- Test: `web-ui-v2/src/lib/datetime.test.ts`

**Interfaces:**
- Produces: `formatServerDateTime(iso: string): string`, `formatServerTime(iso: string): string`, `SERVER_TIME_ZONE` (exported constant, `"America/La_Paz"`) — used by Tasks 5, 6, 7.

- [ ] **Step 1: Write the failing test**

```typescript
// web-ui-v2/src/lib/datetime.test.ts
import { describe, it, expect } from "vitest";
import { formatServerDateTime, formatServerTime, SERVER_TIME_ZONE } from "./datetime";

describe("SERVER_TIME_ZONE", () => {
  it("is America/La_Paz", () => {
    expect(SERVER_TIME_ZONE).toBe("America/La_Paz");
  });
});

describe("formatServerDateTime", () => {
  it("formats a UTC ISO timestamp in America/La_Paz (UTC-4, no DST)", () => {
    // 2026-08-26T18:31:13.144564Z UTC -> 14:31 in America/La_Paz (UTC-4)
    const result = formatServerDateTime("2026-08-26T18:31:13.144564Z");
    expect(result).toContain("14:31");
    expect(result).toMatch(/26[/-]8|8[/-]26|ago/i); // date portion present in some locale-valid form
  });
});

describe("formatServerTime", () => {
  it("formats just the time portion in America/La_Paz", () => {
    const result = formatServerTime("2026-08-26T18:31:13.144564Z");
    expect(result).toContain("14:31");
    expect(result).not.toMatch(/2026/); // no date portion
  });

  it("handles midnight UTC correctly (crosses to previous day locally)", () => {
    // 2026-08-26T02:00:00Z UTC -> 2026-08-25 22:00 in America/La_Paz (UTC-4)
    const result = formatServerTime("2026-08-26T02:00:00Z");
    expect(result).toContain("22:00");
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- datetime.test.ts`
Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement it**

```typescript
// web-ui-v2/src/lib/datetime.ts
/**
 * Every date/time shown in this app must read the same wall-clock time regardless of which
 * operator's machine (and its own OS locale/timezone settings) is looking at the screen --
 * `new Date(x).toLocaleString()` (what the app being replaced does everywhere) depends on the
 * browser's own timezone, which is the actual bug, not just a formatting nicety. Pin to the
 * real deployment's timezone instead. A future multi-site deployment in a different timezone
 * is then a one-line change here, not a search-and-replace across every page.
 */
export const SERVER_TIME_ZONE = "America/La_Paz";

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

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- datetime.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web-ui-v2/src/lib/datetime.ts web-ui-v2/src/lib/datetime.test.ts
git commit -m "feat(web-ui-v2): regionalized date/time formatting pinned to America/La_Paz"
```

---

### Task 2: SSE client

**Files:**
- Create: `web-ui-v2/src/lib/sse.ts`
- Test: `web-ui-v2/src/lib/sse.test.ts`

**Interfaces:**
- Produces: `RtEvent` type, `subscribeSse(onMessage: (evt: RtEvent) => void, options?: SseOptions): () => void` — used by Task 8.
- `SseOptions = { site?: string; line?: string; area?: string; cell?: string; edge?: string; tag?: string; excludeRaw?: boolean; replay?: boolean }`

This is a direct port of `web-ui/lib/sse.ts`'s `subscribeSse` (the `subscribeOpsSse` half is NOT ported — this app's ops-events reading already goes through `GET /api/ops/events` polling in the diagnostics panel, Task 14 of the earlier plan; no SSE-based ops stream is needed here). The only change from the original: `process.env.NEXT_PUBLIC_SSE_URL` (Next.js-specific env access, doesn't exist in Vite) is replaced with a hardcoded `/api/stream/events` default, matching how the rest of `web-ui-v2`'s `api-client.ts` never uses env-based API base overrides.

- [ ] **Step 1: Write the failing test using a fake `EventSource`**

```typescript
// web-ui-v2/src/lib/sse.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { subscribeSse, type RtEvent } from "./sse";

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  url: string;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  listeners: Record<string, ((ev: MessageEvent) => void)[]> = {};
  closed = false;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  addEventListener(type: string, handler: (ev: MessageEvent) => void) {
    (this.listeners[type] ??= []).push(handler);
  }
  close() {
    this.closed = true;
  }
  emit(type: "message" | "runtime", data: unknown) {
    const ev = { data: JSON.stringify(data) } as MessageEvent;
    if (type === "message") this.onmessage?.(ev);
    for (const h of this.listeners[type] ?? []) h(ev);
  }
}

describe("subscribeSse", () => {
  beforeEach(() => {
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("connects to /api/stream/events with query params from options", () => {
    subscribeSse(() => {}, { site: "plant-a", line: "line-main", excludeRaw: true });
    const es = FakeEventSource.instances[0];
    expect(es.url).toContain("/api/stream/events?");
    expect(es.url).toContain("site=plant-a");
    expect(es.url).toContain("line=line-main");
    expect(es.url).toContain("exclude_raw=true");
  });

  it("connects with no query string when no options are given", () => {
    subscribeSse(() => {});
    expect(FakeEventSource.instances[0].url).toBe("/api/stream/events");
  });

  it("delivers events from both the bare message event and the named 'runtime' event", () => {
    const received: RtEvent[] = [];
    subscribeSse((evt) => received.push(evt));
    const es = FakeEventSource.instances[0];
    const payload = { event_type: "telemetry", site: "plant-a", agent: "edge-mix-1", payload: { v: 1 }, published_at: "2026-08-26T18:00:00Z" };
    es.emit("message", payload);
    es.emit("runtime", { ...payload, event_type: "runtime-variant" });
    expect(received).toHaveLength(2);
    expect(received[0].event_type).toBe("telemetry");
    expect(received[1].event_type).toBe("runtime-variant");
  });

  it("stamps received_at_ms on delivery", () => {
    const received: RtEvent[] = [];
    subscribeSse((evt) => received.push(evt));
    const es = FakeEventSource.instances[0];
    es.emit("message", { event_type: "x", site: "s", agent: "a", payload: {}, published_at: "2026-08-26T18:00:00Z" });
    expect(received[0].received_at_ms).toBeTypeOf("number");
  });

  it("ignores malformed JSON payloads instead of throwing", () => {
    const received: RtEvent[] = [];
    subscribeSse((evt) => received.push(evt));
    const es = FakeEventSource.instances[0];
    expect(() => es.onmessage?.({ data: "not json" } as MessageEvent)).not.toThrow();
    expect(received).toHaveLength(0);
  });

  it("returns an unsubscribe function that closes the EventSource", () => {
    const unsubscribe = subscribeSse(() => {});
    const es = FakeEventSource.instances[0];
    expect(es.closed).toBe(false);
    unsubscribe();
    expect(es.closed).toBe(true);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- sse.test.ts`
Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement it**

```typescript
// web-ui-v2/src/lib/sse.ts
/**
 * Ported from web-ui/lib/sse.ts's subscribeSse. This connects to central-server's real,
 * already-existing GET /api/stream/events SSE endpoint. Polling (fetchEdgesCurrent/
 * fetchDevicesCurrent/fetchTagsCurrent, refetchInterval: 2500) stays the reliable base state
 * everywhere this is used -- this is an additive low-latency layer on top, never the sole
 * source of truth. If the SSE connection silently drops, the next 2.5s poll self-heals; no
 * separate reconnect/staleness-detection logic is needed as a result.
 */
export type RtEvent = {
  event_type: string;
  site: string;
  agent: string;
  payload: unknown;
  published_at: string;
  received_at_ms?: number;
};

export type SseOptions = {
  site?: string;
  line?: string;
  area?: string;
  cell?: string;
  edge?: string;
  tag?: string;
  excludeRaw?: boolean;
  replay?: boolean;
};

export function subscribeSse(onMessage: (evt: RtEvent) => void, options?: SseOptions): () => void {
  const baseUrl = "/api/stream/events";
  const qs = new URLSearchParams();
  if (options?.site) qs.set("site", options.site);
  if (options?.line) qs.set("line", options.line);
  if (options?.area) qs.set("area", options.area);
  if (options?.cell) qs.set("cell", options.cell);
  if (options?.edge) qs.set("edge", options.edge);
  if (options?.tag) qs.set("tag", options.tag);
  if (options?.excludeRaw !== undefined) qs.set("exclude_raw", String(options.excludeRaw));
  if (options?.replay !== undefined) qs.set("replay", String(options.replay));
  const url = qs.size > 0 ? `${baseUrl}?${qs.toString()}` : baseUrl;

  const es = new EventSource(url);
  const handler = (ev: MessageEvent) => {
    try {
      const parsed = JSON.parse(ev.data) as RtEvent;
      parsed.received_at_ms = Date.now();
      onMessage(parsed);
    } catch {
      // ignore malformed payloads
    }
  };
  es.onmessage = handler;
  es.addEventListener("runtime", handler as EventListener);
  return () => es.close();
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- sse.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web-ui-v2/src/lib/sse.ts web-ui-v2/src/lib/sse.test.ts
git commit -m "feat(web-ui-v2): port SSE client from web-ui/lib/sse.ts"
```

---

### Task 3: Devices and context-hierarchy API functions

**Files:**
- Modify: `web-ui-v2/src/lib/api-client.ts`
- Test: `web-ui-v2/src/lib/api-client.test.ts`

**Interfaces:**
- Consumes: `getJson` (existing, `api-client.ts`).
- Produces: `DeviceCurrent` type, `fetchDevicesCurrent(limit, filter?): Promise<DeviceCurrent[]>`, `ContextOption` type, `fetchLines(site?): Promise<ContextOption[]>`, `fetchAreas(site?, line?): Promise<ContextOption[]>`, `fetchCells(site?, line?, area?): Promise<ContextOption[]>` — used by Tasks 4, 5, 6.

- [ ] **Step 1: Write the failing tests**

```typescript
// web-ui-v2/src/lib/api-client.test.ts (append to the existing file)
import { fetchDevicesCurrent, fetchLines, fetchAreas, fetchCells } from "./api-client";

describe("fetchDevicesCurrent", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify([{ device_code: "dev-1", state: "connected" }]), { status: 200 })));
  });

  it("calls /api/devices/current with limit and filter", async () => {
    await fetchDevicesCurrent(50, { site: "plant-a", edge: "edge-mix-1" });
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/devices/current?");
    expect(url).toContain("limit=50");
    expect(url).toContain("site=plant-a");
    expect(url).toContain("edge=edge-mix-1");
  });
});

describe("fetchLines / fetchAreas / fetchCells", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify([{ code: "line-main", name: "Line Main" }]), { status: 200 })));
  });

  it("fetchLines calls /api/context/lines with site", async () => {
    await fetchLines("plant-a");
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/context/lines?");
    expect(url).toContain("site=plant-a");
  });

  it("fetchAreas calls /api/context/areas with site and line", async () => {
    await fetchAreas("plant-a", "line-main");
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/context/areas?");
    expect(url).toContain("site=plant-a");
    expect(url).toContain("line=line-main");
  });

  it("fetchCells calls /api/context/cells with site, line, and area", async () => {
    await fetchCells("plant-a", "line-main", "area-pack");
    const [url] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toContain("/api/context/cells?");
    expect(url).toContain("area=area-pack");
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- api-client.test.ts`
Expected: FAIL (`fetchDevicesCurrent`/`fetchLines`/`fetchAreas`/`fetchCells` not exported).

- [ ] **Step 3: Implement it**

Append to `web-ui-v2/src/lib/api-client.ts` (after the existing `fetchEdgesCurrent` function, keeping `LiveFilter`/`toQuery` as-is):

```typescript
export type DeviceCurrent = {
  site_code: string;
  line_code: string | null;
  area_code: string | null;
  cell_code: string | null;
  edge_code: string;
  device_code: string;
  connection_id: string | null;
  state: string;
  severity: string;
  reason: string | null;
  tags_connected: number;
  tags_stale: number;
  tags_disconnected: number;
  last_change_at: string;
  last_seen_at: string;
};

export function fetchDevicesCurrent(limit = 200, filter?: LiveFilter): Promise<DeviceCurrent[]> {
  const qs = toQuery({ limit, ...filter });
  return getJson<DeviceCurrent[]>(`/api/devices/current?${qs}`);
}

export type ContextOption = {
  code: string;
  name: string;
};

export function fetchLines(site?: string): Promise<ContextOption[]> {
  const qs = toQuery({ site });
  return getJson<ContextOption[]>(`/api/context/lines?${qs}`);
}

export function fetchAreas(site?: string, line?: string): Promise<ContextOption[]> {
  const qs = toQuery({ site, line });
  return getJson<ContextOption[]>(`/api/context/areas?${qs}`);
}

export function fetchCells(site?: string, line?: string, area?: string): Promise<ContextOption[]> {
  const qs = toQuery({ site, line, area });
  return getJson<ContextOption[]>(`/api/context/cells?${qs}`);
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- api-client.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web-ui-v2/src/lib/api-client.ts web-ui-v2/src/lib/api-client.test.ts
git commit -m "feat(web-ui-v2): add fetchDevicesCurrent and context-hierarchy (lines/areas/cells) API functions"
```

---

### Task 4: Connectivity model (edge/device lamp)

**Files:**
- Create: `web-ui-v2/src/lib/connectivity.ts`
- Test: `web-ui-v2/src/lib/connectivity.test.ts`

**Interfaces:**
- Consumes: `EdgeCurrent`, `DeviceCurrent` types (Tasks 5's predecessor / Task 3).
- Produces: `EDGE_STALE_AFTER_SECS` (constant, `45`), `ONLINE_STATUSES` (`Set<string>`, `{"online", "ok"}`), `edgeConnected(edge: EdgeCurrent | undefined, nowMs?: number): boolean`, `lampFromDeviceState(device: DeviceCurrent | undefined, edgeConn: boolean): "good" | "warn" | "bad"` — used by Tasks 5, 6.

This replaces `edges-online-badge.tsx`'s existing `ONLINE_STATUSES = new Set(["online"])` (Task 9's finding, which only checked one of the two real backend literals) with the two-literal check `web-ui`'s own `edgeConnected` already uses. `edgeConnected` takes an optional `nowMs` parameter (defaulting to `Date.now()`) specifically so the test can pin "now" instead of racing the real clock.

- [ ] **Step 1: Write the failing tests**

```typescript
// web-ui-v2/src/lib/connectivity.test.ts
import { describe, it, expect } from "vitest";
import { edgeConnected, lampFromDeviceState, EDGE_STALE_AFTER_SECS, ONLINE_STATUSES } from "./connectivity";
import type { EdgeCurrent, DeviceCurrent } from "./api-client";

const NOW = new Date("2026-08-26T18:00:00Z").getTime();

function edge(status: string, secsAgo: number): EdgeCurrent {
  const lastSeen = new Date(NOW - secsAgo * 1000).toISOString();
  return { site_code: "plant-a", line_code: null, area_code: null, cell_code: null, edge_code: "e1", status, last_seen_at: lastSeen, outbox_depth: 0, outbox_oldest_secs: null, action_metrics: {} };
}

describe("ONLINE_STATUSES / EDGE_STALE_AFTER_SECS", () => {
  it("recognizes both real backend literals", () => {
    expect(ONLINE_STATUSES.has("online")).toBe(true);
    expect(ONLINE_STATUSES.has("ok")).toBe(true);
  });
  it("is 45 seconds", () => {
    expect(EDGE_STALE_AFTER_SECS).toBe(45);
  });
});

describe("edgeConnected", () => {
  it("returns false for undefined edge", () => {
    expect(edgeConnected(undefined, NOW)).toBe(false);
  });
  it("returns true for status='online', fresh last_seen_at", () => {
    expect(edgeConnected(edge("online", 10), NOW)).toBe(true);
  });
  it("returns true for status='ok', fresh last_seen_at (the badge-bug fix)", () => {
    expect(edgeConnected(edge("ok", 10), NOW)).toBe(true);
  });
  it("returns false for status='disconnected' even if last_seen_at is fresh", () => {
    expect(edgeConnected(edge("disconnected", 1), NOW)).toBe(false);
  });
  it("returns false when last_seen_at is older than 45s even if status looks online", () => {
    expect(edgeConnected(edge("online", 46), NOW)).toBe(false);
  });
  it("returns true exactly at the 45s boundary", () => {
    expect(edgeConnected(edge("online", 45), NOW)).toBe(true);
  });
});

describe("lampFromDeviceState", () => {
  const connectedDevice: DeviceCurrent = { site_code: "plant-a", line_code: null, area_code: null, cell_code: null, edge_code: "e1", device_code: "d1", connection_id: null, state: "connected", severity: "info", reason: null, tags_connected: 5, tags_stale: 0, tags_disconnected: 0, last_change_at: "", last_seen_at: "" };

  it("returns 'bad' if the edge itself is not connected, regardless of device state", () => {
    expect(lampFromDeviceState(connectedDevice, false)).toBe("bad");
  });
  it("returns 'good' for a connected device on a connected edge", () => {
    expect(lampFromDeviceState(connectedDevice, true)).toBe("good");
  });
  it("returns 'warn' for a stale device on a connected edge", () => {
    expect(lampFromDeviceState({ ...connectedDevice, state: "stale" }, true)).toBe("warn");
  });
  it("returns 'bad' for a disconnected device on a connected edge", () => {
    expect(lampFromDeviceState({ ...connectedDevice, state: "disconnected" }, true)).toBe("bad");
  });
  it("returns 'warn' for an undefined device on a connected edge (unknown state, not a hard failure)", () => {
    expect(lampFromDeviceState(undefined, true)).toBe("warn");
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- connectivity.test.ts`
Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement it**

```typescript
// web-ui-v2/src/lib/connectivity.ts
import type { EdgeCurrent, DeviceCurrent } from "./api-client";

/**
 * Matches central-server's own CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT (crates/central-server/
 * src/api.rs's default_edge_stale_after_secs()). web-ui's NEXT_PUBLIC_EDGE_STALE_SECS env var
 * is Next.js-specific plumbing that doesn't carry over to Vite -- hardcode the same value
 * instead of reintroducing an env var for it.
 */
export const EDGE_STALE_AFTER_SECS = 45;

/**
 * central-server writes two different literals into edge_current_state.status depending on
 * which ingestion path last touched the row: insert_telemetry hardcodes "online"
 * (postgres.rs:642-648); insert_health writes the edge-agent's own health-message literal,
 * "ok"/"degraded" (postgres.rs:681-700, compute_health_status() in edge-agent's mqtt_bridge.rs).
 * This is a real, already-documented backend inconsistency this frontend redesign doesn't fix
 * at the source -- but checking both literals here (exactly what web-ui/components/
 * context-bar.tsx already does) makes the frontend correct regardless of which one is live in
 * the column at read time. This is the fix for the "edges online 0/n" badge bug.
 */
export const ONLINE_STATUSES = new Set(["online", "ok"]);

function ageSecsFromIso(ts: string, nowMs: number): number {
  const t = new Date(ts).getTime();
  if (Number.isNaN(t)) return Number.POSITIVE_INFINITY;
  return Math.max(0, Math.floor((nowMs - t) / 1000));
}

export function edgeConnected(edge: EdgeCurrent | undefined, nowMs: number = Date.now()): boolean {
  if (!edge) return false;
  const status = String(edge.status || "").toLowerCase();
  const okState = ONLINE_STATUSES.has(status);
  return okState && ageSecsFromIso(edge.last_seen_at, nowMs) <= EDGE_STALE_AFTER_SECS;
}

export function lampFromDeviceState(device: DeviceCurrent | undefined, edgeConn: boolean): "good" | "warn" | "bad" {
  if (!edgeConn) return "bad";
  const state = String(device?.state || "").toLowerCase();
  if (state === "connected") return "good";
  if (state === "stale") return "warn";
  if (state === "disconnected") return "bad";
  return "warn";
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- connectivity.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web-ui-v2/src/lib/connectivity.ts web-ui-v2/src/lib/connectivity.test.ts
git commit -m "feat(web-ui-v2): port edge/device connectivity model, fix online-status two-literal bug"
```

---

### Task 5: ContextBar — real Line/Area/Cell/Edge selectors, cascade-clear, badge fix

**Files:**
- Modify: `web-ui-v2/src/components/context-bar.tsx`
- Modify: `web-ui-v2/src/components/live/edges-online-badge.tsx`
- Modify: `web-ui-v2/src/locales/es.ts`, `web-ui-v2/src/locales/en.ts`
- Test: `web-ui-v2/src/components/context-bar.test.tsx` (extend existing)
- Test: `web-ui-v2/src/components/live/edges-online-badge.test.tsx` (extend existing)

**Interfaces:**
- Consumes: `fetchLines`/`fetchAreas`/`fetchCells`/`fetchEdgesCurrent` (Task 3), `ONLINE_STATUSES` (Task 4), `useAutoSelectFirst` (existing, `web-ui-v2/src/lib/use-auto-select-first.ts`), `useOperationalContextStore` (existing — already has `line`/`area`/`cell`/`edge` state, only the UI was Site-only).
- Produces: `ContextBar` now renders and wires all five levels; `EdgesOnlineBadge` now counts both `"online"` and `"ok"` as online.

- [ ] **Step 1: Add new i18n keys**

Add to `web-ui-v2/src/locales/es.ts`'s `live` block (after `edge: "Edge",`):

```typescript
    clearFilters: "Limpiar filtros",
```

Add to `web-ui-v2/src/locales/en.ts`'s `live` block, matching position:

```typescript
    clearFilters: "Clear filters",
```

(`live.line`/`live.area`/`live.cell`/`live.edge` already exist in both files from an earlier task — reuse them, don't add duplicates.)

- [ ] **Step 2: Write the failing test for the badge fix**

Update `web-ui-v2/src/components/live/edges-online-badge.test.tsx` (find the existing test asserting `"ok"` is NOT counted — that assertion is now wrong and must be replaced):

```typescript
// web-ui-v2/src/components/live/edges-online-badge.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { EdgesOnlineBadge } from "./edges-online-badge";
import type { EdgeCurrent } from "@/lib/api-client";

const onlineEdge = { status: "online", edge_code: "e1" } as EdgeCurrent;
const okEdge = { status: "ok", edge_code: "e2" } as EdgeCurrent;
const offlineEdge = { status: "disconnected", edge_code: "e3" } as EdgeCurrent;

describe("EdgesOnlineBadge", () => {
  it("counts edges with status 'online' OR 'ok' in the numerator (fixes the always-wrong count)", () => {
    render(<EdgesOnlineBadge edges={[onlineEdge, okEdge, offlineEdge]} />);
    expect(screen.getByText("2/3")).toBeInTheDocument();
  });

  it("shows 0/0 with no edges rather than crashing", () => {
    render(<EdgesOnlineBadge edges={[]} />);
    expect(screen.getByText("0/0")).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- edges-online-badge.test.tsx`
Expected: FAIL (`"2/3"` not found — current code only counts `"online"`, giving `"1/3"`).

- [ ] **Step 4: Fix `EdgesOnlineBadge` to use the shared connectivity model**

Replace `web-ui-v2/src/components/live/edges-online-badge.tsx` entirely:

```typescript
import { Badge } from "@/components/ui/badge";
import type { EdgeCurrent } from "@/lib/api-client";
import { ONLINE_STATUSES } from "@/lib/connectivity";
import { useTranslation } from "react-i18next";

export function EdgesOnlineBadge({ edges }: { edges: EdgeCurrent[] }) {
  const { t } = useTranslation();
  const online = edges.filter((e) => ONLINE_STATUSES.has(String(e.status || "").toLowerCase())).length;
  return (
    <Badge title={t("live.edgesOnline")}>
      {online}/{edges.length}
    </Badge>
  );
}
```

- [ ] **Step 5: Run the badge test again to verify it passes**

Run: `cd web-ui-v2 && npm test -- edges-online-badge.test.tsx`
Expected: PASS.

- [ ] **Step 6: Write the failing tests for the ContextBar selectors and cascade-clear**

Extend `web-ui-v2/src/components/context-bar.test.tsx` (keep the existing Site tests, add these):

```typescript
// web-ui-v2/src/components/context-bar.test.tsx (additions)
import userEvent from "@testing-library/user-event";
import * as apiClient from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";

describe("ContextBar cascade selectors", () => {
  beforeEach(() => {
    useOperationalContextStore.setState({ site: "plant-a", line: "", area: "", cell: "", edge: "" });
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([{ site_code: "plant-a" } as never]);
    vi.spyOn(apiClient, "fetchLines").mockResolvedValue([{ code: "line-main", name: "Line Main" }]);
    vi.spyOn(apiClient, "fetchAreas").mockResolvedValue([{ code: "area-pack", name: "Area Pack" }]);
    vi.spyOn(apiClient, "fetchCells").mockResolvedValue([{ code: "cell-1", name: "Cell 1" }]);
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([{ edge_code: "edge-pack-1" } as never]);
  });

  it("renders Line/Area/Cell/Edge dropdowns populated from the real hierarchy endpoints", async () => {
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    const comboboxes = await screen.findAllByRole("combobox");
    expect(comboboxes.length).toBeGreaterThanOrEqual(5); // site + line + area + cell + edge
  });

  it("clears Area/Cell/Edge when Line changes", async () => {
    useOperationalContextStore.setState({ site: "plant-a", line: "line-main", area: "area-pack", cell: "cell-1", edge: "edge-pack-1" });
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    await screen.findAllByRole("combobox");
    useOperationalContextStore.getState().setLine("line-other");
    await waitFor(() => {
      const state = useOperationalContextStore.getState();
      expect(state.area).toBe("");
      expect(state.cell).toBe("");
      expect(state.edge).toBe("");
    });
  });

  it("shows a clear-filters button when any level below Site is selected, and clears them on click", async () => {
    useOperationalContextStore.setState({ site: "plant-a", line: "line-main", area: "", cell: "", edge: "" });
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <ContextBar />
      </QueryClientProvider>
    );
    const clearButton = await screen.findByRole("button", { name: /limpiar filtros/i });
    await userEvent.click(clearButton);
    await waitFor(() => {
      expect(useOperationalContextStore.getState().line).toBe("");
    });
  });
});
```

Add the necessary imports at the top of the test file if not already present: `import { render, screen, waitFor } from "@testing-library/react";`, `import { QueryClient, QueryClientProvider } from "@tanstack/react-query";`, `import { ContextBar } from "./context-bar";`.

- [ ] **Step 7: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- context-bar.test.tsx`
Expected: FAIL (only one combobox exists today; `fetchLines`/`fetchAreas`/`fetchCells` aren't called; no clear-filters button).

- [ ] **Step 8: Implement the cascade selectors**

Replace `web-ui-v2/src/components/context-bar.tsx` entirely:

```typescript
import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchTagsCurrent, fetchLines, fetchAreas, fetchCells, fetchEdgesCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { useAutoSelectFirst } from "@/lib/use-auto-select-first";
import { useTranslation } from "react-i18next";

export function ContextBar() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge, setSite, setLine, setArea, setCell, setEdge } = useOperationalContextStore();

  // No dedicated "list of sites" endpoint exists (verified against api.rs). Deriving the real,
  // currently-reporting site list from tag data is what actually fixes "Site is fixed text".
  const allTags = useQuery({ queryKey: ["all-sites-probe"], queryFn: () => fetchTagsCurrent(1000) });
  const sites = Array.from(new Set((allTags.data ?? []).map((tg) => tg.site_code))).sort();
  useAutoSelectFirst(sites, site, setSite);

  const linesQuery = useQuery({ queryKey: ["ctxb-lines", site], queryFn: () => fetchLines(site) });
  const areasQuery = useQuery({ queryKey: ["ctxb-areas", site, line], queryFn: () => fetchAreas(site, line || undefined) });
  const cellsQuery = useQuery({ queryKey: ["ctxb-cells", site, line, area], queryFn: () => fetchCells(site, line || undefined, area || undefined) });
  const edgesQuery = useQuery({
    queryKey: ["ctxb-edges", site, line, area, cell],
    queryFn: () => fetchEdgesCurrent(200, { site, line: line || undefined, area: area || undefined, cell: cell || undefined }),
  });
  const edgeOptions = Array.from(new Set((edgesQuery.data ?? []).map((e) => e.edge_code))).sort();

  // web-ui's own ContextBar does NOT clear child selections when a parent changes (e.g.
  // picking a different Line while an Area from the old Line is still selected leaves a
  // stale, now-invalid Area value in the store) -- this fixes that gap rather than porting it.
  useEffect(() => {
    setLine("");
    setArea("");
    setCell("");
    setEdge("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [site]);
  useEffect(() => {
    setArea("");
    setCell("");
    setEdge("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [line]);
  useEffect(() => {
    setCell("");
    setEdge("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [area]);
  useEffect(() => {
    setEdge("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cell]);

  const hasSelection = Boolean(line || area || cell || edge);

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select value={site} onValueChange={setSite} disabled={allTags.isError}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.site")} />
        </SelectTrigger>
        <SelectContent>
          {sites.map((s) => (
            <SelectItem key={s} value={s}>
              {s}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={line} onValueChange={setLine}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.line")} />
        </SelectTrigger>
        <SelectContent>
          {(linesQuery.data ?? []).map((l) => (
            <SelectItem key={l.code} value={l.code}>
              {l.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={area} onValueChange={setArea}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.area")} />
        </SelectTrigger>
        <SelectContent>
          {(areasQuery.data ?? []).map((a) => (
            <SelectItem key={a.code} value={a.code}>
              {a.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={cell} onValueChange={setCell}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.cell")} />
        </SelectTrigger>
        <SelectContent>
          {(cellsQuery.data ?? []).map((c) => (
            <SelectItem key={c.code} value={c.code}>
              {c.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select value={edge} onValueChange={setEdge}>
        <SelectTrigger>
          <SelectValue placeholder={t("live.edge")} />
        </SelectTrigger>
        <SelectContent>
          {edgeOptions.map((e) => (
            <SelectItem key={e} value={e}>
              {e}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {hasSelection && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setLine("");
            setArea("");
            setCell("");
            setEdge("");
          }}
        >
          {t("live.clearFilters")}
        </Button>
      )}
      {allTags.isError && <span className="text-xs text-destructive">{t("live.siteError")}</span>}
    </div>
  );
}
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- context-bar.test.tsx edges-online-badge.test.tsx`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add web-ui-v2/src/components/context-bar.tsx web-ui-v2/src/components/context-bar.test.tsx \
        web-ui-v2/src/components/live/edges-online-badge.tsx web-ui-v2/src/components/live/edges-online-badge.test.tsx \
        web-ui-v2/src/locales/es.ts web-ui-v2/src/locales/en.ts
git commit -m "feat(web-ui-v2): real Line/Area/Cell/Edge filter selectors with cascade-clear, fix edges-online badge"
```

---

### Task 6: Live page — connectivity dashboard

**Files:**
- Modify: `web-ui-v2/src/routes/live.tsx`
- Create: `web-ui-v2/src/components/live/connectivity-dot.tsx`
- Modify: `web-ui-v2/src/locales/es.ts`, `web-ui-v2/src/locales/en.ts`
- Test: `web-ui-v2/src/components/live/connectivity-dot.test.tsx`

**Interfaces:**
- Consumes: `fetchDevicesCurrent` (Task 3), `edgeConnected`/`lampFromDeviceState` (Task 4), `formatServerDateTime` (Task 1), `EdgesOnlineBadge` (Task 5).
- Produces: `ConnectivityDot` component (`{ state: "good" | "warn" | "bad"; title?: string }`), used by the restructured `LivePage`.

- [ ] **Step 1: Add new i18n keys**

Add to `web-ui-v2/src/locales/es.ts`'s `live` block:

```typescript
    devicesCardTitle: "Dispositivos",
    lastSeen: "Última vez visto",
    noDevices: "Sin dispositivos reportando",
```

Add to `web-ui-v2/src/locales/en.ts`'s `live` block:

```typescript
    devicesCardTitle: "Devices",
    lastSeen: "Last seen",
    noDevices: "No devices reporting",
```

- [ ] **Step 2: Write the failing test for `ConnectivityDot`**

```typescript
// web-ui-v2/src/components/live/connectivity-dot.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ConnectivityDot } from "./connectivity-dot";

describe("ConnectivityDot", () => {
  it("renders with the good/warn/bad state as a data attribute for styling", () => {
    render(<ConnectivityDot state="good" />);
    expect(screen.getByTestId("connectivity-dot")).toHaveAttribute("data-state", "good");
  });

  it("passes through a title for the tooltip", () => {
    render(<ConnectivityDot state="bad" title="device_state: disconnected" />);
    expect(screen.getByTestId("connectivity-dot")).toHaveAttribute("title", "device_state: disconnected");
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- connectivity-dot.test.tsx`
Expected: FAIL (module doesn't exist).

- [ ] **Step 4: Implement `ConnectivityDot`**

```typescript
// web-ui-v2/src/components/live/connectivity-dot.tsx
/**
 * The "lamp" from web-ui's Live page (green/amber/red dot) -- a colored indicator, not a
 * Badge with text, since operators scan a dense grid of these at a glance. Styling is via
 * data-state so it can be themed centrally (globals.css) without a new dependency.
 */
export function ConnectivityDot({ state, title }: { state: "good" | "warn" | "bad"; title?: string }) {
  return (
    <span
      data-testid="connectivity-dot"
      data-state={state}
      title={title}
      className="inline-block h-2.5 w-2.5 rounded-full data-[state=good]:bg-emerald-500 data-[state=warn]:bg-amber-500 data-[state=bad]:bg-red-500"
    />
  );
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd web-ui-v2 && npm test -- connectivity-dot.test.tsx`
Expected: PASS.

- [ ] **Step 6: Restructure `live.tsx` into a connectivity dashboard**

Replace `web-ui-v2/src/routes/live.tsx` entirely:

```typescript
import { createFileRoute } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchEdgesCurrent, fetchDevicesCurrent, type EdgeCurrent, type DeviceCurrent } from "@/lib/api-client";
import { useOperationalContextStore } from "@/store/context-store";
import { edgeConnected, lampFromDeviceState } from "@/lib/connectivity";
import { formatServerDateTime } from "@/lib/datetime";
import { ContextBar } from "@/components/context-bar";
import { EdgesOnlineBadge } from "@/components/live/edges-online-badge";
import { EdgeDiagnosticsPanel } from "@/components/live/edge-diagnostics-panel";
import { ConnectivityDot } from "@/components/live/connectivity-dot";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { useTranslation } from "react-i18next";

export const Route = createFileRoute("/live")({
  component: LivePage,
});

type DeviceRow = {
  key: string;
  device: DeviceCurrent;
  edge: EdgeCurrent | undefined;
  lamp: "good" | "warn" | "bad";
};

function buildDeviceRows(devices: DeviceCurrent[], edges: EdgeCurrent[]): DeviceRow[] {
  const edgeByCode = new Map(edges.map((e) => [e.edge_code, e]));
  return devices
    .map((d) => {
      const edge = edgeByCode.get(d.edge_code);
      const conn = edgeConnected(edge);
      return { key: `${d.edge_code}|${d.device_code}`, device: d, edge, lamp: lampFromDeviceState(d, conn) };
    })
    .sort((a, b) => a.device.device_code.localeCompare(b.device.device_code));
}

function LivePage() {
  const { t } = useTranslation();
  const { site, line, area, cell, edge } = useOperationalContextStore();
  const filter = {
    site,
    line: line || undefined,
    area: area || undefined,
    cell: cell || undefined,
    edge: edge || undefined,
  };
  const edgesQuery = useQuery({
    queryKey: ["live-edges", filter],
    queryFn: () => fetchEdgesCurrent(200, filter),
    refetchInterval: 2500,
  });
  const devicesQuery = useQuery({
    queryKey: ["live-devices", filter],
    queryFn: () => fetchDevicesCurrent(1000, filter),
    refetchInterval: 2500,
  });

  const rows = useMemo(
    () => buildDeviceRows(devicesQuery.data ?? [], edgesQuery.data ?? []),
    [devicesQuery.data, edgesQuery.data]
  );

  const [diagnosticsEdge, setDiagnosticsEdge] = useState<{ edgeCode: string; site: string } | null>(null);

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center gap-4">
        <ContextBar />
        <EdgesOnlineBadge edges={edgesQuery.data ?? []} />
      </div>
      <h1 className="text-lg font-semibold">{t("live.title")}</h1>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">{t("live.devicesCardTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1">
          {rows.map((r) => (
            <div
              key={r.key}
              role="button"
              tabIndex={0}
              onClick={() => setDiagnosticsEdge({ edgeCode: r.device.edge_code, site: r.device.site_code })}
              onKeyDown={(ev) => {
                if (ev.key === "Enter" || ev.key === " ") {
                  ev.preventDefault();
                  setDiagnosticsEdge({ edgeCode: r.device.edge_code, site: r.device.site_code });
                }
              }}
              className="flex cursor-pointer items-center gap-3 rounded px-2 py-1 font-mono text-xs hover:bg-accent"
            >
              <ConnectivityDot state={r.lamp} title={`device_state: ${r.device.state || "unknown"}`} />
              <span className="min-w-0 flex-1 truncate">{r.device.device_code}</span>
              <span className="text-muted-foreground">{r.device.edge_code}</span>
              <span className="text-muted-foreground">
                {r.device.last_seen_at ? formatServerDateTime(r.device.last_seen_at) : "-"}
              </span>
            </div>
          ))}
          {rows.length === 0 && <p className="text-sm text-muted-foreground">{t("live.noDevices")}</p>}
        </CardContent>
      </Card>
      {diagnosticsEdge && (
        <EdgeDiagnosticsPanel
          edgeCode={diagnosticsEdge.edgeCode}
          site={diagnosticsEdge.site}
          open={diagnosticsEdge !== null}
          onOpenChange={(nextOpen) => {
            if (!nextOpen) setDiagnosticsEdge(null);
          }}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 7: Commit**

```bash
git add web-ui-v2/src/routes/live.tsx web-ui-v2/src/components/live/connectivity-dot.tsx \
        web-ui-v2/src/components/live/connectivity-dot.test.tsx \
        web-ui-v2/src/locales/es.ts web-ui-v2/src/locales/en.ts
git commit -m "feat(web-ui-v2): restructure Live as a device-connectivity dashboard"
```

---

### Task 7: Live telemetry in the diagnostics panel

**Files:**
- Modify: `web-ui-v2/src/components/live/edge-diagnostics-panel.tsx`
- Modify: `web-ui-v2/src/locales/es.ts`, `web-ui-v2/src/locales/en.ts`
- Test: `web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx` (extend existing)

**Interfaces:**
- Consumes: `fetchTagsCurrent` (existing, `api-client.ts`), `formatServerTime` (Task 1).
- Produces: the panel now also lists the selected edge's tags with value/quality/time.

- [ ] **Step 1: Add new i18n keys**

Add to `web-ui-v2/src/locales/es.ts`'s `live.diagnostics` block (after `noEvents`):

```typescript
      telemetry: "Telemetría",
      noTelemetry: "Sin tags reportando para este edge.",
```

Add to `web-ui-v2/src/locales/en.ts`'s `live.diagnostics` block, matching position:

```typescript
      telemetry: "Telemetry",
      noTelemetry: "No tags reporting for this edge.",
```

- [ ] **Step 2: Write the failing test**

Add to `web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx`:

```typescript
// web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx (addition)
describe("EdgeDiagnosticsPanel telemetry section", () => {
  it("shows the selected edge's tags with value, quality, and formatted time", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([
      {
        tag_code: "tag_m1_t001",
        device_code: "dev-mix-1",
        edge_code: "edge-mix-1",
        site_code: "plant-a",
        ts: "2026-08-26T18:31:13.144564Z",
        value: 12.5,
        quality: { status: "Good" },
      } as never,
    ]);
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-mix-1" site="plant-a" open onOpenChange={() => {}} />);
    expect(await screen.findByText("tag_m1_t001")).toBeInTheDocument();
    expect(screen.getByText("12.5")).toBeInTheDocument();
    expect(screen.getByText("Good")).toBeInTheDocument();
    expect(screen.getByText(/14:31/)).toBeInTheDocument(); // formatServerTime, America/La_Paz = UTC-4
  });

  it("shows the no-telemetry message when the edge has no tags", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([]);
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    render(<EdgeDiagnosticsPanel edgeCode="edge-mix-1" site="plant-a" open onOpenChange={() => {}} />);
    expect(await screen.findByText(/sin tags reportando/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- edge-diagnostics-panel.test.tsx`
Expected: FAIL (no telemetry section exists yet).

- [ ] **Step 4: Implement the telemetry section**

Modify `web-ui-v2/src/components/live/edge-diagnostics-panel.tsx`:

Add to the imports:
```typescript
import { fetchTagsCurrent, type TagCurrent } from "@/lib/api-client";
import { formatServerTime } from "@/lib/datetime";
```

Add state (alongside the existing `events`/`eventsError` state):
```typescript
  const [tags, setTags] = useState<TagCurrent[]>([]);
```

Add a new effect (alongside the existing events-fetching effect), fetching this edge's current tags on open and refreshing every 2.5s while the panel stays open (matching the rest of the app's poll interval — this panel does not yet consume the SSE selected-item pipeline, that's Task 8):

```typescript
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const load = () => {
      fetchTagsCurrent(200, { edge: edgeCode }).then((data) => {
        if (!cancelled) setTags(data);
      }).catch(() => {
        // Telemetry fetch failures don't block the rest of the panel (reset, events) --
        // an empty list just falls through to the "no telemetry" empty state.
      });
    };
    load();
    const interval = setInterval(load, 2500);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [open, edgeCode]);
```

Add the telemetry section to the JSX, after the reset feedback block and before the `<h3>{t("live.diagnostics.recentEvents")}</h3>` heading:

```typescript
          <h3 className="mt-2 text-sm font-medium">{t("live.diagnostics.telemetry")}</h3>
          {tags.length === 0 && (
            <p className="text-sm text-muted-foreground">{t("live.diagnostics.noTelemetry")}</p>
          )}
          <ul className="flex flex-col gap-1 text-xs">
            {tags.map((tg) => (
              <li key={tg.tag_code} className="flex items-center justify-between gap-2 border-b py-1 font-mono">
                <span className="truncate">{tg.tag_code}</span>
                <span>{String(tg.value)}</span>
                <span className="text-muted-foreground">{tg.quality?.status ?? "-"}</span>
                <span className="text-muted-foreground">{tg.ts ? formatServerTime(tg.ts) : "-"}</span>
              </li>
            ))}
          </ul>
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- edge-diagnostics-panel.test.tsx`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web-ui-v2/src/components/live/edge-diagnostics-panel.tsx web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx \
        web-ui-v2/src/locales/es.ts web-ui-v2/src/locales/en.ts
git commit -m "feat(web-ui-v2): live telemetry section in the edge diagnostics panel"
```

---

### Task 8: Wire SSE into the Live grid and diagnostics panel

**Files:**
- Modify: `web-ui-v2/src/routes/live.tsx`
- Modify: `web-ui-v2/src/components/live/edge-diagnostics-panel.tsx`
- Test: extend `web-ui-v2/src/routes/live.test.tsx` (create if it doesn't exist) and `edge-diagnostics-panel.test.tsx`

**Interfaces:**
- Consumes: `subscribeSse`, `RtEvent` (Task 2).
- Produces: both the Live grid and the diagnostics panel's telemetry list now patch from SSE events between polls, without SSE ever being the sole data source (poll stays authoritative, per the spec's §1).

- [ ] **Step 1: Write the failing test for the Live page's SSE grid patch**

```typescript
// web-ui-v2/src/routes/live.test.tsx
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as apiClient from "@/lib/api-client";
import * as sse from "@/lib/sse";
import "../lib/i18n";

// live.tsx is a route component (createFileRoute) -- test the underlying LivePage logic via a
// minimal standalone render matching the pattern already used for other route-adjacent tests
// in this codebase (see app-shell.test.tsx's router-free RouterProvider setup) is unnecessary
// here since LivePage itself has no router-specific behavior; import and render it directly.
import { Route } from "./live";

describe("Live page SSE patching", () => {
  let sseHandler: ((evt: sse.RtEvent) => void) | undefined;

  beforeEach(() => {
    vi.spyOn(apiClient, "fetchEdgesCurrent").mockResolvedValue([
      { edge_code: "edge-mix-1", site_code: "plant-a", status: "online", last_seen_at: new Date().toISOString() } as never,
    ]);
    vi.spyOn(apiClient, "fetchDevicesCurrent").mockResolvedValue([
      { edge_code: "edge-mix-1", device_code: "dev-mix-1", site_code: "plant-a", state: "connected" } as never,
    ]);
    vi.spyOn(sse, "subscribeSse").mockImplementation((onMessage) => {
      sseHandler = onMessage;
      return () => {};
    });
  });

  it("subscribes to SSE on mount alongside the existing poll", async () => {
    const qc = new QueryClient();
    const LivePageComponent = Route.options.component!;
    render(
      <QueryClientProvider client={qc}>
        <LivePageComponent />
      </QueryClientProvider>
    );
    await screen.findByText("dev-mix-1");
    expect(sse.subscribeSse).toHaveBeenCalled();
    expect(sseHandler).toBeTypeOf("function");
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- live.test.tsx`
Expected: FAIL (`subscribeSse` is never called — `live.tsx` doesn't import `lib/sse` yet).

- [ ] **Step 3: Wire SSE into `live.tsx`**

Add to the imports in `web-ui-v2/src/routes/live.tsx`:
```typescript
import { useEffect, useRef } from "react";
import { subscribeSse, type RtEvent } from "@/lib/sse";
import { useQueryClient } from "@tanstack/react-query";
```

(`useMemo`/`useState` are already imported — extend that import line rather than duplicating it.)

Inside `LivePage`, after the `devicesQuery` declaration, add the SSE subscription. This patches the already-fetched query caches directly (via `queryClient.setQueryData`) rather than maintaining a second, parallel copy of the data — the poll (`refetchInterval: 2500`) remains the periodic source of truth that this patch sits on top of and that self-heals any drift:

```typescript
  const queryClient = useQueryClient();
  const pendingRef = useRef<Map<string, RtEvent>>(new Map());

  useEffect(() => {
    const unsubscribe = subscribeSse(
      (evt) => {
        const payload = evt.payload as { tag_id?: string; device_id?: string } | undefined;
        const key = payload?.device_id ?? payload?.tag_id;
        if (!key) return;
        pendingRef.current.set(key, evt);
      },
      { site, line: line || undefined, area: area || undefined, cell: cell || undefined, edge: edge || undefined, excludeRaw: true }
    );
    const flush = setInterval(() => {
      if (pendingRef.current.size === 0) return;
      pendingRef.current.clear();
      // A real-time nudge: invalidate so the next poll tick (already running every 2.5s) fires
      // sooner instead of waiting out the full interval. This deliberately does NOT hand-patch
      // individual device/edge objects in the cache -- reusing the same fetchDevicesCurrent/
      // fetchEdgesCurrent path that already normalizes and shapes this data keeps there being
      // exactly one code path that produces what the grid renders, matching the spec's "poll
      // stays authoritative" decision instead of maintaining a second, divergence-prone copy.
      queryClient.invalidateQueries({ queryKey: ["live-edges", filter] });
      queryClient.invalidateQueries({ queryKey: ["live-devices", filter] });
    }, 120);
    return () => {
      clearInterval(flush);
      unsubscribe();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [site, line, area, cell, edge]);
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web-ui-v2 && npm test -- live.test.tsx`
Expected: PASS.

- [ ] **Step 5: Write the failing test for the diagnostics panel's SSE-driven telemetry**

Add to `web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx`:

```typescript
describe("EdgeDiagnosticsPanel SSE telemetry patch", () => {
  it("subscribes to SSE scoped to the selected edge while open", async () => {
    vi.spyOn(apiClient, "fetchTagsCurrent").mockResolvedValue([]);
    vi.spyOn(apiClient, "fetchEdgeEvents").mockResolvedValue([]);
    const subscribeSpy = vi.spyOn(sse, "subscribeSse").mockImplementation(() => () => {});
    render(<EdgeDiagnosticsPanel edgeCode="edge-mix-1" site="plant-a" open onOpenChange={() => {}} />);
    await waitFor(() => {
      expect(subscribeSpy).toHaveBeenCalledWith(expect.any(Function), expect.objectContaining({ edge: "edge-mix-1" }));
    });
  });

  it("does not subscribe to SSE when the panel is closed", () => {
    const subscribeSpy = vi.spyOn(sse, "subscribeSse").mockImplementation(() => () => {});
    render(<EdgeDiagnosticsPanel edgeCode="edge-mix-1" site="plant-a" open={false} onOpenChange={() => {}} />);
    expect(subscribeSpy).not.toHaveBeenCalled();
  });
});
```

Add `import * as sse from "@/lib/sse";` to the test file's imports if not already present.

- [ ] **Step 6: Run it to verify it fails**

Run: `cd web-ui-v2 && npm test -- edge-diagnostics-panel.test.tsx`
Expected: FAIL (panel doesn't call `subscribeSse` yet).

- [ ] **Step 7: Wire SSE into the diagnostics panel's telemetry effect**

Modify the telemetry-fetching effect added in Task 7 to also subscribe to SSE while open, nudging the same `load()` poll sooner on an incoming event for this edge (same "poll stays authoritative, SSE just accelerates it" pattern as Task 8 Step 3 — no separate SSE-only data path):

Add to the imports:
```typescript
import { subscribeSse } from "@/lib/sse";
```

Replace the telemetry effect from Task 7 with:

```typescript
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const load = () => {
      fetchTagsCurrent(200, { edge: edgeCode }).then((data) => {
        if (!cancelled) setTags(data);
      }).catch(() => {
        // Telemetry fetch failures don't block the rest of the panel (reset, events) --
        // an empty list just falls through to the "no telemetry" empty state.
      });
    };
    load();
    const interval = setInterval(load, 2500);
    const unsubscribeSse = subscribeSse(() => load(), { edge: edgeCode, excludeRaw: true });
    return () => {
      cancelled = true;
      clearInterval(interval);
      unsubscribeSse();
    };
  }, [open, edgeCode]);
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd web-ui-v2 && npm test -- edge-diagnostics-panel.test.tsx`
Expected: PASS.

- [ ] **Step 9: Run the full test suite to confirm no regressions**

Run: `cd web-ui-v2 && npm test`
Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add web-ui-v2/src/routes/live.tsx web-ui-v2/src/routes/live.test.tsx \
        web-ui-v2/src/components/live/edge-diagnostics-panel.tsx web-ui-v2/src/components/live/edge-diagnostics-panel.test.tsx
git commit -m "feat(web-ui-v2): layer SSE-driven low-latency updates on top of the existing poll"
```

---

### Task 9: Live verification against the real local stack

**Files:** None (this task verifies Tasks 1-8's combined output; no new code).

- [ ] **Step 1: Ensure the real local stack is up**

```bash
cd D:/ifascada/.claude/worktrees/live-page-connectivity-redesign
docker compose -p ifascada -f docker-compose.scada.yml --profile central --profile seed up -d
docker compose -p ifascada -f docker-compose.edge-sim.yml up -d
```

Confirm `curl http://127.0.0.1:8088/health/live` returns `{"status":"ok"}` before proceeding.

- [ ] **Step 2: Run `web-ui-v2` in dev mode**

```bash
cd web-ui-v2
npm run dev
```

- [ ] **Step 3: Verify with Playwright against `http://127.0.0.1:3002/live`**

Navigate to the page and confirm, with a snapshot/screenshot at each point:
- Line/Area/Cell/Edge selectors are present and populated with real values (not just Site).
- Selecting a Line narrows Area's options; changing Line again after Area/Cell/Edge were set clears them (cascade-clear from Task 5).
- The edges-online badge shows a nonzero count when real edge-sim containers are healthy (the badge-bug fix from Task 4/5) — cross-check against `docker ps --filter name=ifascada-edge-sim` and `curl http://127.0.0.1:8088/api/edges/current` directly.
- The device list shows connectivity dots (green for connected devices on connected edges) and a `last_seen_at` time in `America/La_Paz` (compare against the raw UTC timestamp from the API: the displayed time must be exactly 4 hours earlier, e.g. `18:31` UTC → `14:31` on screen).
- Clicking a device row opens the diagnostics panel, showing that edge's live telemetry (tag/value/quality/time) alongside the existing events list and reset button.
- Disconnect one edge-sim container (`docker network disconnect ifascada_default ifascada-edge-sim-pack-1`, same technique as this session's earlier MQTT investigation), confirm its device rows go red/amber within the 45s staleness window without a page reload, then reconnect it and confirm recovery is reflected too.

- [ ] **Step 4: Confirm no regression in History or the existing diagnostics panel's reset flow**

Navigate to `http://127.0.0.1:3002/history`, confirm it still loads and functions (this plan doesn't touch it). Trigger a reset from the diagnostics panel on a real disconnected edge, confirm the existing "command sent" → "confirmed recovered" / "no recovery" flow (Task 14 of the earlier plan) still works exactly as before, now alongside the new telemetry section.

- [ ] **Step 5: Stop the dev server and leave the stack in whatever state Task 17 of the original plan left it in** (running, healthy — don't tear it down unless it was down before this task started).

- [ ] **Step 6: Commit** (only if Step 3/4's verification surfaced a fix needed — otherwise this task has no commit of its own, it's a verification gate)

```bash
git log --oneline -10   # confirm the 8 prior tasks' commits are all present and this task added nothing further
```

---
