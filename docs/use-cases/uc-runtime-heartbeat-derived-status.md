# UC-CENTRAL-STATUS-HEARTBEAT-001: Estado efectivo por heartbeat

## Objetivo
Garantizar que `edge`, `device` y `tag` cambien a estado no activo cuando expira el heartbeat, incluso sin telemetría nueva.

## Regla funcional
1. Si `now - edge_current_state.last_seen_at > CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT`:
   - `edge` efectivo = `disconnected`
   - `device` efectivo = `disconnected` con `reason=edge_offline_or_stale`
   - `tag_status` efectivo = `disconnected`
2. Esta derivación es de lectura (API current), no depende de eventos entrantes.

## Implementación
1. API:
   - `crates/central-server/src/api.rs`
   - Endpoints:
     - `GET /api/edges/current`
     - `GET /api/devices/current`
     - `GET /api/tags/current`
2. UI live:
   - `web-ui/app/live/page.tsx`
   - polling periódico de `edges/tags/devices` para reflejar timeout sin SSE.
3. Persistencia:
   - `crates/central-server/src/persistence/postgres.rs`
   - fallback de `edge_stale_after_secs` por env (sin dependencia de `edges.metadata_json` en esta ruta).

## Configuración
1. `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT` (default `45`).

## Pruebas de integración (TDD)
Archivo:
1. `crates/central-server/tests/api_runtime_status_heartbeat_contract_tests.rs`

Casos:
1. `edges_current_marks_disconnected_when_heartbeat_expired`
2. `devices_current_marks_disconnected_when_edge_heartbeat_expired`
3. `tags_current_marks_disconnected_when_edge_heartbeat_expired`
