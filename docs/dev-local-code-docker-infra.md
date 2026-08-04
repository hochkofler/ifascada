# Development Mode: Local Code + Docker Infrastructure

Referencia principal de instalacion y variables:
1. `docs/guia-instalacion-configuracion-completa.md`
2. Deploy central en contenedor: `docs/deploy-central-compose.md`

## Goal
Run only infrastructure in Docker and keep all Rust services (`central-server`, `edge-agent`) running locally for fast iteration.

## Infra stack
Start:
```powershell
docker compose -f docker-compose.scada.yml up -d
```

Stop:
```powershell
docker compose -f docker-compose.scada.yml down
```

Shortcut scripts:
```powershell
.\scripts\dev-stack-up.ps1
.\scripts\dev-stack-down.ps1
```

Integrated local run (infra + central + edge manual + hmi):
```powershell
.\scripts\dev-run-all-local.ps1 -ComPort COM7
```

Integrated local run with simulated edge-agents in Docker (4 edges, 20 tags):
```powershell
.\scripts\dev-run-all-local.ps1 -ComPort COM7 -StartSimEdges -RebuildSimEdges
```

If simulated edges show DNS errors to MQTT (`failed to lookup address information`):
1. Ensure base infra is up first (creates Docker network `ifascada_default` and broker container):
```powershell
docker compose -f docker-compose.scada.yml up -d
```
2. Then start simulated edges:
```powershell
docker compose -f docker-compose.edge-sim.yml up -d --build
```
3. Verify network attachment:
```powershell
docker network inspect ifascada_default
```

Smoke E2E (single command):
```powershell
.\scripts\smoke-e2e-live.ps1 -ComPort COM7 -UiPort 3015
```

Stop infra + simulated edge-agents:
```powershell
.\scripts\dev-stack-down.ps1 -WithSimEdges
```

Services and host ports:
1. TimescaleDB: `127.0.0.1:55432`
2. Redis: `127.0.0.1:56379`
3. Mosquitto: `127.0.0.1:51883`
4. pgAdmin: `http://127.0.0.1:58080`

## Central-server local
```powershell
$env:CENTRAL_MQTT_ENABLED = "true"
$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "51883"
$env:CENTRAL_PG_DSN = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable"
$env:CENTRAL_REDIS_ENABLED = "true"
$env:CENTRAL_REDIS_URL = "redis://127.0.0.1:56379/"
$env:CENTRAL_REDIS_EVENT_CHANNEL = "scada:rt:events"
$env:CENTRAL_REDIS_KEY_TTL_SECS = "300"
cargo run -p central-server
```

## Edge-agent local
```powershell
$env:EDGE_MQTT_ENABLED = "true"
$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "51883"
$env:EDGE_SITE = "plant-a"
$env:EDGE_AGENT = "edge-01"
$env:EDGE_BOOTSTRAP_PATH = "crates/edge-agent/config/bootstrap.dev.manual-scale.json"
cargo run -p edge-agent
```

Bootstrap `bootstrap.dev.manual-scale.json` includes:
1. One `SerialAscii` real connection for manual scale on COM port.
2. No inline mock, to keep logs clean in the manual edge.

## Simulated edges as containers
Containerized simulated edges:
1. `ifascada-edge-sim-pack-1`
2. `ifascada-edge-sim-pack-2`
3. `ifascada-edge-sim-mix-1`
4. `ifascada-edge-sim-mix-2`

Each container runs `edge-agent` with `Simulator` driver and publishes 5 tags (20 total) to MQTT.
Each simulated edge uses a unique `MQTT_CLIENT_ID` to avoid broker session collisions.

Useful commands:
```powershell
docker logs -f ifascada-edge-sim-pack-1
docker stop ifascada-edge-sim-pack-1
docker start ifascada-edge-sim-pack-1
docker ps --filter "name=ifascada-edge-sim"
```

This gives isolated logs and controlled connect/disconnect tests per simulated edge.

## Web HMI local (React/Next)
```powershell
cd web-ui
npm install
npm run dev
```

Default URL:
- `http://127.0.0.1:3001`

Environment:
1. `NEXT_PUBLIC_API_BASE=http://127.0.0.1:8088`
2. `NEXT_PUBLIC_SSE_URL=http://127.0.0.1:8088/api/stream/events`

Manual scale frame format (ASCII line):
1. `+ 12.4354 g`
2. `-0.120 kg`
3. `  8.0000   g  `

## Why this mode
1. No container rebuild for each code change.
2. Faster compile/test loop in local Rust toolchain.
3. Same infra behavior as production-like environment.

## Distributed deployment example (LAN)
Scenario:
1. Infra (Docker): `192.168.103.40`
2. Central server: `192.168.103.41`
3. Edge 1: `192.168.103.51`
4. Edge 2: `192.168.103.52`

### 1) Infra host (`192.168.103.40`)
```powershell
docker compose -f docker-compose.scada.yml up -d
```

Required reachable ports from other hosts:
1. MQTT: `51883/tcp`
2. Postgres/Timescale: `55432/tcp`
3. Redis: `56379/tcp`

### 2) Central host (`192.168.103.41`)
```powershell
$env:CENTRAL_MQTT_ENABLED = "true"
$env:MQTT_HOST = "192.168.103.40"
$env:MQTT_PORT = "51883"
$env:CENTRAL_MQTT_CLIENT_ID = "central-server-01"
$env:CENTRAL_MQTT_TOPIC_FILTERS = "scada/+/edge/+/telemetry/tag/+,scada/+/edge/+/cmd/action/result,scada/+/edge/+/cmd/write/ack,scada/+/edge/+/audit/action,scada/+/edge/+/audit/write,scada/+/edge/+/health/runtime,scada/+/edge/+/alerts/runtime,scada/+/edge/+/alerts/runtime/ack,scada/+/edge/+/alerts/runtime/ack/result,scada/+/edge/+/config/apply/result,scada/+/edge/+/control/reset/result,scada/+/edge/+/conn/state,scada/+/edge/+/device/conn/state"

$env:CENTRAL_API_ENABLED = "true"
$env:CENTRAL_API_BIND = "0.0.0.0:8088"

$env:CENTRAL_PG_DSN = "host=192.168.103.40 port=55432 user=postgres dbname=rustscada sslmode=disable"
$env:CENTRAL_REDIS_ENABLED = "true"
$env:CENTRAL_REDIS_URL = "redis://192.168.103.40:56379/"
$env:CENTRAL_REDIS_EVENT_CHANNEL = "scada:rt:events"
$env:CENTRAL_REDIS_KEY_TTL_SECS = "300"

$env:CENTRAL_EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
$env:CENTRAL_EDGE_CONFIG_SIGNING_SECRET = "dev-edge-config-signing-secret"
$env:CENTRAL_EDGE_CONFIG_SIGNING_KEY_ID = "v1"
$env:CENTRAL_EDGE_RUNTIME_CONFIG_PATH = "C:\ifascada\crates\edge-agent\config\bootstrap.example.json"

cargo run -p central-server
```

### 3) Edge host (`192.168.103.51`) - Edge 1
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

### 4) Edge host (`192.168.103.52`) - Edge 2
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

### 5) Quick checks
1. `http://192.168.103.41:8088/health/live`
2. `http://192.168.103.41:8088/api/edges/current`

## FAQ (operational)
1) Where is DB password configured in central?
- In `CENTRAL_PG_DSN`. Example:
```powershell
$env:CENTRAL_PG_DSN = "host=192.168.103.40 port=55432 user=postgres password=YOUR_PASSWORD dbname=rustscada sslmode=disable"
```

2) What is `http://<host>:3015/live` used for?
- Web HMI route only (UI). It is not required for central-edge telemetry processing.

3) Why `EDGE_MQTT_ENABLED=true`?
- `true`: edge starts MQTT bridge, publishes telemetry/health, receives MQTT control messages.
- `false`: edge does not run MQTT bridge; it starts runtime briefly and exits, so no MQTT integration.

4) Why set `EDGE_SITE`?
- Current MQTT topic namespace uses `scada/{site}/edge/{agent}/...`.
- So `EDGE_SITE` is required to build publish/subscribe topics deterministically.

5) Why `EDGE_CONFIG_URL` if architecture is MQTT-first?
- `EDGE_CONFIG_URL` is optional and only for signed runtime config pull/check over HTTP.
- If omitted, edge can run MQTT-only with local bootstrap (`EDGE_BOOTSTRAP_PATH`) and no central HTTP config sync.

6) Minimum DB data required for edge to "work"
- Edge runtime itself does not need central DB.
- For central ingestion only (raw events): no catalog rows are required; telemetry goes to `telemetry_ingest_events`.
- For full live model (`/api/tags/current`, device/connection state):
1. `sites`
2. `edges`
3. `devices`
4. `tags`
5. (recommended) `connections` + `devices.connection_id` mapping

Practical minimum seed:
1. Run migrations up to `0012`.
2. Run `0004_dev_seed_minimal_catalog.sql` as baseline and adapt site/edge/tag IDs to your deployment.
