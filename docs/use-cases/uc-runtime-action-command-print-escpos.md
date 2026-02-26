# UC-RUNTIME-020: Generic Action Command + ESC/POS Print

## Goal
Implement a generic action command model reusable for edge and central use cases, starting with `print.escpos`.

## Architecture Mapping
1. Command topic from central/web-ui:
   - `scada/{site}/edge/{edge}/cmd/action`
2. Edge result topic:
   - `scada/{site}/edge/{edge}/cmd/action/result`
3. Edge audit topic:
   - `scada/{site}/edge/{edge}/audit/action`
4. Central API entrypoint:
   - `POST /api/edges/action`

## Message Contracts
1. Action command:
```json
{
  "schema_version": 1,
  "source": "web-ui",
  "request_id": "ui-print-123",
  "action_type": "print.escpos",
  "target": "edge",
  "payload": { "lines": ["IFA SCADA", "TAG: x", "VALUE: y"] }
}
```
2. Action result:
```json
{
  "schema_version": 1,
  "source": "edge/edge-01",
  "request_id": "ui-print-123",
  "action_type": "print.escpos",
  "accepted": true,
  "reason": null,
  "timestamp": "2026-02-24T00:00:00Z"
}
```
3. Action audit:
```json
{
  "schema_version": 1,
  "source": "edge/edge-01",
  "request_id": "ui-print-123",
  "action_type": "print.escpos",
  "outcome": "Applied",
  "reason": null,
  "payload": { "...": "..." },
  "timestamp": "2026-02-24T00:00:00Z"
}
```

## Unified Print Payload
`print.escpos` now supports two modes:
1. Direct lines:
```json
{
  "action_type": "print.escpos",
  "target": "edge",
  "payload": { "lines": ["IFA SCADA", "TAG: x", "VALUE: y"] }
}
```
2. Buffer print (same action type):
```json
{
  "action_type": "print.escpos",
  "target": "edge",
  "payload": {
    "mode": "from_buffer",
    "buffer_id": "weights_session_1",
    "clear_after_print": true
  }
}
```

Backward compatibility:
1. `print.escpos.from_buffer` is still accepted.

Generic device command envelope:
1. `action_type: "device.command"` routes by `payload.device_id` + `payload.command`.
2. Supported commands today:
   - `print` (routes to `print.escpos`)
   - `connection.check` (routes to `connection.check`)

Example:
```json
{
  "schema_version": 1,
  "source": "web-ui",
  "request_id": "ui-devcmd-001",
  "action_type": "device.command",
  "target": "edge",
  "payload": {
    "device_id": "dev_printer_u220",
    "command": "print",
    "args": {
      "lines": ["IFA SCADA", "PRINT FROM DEVICE.COMMAND"]
    }
  }
}
```

## Edge Execution
1. `print.escpos` supports:
   - TCP printer sink (`EDGE_ESCPOS_TCP_HOST` + `EDGE_ESCPOS_TCP_PORT`)
   - Windows shared printer sink (`EDGE_ESCPOS_WINDOWS_SHARE`, example `\\192.168.103.154\EPSON TM-U220 Receipt LCC`)
   - file sink fallback (`EDGE_ESCPOS_OUTPUT_PATH`)
2. File sink is default for local/dev validation.
3. For production printers without telemetry, configure device status as on-demand in central:
   - `devices.metadata_json.status_policy.mode = "on_demand"`
   - optional `devices.metadata_json.status_policy.stale_after_secs`
   - status is derived from `device.connection.*` operational events emitted by print job execution.
4. Optional generic startup probe for single-use devices:
   - `EDGE_ON_DEMAND_PROBE_ENABLED=true`
   - `EDGE_ON_DEMAND_PROBE_CONNECTION_ID=...`
   - `EDGE_ON_DEMAND_PROBE_DEVICE_ID=...`
   - uses configured TCP target and publishes `device/conn/state` on edge startup.
5. Manual connectivity check action supported:
   - `action_type: "connection.check"`
   - payload supports `host`/`port` (or `printer.host`/`printer.port`) and optional `timeout_ms`.

## Automatic Trigger (Current)
1. Preferred:
   - define `automations[]` in signed runtime config payload (`central` source of truth).
2. Temporary fallback (compatibility):
   - `EDGE_AUTO_PRINT_NONPOS_ENABLED=true`
   - `EDGE_AUTO_PRINT_TAGS=tag_scale_manual_compound`
   - `EDGE_AUTO_PRINT_CONSECUTIVE=2`
3. Both paths now flow through the same generic `AutomationEngine` and the same action/audit topics.
4. No bypass path: all automated actions still produce `cmd/action/result` + `audit/action`.

## Central Persistence
1. `cmd/action/result` and `audit/action` are ingested to `operational_events`.
2. Event types:
   - `action.command.accepted`
   - `action.command.rejected`
   - `action.audit`

## Web-UI Integration
1. Added API function: `postEdgeAction(...)` in `web-ui/lib/api.ts`.
2. Live view includes `Print ESC/POS` button for selected tag (manual path).

## Notes
1. `scope=edge|central|auto` is supported in action definition.
2. For production, use explicit scope (`edge` or `central`) to avoid duplicate execution strategies.
3. Print actions are idempotent by `request_id` in edge runtime; duplicate request IDs do not print twice.

## Trigger Troubleshooting
1. `update_mode`:
   - For event-driven scales, use `on_message` so repeated equal values are also evaluated by triggers.
2. Time window:
   - If `within_ms` is too short, the consecutive counter resets between operator prints.
   - Use `within_ms: null` (or remove it) if you do not need strict timing.
3. Numeric extraction:
   - Compound payload must include numeric `value`, e.g. `{"value":-0.1,"unit":"g","raw":"- 0.1000 g"}`.
4. Runtime diagnostics:
   - Enable `application::automation::engine=debug`.
   - Each evaluation logs: `matched`, `fired`, `consecutive_before`, `consecutive_after`.
