# CI/CD and Update Protocol Design

**Status:** Approved for implementation on 2026-08-14.

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
- Changing how `edge-agent` is deployed in the field. It keeps `update-edge.ps1` as-is; this spec
  only adds a CI stage that builds and publishes its package under the same tag-driven pipeline.

## Architecture Overview

```
tag "central-vX.Y.Z" / "webui-vX.Y.Z" / "edge-vX.Y.Z"  (pushed to GitHub)
        |
        v
GitHub Actions workflow, one per component, path/tag-prefix filtered
  (a tag for one component does not build the other two)
        |
        v
Self-hosted runner (developer workstation: already has Docker, already builds
these images today, already holds the SSH deploy keys)
  - central-server / web-ui: cargo test / cargo check or npm run build,
    docker build, docker save -> .tar
  - edge-agent: build-edge-package.ps1 (existing script, unchanged)
  - artifact attached to a GitHub Release under that component's tag
        |
        v
Manual approval gate (GitHub Environment "production", required reviewer)
        |
        v
Deploy job (runs on the same runner, over SSH to the target host)
  - central-server / web-ui -> Update Protocol v1 (below)
  - edge-agent -> existing update-edge.ps1, invoked remotely over SSH
```

## Components

1. **Per-component GitHub Actions workflows.** Triggered only by their own tag prefix
   (`central-v*`, `webui-v*`, `edge-v*`), matching the existing scoped-release convention already
   used in `docs/releases/RELEASE-1.1.2.md` ("Edge runtime only. No central changes."). A change
   to one component never forces a rebuild of the other two.
2. **Self-hosted GitHub Actions runner**, installed on the developer workstation used throughout
   this migration. It already has Docker and already produces the images used to install central
   on `.154`; registering it as a runner removes the manual "build here, scp there" step without
   introducing a new machine. Holds the SSH deploy keys (`ifascada_migracion_154` and its future
   equivalent for `.98`/other sites) — this is the trust boundary of the whole pipeline and is
   called out explicitly under Security below.
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

## Security Notes

- The self-hosted runner living on the developer workstation is the pipeline's trust boundary: it
  holds SSH keys capable of deploying to production hosts. This is an accepted trade-off for now
  (chosen explicitly over a dedicated CI machine or running the runner on `.154` itself, which
  would compete with the constrained 8GB/4-core production host during builds).
- `POSTGRES_HOST_AUTH_METHOD=trust` is carried forward unchanged from the current `.98` production
  configuration into `.154`. This is a known gap, explicitly deferred, not silently dropped.

## Open Follow-Ups (not part of this implementation pass)

- Harden Postgres authentication (`trust` -> `scram-sha-256` with a real password) on whichever
  host is production at the time.
- Build the v2 blue-green protocol once v1 has run in production long enough to justify the added
  complexity.
- `edge-scale-com3-test` was explicitly excluded from the `.98 -> .154` migration scope; it is not
  part of this pipeline's initial rollout either.
