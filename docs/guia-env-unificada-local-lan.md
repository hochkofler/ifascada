# IFASCADA - Guia de Configuracion Unificada con `.env` (Local + LAN)

## 1. Objetivo
Definir una forma unica y simple de configuracion usando archivos `.env` para:
1. Infraestructura Docker.
2. `central-server`.
3. `edge-agent`.
4. `web-ui`.

Escenario objetivo actual:
1. Todo corre en `192.168.103.70`.

## 2. Regla operativa
1. No depender de `$env:...` manual en cada consola.
2. Guardar variables por componente en archivos `.env`.
3. Ejecutar binarios Rust con `launcher` para cargar `.env`.

## 3. Archivos `.env` recomendados

### 3.1 `./.env.central`
```env
RUST_LOG=info,central_server=debug,central_server::mqtt_consumer=debug,central_server::ingestion=debug,central_server::persistence::postgres=debug

CENTRAL_MQTT_ENABLED=true
CENTRAL_API_ENABLED=true
CENTRAL_API_BIND=0.0.0.0:8088

MQTT_HOST=192.168.103.70
MQTT_PORT=51883
CENTRAL_MQTT_CLIENT_ID=central-server-01
CENTRAL_MQTT_TOPIC_FILTERS=scada/+/edge/+/telemetry/tag/+,scada/+/edge/+/cmd/action/result,scada/+/edge/+/cmd/write/ack,scada/+/edge/+/audit/action,scada/+/edge/+/audit/write,scada/+/edge/+/health/runtime,scada/+/edge/+/alerts/runtime,scada/+/edge/+/alerts/runtime/ack,scada/+/edge/+/alerts/runtime/ack/result,scada/+/edge/+/config/apply/result,scada/+/edge/+/control/reset/result,scada/+/edge/+/conn/state,scada/+/edge/+/device/conn/state
CENTRAL_MQTT_CLEAN_SESSION=false
CENTRAL_MQTT_MANUAL_ACKS=true

CENTRAL_PG_DSN=host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable

CENTRAL_REDIS_ENABLED=true
CENTRAL_REDIS_URL=redis://192.168.103.70:56379/
CENTRAL_REDIS_EVENT_CHANNEL=scada:rt:events
CENTRAL_REDIS_KEY_TTL_SECS=300
```

### 3.2 `./.env.edge-com7`
```env
RUST_LOG=info,edge_agent=debug,edge_agent::mqtt_bridge=debug,application::runtime::connection_runtime=debug

EDGE_MQTT_ENABLED=true
EDGE_SITE=plant-a
EDGE_AGENT=edge-01

MQTT_HOST=192.168.103.70
MQTT_PORT=51883
MQTT_CLIENT_ID=edge-01

EDGE_BOOTSTRAP_PATH=crates/edge-agent/config/bootstrap.dev.manual-scale.json

# Generic actions / ESC-POS
EDGE_ESCPOS_OUTPUT_PATH=./data/escpos_output.bin
# EDGE_ESCPOS_TCP_HOST=192.168.103.200
# EDGE_ESCPOS_TCP_PORT=9100
# Optional generic on-demand startup probe (single-use device)
# EDGE_ON_DEMAND_TCP_HOST=192.168.103.154
# EDGE_ON_DEMAND_TCP_PORT=9100
# EDGE_ON_DEMAND_PROBE_ENABLED=true
# EDGE_ON_DEMAND_PROBE_TIMEOUT_MS=1200
# EDGE_ON_DEMAND_PROBE_CONNECTION_ID=conn_printer_u220_1
# EDGE_ON_DEMAND_PROBE_DEVICE_ID=dev_printer_u220

# Optional trigger: auto print after N consecutive <= 0
EDGE_AUTO_PRINT_NONPOS_ENABLED=false
EDGE_AUTO_PRINT_TAGS=tag_scale_manual_compound
EDGE_AUTO_PRINT_CONSECUTIVE=2
```

### 3.3 `./web-ui/.env.local`
```env
NEXT_PUBLIC_API_BASE=http://192.168.103.70:8088
NEXT_PUBLIC_SSE_URL=http://192.168.103.70:8088/api/stream/events
NEXT_PUBLIC_OPS_SSE_URL=http://192.168.103.70:8088/api/ops/events/stream
NEXT_PUBLIC_EDGE_STALE_SECS=45
```

## 4. Infraestructura Docker

### 4.1 Levantar
```powershell
docker compose -f docker-compose.scada.yml up -d
```

### 4.2 Verificar
```powershell
docker ps --filter "name=ifascada-"
```

Servicios esperados:
1. MQTT: `192.168.103.70:51883`
2. Postgres/Timescale: `192.168.103.70:55432`
3. Redis: `192.168.103.70:56379`
4. pgAdmin: `http://192.168.103.70:58080`

## 5. Ejecutar servicios con `.env`

## 5.1 Central
```powershell
cargo run -p launcher -- --env-file .env.central -- cargo run -p central-server
```

## 5.2 Edge
```powershell
cargo run -p launcher -- --env-file .env.edge-com7 -- cargo run -p edge-agent
```

## 5.3 Web UI (LAN)
```powershell
cd web-ui
npm run dev -- --hostname 0.0.0.0 -p 3015
```

UI:
1. `http://192.168.103.70:3015/live`

## 6. Reinicios y persistencia de configuracion
1. Si reinicias Docker, se mantiene lo definido en `docker-compose`.
2. Las variables de PowerShell (`$env:...`) no persisten entre sesiones.
3. Los `.env` de `central` y `edge` se vuelven a aplicar cada vez que lanzas con `launcher`.
4. Si cambias `web-ui/.env.local`, reinicia `npm run dev`.

## 7. Migraciones DB (si hace falta)
```powershell
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0001_core_postgres.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0002_timescale_historian.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0003_tag_naming_governance.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0005_fix_tag_naming_constraint_regex.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0006_context_hierarchy.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0004_dev_seed_minimal_catalog.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0007_dev_seed_context_hierarchy.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0009_operational_events.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0010_connection_domain_state.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0011_device_domain_state.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0012_edges_metadata_json.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0016_telemetry_received_at.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0013_scale_manual_config_in_catalog.sql"
psql "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0014_dev_seed_modbus_rtu_com10_multi_slave.sql"
```

Distribucion de configuracion sin tablas nuevas:
1. `connections.metadata_json`: `transport`, `frame`, `parser`, `timeouts`.
2. `devices.metadata_json`: politicas de runtime por dispositivo.
3. `tags.metadata_json`: pipeline/transformacion y preferencias de visualizacion.

## 8. Troubleshooting rapido
1. API vive: `http://192.168.103.70:8088/health/live`
2. UI vacia pero API viva:
   - revisar ingesta MQTT,
   - revisar catalogo (`sites/edges/devices/tags`),
   - revisar filtros de contexto en UI.
3. Error `127.0.0.1` desde otra PC:
   - corregir `web-ui/.env.local`,
   - reiniciar `npm run dev`.
