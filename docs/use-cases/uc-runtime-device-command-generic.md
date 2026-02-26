# UC: Generic `device.command` Actions in Edge

## Goal
Execute device-level commands from inside edge (automations) or outside edge (MQTT/API), with one extensible contract.

## Contract
1. Topic:
   - `scada/{site}/edge/{edge}/cmd/action`
2. Message:
```json
{
  "schema_version": 1,
  "source": "central-api",
  "request_id": "cmd-001",
  "action_type": "device.command",
  "target": "edge",
  "payload": {
    "device_id": "dev_printer_u220",
    "command": "print",
    "args": {
      "lines": ["IFA SCADA", "HELLO"]
    }
  }
}
```

## Supported commands (current)
1. `print` / `print.escpos` -> routes to existing ESC/POS executor.
2. `connection.check` / `check` -> routes to existing connectivity check executor.

## Behavior
1. Works for:
   - external commands (`cmd/action`)
   - internal automations (same action contract in runtime config).
2. Keeps idempotency by `request_id` through orchestrator policy.
3. Preserves existing `cmd/action/result` + `audit/action` observability.

## Extensibility
1. Add new command mappings in `DeviceCommandExecutor` (edge-agent).
2. Future examples:
   - `alarm.notify`
   - `email.send`
   - `file.export`
3. No domain coupling to concrete infrastructure handlers.
