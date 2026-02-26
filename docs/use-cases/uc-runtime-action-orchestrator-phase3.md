# UC: Runtime Action Orchestrator (Phase 3)

## Objetivo
Separar ejecución de acciones del `mqtt_bridge` usando un contrato genérico:
`ActionRequest -> ActionExecutor`.

## Implementación
Nuevo módulo:
- `crates/edge-agent/src/action_orchestrator.rs`

Contrato:
1. `ActionRequest` (request_id, action_type, target, payload)
2. `ActionExecutor` (trait async)
3. `ActionOrchestrator` (registro de ejecutores por `action_type`)

Ejecutores built-in:
1. `print.escpos`
2. `print.escpos.from_buffer`
3. `buffer.weights.accumulate`
4. `connection.check`
5. `print.persist` (no-op local, marker para pipeline central)

## Integración
`mqtt_bridge` ahora:
1. construye `ActionOrchestrator::new_default()`
2. convierte `EdgeActionCommandMessage` a `ActionRequest`
3. delega ejecución al orquestador

Observabilidad agregada:
1. `health/runtime` incluye `action_metrics` por tipo de acción.
2. Contadores por `action_type`:
   - `received_total`
   - `accepted_total`
   - `failed_total`

Consumo en Web UI:
1. `GET /api/edges/current` expone `action_metrics` del último `edge_health_events.payload_json`.
2. `/live` muestra tabla `Edge Action Metrics` para el edge del tag seleccionado.

Esto aplica tanto para:
1. comandos MQTT manuales (`cmd/action`)
2. acciones disparadas por automations en runtime

## Compatibilidad
No se cambian:
1. topics MQTT
2. payloads de comandos/resultados/auditoría
3. semántica funcional de acciones existentes

## Validación
1. `cargo test -p edge-agent` verde
2. `cargo check -p application` verde
3. `cargo check -p central-server` verde
