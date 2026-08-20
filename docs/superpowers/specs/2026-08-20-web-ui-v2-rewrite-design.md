# web-ui-v2: Ground-Up Rewrite of the Operator Frontend — Design

**Status:** Approved by user in chat, pending written spec review.
**Author:** masbun + Claude, from a live debugging/brainstorming session on 2026-08-20.

## Motivation

The current `web-ui` (Next.js App Router) has drifted from how the system is actually
used. In the user's own words, page by page:

| Page | Real usage today |
|---|---|
| `/` (Overview) | "No muestra nada interesante" — too generic to be useful |
| `/trends` | "NO SE USA" |
| `/trends-accum` | "SE USA MENOS" |
| `/history` | Used, but the filtering process needs to be better |
| `/commands` | "No se para que sirve y realmente no lo uso" |
| `/audit` | Not used — doesn't show the info needed |
| `/live` | Used, "el que mejor funciona junto con history, pero podría ser mucho mejor" |

The real daily workflow is **weighing and printing tickets**, and only `Live` and
`History` are actually load-bearing for it. Everything else is legacy surface area
that accumulated without a clear purpose.

On top of the page-level triage, three concrete defects were found and are already
fixed on `main` as of this spec (see "Already fixed" below) or are in scope for this
rewrite (see "In scope").

## Already fixed (separate PRs, landed before this spec)

These were found and fixed live during the same session, verified against a real
local stack (central-server + MQTT edge simulators). Not part of this rewrite's
implementation — just recorded here so the rewrite doesn't have to re-discover them:

- **History cross-page selection reset**: selection was keyed by on-page row index and
  wiped by a `useEffect` on every page change. Fixed by keying selection by an
  absolute index into the fully-fetched (client-paginated) row set. (PR #19)
- **History had no date-range filter at all** despite production's now-superseded
  `web-ui:1.0.2` having one (from code that was never merged to this repo — see
  "Orphaned prior work" below). Added a client-side date filter that never touches
  the Tag selector. (PR #19)
- **`selectedTag` defaulted to a nonexistent placeholder** (`"tag_hr_0"`), which made
  History/Trends/Accum Trends look like a tag was selected while no data loaded,
  until the user manually reselected. Fixed with a shared `useAutoSelectFirstTag`
  hook. (PR #20)
- **`web-ui:1.0.3`'s API proxy was broken in production** (`CENTRAL_API_UPSTREAM` was
  a build-time-baked Next.js `rewrites()` target, and the Dockerfile never set it at
  build time, so every `/api/*` call hit the container's own loopback). Fixed by
  passing it as a Docker build arg. This is Next.js-specific behavior that the
  Vite rewrite sidesteps entirely (see "Reverse proxy" below).

## Orphaned prior work — explicitly not reused

A `git stash` (`stash@{0}`, 2026-08-04) and an unmerged branch
(`codex/security-action-executions`) both contain a more complete, real version of
date filtering, a `Prints` page, and backend security/action-execution work that
predates and exceeds what's in this repo's `main`. The user explicitly decided **not**
to recover either — both are left untouched and orphaned. This rewrite starts clean
from current `main`'s domain/API surface, not from that old work.

## Scope

### Pages kept and rebuilt

- **Live** — real-time connection/telemetry view. In addition to a visual rebuild,
  this needs a **root-cause investigation** (not just a UI fix) into the reported
  inconsistency: tags shown as "connected" that have no working telemetry. This is
  suspected to share a root cause with the already-documented
  `docs/finding-mqtt-client-stale-session-detection.md` (central-server's MQTT
  consumer client believing a session is alive after the broker has effectively
  dropped it). Investigate with `superpowers:systematic-debugging` before assuming a
  frontend-only fix.
- **History** — the weighing/printing workflow. Rebuilt on the new `DataTable` system
  (see below) with:
  - Value's unit displayed alongside the number (currently missing; often the most
    important part of the value).
  - A numeric filter, `Value > x` (default `x = 0`, since that's the common real
    query — "positive weights only").
  - **Removed** `tag_code`/`site_code`/`edge_code` columns — the view is always
    pre-scoped to exactly one tag/device by the existing Tag selector, so those
    columns are constant and add no information.
  - Multi-row selection via **shift-click range select** (sequential), not just
    individual checkboxes, for selecting a contiguous range of weights to print
    together.

### New capability

- **Edge diagnostics panel**, reachable from Live (e.g. clicking a disconnected/
  stale edge): recent logs/events for that edge, and a **working** reset action with
  real success/failure feedback. Today "reset" nominally exists in Commands but the
  user doesn't know if it actually works — that's the gap being closed. Requires
  checking what `central-server`/`edge-agent` actually expose today for edge event
  history and reset before designing the exact API shape (implementation-plan-level
  detail, not decided in this spec).

### Removed entirely

`Overview`, `Trends`, `Trends-accum`, `Commands`, `Audit` — confirmed unused or
unclear-purpose by the user. Not hidden, not soft-deprecated: removed from this new
app's route tree. (The old `web-ui` keeps them as-is until it's retired — see
"Rollout".)

### Cross-cutting fixes

- **"edges online 0/n" always reads 0** in the shared header — not populated
  correctly. Root-cause before fixing (Phase 1 of systematic-debugging): is the
  numerator query wrong, or is the underlying edge-online domain state itself never
  set? This may be related to the same MQTT staleness issue affecting Live.
- **Site is a fixed text field**; there is a real list of sites in the domain model
  (context hierarchy) — replace with a proper dropdown, consistent with how
  Line/Area/Cell/Edge already work.

### Out of scope for this spec (explicitly)

- Implementing real authentication. See "Auth" below — this spec only leaves room
  for it.
- Recovering anything from the orphaned stash/branch.
- Changing `central-server`'s domain model beyond whatever the Live/edges-online
  root-cause investigation requires.
- Retiring the current `web-ui` — that's a separate future decision (see "Rollout").

## Architecture

### Parallel rollout, not an in-place rewrite

The new app is a **second, independent frontend** — codename `web-ui-v2` — built and
deployed alongside the current `web-ui`, not replacing it yet:

- New service in the compose stack, its own container/port (**3002**, vs the
  existing `3001`), sharing the same `central-server` backend. No conflict — both
  are just HTTP/SSE clients of the same API.
- Own CI/CD workflow, modeled directly on the existing `.github/workflows/web-ui.yml`
  pattern (own Dockerfile, own tag prefix e.g. `webuiv2-v*`, same `production`
  environment gate).
- Zero risk to real operators during development: nothing here is exposed to them
  until the cutover is explicitly decided later. The cutover itself (pointing
  operators at :3002 and retiring the old container) is a follow-up decision, not
  part of this spec.
- This also lets the framework migration (Next.js → Vite) and the content rewrite
  (which pages exist, what they show) be validated together without threatening the
  production system that real work depends on today.

### Why Vite + TanStack Router instead of Next.js

Verified in the current codebase before deciding this: every single page in `web-ui`
is `"use client"` (zero Server Components), there is no `next/image`/`next/font`
usage, and the only API route is a trivial `/api/health`. Next.js's only real
contribution today is (a) file-based routing and (b) the `next.config.mjs`
`rewrites()` proxy to `central-server` — neither requires Next.js specifically, and
the proxy is exactly what caused the `CENTRAL_API_UPSTREAM` production incident
(a value baked at Docker build time because Next.js's rewrites are resolved at
`next build`, not at container start).

Moving to Vite + TanStack Router:
- Matches the stack the user already runs successfully in bigger projects (`ifahub`'s
  `apps/ifa-web`), and is a prerequisite for reusing `@ifahub/ui`/`@ifahub/tables`
  without adapting them to a different router paradigm.
- Replaces the Next.js proxy with an **nginx `proxy_pass` + `envsubst` template**,
  resolved at container *start* time rather than baked into the image at build time —
  structurally immune to the exact bug class that caused the incident.
- The `/api/health` endpoint moves from a Next.js route handler to a plain nginx/
  container-level healthcheck (simpler, not a regression).
- Real-time (`EventSource`/SSE) is a browser API, not a Next.js feature — verified
  `lib/sse.ts` never touches a Next.js server capability, so nothing here is lost.

### Component strategy: vendor from ifahub, don't add it as a live dependency

`ifahub` (`D:\ifaplatform\ifahub`, `github.com/ifamasbun/ifahub`) has two things worth
reusing, both React-19/Tailwind-v4 based:

- **`libs/ui`** (`@ifahub/ui`) — generic shadcn-style primitives (table, sidebar,
  select, form, sheet, dialog, command, badge, tabs, etc.). No coupling to ifahub's
  business logic; depends only on Radix/Tailwind/`class-variance-authority`.
- **`libs/tables`** (`@ifahub/tables`) — a full `DataTable` system: `DataTable`,
  `DataTableToolbar`, `DataTableSearch`, `DataTableColumnsDialog`,
  `DataTablePagination`, `DataTableSavedViews`, loading/empty/error states, built on
  `@tanstack/react-table`. This is the "potente componente de tablas que permite
  filtro" the user asked to reuse — it is exactly the right building block for
  History's rebuild (search, column filters, saved views, pagination all included).
  One file, `useTableLayouts.ts`, imports `useCan` from `@ifahub/auth` for
  permission-gated saved views; everything else in the table system has zero auth
  coupling.

These are copied into this repo's new frontend (vendored, adapted) rather than
consumed as a live cross-repo dependency — this is the standard usage model for
shadcn-style component sources (you own the code), and it avoids coupling this
repo's release cadence to ifahub's.

### Auth: door left open, not implemented

`ifahub` uses OIDC against a self-hosted **Authentik** instance (`@ifahub/auth`, via
`react-oidc-context`/`oidc-client-ts`) — confirmed neither `web-ui` nor
`central-server` has any user-authentication today (only an edge-enrollment token,
unrelated to human operators). This rewrite does **not** implement login. It leaves
room for it consistently with ifahub's own pattern, so that adding real auth later
is additive, not a rearchitecture:

- `useTableLayouts.ts` (vendored from `@ifahub/tables`) keeps its `useCan` call site,
  backed for now by a no-op stub (`useCan` always returns `true`) instead of being
  stripped out.
- The API client has a single call site where an `Authorization` header would be
  injected later (empty/no-op today).
- Route tree structure allows a `beforeLoad` guard to be added per-route later
  without restructuring.

### i18n

`web-ui` today has zero i18n — UI labels are hardcoded English strings while the real
business data (automation names, etc.) is Spanish. `ifahub` already solved this
cleanly: `i18next` + `react-i18next`, initialized once
(`apps/ifa-web/src/lib/i18n.ts` is the reference), with Spanish as the default and
fallback language, and per-package dictionaries (`libs/ui/src/locales/es.ts`,
`libs/tables/src/locales/es.ts`) merged into the app's own resource bundle at init.

`web-ui-v2` adopts the same pattern verbatim: one `src/lib/i18n.ts` bootstrap module,
`lng: "es"`/`fallbackLng: "es"`, importing the vendored `@ifahub/ui`/`@ifahub/tables`
dictionaries plus a new dictionary for ifascada-specific strings (Live, History,
diagnostics panel). English can be added later the same way ifahub structures it
(a second `resources.en` entry) without any redesign.

## Testing / verification

No React component test harness exists in this project today (confirmed: no Jest/
Vitest/RTL config). Given the scale of this rewrite, the implementation plan should
weigh whether to introduce one now (`ifahub`'s `ifa-web` already uses Playwright for
e2e — `apps/ifa-web`'s `e2e`/`e2e:l0` scripts are a directly reusable reference) rather
than relying solely on manual Playwright-driven verification against the local stack,
as was done for the smaller fixes earlier in this session. This is a decision for the
implementation plan, not fixed here.

## Rollout

1. Build `web-ui-v2` against the local dev stack already running (central-server +
   MQTT edge simulators, `docker-compose.scada.yml` + `docker-compose.edge-sim.yml`).
2. PR review per page/slice as it's built (following this session's established
   pattern of PR-based review for product work, direct push only for infra
   emergencies).
3. Deploy to `.154` on port 3002 via its own CI/CD pipeline, verified independently
   (health check + real API calls + screenshot), without touching the existing
   `web-ui:3001` container or its traffic.
4. Cutover (pointing real operators at :3002, retiring the old container) is a
   separate, explicit future decision — not scheduled as part of this spec.
