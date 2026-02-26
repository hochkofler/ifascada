# edge-agent crate

## Purpose
Runtime process for edge execution: protocol polling, command handling, telemetry publishing, and local resilience.

## Responsibilities
1. Start runtime from signed central config or verified local cache.
2. Execute connection/tag runtime loops.
3. Bridge MQTT topics (`telemetry`, `health`, `cmd`, `cmd/action`, `config/apply`).
4. Persist outbox locally (SQLite) when MQTT is unavailable.
5. Publish operational state (`connection/device/tag`) and action outcomes.

## Key modules
1. `bootstrap.rs`: signed config lifecycle (fetch/check/verify/cache).
2. `mqtt_bridge.rs`: MQTT orchestration and handler routing.
3. `action_orchestrator.rs`: generic action execution contract.
4. `mqtt_outbox.rs`: durable store-forward for MQTT publishes.

## Validation
```powershell
cargo test -q -p edge-agent
```
