# UC-RUNTIME-022: Buffer Positive Weights + Double-Negative Print

## Objective
Execute fully local edge behavior for printing workflows:
1. Accumulate positive weights.
2. On double negative, print accumulated weights.
3. Emit `print.persist` for central-side persistence/audit workflow.

## Configuration Source
Tag-scoped automations in `tags.metadata_json.automations[]` for `tag_scale_manual_compound`.

Apply SQL:
```powershell
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "scripts/sql-configure-print-buffer-tag.sql"
```

## Runtime Behavior
1. Edge trigger evaluator runs locally (no central dependency).
2. Local actions:
   - `buffer.weights.accumulate`
   - `print.escpos` with `payload.mode=from_buffer`
3. Central intent action:
   - `print.persist` (published/audited through existing action topics).

## Offline/Failure Semantics
1. Print execution path is local-first.
2. MQTT publish is best-effort.
3. If MQTT unavailable, result/audit are stored in edge outbox SQLite and replayed later.

## Validation Steps
1. Restart edge-agent to reload signed config.
2. Send sequence:
   - `+ 1.0000 g`
   - `+ 1.2000 g`
   - `- 0.1000 g`
   - `- 0.2000 g`
3. Verify local print output:
```powershell
Get-Content .\data\escpos_output.bin -Tail 120
```
Expected lines:
- `BUFFER: weights_session_1`
- `COUNT: 2`

4. Verify action topics:
```powershell
docker exec -it ifascada-mosquitto sh -lc "mosquitto_sub -h localhost -p 1883 -t 'scada/plant-a/edge/edge-com-01/cmd/action/result' -v"
docker exec -it ifascada-mosquitto sh -lc "mosquitto_sub -h localhost -p 1883 -t 'scada/plant-a/edge/edge-com-01/audit/action' -v"
```
