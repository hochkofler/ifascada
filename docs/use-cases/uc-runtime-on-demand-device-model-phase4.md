# UC: Runtime On-Demand Device Model (Phase 4)

## Goal
Support single-use devices (for example printers/actuators) without telemetry polling, while keeping reliable device connectivity lamps.

## Scope
1. Device status mode in central:
   - `devices.metadata_json.status_policy.mode = "on_demand"`
   - optional `stale_after_secs`
2. Edge optional startup probe:
   - `EDGE_ON_DEMAND_PROBE_ENABLED=true`
   - `EDGE_ON_DEMAND_PROBE_CONNECTION_ID=...`
   - `EDGE_ON_DEMAND_PROBE_DEVICE_ID=...`
   - host/port from payload or `EDGE_ON_DEMAND_TCP_HOST` + `EDGE_ON_DEMAND_TCP_PORT`
3. Manual check command:
   - `action_type = "connection.check"`

## Behavior Contract
1. Device lamp is driven by operational connectivity events (`device.connection.*`, `device.status.*`), not by tag telemetry.
2. `on_demand` devices can remain observable with zero historian spam.
3. Startup probe publishes one `state/device/conn` message when enabled and fully configured.
4. Manual `connection.check` can publish success/failure for explicit checks.

## Test Coverage
1. Central status contract:
   - `crates/central-server/tests/api_device_status_contract_tests.rs`
2. Edge action command path:
   - `crates/edge-agent/src/mqtt_bridge.rs` (`test_connection_check_requires_target`)
3. Edge startup probe path:
   - `crates/edge-agent/src/mqtt_bridge.rs`:
     - `test_on_demand_startup_probe_publishes_connected_state`
     - `test_on_demand_startup_probe_skips_when_ids_missing`

## Operational Notes
1. Recommended for printers and other actuators where periodic telemetry has no value.
2. Keep `stale_after_secs` explicit per device family to avoid ambiguous lamps.
