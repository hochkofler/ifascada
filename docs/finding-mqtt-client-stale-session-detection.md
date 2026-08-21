# Finding: MQTT Clients Can Believe They're Connected Long After the Broker Dropped Them

**Status:** Open — needs triage/fix.
**Affects:** `edge-agent` (`crates/edge-agent/src/mqtt_bridge.rs`) and `central-server` (`crates/central-server/src/mqtt_consumer.rs`), both built on `rumqttc`.
**Discovered:** 2026-08-18, while investigating "edges not connecting to central" after mosquitto was restarted several times during onboarding of a new edge (`lcc01`). See `docs/runbook-core-operations.md` §3.4/§3.5/§6 for the operational incident and workarounds.

## Summary

Both the edge and central MQTT clients can end up with a **dead session that they do not detect**, continuing to log successful sends (`mqtt publish ok`, no reconnect warnings) while nothing actually reaches the other side. Recovery currently requires a **manual process restart** — neither side self-heals.

## Evidence (edge side, cross-machine correlated to the second)

| Time (UTC) | Event | Source |
|---|---|---|
| 11:47:13 | mosquitto container restarts | `.98`, `docker logs ifascada-mosquitto` |
| 11:47:15 | `lcc01` reconnects to the broker | `.98`, `/mosquitto/log/mosquitto.log` |
| **11:47:31** | mosquitto disconnects `lcc01`: `"exceeded timeout"` (missed keepalive, `k10` = 10s) | `.98`, `/mosquitto/log/mosquitto.log` |
| 11:47:31 → 13:00:40 | **`lcc01` never appears connected in mosquitto again — a 1h13m gap** | `.98`, `/mosquitto/log/mosquitto.log` (zero matching lines) |
| (throughout the gap) | edge-agent keeps logging `mqtt publish ok topic='.../conn/state'` and `.../health/runtime` at their normal cadence, with no reconnect/error log lines | `.154`, `edge.out.log` |
| 13:00:36 | edge-agent process manually restarted (unrelated troubleshooting step) | `.154`, `edge.task.log` |
| 13:00:40 | `lcc01` reconnects for real | `.98`, `/mosquitto/log/mosquitto.log` |

The equivalent pattern was observed independently on the central side (§3.4 in the runbook): `central-server`'s consumer client (`central-server-01`, `clean_session=false`) kept reporting a healthy wildcard subscription at boot but stopped receiving messages from at least one edge after the broker had been restarted multiple times — recovered only by `docker restart ifascada-central-server`.

## Why this matters

"`mqtt publish ok`" in the edge log only means the message was **handed to the client library's outbound queue**, not that it reached the broker. If the underlying TCP session is dead (broker-initiated disconnect, e.g. a keepalive timeout) but the client's event loop doesn't promptly observe the socket close, the client can silently queue/"succeed" indefinitely. During the 2026-08-18 incident this produced over an hour of **total data loss with a fully green-looking edge log** — nothing in the edge's own log would have told an operator anything was wrong without cross-checking the broker's client log on the other machine.

## Working hypothesis

- `k10` (10s keepalive) with mosquitto disconnecting on a missed ping is expected MQTT behavior.
- What's not expected: the edge's poll loop (`crates/edge-agent/src/mqtt_bridge.rs`, the `Err(e)` branch around the `event_loop.poll()` call, see the `"MQTT event loop error: {}; retrying poll in 1s"` log) did not fire during the entire 1h13m gap. That branch is the only place reconnection/backoff is triggered — if `poll()` doesn't return an error for a connection the broker already closed, the client has no way to notice.
- Likely candidates: no OS-level TCP keepalive configured on the socket (so a half-open connection — e.g. broker closed cleanly but the FIN was lost/delayed, or a NAT/stateful device between the machines dropped state silently — is never surfaced to the application), and/or `rumqttc`'s internal handling of a server-initiated disconnect not being surfaced as a `poll()` error in this version/configuration.
- The central-side symptom (§3.4) may or may not share the exact same mechanism (`clean_session=false` persistent session vs. the edge's `clean_session=true`), but the *shape* of the bug is the same: a session the broker considers dead, that the client believes is alive.

## Suggested fix directions (needs engineering triage, not yet implemented)

1. Enable OS-level TCP keepalive on the `rumqttc` socket (shorter interval than the MQTT-level keepalive) so a half-open connection is detected by the OS and surfaced to the application promptly.
2. Don't rely solely on `event_loop.poll()` erroring to detect staleness — track time-since-last-broker-activity (e.g. last received `PINGRESP`/ack) and force a reconnect if it exceeds `keep_alive * 1.5` even without an explicit error.
3. Only log `"mqtt publish ok"` after actual broker acknowledgment (QoS1 `PUBACK`) is observed, not merely after the message is accepted into the client's internal queue — or at minimum, log queue-depth/ack-lag so a stuck queue is visible without needing the broker's own log.
4. For central's persistent session (§3.4): consider whether `clean_session=false` is actually buying anything here, or whether a `clean_session=true` consumer with an idempotent/at-least-once ingest path would be simpler and self-heal the same way the edges do.
5. Whatever fix is chosen, add a repro test: kill the broker's TCP connection to a connected client out-of-band (e.g. `docker network disconnect`/firewall rule) without a clean FIN, and assert the client reconnects within a bounded time — this is exactly the failure mode that went undetected for 73 minutes in production.

## Repro sketch (not yet automated)

1. Connect edge-agent to a real broker.
2. Simulate a broker-side kick without a clean close reaching the client (e.g. `iptables -A INPUT -s <edge_ip> -j DROP` on the broker host right after forcing a keepalive-timeout disconnect, so the client's FIN/RST never arrives).
3. Expect (today): edge keeps logging `mqtt publish ok` indefinitely, no reconnect attempt.
4. Expect (after fix): edge detects staleness within `~keep_alive * 1.5` and reconnects/logs an error.

## Evidence — 2026-08-21: web-ui-v2 Task 10 repro attempt (does NOT reproduce this finding's mechanism)

**Trigger:** web-ui-v2 rewrite Task 10, investigating a reported Live-view symptom ("tags show 'connected' with no real telemetry"), suspected up front to be the same underlying mechanism as this finding. Ran the brief's own repro shape — a hard `docker network disconnect` against one edge simulator container, not a graceful stop — against the current local stack (`SEED_PROFILE=sim20`, `edge-pack-1` real telemetry-producing edge).

**Method:** `docker network disconnect ifascada_default ifascada-edge-sim-pack-1` (a genuine network-interface removal, not a firewall `DROP`), then polled `/api/edges/current` and `/api/tags/current?edge=edge-pack-1` every 5s, then `docker network connect ifascada_default ifascada-edge-sim-pack-1` to restore it. `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT` confirmed at its code default of **45s** (`default_edge_stale_after_secs()` in both `crates/central-server/src/api.rs` and `crates/central-server/src/persistence/postgres.rs`; freshness is computed at query time as `EXTRACT(EPOCH FROM (now() - last_seen_at))` compared against this threshold — not dependent on the mqtt consumer's own belief about its connection health).

**Timeline (UTC, 2026-08-21):**

| Time | Event |
|---|---|
| 13:32:34.006–.120 | Last real telemetry from `edge-pack-1` ingested (`central_server::ingestion`: `ingest telemetry persisted ... agent='edge-pack-1'`), in flight when the disconnect landed |
| 13:32:34.587 | `docker network disconnect` issued |
| 13:32:58.852 | `tag_p1_t001` already `state:"stale"`, `reason:"tag_window_soft_expired"` — tag-level soft staleness (governed by `historian_max_interval_secs:60` metadata, not the 45s edge threshold) kicks in within ~24s |
| 13:33:14.633 | `edge-pack-1` still reads `state:"online"`/`status:"online"`, `reason_code:"derived_connected"` (last poll before the flip) |
| 13:33:19.874 | `edge-pack-1` flips to `state:"disconnected"`, `reason_code:"edge_offline_or_stale"` — **45.3s after the disconnect**, matching the `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT=45` threshold almost exactly; tag state follows the same transition one poll later |
| 13:33:26.819 | `central_server::persistence::postgres`: `device_status transition: site=plant-a edge=edge-pack-1 device=dev-pack-1 state=disconnected (c=0 s=0 d=5)` |
| 13:33:19 → 13:35:30 | Confirmed stable at `disconnected`/`edge_offline_or_stale` on every 5s poll — no flapping back to "online" |
| 13:35:34.008 | `docker network connect ifascada_default ifascada-edge-sim-pack-1` issued to reconnect (first attempt raced with an in-progress container restart and returned a daemon error, but the endpoint was actually created — confirmed by `docker inspect` showing the network attached) |
| 13:35:39.593 | Container's own restart-policy-driven restart (see below) picks up the restored network — no manual `docker restart` was needed |
| 13:35:41.429 | First fresh telemetry ingested from `edge-pack-1` again |
| 13:35:42.145 | `/api/edges/current` and `/api/tags/current` both read `online`/`connected` again — **full recovery ~8s after the reconnect command, ~3m07s total outage** |
| 13:36:13+ | Restart count stable at 12 (no further crash-loop iterations); all 4 sim edges (`edge-pack-1/-2`, `edge-mix-1/-2`) confirmed `online` |

**Edge-side detail that diverges from this finding's documented mechanism:** the `edge-sim-pack-1` container did **not** sit silently believing it was connected. `docker network disconnect` triggered an immediate, loud failure: `rumqttc`'s `event_loop.poll()` returned `MQTT event loop error: I/O: failed to lookup address information: Temporary failure in name resolution`, which propagated as an **unhandled `Error`** that crashed the edge-agent process outright. Docker's restart policy then crash-looped the container (`Starting Clean Slate Edge Agent` at 13:33:08, 13:33:21, 13:33:47, 13:34:39 — increasing backoff), each attempt failing the same DNS lookup until the network was restored, at which point the very next restart (13:35:39) succeeded and stayed up.

**Conclusion: this repro does NOT reproduce this finding's "silent, undetected dead session" mechanism, on either side.**

- **Central side:** the derived "online"/"connected" state is *not* the frozen-forever bug this finding describes. It's a query-time computation (`now() - last_seen_at` vs. a 45s threshold) that is completely decoupled from whether central's own mqtt consumer client "believes" it's connected — it degrades and recovers correctly and automatically, with no restart needed, purely from telemetry going quiet. This is the opposite of what this finding's §3.4 central-side note describes (a persistent `clean_session=false` consumer session that stayed "healthy" while actually not receiving messages from an edge, needing `docker restart ifascada-central-server` to fix). This test disconnected the **edge's** network, not central's broker session — it never exercised the code path this finding is actually about on the central side.
- **Edge side:** a hard `docker network disconnect` is a much blunter instrument than the finding's repro sketch (a firewall `DROP` with the FIN/RST lost, or a missed-keepalive disconnect the client doesn't notice). Removing the interface entirely fails DNS resolution immediately and loudly, crashing the process — it does not produce the "`mqtt publish ok` logged forever, no error, no reconnect" silent-hang symptom this finding documents. If anything, this shows the *opposite* failure mode is also present and unhandled: an `Err(e)` from `event_loop.poll()` is not caught/retried in-process at all in this configuration (`crates/edge-agent/src/mqtt_bridge.rs`) — it kills the whole agent, relying entirely on the container orchestrator's restart policy to recover. That's a distinct, previously-undocumented finding worth flagging separately (loud crash-loop instead of graceful reconnect/backoff — different failure mode, not "no fix implemented" for the silent case, but "no in-process resilience" for this loud case either).

**What this means for the original Task 10 symptom ("Live shows tags connected with no real telemetry"):** not reproduced by this method. The correct central-side staleness computation self-heals within 45s regardless of the mqtt consumer's own connection awareness, so a plain edge-side network drop cannot produce a Live view stuck showing "connected" indefinitely. **To actually test whether this finding's central-side mechanism (§3.4: `central-server-01`'s persistent session going silently stale while edges keep publishing fine) is what Live is hitting, the repro needs to target central's own broker session** — e.g. restart/bounce `mosquitto` (or drop only `ifascada-central-server`'s connection to it) while edge simulators keep running, and check whether `central-server`'s consumer notices before falling back on the same 45s age-based staleness (which would eventually mask the bug from `/api/edges/current` too, just later and only after *all* edges going quiet at once — a distinguishing symptom from a single edge looking stuck). That repro is unattempted here and is what Task 11 should pursue if it wants to hit the actual documented mechanism instead of a normal (and evidently correctly self-healing) edge-offline path.

**Stack left healthy:** `edge-pack-1` reconnected and confirmed `online`/`connected` with fresh telemetry as of 13:36:13Z; container restart count stable (12, no further restarts); all four edge simulators (`edge-pack-1/-2`, `edge-mix-1/-2`) online. No changes made to `crates/central-server`, `web-ui/`, or `web-ui-v2/`.
