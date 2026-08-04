# central-1.0.0-clean-runtime

Paquete minimo para instalar central y web-ui sin codigo fuente Rust/Next.

## Contenido
- docker-compose.yml
- .env.example
- scripts/install-central.ps1
- scripts/db-seed.sh
- sql/migrations/*.sql
- docker/mosquitto/mosquitto.conf
- images/ (opcional, para llevar central-server-1.0.0.tar y web-ui-1.0.0.tar)

## Instalacion
1. Copiar `.env.example` a `.env` y ajustar variables.
2. (Opcional) copiar `central-server-1.0.0.tar` y `web-ui-1.0.0.tar` en `images\`.
3. Ejecutar central + web-ui:

```powershell
.\scripts\install-central.ps1 -SeedProfile minimal -WithImageLoad -WithWebUi
```

## Verificacion
```powershell
docker compose -f .\docker-compose.yml ps
docker logs -f ifascada-central-server
docker logs -f ifascada-web-ui
```

Health central: http://127.0.0.1:8088/health/live
Web UI: http://127.0.0.1:3001
pgAdmin: http://127.0.0.1:58080

## Nota de red interna
Si central no conecta al broker y muestra `failed to lookup address information`, verificar:
1. `MQTT_HOST_INTERNAL=ifascada-mosquitto` en `.env`.
2. Recrear central:
```powershell
docker compose -f .\docker-compose.yml --profile central up -d --force-recreate central-server
```

Si web-ui responde `server error` en rutas `/api/*` (ej. `/api/tags/current`), verificar:
1. `CENTRAL_API_UPSTREAM=http://ifascada-central-server:8088` en `.env`.
2. `NEXT_PUBLIC_API_BASE=/api` y `NEXT_PUBLIC_SSE_URL=/api/stream/events`.
3. Recrear web-ui:
```powershell
docker compose -f .\docker-compose.yml --profile webui up -d --force-recreate web-ui
```
