# Release 1.1.2

Release date: 2026-08-12
Version: `1.1.2`
Scope: Edge runtime only (`edge-agent`). No central changes.

## Summary
Fixes a defect where an edge could permanently stop receiving inbound MQTT
commands (manual web-UI print requests, config apply, alert ack, control
reset) after any transient network reconnect, while outbound publish and
local trigger-fired automations kept working normally — making the failure
invisible in logs.

## Root cause
`rumqttc::MqttOptions` defaults to `clean_session=true`. The bridge
subscribed to its inbound topics only once, at startup, before entering the
main event loop. On any reconnect (network blip, broker restart, keepalive
timeout), the broker discarded the previous session's subscriptions and the
bridge never re-subscribed, since the loop's `Err` branch only retried
`poll()` without re-issuing `subscribe()`.

## Fix
1. `crates/edge-agent/src/mqtt_bridge.rs`: subscriptions are now re-issued
   whenever the client observes `ConnAck { session_present: false }`, not
   just at startup.
2. `crates/edge-agent/build.rs` (new): stamps the binary with the short git
   commit SHA (+ `-dirty` marker), logged at startup as
   `version=<crate_version> git_sha=<sha>`. Closes a separate traceability
   gap found while diagnosing this issue: `edge-agent.exe` is gitignored and
   a running binary previously could not be traced back to a source commit.

## Included scope
1. Edge runtime binary (`edge-agent.exe`) rebuilt from this fix.
2. No config schema change (`config_schema_version` stays `1`).
3. No change to central, database, or MQTT topic contracts.

## Verification performed
```
cargo check -p edge-agent   → no errors/warnings
cargo test  -p edge-agent   → 50 passed; 0 failed
```
A new test (`test_subscribe_all_topics_issues_every_inbound_topic`) pins the
exact set of topics re-subscribed. It does not start a real broker, so it
cannot exercise the full reconnect → resubscribe flow end to end; that must
be confirmed on a real edge after deployment (see Rollback/verification
below).

## Deployment model
Built and packaged with the existing safe updater tooling:
```
deploy/edge-1.0.0-runtime/scripts/build-edge-package.ps1 -Version 1.1.2
```
This regenerates `deploy/edge-1.0.0-runtime/bin/edge-agent.exe` and
`deploy/edge-1.0.0-runtime/release-manifest.json` (both gitignored build
artifacts; distributed via the GitHub Release for this tag, not committed).

On each Windows edge, apply with:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\update-edge.ps1
```
run from a copy of the package root containing the new `bin\edge-agent.exe`
and `release-manifest.json`. The script stops the running task/service,
backs up the current binary+manifest under `releases\<old_version>\...`,
verifies SHA-256 before and after replacing the binary, restarts the
runtime, and automatically rolls back to the backed-up binary if the edge
does not become healthy within the timeout.

## Verification after deploying to an edge
```powershell
Get-Content "C:\ProgramData\ifascada\edge\logs\edge.out.log" | Select-String "Starting Clean Slate Edge Agent"
```
Confirm the logged `git_sha` matches this release's commit before assuming
the fix is active. Then confirm the resubscribe path specifically: trigger a
brief network interruption on the edge (or restart the local Mosquitto
broker/container reachable from it), and confirm
`mqtt re-subscription after reconnect completed` appears in `edge.out.log`,
followed by a successful manual print from the web UI for that edge.

## Breaking changes
None.

## Rollback strategy
1. `update-edge.ps1` rolls back automatically on failed health check.
2. Manual rollback: stop the task/service, restore
   `bin\edge-agent.exe` and `release-manifest.json` from
   `releases\<previous_version>\<timestamp>\` under the edge install root,
   restart.
