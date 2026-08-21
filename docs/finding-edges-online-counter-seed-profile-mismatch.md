# Finding: "Edges Online 0/n" Reads 0 Because Actively-Reporting Edges Are Invisible to `edge_current_state`

**Status:** Open — root-caused, not yet fixed. Task 9/11 of `docs/superpowers/plans/2026-08-20-web-ui-v2-rewrite.md` may fix the `central-server` side per that plan's Global Constraints ("No `central-server` domain-model changes beyond what a root-caused bug fix in Tasks 9/11 actually requires").
**Affects:** `central-server` (`crates/central-server/src/persistence/postgres.rs` — `insert_telemetry`/`insert_health`/`resolve_tag_ref`/`resolve_edge_id`), the local dev stack's seed tooling (`docker/db-seed.sh`, `crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql`), and any frontend that trusts `/api/edges/current`'s row count as the denominator for "edges online" (`web-ui`'s `context-bar.tsx` today; `web-ui-v2`'s equivalent, planned in Task 9).
**Discovered:** 2026-08-21, investigating Task 8 of the `web-ui-v2` rewrite plan ("edges online 0/n" always reads 0) against the real local stack (`docker-compose.scada.yml` profiles `central`+`seed`, plus `docker-compose.edge-sim.yml`, project name `ifascada`).

## Summary

`GET /api/edges/current` returned 9 rows, **all** `status: "disconnected"` — but the 4 edge-sim containers that are actually running and actively publishing telemetry and heartbeats right now (`edge-mix-1`, `edge-mix-2`, `edge-pack-1`, `edge-pack-2`) **do not appear in that response at all**, not even as "disconnected". The 9 rows that do appear are unrelated leftover/stale test edges, none of which are currently running.

Root cause: the local stack's one-shot `db-seed` container was run with `SEED_PROFILE=minimal` (the compose default), which seeds only `edge-com-01`, `edge-modbus-01`, `edge-sim-01` into the `edges`/`devices`/`tags` tables (migration `0015_dev_seed_minimal_three_edges.sql`). It never runs `0008_dev_seed_sim20_multi_area.sql`, the migration that provisions `edge-mix-1`, `edge-mix-2`, `edge-pack-1`, `edge-pack-2` and their devices/tags — that migration is only included in the `sim20` and `full` seed profiles. Meanwhile `docker-compose.edge-sim.yml` unconditionally starts exactly those 4 sim containers, with no coupling to (or check of) which `SEED_PROFILE` the database was actually seeded with.

Because those 4 edge_codes have no row in the `edges` table at all, every telemetry and health message they publish fails `central-server`'s DB-lookup join (`resolve_tag_ref` / `resolve_edge_id`, both `site.code + edge_code` lookups against the `edges` table) and falls into a "mapping not found" branch that logs a WARN and returns early **without ever touching `edge_current_state`**. There is no code path that creates a placeholder/"unknown" row for an edge_code the DB doesn't recognize — the edge is simply invisible to `/api/edges/current`, with no error surfaced anywhere an operator or the frontend would see it (the WARN is `RUST_LOG`-gated and easy to miss).

The frontend's counter logic itself is correct and not part of this bug (see "Frontend check" below) — it is faithfully reporting `0/9`, i.e. zero of the nine known edges are online. The bug is that the four edges that are actually online were never known to the database in the first place.

## Evidence

**1. `/api/edges/current` (reproduced 2026-08-21, ~12:52 UTC):**
9 rows, all `"status":"disconnected"`. `edge_code`s: `edge-com-01`, `rtedge1772416927`, `dp-edge-1772416506084129700`, `dc-edge-1772416506037210800`, `dt-edge-1772416506084129700`, and 4 more March-2026-dated leftover test edges. No row for `edge-mix-1`/`edge-mix-2`/`edge-pack-1`/`edge-pack-2`.

**2. Running containers (`docker ps`):** `ifascada-edge-sim-mix-1`, `-mix-2`, `-pack-1`, `-pack-2` all `Up`, `EDGE_AGENT` env = `edge-mix-1`/`edge-mix-2`/`edge-pack-1`/`edge-pack-2`, `EDGE_SITE=plant-a`. No container for `edge-com-01` or any of the other 9 DB-known edges is running.

**3. `central-server` logs — every telemetry message for the 4 sim edges hits the unmapped branch** (`docker logs ifascada-central-server`):

```
DEBUG central_server::ingestion: ingest parsed topic='scada/plant-a/edge/edge-mix-2/telemetry/tag/tag_m2_t002' kind=TelemetryTag ...
WARN  central_server::persistence::postgres: telemetry mapping not found for site='plant-a' edge='edge-mix-2' tag='tag_m2_t002'; only raw ingest event persisted
DEBUG central_server::ingestion: ingest telemetry persisted site='plant-a' agent='edge-mix-2' tag='tag_m2_t002' ts=...
```

Every single telemetry line for `edge-mix-1`, `edge-mix-2`, `edge-pack-1`, `edge-pack-2` in the log is immediately preceded by a matching `telemetry mapping not found` WARN. `resolve_tag_ref` (`crates/central-server/src/persistence/postgres.rs:1680`) requires `sites JOIN edges JOIN devices JOIN tags` all present for `(site.code, edge.edge_code, tag.tag_code)`; if it returns `None`, `insert_telemetry` (postgres.rs:509) skips the `edge_current_state` upsert (postgres.rs:642-652) entirely and only writes the raw `telemetry_ingest_events` row.

**4. `central-server` also receives correct heartbeats from all 4 sim edges, and they hit the same wall:**

```
mqtt incoming publish edge-mix-1 health/runtime   — 463 messages (and same count for mix-2/pack-1/pack-2)
mqtt incoming publish edge-mix-1 conn/state       — 3 messages (startup)  (same for mix-2/pack-1/pack-2)
```

```
WARN central_server::persistence::postgres: health mapping not found for site='plant-a' edge='edge-mix-1'; event stored without edge_id
DEBUG central_server::ingestion: ingest health persisted site='plant-a' agent='edge-mix-1' status='ok' ts=...
```

`resolve_edge_id` (postgres.rs:1548, needs only `sites JOIN edges`) also returns `None` for these 4 edge_codes, so `insert_health` (postgres.rs:665) stores the raw `edge_health_events` row with `edge_id = NULL` and — critically — **never reaches the `edge_current_state` upsert** (postgres.rs:681-700), because that block is gated on `if let Some(edge_id) = edge_id`. The heartbeats are proof the edge-agent side is working exactly as designed (`status: "ok"` every ~30s); the failure is entirely on central-server's inability to associate them with a known edge.

**5. Direct DB confirmation** (`docker exec ifascada-timescaledb psql -U postgres -d rustscada`):

```
SELECT s.code AS site, e.edge_code, e.id FROM edges e JOIN sites s ON s.id=e.site_id;
```
→ 11 rows total. Under `site='plant-a'`: only `edge-com-01`, `edge-modbus-01`, `edge-sim-01`. **No row for `edge-mix-1`, `edge-mix-2`, `edge-pack-1`, or `edge-pack-2` exists in the `edges` table at all.**

```
SELECT edge_id, status, count(*) FROM edge_health_events GROUP BY edge_id, status;
```
→ `edge_id=NULL, status='ok', count=1888` (the 4 sim edges' heartbeats, homeless) vs. `edge_id=1 ('edge-com-01'), status='ok', count=1264`.

```
SELECT edge_code, count(*) FROM telemetry_ingest_events GROUP BY edge_code;
```
→ `edge-pack-1: 97820, edge-pack-2: 93875, edge-mix-2: 93176, edge-mix-1: 85620, edge-com-01: 38`. Massive, continuously growing telemetry volume for the 4 "invisible" edges vs. a stale trickle (38, from Aug 20) for the one DB-known `plant-a` edge that's actually running a real container right now — none.

**6. Seed-profile mismatch, the actual root cause:**

`docker-compose.scada.yml`'s `db-seed` service:
```yaml
environment:
  SEED_PROFILE: ${SEED_PROFILE:-minimal}
```

`docker/db-seed.sh`:
```sh
case "${SEED_PROFILE}" in
  minimal)
    seed_files="/migrations/0015_dev_seed_minimal_three_edges.sql /migrations/0017_printer_device_command_and_negative_trigger.sql"
    ;;
  sim20)
    seed_files="/migrations/0004_dev_seed_minimal_catalog.sql /migrations/0007_dev_seed_context_hierarchy.sql /migrations/0008_dev_seed_sim20_multi_area.sql"
    ;;
  full)
    seed_files="/migrations/0004_dev_seed_minimal_catalog.sql /migrations/0007_dev_seed_context_hierarchy.sql /migrations/0008_dev_seed_sim20_multi_area.sql /migrations/0013_scale_manual_config_in_catalog.sql /migrations/0014_dev_seed_modbus_rtu_com10_multi_slave.sql /migrations/0017_printer_device_command_and_negative_trigger.sql"
    ;;
esac
```

`crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql` is exactly the migration that `INSERT`s `edge-pack-1`, `edge-pack-2`, `edge-mix-1`, `edge-mix-2` (site `plant-a`), one `Simulator` device each (`dev-pack-1/2`, `dev-mix-1/2`), and 5 tags each named `tag_p1_t001..005` / `tag_m1_t001..005` etc. — which line up exactly with the `tag_ids` in `crates/edge-agent/config/bootstrap.sim.edge-mix-1.json` (`tag_m1_t001`..`tag_m1_t005`) and the sibling bootstrap files for mix-2/pack-1/pack-2. This migration was clearly written for, and matches, `docker-compose.edge-sim.yml`'s 4 sim containers — it just was never applied in this run, because `SEED_PROFILE` defaulted to `minimal` rather than `sim20`/`full`. Confirmed directly: `docker inspect ifascada-db-seed --format '{{.Config.Env}}'` shows `SEED_PROFILE=minimal` was the value actually used for this stack.

`_sqlx_migrations` exists but has 0 rows — migrations here are applied via `db-seed.sh`'s plain `psql -f` loop, not `sqlx migrate`, so there is no tracking of which of the numbered files have run; whether `0008` ran is entirely a function of which `SEED_PROFILE` string was passed at `docker compose up`, with no automated check that the profile matches whichever edge-sim/edge-agent containers are also running.

**7. `edge-com-01`'s "disconnected" status is unrelated and correct**, not the same bug: it *is* a properly-provisioned `plant-a` edge (id `1`, has a real `devices`/`tags` mapping — `dev_scale_manual_1` / `tag_scale_manual_compound`), and its `edge_current_state.last_seen_at` is `2026-08-20 18:40:33+00`, roughly 18+ hours before this reproduction (2026-08-21 ~12:52 UTC) — vastly past the `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT` default of 45 seconds (`crates/central-server/src/api.rs:310-316`). No `edge-com-01` container is running in the current stack (`docker ps` confirms). Its "disconnected" status is the staleness logic (`crates/central-server/src/api.rs:374-381`, `crates/domain/src/device/status.rs:67-78`) working exactly as intended on a genuinely-stopped edge. The staleness/`is_edge_online` computation itself was checked and is correct: it compares `NOW() - last_seen_at` against `edge_stale_after_secs` with a `GREATEST(0, …)` guard and returns `'disconnected'` when the row is missing or stale — no inversion, no timezone bug.

**8. Frontend check — no bug found in the counter logic itself:**

`web-ui/components/context-bar.tsx` (old Next.js app):
```ts
function isOnline(status: string) {
  return String(status || "").toLowerCase() === "online";
}
...
const onlineEdges = edgeRows.filter((e) => isOnline(e.status)).length;
...
edges online {onlineEdges}/{edgeRows.length}
```
This is correct: numerator = rows with `status === "online"`, denominator = total rows returned by `/api/edges/current`. Given the API returns 9 rows all `"disconnected"`, this necessarily renders "edges online 0/9" — which is exactly what was observed. The bug is entirely upstream of this component: the denominator itself excludes 4 real, actively-reporting edges that never got a row.

`web-ui-v2/src/components/context-bar.tsx` does not yet implement an "edges online" counter at all (it currently only renders a site selector derived from `fetchTagsCurrent`) — this is Task 9's job, and Task 9 should build its counter the same way (`status === "online"` over `/api/edges/current`'s full row set) once the underlying data gap above is addressed or worked around, per the "What Task 9 needs to know" note below.

## Why this matters

This is a silent, invisible failure mode with no operator-facing signal: an edge that is actively and correctly publishing telemetry and heartbeats produces **zero** rows, **zero** errors, and **zero** indication anywhere in the API surface (`/api/edges/current`, `/api/tags/current` would actually show its raw telemetry-derived rows are also absent per-tag since `tag_current_state` is likewise gated on the same `resolve_tag_ref` join) that it exists. The only trace is a `WARN`-level log line that requires `RUST_LOG` to include `central_server=warn` or higher and someone to be tailing `central-server`'s logs at the right time. In production this is exactly the shape of failure this repo has already hit twice before (see `docs/finding-mqtt-client-stale-session-detection.md` and `docs/finding-lcc01-bala11-13-silent-serial.md`): a device is transmitting correctly, and the operator has no way to know central isn't receiving/recording it, because "no data" and "no such device" render identically (nothing) on `/api/edges/current`.

In this dev-stack case the mechanism is a `SEED_PROFILE` mismatch, easily fixed by re-running `db-seed` with `SEED_PROFILE=sim20` (or `full`). But the deeper, non-dev-only issue is architectural: **`central-server` has no path to surface "I am receiving traffic from an edge_code I don't recognize" as anything other than a buried WARN log.** That gap would reproduce identically in a real deployment any time an edge starts publishing before central's provisioning/enrollment data for it lands (a device commissioned early, a typo'd `edge_code` in a bootstrap config, a config rollback that drops a device's DB rows while the physical edge keeps running) — and "edges online 0/n" would silently under-report with no error for an operator to notice, exactly as reproduced here.

## Suggested fix directions (needs engineering triage, not yet implemented)

1. **Immediate/dev-stack fix:** re-run `db-seed` with `SEED_PROFILE=sim20` (or `full`) so `0008_dev_seed_sim20_multi_area.sql` actually provisions `edge-mix-1/2`, `edge-pack-1/2` before `docker-compose.edge-sim.yml`'s containers are started against this DB — or, more robustly, make `docker-compose.edge-sim.yml` declare which `SEED_PROFILE` it requires (e.g. a comment/`depends_on` note, or a startup check) so the mismatch can't silently recur.
2. **`central-server` visibility fix (candidate for Task 9/11 per the plan's Global Constraints):** when `resolve_edge_id`/`resolve_tag_ref` fail to find a mapping for a `(site, edge_code)` that is nonetheless receiving MQTT traffic, surface that as something an operator-facing API/UI can see — options include: (a) auto-create a minimal placeholder `edges` row (status `"unmapped"` or similar) the first time traffic arrives from an unrecognized `edge_code`, so it appears in `/api/edges/current` instead of being invisible; (b) track "unmapped edge_code sightings" in a small in-memory or DB-backed counter/table exposed via a new endpoint or field, so `web-ui-v2`'s Live page can render something like "3 unmapped edges seen" instead of silence; or (c) at minimum, promote the "mapping not found" log to a rate-limited `operational_events` row (the table already exists and is used for exactly this kind of surfaced condition elsewhere in `postgres.rs`, e.g. `edge.status.changed`) so it's queryable via `/api/ops/events` without needing log access.
3. Whichever fix is chosen, add a repro test: seed a DB with `minimal`, start a fake MQTT publisher for an edge_code that was never seeded, and assert the health/telemetry ingest paths produce *some* queryable, API-visible signal — not just a WARN log — matching this session's exact reproduction (`docker-compose.scada.yml` + `docker-compose.edge-sim.yml`, `SEED_PROFILE=minimal`).

## What Task 9 needs to know

- `web-ui-v2`'s "edges online" counter, when built, should compute it the same way the old `web-ui` does — `status === "online"` over the full `/api/edges/current` row set — because that logic is already correct; there is no frontend-side fix needed for the counter's *formula*.
- However, wiring that formula up against the *current* dev stack will still show `0/9` (or `0/13` if `SEED_PROFILE=sim20`/`full` is applied first, since `edge-com-01` etc. would still read "disconnected" while actually-online edges may or may not yet have a real row depending on whether the architectural gap in Suggested Fix #2 is addressed) until either (a) the dev stack is re-seeded with a profile matching `docker-compose.edge-sim.yml`, or (b) a `central-server` fix per Suggested Fix #2 makes previously-invisible-but-transmitting edges appear at all. Task 9 should not assume a `0/n` reading during its own manual verification means its own frontend code is broken — cross-check against this finding first.
- If Task 9/11 is the one to implement Suggested Fix #2, note that `edge_current_state`, `edge_health_events`, and `telemetry_ingest_events`/`tag_current_state` all currently share the same `resolve_edge_id`/`resolve_tag_ref` gate (`crates/central-server/src/persistence/postgres.rs`) — a fix there would need to be threaded through all of `insert_telemetry`, `insert_health`, and probably `insert_alert`/`insert_action_result` too, not just one call site.
