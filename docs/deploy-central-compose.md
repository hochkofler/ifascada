# Deploy central con Docker Compose (infra + seed + central-server)

Este flujo mantiene el enfoque separado:
1. `central` en Docker Compose.
2. `edge` como instalador/servicio del SO.

## 1) Arranque robusto en un comando

```powershell
.\scripts\deploy-central.ps1 -SeedProfile minimal -BuildCentral
```

Que hace:
1. Levanta infra base (`timescaledb`, `redis`, `mosquitto`).
2. Ejecuta migraciones + seed con el perfil elegido (`minimal|sim20|full`) usando `db-seed`.
3. Arranca `central-server` en contenedor.

## 2) Perfiles de seed

1. `minimal`:
   - `0015_dev_seed_minimal_three_edges.sql`
   - `0017_printer_device_command_and_negative_trigger.sql`
2. `sim20`:
   - `0004_dev_seed_minimal_catalog.sql`
   - `0007_dev_seed_context_hierarchy.sql`
   - `0008_dev_seed_sim20_multi_area.sql`
3. `full`:
   - `sim20` + `0013` + `0014` + `0017`

## 3) Comandos directos Compose (sin script)

Infra:
```powershell
docker compose -f docker-compose.scada.yml up -d timescaledb redis mosquitto
```

Seed:
```powershell
$env:SEED_PROFILE = "minimal"
docker compose -f docker-compose.scada.yml --profile seed up --abort-on-container-exit --exit-code-from db-seed db-seed
docker compose -f docker-compose.scada.yml rm -f db-seed
Remove-Item Env:SEED_PROFILE -ErrorAction SilentlyContinue
```

Central:
```powershell
docker compose -f docker-compose.scada.yml --profile central up -d --build central-server
```

## 4) Verificacion

```powershell
docker compose -f docker-compose.scada.yml ps
docker logs -f ifascada-central-server
```

Health:
1. `http://127.0.0.1:8088/health/live`
2. `http://127.0.0.1:8088/api/edges/current`
