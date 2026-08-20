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
