# UC: Printer Device Command Workflow (DB-driven)

## Goal
Enable printer command execution as a device-level action, reusable for future actuators (alarm/email/export), with trigger pipeline from scale tag.

## Migration
Apply:
```powershell
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0017_printer_device_command_and_negative_trigger.sql"
```

## What it configures
1. Connection: `conn_printer_u220_1` (catalog only, on-demand use).
2. Device: `dev_printer_u220` with:
   - `status_policy.mode = on_demand`
   - Windows shared printer transport metadata (`transport.windows.share`)
3. Tag `tag_scale_manual_compound`:
   - `metadata_json.pipeline` for compound extract + display format.
   - automations:
     - buffer positive samples
     - on 2 consecutive negatives:
       - `device.command` -> `connection.check`
       - `device.command` -> `print`
       - `print.persist` (central scope)

## Runtime expectations
1. Trigger evaluation requires `update_mode = on_message` for repeated equal values.
2. `device.command` runs in edge action orchestrator.
3. `connection.check` publishes `device/conn/state` events (connection/device lamps).

## Printer share override
Edit migration payload if printer share changes:
1. `\\192.168.103.154\EPSON TM-U220 Receipt LCC`
