# IFASCADA - Guia Completa de Instalacion y Configuracion

## 1. Objetivo
Esta guia documenta:
1. Instalacion base.
2. Arranque de infraestructura, `central-server`, `edge-agent` y `web-ui`.
3. Configuracion distribuida en red LAN.
4. Referencia completa de variables de entorno usadas por el codigo actual.

Guia recomendada para operacion diaria con `.env`:
1. `docs/guia-env-unificada-local-lan.md`

## 2. Requisitos
1. Git.
2. Rust toolchain + Cargo (estable).
3. Node.js 20+ y npm.
4. Docker Desktop (o Docker Engine) con `docker compose`.
5. `psql` (recomendado, para migraciones manuales).
6. Opcional para pruebas MQTT: `mosquitto_pub`, `mosquitto_sub`.

## 3. Estructura de despliegue recomendada
1. Infraestructura en Docker:
   - Timescale/Postgres
   - Redis
   - Mosquitto
   - pgAdmin
2. Servicios de codigo en host (sin Docker):
   - `central-server`
   - `edge-agent`
   - `web-ui`

## 4. Instalacion base
```powershell
git clone <repo-url> ifascada
cd ifascada
```

Compilar binarios (opcional, recomendado para despliegue):
```powershell
cargo build --release -p central-server
cargo build --release -p edge-agent
```

Instalar UI:
```powershell
cd web-ui
npm install
cd ..
```

## 5. Infraestructura (Docker)
Arranque:
```powershell
docker compose -f docker-compose.scada.yml up -d
```

Servicios y puertos host por defecto:
1. Postgres/Timescale: `55432`
2. Redis: `56379`
3. Mosquitto MQTT: `51883`
4. pgAdmin: `58080`

## 6. Base de datos: migraciones y seed minimo
Aplicar migraciones (orden recomendado):
```powershell
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0001_core_postgres.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0002_timescale_historian.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0003_tag_naming_governance.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0005_fix_tag_naming_constraint_regex.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0006_context_hierarchy.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0004_dev_seed_minimal_catalog.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0007_dev_seed_context_hierarchy.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0009_operational_events.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0010_connection_domain_state.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0011_device_domain_state.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0012_edges_metadata_json.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0013_scale_manual_config_in_catalog.sql"
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "crates/central-server/migrations/0014_dev_seed_modbus_rtu_com10_multi_slave.sql"
```

Nota de funcionamiento:
1. Sin catalogo (`sites/edges/devices/tags`), la central igual persiste ingesta cruda en `telemetry_ingest_events`.
2. Para estados live completos (`/api/tags/current`, `/api/devices/current`, etc.) se requiere catalogo.

## 7. Arranque de `central-server`
Ejemplo minimo local:
```powershell
$env:CENTRAL_MQTT_ENABLED = "true"
$env:CENTRAL_API_ENABLED = "true"
$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "51883"
$env:CENTRAL_PG_DSN = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable"
$env:CENTRAL_REDIS_ENABLED = "true"
$env:CENTRAL_REDIS_URL = "redis://127.0.0.1:56379/"
$env:CENTRAL_API_BIND = "127.0.0.1:8088"

cargo run -p central-server
```

## 8. Arranque de `edge-agent`
### 8.1 Modo MQTT-only (sin config remota por HTTP)
```powershell
$env:EDGE_MQTT_ENABLED = "true"
$env:EDGE_SITE = "plant-a"
$env:EDGE_AGENT = "edge-01"
$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "51883"
$env:EDGE_BOOTSTRAP_PATH = "crates/edge-agent/config/bootstrap.dev.manual-scale.json"

cargo run -p edge-agent
```

### 8.2 Modo config remota firmada (HTTP + MQTT)
```powershell
$env:EDGE_MQTT_ENABLED = "true"
$env:EDGE_SITE = "plant-a"
$env:EDGE_AGENT = "edge-01"
$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "51883"

$env:EDGE_CONFIG_URL = "http://127.0.0.1:8088"
$env:EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
$env:EDGE_CONFIG_HMAC_SECRET = "dev-edge-config-signing-secret"
$env:EDGE_CONFIG_KEY_ID = "v1"

cargo run -p edge-agent
```

## 9. Arranque de `web-ui`
```powershell
cd web-ui
$env:NEXT_PUBLIC_API_BASE = "http://127.0.0.1:8088"
$env:NEXT_PUBLIC_SSE_URL = "http://127.0.0.1:8088/api/stream/events"
$env:NEXT_PUBLIC_OPS_SSE_URL = "http://127.0.0.1:8088/api/ops/events/stream"
npm run dev -- --hostname 0.0.0.0 -p 3015
```

Ruta live:
1. `http://<host>:3015/live`

## 9.1 Launcher simple para cargar `.env` y ejecutar binario
Se agregó el binario `launcher` (crate `crates/launcher`) para cargar variables desde un archivo `.env` y luego ejecutar el programa objetivo.

Build:
```powershell
cargo build --release -p launcher
```

Uso:
```powershell
launcher [--env-file <ruta-env>] [--config <ruta-toml>] -- <programa> [args...]
```

Ejemplos:
```powershell
.\target\release\launcher.exe --env-file .\release\secrets\central.env -- .\target\release\central-server.exe
.\target\release\launcher.exe --env-file .\release\secrets\edge-51.env -- .\target\release\edge-agent.exe
.\target\release\launcher.exe --env-file .\release\secrets\edge-51.env --config .\release\config\edge-51.toml -- .\target\release\edge-agent.exe
```

Formato `.env` soportado:
1. `KEY=VALUE`
2. lineas vacias y comentarios con `#`
3. valores con comillas simples o dobles

Nota:
1. `--config` agrega automaticamente `--config <ruta>` al comando objetivo.
2. El binario objetivo debe soportar ese argumento para que aplique.

## 10. Ejemplo distribuido (LAN)
Topologia:
1. Infra: `192.168.103.40`
2. Central: `192.168.103.41`
3. Edge A: `192.168.103.51`
4. Edge B: `192.168.103.52`

Central (`192.168.103.41`):
```powershell
$env:CENTRAL_MQTT_ENABLED = "true"
$env:CENTRAL_API_ENABLED = "true"
$env:MQTT_HOST = "192.168.103.40"
$env:MQTT_PORT = "51883"
$env:CENTRAL_PG_DSN = "host=192.168.103.40 port=55432 user=postgres dbname=rustscada sslmode=disable"
$env:CENTRAL_REDIS_ENABLED = "true"
$env:CENTRAL_REDIS_URL = "redis://192.168.103.40:56379/"
$env:CENTRAL_API_BIND = "0.0.0.0:8088"
cargo run -p central-server
```

Edge A (`192.168.103.51`):
```powershell
$env:EDGE_MQTT_ENABLED = "true"
$env:EDGE_SITE = "plant-a"
$env:EDGE_AGENT = "edge-51"
$env:MQTT_CLIENT_ID = "edge-51-client"
$env:MQTT_HOST = "192.168.103.40"
$env:MQTT_PORT = "51883"
$env:EDGE_CONFIG_URL = "http://192.168.103.41:8088"
$env:EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
$env:EDGE_CONFIG_HMAC_SECRET = "dev-edge-config-signing-secret"
$env:EDGE_CONFIG_KEY_ID = "v1"
cargo run -p edge-agent
```

Edge B (`192.168.103.52`):
```powershell
$env:EDGE_MQTT_ENABLED = "true"
$env:EDGE_SITE = "plant-a"
$env:EDGE_AGENT = "edge-52"
$env:MQTT_CLIENT_ID = "edge-52-client"
$env:MQTT_HOST = "192.168.103.40"
$env:MQTT_PORT = "51883"
$env:EDGE_CONFIG_URL = "http://192.168.103.41:8088"
$env:EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
$env:EDGE_CONFIG_HMAC_SECRET = "dev-edge-config-signing-secret"
$env:EDGE_CONFIG_KEY_ID = "v1"
cargo run -p edge-agent
```

Firewall minimo:
1. Infra host: `51883`, `55432`, `56379`
2. Central host: `8088`
3. Web host: puerto UI (ej. `3015`)

## 11. Referencia completa de variables de entorno

### 11.1 Infra Docker (`docker-compose.scada.yml`)
| Variable | Default en compose | Requerida | Descripcion |
|---|---|---:|---|
| `POSTGRES_USER` | `postgres` | si | Usuario de Postgres/Timescale |
| `POSTGRES_PASSWORD` | `postgres` | si (salvo trust local) | Password de Postgres |
| `POSTGRES_DB` | `rustscada` | si | Base inicial |
| `POSTGRES_HOST_AUTH_METHOD` | `trust` | no | Metodo de autenticacion host (dev) |
| `PGADMIN_DEFAULT_EMAIL` | `admin@ifascada.com` | si (si usas pgAdmin) | Usuario pgAdmin |
| `PGADMIN_DEFAULT_PASSWORD` | `admin` | si (si usas pgAdmin) | Password pgAdmin |

### 11.2 Central server
| Variable | Default | Requerida | Descripcion |
|---|---|---:|---|
| `RUST_LOG` | `central_server=info,central-server=info,info` | no | Nivel de logs |
| `CENTRAL_MQTT_ENABLED` | `true` | no | Habilita consumidor MQTT |
| `CENTRAL_API_ENABLED` | `true` | no | Habilita API HTTP |
| `CENTRAL_PG_DSN` | `host=127.0.0.1 user=postgres password=postgres dbname=ifascada` | si en prod | DSN de Postgres (incluye password) |
| `CENTRAL_REDIS_ENABLED` | `false` | no | Habilita cache/event bus en Redis |
| `CENTRAL_REDIS_URL` | `redis://127.0.0.1:6379/` | si si Redis habilitado | URL Redis |
| `CENTRAL_REDIS_EVENT_CHANNEL` | `scada:rt:events` | no | Canal pub/sub para realtime |
| `CENTRAL_REDIS_KEY_TTL_SECS` | `300` | no | TTL de claves realtime |
| `MQTT_HOST` | `127.0.0.1` | si si MQTT habilitado | Host broker MQTT |
| `MQTT_PORT` | `1883` | si si MQTT habilitado | Puerto broker MQTT |
| `CENTRAL_MQTT_CLIENT_ID` | `central-server-01` | no | Client ID MQTT ingesta |
| `CENTRAL_MQTT_TOPIC_FILTERS` | CSV (topics soportados por ingesta) | no | Override de filtros de suscripcion (si no se define, central usa lista explicita segura) |
| `CENTRAL_MQTT_CMD_CLIENT_ID` | `central-server-cmd-01` | no | Client ID MQTT para publicar comandos |
| `CENTRAL_API_BIND` | `127.0.0.1:8088` | no | Bind API (`0.0.0.0` para LAN) |
| `CENTRAL_EDGE_ENROLL_TOKEN` | `dev-edge-enroll-token` | si si config remota | Token de enrolamiento edge |
| `CENTRAL_EDGE_CONFIG_SIGNING_SECRET` | `dev-edge-config-signing-secret` | si si config remota | Secreto HMAC para firmar config |
| `CENTRAL_EDGE_CONFIG_SIGNING_KEY_ID` | `v1` | no | Identificador de llave de firma |
| `CENTRAL_EDGE_RUNTIME_CONFIG_PATH` | `crates/edge-agent/config/bootstrap.example.json` | si si config remota | Ruta del payload runtime firmado |
| `CENTRAL_OPS_EVENTS_RETENTION_DAYS` | `90` | no | Retencion de `operational_events` |
| `CENTRAL_OPS_EVENTS_CLEANUP_INTERVAL_SECS` | `3600` | no | Frecuencia de limpieza |
| `CENTRAL_OPS_EVENTS_CLEANUP_ENABLED` | `true` | no | Activa limpieza periodica |
| `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT` | `45` | no | Umbral stale para estado de edge/tag/device |

### 11.3 Edge agent
| Variable | Default | Requerida | Descripcion |
|---|---|---:|---|
| `RUST_LOG` | `edge_agent=info,info` | no | Nivel de logs |
| `EDGE_MQTT_ENABLED` | `false` | si para operacion normal | Habilita bridge MQTT |
| `EDGE_SITE` | `default-site` | recomendada | Site usado en topic MQTT |
| `EDGE_AGENT` | `edge-01` | recomendada | Identidad del edge en topics/config |
| `MQTT_HOST` | `127.0.0.1` | si si MQTT habilitado | Host broker |
| `MQTT_PORT` | `1883` | si si MQTT habilitado | Puerto broker |
| `MQTT_CLIENT_ID` | `edge-agent-01` | recomendada | Client ID MQTT |
| `MQTT_OUTBOX_PATH` | `./data/mqtt_outbox.db` | no | SQLite outbox local |
| `MQTT_OUTBOX_FLUSH_BATCH` | `50` | no | Tamano de flush outbox |
| `MQTT_OUTBOX_MAX_MESSAGES` | `10000` | no | Capacidad outbox |
| `MQTT_OUTBOX_ACTIVE_KEY_ID` | `v1` | no | Key ID activa outbox |
| `MQTT_OUTBOX_PREV_KEY_ID` | none | no | Key ID previa rotacion |
| `MQTT_OUTBOX_ENCRYPTION_SECRET` | none | no | Secreto cifrado payload outbox |
| `MQTT_OUTBOX_HMAC_SECRET` | none | no | Secreto firma payload outbox |
| `MQTT_OUTBOX_PREV_ENCRYPTION_SECRET` | none | no | Secreto cifrado previo |
| `MQTT_OUTBOX_PREV_HMAC_SECRET` | none | no | Secreto firma previo |
| `MQTT_HEALTH_PUBLISH_INTERVAL_SECS` | `30` | no | Intervalo health runtime |
| `MQTT_HEALTH_OUTBOX_DEPTH_WARN` | `1000` | no | Umbral warning profundidad outbox |
| `MQTT_HEALTH_OUTBOX_OLDEST_SECS_WARN` | `300` | no | Umbral warning antiguedad outbox |
| `MQTT_ALERT_DEGRADED_STREAK` | `3` | no | Streak para alerta degradacion |
| `MQTT_ALERT_RECOVERED_STREAK` | `3` | no | Streak para alerta recuperacion |
| `MQTT_ALERT_DEDUP_WINDOW_SECS` | `300` | no | Ventana deduplicacion alertas |
| `EDGE_CONFIG_URL` | none | no | URL central para check/pull config firmada |
| `EDGE_ENROLL_TOKEN` | `dev-edge-enroll-token` | si si `EDGE_CONFIG_URL` usado | Token de enrolamiento |
| `EDGE_CONFIG_HMAC_SECRET` | `dev-edge-config-signing-secret` | si si `EDGE_CONFIG_URL` usado | Secreto de validacion firma config |
| `EDGE_CONFIG_KEY_ID` | none | no | Validacion estricta `key_id` |
| `EDGE_RUNTIME_CACHE_PATH` | `./data/runtime_config.signed.json` | no | Cache local config firmada |
| `EDGE_CONFIG_APPLY_RECEIPT_PATH` | `./data/config_apply_receipt.json` | no | Recibo local de apply |
| `EDGE_CONFIG_CHECK_INTERVAL_SECS` | `120` | no | Frecuencia de check config |
| `EDGE_CONFIG_CHECK_JITTER_SECS` | `20` | no | Jitter check config |
| `EDGE_BOOTSTRAP_PATH` | none (`./config/bootstrap.json` fallback) | no | Bootstrap local cuando no hay config remota |

### 11.4 Web UI (Next.js)
| Variable | Default | Requerida | Descripcion |
|---|---|---:|---|
| `NEXT_PUBLIC_API_BASE` | `http://127.0.0.1:8088` | recomendada en LAN | Base URL API central |
| `NEXT_PUBLIC_SSE_URL` | `http://127.0.0.1:8088/api/stream/events` | recomendada en LAN | URL SSE runtime |
| `NEXT_PUBLIC_OPS_SSE_URL` | `http://127.0.0.1:8088/api/ops/events/stream` | recomendada en LAN | URL SSE operational events |
| `NEXT_PUBLIC_EDGE_STALE_SECS` | `45` | no | Umbral visual stale en UI |

## 12. Configuracion minima de DB para operacion
Para que el edge "publishe" no se requiere DB central.

Para tener vistas y resolucion completa en central:
1. Ejecutar migraciones `0001` a `0012`.
2. Minimo catalogo:
   - `sites`
   - `edges`
   - `devices`
   - `tags`
3. Recomendado para observabilidad completa:
   - `connections`
   - `devices.connection_id` mapeado

Si no existe mapeo de tag en catalogo:
1. Se persiste evento en `telemetry_ingest_events`.
2. No se completa `tag_current_state`/historian por `tag_id`.

Configuracion operativa sin ampliar esquema:
1. `connections.metadata_json` para `transport/frame/parser/timeouts`.
2. `devices.metadata_json` para politica de runtime por dispositivo.
3. `tags.metadata_json` para pipeline/transformacion de variable.

## 13. Notas de arquitectura (importante)
1. MQTT es el backbone operativo para telemetria/comandos/health.
2. `EDGE_CONFIG_URL` es opcional y solo aplica a sincronizacion de config firmada por HTTP.
3. Si quieres modo estricto MQTT-only, no uses `EDGE_CONFIG_URL` y define `EDGE_BOOTSTRAP_PATH`.
