# central-server

Phase B1 scaffold for central ingestion and persistence boundaries.

## Current modules
1. `topic`
- SCADA MQTT topic parser.

2. `messages`
- Typed payload contracts for telemetry, health, alerts, write ack, write audit.

3. `persistence`
- `CentralPersistence` trait (port) for infrastructure adapters.

4. `ingestion`
- Ingestion service that routes topic + payload into persistence calls.

## Planned adapters
1. PostgreSQL/Timescale adapter (source of truth/historian).
2. Redis cache adapter (current-state acceleration + SSE fan-out).
3. MQTT consumer runtime (topic subscription and delivery loop).

## Migrations
Initial SQL lives in `migrations/`:
1. `0001_core_postgres.sql`
2. `0002_timescale_historian.sql`
3. `0003_tag_naming_governance.sql`
4. `0010_connection_domain_state.sql`
5. `0011_device_domain_state.sql`
6. `0012_edges_metadata_json.sql`
7. `0013_scale_manual_config_in_catalog.sql`
8. `0014_dev_seed_modbus_rtu_com10_multi_slave.sql`

## Naming governance
Canonical tag naming format:
- `SITE.AREA.UNIT.DEVICE.SIGNAL.ATTRIBUTE`
- regex:
  - `^[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,12}\.[A-Z0-9_]{2,16}\.[A-Z0-9_]{2,8}\.[A-Z0-9_]{2,8}$`
- telemetry/audit ack resolution tries, in order:
  1. `tags.tag_code`
  2. `tags.tag_code_canonical`
  3. `tags.aliases_json` contains received tag code

Examples:
1. `PLANTA1.CALDERA.U01.PT101.PRES.PV`
2. `SITEA.AREA01.UNIT2.FT203.FLOW.SP`

## Runtime env
1. `CENTRAL_MQTT_ENABLED` (default: `true`)
2. `MQTT_HOST` (default: `127.0.0.1`)
3. `MQTT_PORT` (default: `1883`)
4. `CENTRAL_MQTT_CLIENT_ID` (default: `central-server-01`)
5. `CENTRAL_MQTT_TOPIC_FILTERS` (optional CSV override)
   - default topics:
   - `scada/+/edge/+/telemetry/tag/+`
   - `scada/+/edge/+/cmd/action/result`
   - `scada/+/edge/+/cmd/write/ack`
   - `scada/+/edge/+/audit/action`
   - `scada/+/edge/+/audit/write`
   - `scada/+/edge/+/health/runtime`
   - `scada/+/edge/+/alerts/runtime`
   - `scada/+/edge/+/alerts/runtime/ack`
   - `scada/+/edge/+/alerts/runtime/ack/result`
   - `scada/+/edge/+/config/apply/result`
   - `scada/+/edge/+/control/reset/result`
   - `scada/+/edge/+/conn/state`
   - `scada/+/edge/+/device/conn/state`
6. `CENTRAL_PG_DSN`
   - default: `host=127.0.0.1 user=postgres password=postgres dbname=ifascada`
7. `CENTRAL_REDIS_ENABLED` (default: `false`)
8. `CENTRAL_REDIS_URL` (default: `redis://127.0.0.1:6379/`)
9. `CENTRAL_REDIS_EVENT_CHANNEL` (default: `scada:rt:events`)
10. `CENTRAL_REDIS_KEY_TTL_SECS` (default: `300`)
