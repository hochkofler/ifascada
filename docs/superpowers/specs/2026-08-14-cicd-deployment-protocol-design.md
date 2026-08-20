# CI/CD and Update Protocol Design

**Status:** Approved for implementation on 2026-08-14. Amended 2026-08-18: replaced the
SSH-push edge-agent deployment model with a pull-based one (see "Edge-Agent Update Protocol"
below) — the original model does not scale past a handful of edges and was never implemented.

## Goal

Give `central-server`, `web-ui`, and `edge-agent` a formal, versioned, repeatable way to build,
release, and update — closing the gap between edge (already mature: `update-edge.ps1` safe
updater with SHA-256 verification, snapshot, and automatic rollback) and central/web-ui (today:
manual `docker compose --force-recreate`, no verification, no rollback). The `.98 → .154` central
migration is the first real deployment this protocol is validated against; the protocol itself is
meant to apply to any future site, not just this one.

## Non-Goals (v1)

- Zero/near-zero downtime during a central-server or web-ui update. v1 accepts a short cutover
  window (stop old, start new, verify, rollback-if-unhealthy). A blue-green protocol removing that
  window is designed below as v2 and left for a later, separate implementation once v1 is proven
  in production.
- Hardening `POSTGRES_HOST_AUTH_METHOD` away from `trust`. Deferred by explicit decision; tracked
  as follow-up work, not part of this spec.
- Rewriting `update-edge.ps1`'s core snapshot/swap/rollback logic. It stays exactly as-is and
  proven; this spec only changes how the package *arrives* at the edge (pull over HTTP instead of
  a human copying it locally) — see Edge-Agent Update Protocol below.
- An "update now" push channel for edges (e.g. an MQTT nudge to skip the wait for the next poll).
  Left as a follow-up; v1 relies on the periodic poll interval alone.

## Architecture Overview

```
tag "central-vX.Y.Z" / "webui-vX.Y.Z" / "edge-vX.Y.Z"  (pushed to GitHub)
        |
        v
GitHub Actions workflow, one per component, path/tag-prefix filtered
  (a tag for one component does not build the other two)
        |
        v
Self-hosted runner (developer workstation, i.e. the machine used throughout this
migration: already has Docker, already builds these images today, holds a
dedicated CI deploy key scoped to central-server/web-ui hosts only)
  - central-server / web-ui: cargo test / cargo check or npm run build,
    docker build, docker save -> .tar
  - edge-agent: build-edge-package.ps1 (existing script, unchanged)
  - artifact attached to a GitHub Release under that component's tag
        |
        v
Manual approval gate (GitHub Environment "production", required reviewer)
        |
        v
Deploy job (runs on the same runner)
  - central-server / web-ui -> over SSH to the target host, Update Protocol v1 (below)
  - edge-agent -> nothing to do here; central serves the artifact and every edge
    pulls it on its own schedule (Edge-Agent Update Protocol, below) — the runner
    never connects to an edge
```

## Components

1. **Per-component GitHub Actions workflows.** Triggered only by their own tag prefix
   (`central-v*`, `webui-v*`, `edge-v*`), matching the existing scoped-release convention already
   used in `docs/releases/RELEASE-1.1.2.md` ("Edge runtime only. No central changes."). A change
   to one component never forces a rebuild of the other two.
2. **Self-hosted GitHub Actions runner**, installed on the developer workstation used throughout
   this migration. It already has Docker and already produces the images used to install central
   on `.154`; registering it as a runner removes the manual "build here, scp there" step without
   introducing a new machine. Holds a **dedicated CI deploy SSH key**, generated specifically for
   this pipeline (not the ad-hoc personal keys used during the `.98 -> .154` migration), scoped
   only to `central-server`/`web-ui` hosts (today: `.154`) — this is the trust boundary of the
   whole pipeline and is called out explicitly under Security below. It never holds credentials
   to any edge; edges pull their own updates (see Edge-Agent Update Protocol below), so this
   boundary does not grow as the number of edges grows.
3. **`/api/health` endpoint in `web-ui`.** Does not exist today (checked: only page content, no
   health route). Required so the deploy job has something to poll after starting the new
   container, the same way central already exposes `/health/live`.
4. **GitHub Environment `production`** with a required reviewer. Implements the manual gate
   between "artifact built and tested" and "artifact touches a real site."
5. **Deploy scripts** (new, one per Docker component: central-server, web-ui) implementing the
   Update Protocol v1 described next. Invoked by the runner over the same SSH channel already used
   throughout this migration.

## Update Protocol v1 (with cutover)

No proxy, no blue-green, no MQTT client-id overlap handling — none of that is needed once old and
new never run at the same time.

1. Copy the new image tar to the target host (already-open SSH channel) and `docker load` it.
2. Stop the running container (`central-server` or `web-ui`).
3. Start the new container under the same name/port.
4. Poll its health endpoint (`/health/live` for central, `/api/health` for web-ui) with a bounded
   timeout.
5. **Healthy within timeout:** done. The previous image tag stays cached locally for a manual
   rollback if a problem surfaces later that the health check didn't catch.
6. **Not healthy within timeout:** automatic rollback — stop the new container, start the previous
   image tag (already local, no re-download needed), confirm it reports healthy again, and fail
   the deploy job. This is the same guarantee `update-edge.ps1` already gives edge-agent, applied
   here without needing the zero-downtime machinery.

Cutover window is whatever the new container needs to start and connect (DB/MQTT/Redis) — expected
single-digit seconds for central-server, faster for web-ui.

## Edge-Agent Update Protocol (Pull-Based)

**Why not SSH push, like the original 2026-08-14 draft assumed:** pushing updates from the CI
runner (or from central) to every edge over SSH means holding — and rotating, and auditing —
credentials to every edge machine in the plant, on a network the runner may not even be able to
reach directly. It does not scale past a handful of edges and reintroduces exactly the kind of
manual-access sprawl this whole CI/CD effort exists to remove. Edges already trust central for
their runtime configuration (`/api/edge/config/check`, enrollment token + HMAC-signed response,
polled on a schedule); this protocol extends that same trust relationship to binary updates
instead of inventing a second one.

**New pieces (the existing `update-edge.ps1` snapshot/swap/rollback logic is untouched):**

1. **`GET /api/edge/agent-package`** (new central-server endpoint, separate from the HMAC-signed
   config-check contract so the two don't get entangled). Returns `{version, sha256,
   download_url}` for the latest `edge-agent` release the CI pipeline has published. Behind the
   same enrollment-token check already used elsewhere. v1 has exactly one "latest" version for
   every edge across every site — no per-site staged/canary rollout. Every edge that checks in
   gets pointed at the same artifact; if staged rollout is ever needed, that's a deliberate later
   extension, not part of this design.
2. **A new, independent scheduled task on each edge** (separate from the `ifascada-edge` runtime
   task, so an update check can never compete with or destabilize a live connection) that, on an
   interval (default: hourly, configurable): calls the endpoint above, compares `version` against
   the locally installed `release-manifest.json`, and — only if newer — downloads the artifact to
   a temp folder.
3. **`update-edge.ps1` gains a version-gate.** Today it unconditionally applies whatever package
   it's pointed at (correct for its original "a human already decided to update" use case). For
   unattended, periodic invocation it needs to skip the snapshot/swap entirely when the installed
   version is already current, so a healthy edge with no pending update does nothing every hour.

**Data flow:** CI publishes release -> central-server's release-serving path picks it up (exact
mechanism — GitHub Releases directly vs. central mirroring the artifact locally — decided during
implementation planning, see Open Follow-Ups) -> edge's scheduled check task polls
`/api/edge/agent-package` -> newer version found -> download to temp -> SHA-256 verified against
the manifest (defense in depth beyond transport trust) -> `update-edge.ps1` invoked against the
temp folder -> existing snapshot/swap/health-check/rollback flow runs unchanged.

**Error handling:** a failed download or a SHA-256 mismatch never touches the live runtime — the
task simply retries on its next scheduled interval. A failure inside `update-edge.ps1` itself is
already covered by its existing automatic rollback (unchanged by this protocol).

**Known limitation, explicitly accepted for v1:** the update check travels over the same
edge-to-central network path already used for MQTT and config polling. The 2026-08-18 incident
(`docs/finding-mqtt-client-stale-session-detection.md`) showed that path can go silently stale for
over an hour without either side noticing. A degraded connection therefore delays update delivery
the same way it already delays config delivery today — this protocol does not make that worse, but
it does not fix it either. Left as a follow-up rather than blocking this design, since fixing it
properly means fixing the underlying stale-session detection gap, not adding update-specific
workarounds on top of a known-fragile channel.

## Update Protocol v2 (planned, not in this implementation pass)

Once v1 has run in production and the team wants to remove the cutover window:

- A lightweight reverse proxy (Caddy) becomes the fixed host-port endpoint in front of
  `central-server` and `web-ui`. The new container starts on an internal port while the proxy
  still points at the old one; only after the new one passes its health check does the proxy
  switch upstream via a hot config reload (no dropped connections on the port itself), and only
  then does the old container stop. A failed health check means the proxy config is never
  touched — zero impact.
- MQTT ingestion continuity relies on standard broker behavior: both instances share
  `CENTRAL_MQTT_CLIENT_ID`, so the moment the new instance connects, the broker drops the old
  session and ingestion resumes on the new instance within a sub-second window. `edge-agent`'s
  existing local outbox (`MQTT_OUTBOX_PATH`) already covers any message that lands exactly in that
  gap — no new buffering logic needed on the edge side.
- Known residual gap even under v2: already-open SSE connections (`/api/stream/events`,
  `/api/ops/events/stream`) stay attached to the old container until it actually stops, then the
  browser's `EventSource` reconnects on its own — a live-dashboard blip of a few seconds, not
  eliminated by this design. Removing it entirely would require keeping the old instance alive
  until every SSE client disconnects on its own (a grace-period drain), which is out of scope
  unless the team decides that blip still matters after seeing v2 in practice.
- Schema changes during the v2 overlap window must be additive-only in the release that
  overlaps (new columns/tables, nothing dropped or renamed) since old and new code briefly run
  against the same database; destructive changes move to a later release once no old-version
  instance remains.

## Error Handling

- Build or test failure stops the pipeline before any artifact is produced or published; no
  target host is touched.
- Deploy-time health-check failure triggers the v1 automatic rollback described above and marks
  the GitHub Actions deploy job as failed.
- A tag pushed for a component with no corresponding workflow change is a no-op for the other two
  components' pipelines (path/prefix filtering, not a shared version number).

## Testing

- `central-server`: `cargo check` + `cargo test`, same gate already proven for `edge-agent` in
  `RELEASE-1.1.2.md`.
- `web-ui`: `npm run build` at minimum before packaging; any existing lint/test script the project
  already runs.
- `edge-agent`: unchanged — its existing build/test/package flow (`build-edge-package.ps1`) is
  reused as-is inside the new tag-triggered workflow.
- Edge-Agent Update Protocol (new): `update-edge.ps1`'s new version-gate needs a test asserting it
  is a no-op when the installed version already matches (no snapshot/swap triggered); the new
  `/api/edge/agent-package` endpoint needs the same enrollment-token-rejection test pattern already
  used for `/api/edge/config/check`.

## Security Notes

- The self-hosted runner living on the developer workstation is the pipeline's trust boundary: it
  holds a dedicated CI deploy SSH key capable of deploying to `central-server`/`web-ui` hosts. This
  is an accepted trade-off for now (chosen explicitly over a dedicated CI machine or running the
  runner on `.154` itself, which would compete with the constrained 8GB/4-core production host
  during builds). The key is scoped to central/web-ui hosts only and generated specifically for
  this pipeline, separate from any personal/ad-hoc key used for manual operations — if it needs to
  be revoked, that revocation does not also cut off manual SSH access, and vice versa. It does not,
  and structurally cannot, grow to cover edges: edges pull their own updates and never receive an
  inbound connection from the runner.
- `POSTGRES_HOST_AUTH_METHOD=trust` is carried forward unchanged from the current `.98` production
  configuration into `.154`. This is a known gap, explicitly deferred, not silently dropped.

## Open Follow-Ups (not part of this implementation pass)

- Harden Postgres authentication (`trust` -> `scram-sha-256` with a real password) on whichever
  host is production at the time.
- Build the v2 blue-green protocol once v1 has run in production long enough to justify the added
  complexity.
- `edge-scale-com3-test` was explicitly excluded from the `.98 -> .154` migration scope; it is not
  part of this pipeline's initial rollout either.
- Decide, during implementation planning, exactly where central-server serves the edge-agent
  artifact from: proxying/redirecting to the GitHub Release directly (simplest, but requires every
  edge to have outbound internet access — true for `.154` today, not guaranteed for every future
  site), vs. central mirroring the artifact to local disk after each publish (works on air-gapped
  plant networks, adds a small sync step to the deploy job).
- An MQTT "check now" nudge (mentioned in Non-Goals) so an edge doesn't have to wait for its next
  poll interval to pick up an urgent update.
- Fixing the underlying MQTT/HTTP stale-connection detection gap
  (`docs/finding-mqtt-client-stale-session-detection.md`) would also make update delivery more
  timely, not just telemetry — it's tracked there, not duplicated here, but the two are related.
