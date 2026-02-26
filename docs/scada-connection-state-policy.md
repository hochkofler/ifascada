# Politica Unificada de Estado de Conexion (Edge / Connection / Device / Tag)

## Objetivo
Definir una unica politica de estados para que las lamparas del HMI sean deterministas y no mezclen:
- conectividad/comunicacion,
- frescura de datos,
- calidad de dato (`quality`).

## Estados canonicos
Todas las entidades usan solo:
- `connected`
- `stale`
- `disconnected`

## Estado actual (implementado hoy)
1. `edge`
- Se deriva en API desde `edge_current_state.status` + timeout por heartbeat (`CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT`, default 45s).
- Si expira heartbeat, API ya lo fuerza a `disconnected`.

2. `connection`
- Se deriva de eventos runtime (`conn/state`): `connected`, `connecting`, `failed`, `disconnected`.
- Central persiste en `connection_current_state`.

3. `tag`
- Se deriva por precedencia: `edge -> connection -> sample_age`.
- Regla vigente en dominio:
  - edge no online/stale => `disconnected`
  - connection failed/disconnected => `disconnected`
  - connection connecting => `stale`
  - si `expected_interval_ms` no existe => `connected`
  - si existe => `stale` cuando `sample_age` supera ventana.
- `quality` no define `tag_status`.

4. `device`
- Se deriva por precedencia: `edge -> connection -> agregado de tags`.
- Histeresis/debounce ya implementada en central para evitar flapping de `device.status.*`.

5. Observabilidad de protocolo por device
- Eventos `device.connection.connected` / `device.connection.error` ya existen en `operational_events`.
- Hoy son observabilidad/auditoria; aun no gobiernan directamente todas las lamparas.

## Politica objetivo (debe regir API + UI)
## 1) Edge
Regla:
1. `connected` si `status in {ok, online}` y `now - last_seen_at <= edge_stale_after_secs`.
2. `stale` si heartbeat retrasado pero no vencido duro (opcional por ventana intermedia).
3. `disconnected` si no heartbeat o vencido.

Recomendacion inicial:
- `edge_stale_after_secs = 45`
- `edge_disconnected_after_secs = 90` (si habilitamos estado intermedio real de edge).

## 2) Connection
Regla:
1. `disconnected` si ultimo estado es `failed/disconnected` o si no hay update dentro de timeout de sesion.
2. `stale` si esta en `connecting` o sin confirmacion reciente.
3. `connected` si ultimo estado `connected` reciente.

## 3) Tag
Regla:
1. Si edge disconnected => tag disconnected.
2. Si connection disconnected/failed => tag disconnected.
3. Si connection connecting => tag stale.
4. Si hay politica temporal del tag:
   - `connected` dentro de ventana,
   - `stale` fuera de ventana suave,
   - `disconnected` fuera de ventana dura.
5. `quality` no cambia la lampara de conexion; solo informa calidad.

Regla clave:
- Todo tag debe tener politica temporal explicita (`expected_interval_ms` o perfil equivalente).
- Si falta, no asumimos `connected` permanente en produccion.

## 4) Device
Regla:
1. Precedencia superior: edge/connection.
2. Si edge o connection cae => `disconnected`.
3. Si no cae:
   - `connected` si al menos un tag `connected`,
   - `stale` si hay tags stale y ninguno connected,
   - `disconnected` si todos disconnected.
4. Aplicar histeresis/debounce para anti-flapping.

## Separacion obligatoria de conceptos
1. `*_status` (lamparas):
- representa conectividad y frescura.

2. `quality`:
- representa validez/calidad del valor.

3. `device.connection.*`:
- representa exito/error de comunicacion de protocolo por dispositivo (audit/ops).

Nunca usar `quality` como sustituto de `*_status`.

## Extrapolacion a API
Endpoints `current` deben devolver, para cada entidad:
1. `state` (`connected|stale|disconnected`)
2. `reason_code` (estable, machine-friendly)
3. `last_seen_at`
4. `last_change_at`
5. `source` (`heartbeat|connection_runtime|derived`)

`reason_code` inicial sugerido:
- `edge_offline_or_stale`
- `connection_disconnected`
- `connection_connecting`
- `tag_window_soft_expired`
- `tag_window_hard_expired`
- `device_all_tags_disconnected`
- `device_has_stale_tags`

## Extrapolacion a UI (HMI)
1. Lamparas (verde/amarillo/rojo) solo por `state`.
2. Badge de calidad aparte (`Good/Bad` + reason).
3. Tooltip o detalle usa `reason_code` y timestamps.
4. Audit expone transiciones de:
- `edge.status.*`
- `connection.*`
- `device.status.*`
- `device.connection.*`
- `tag.status.*` (cuando se incorpore persistencia de transicion).

## Brecha actual -> objetivo
1. Hoy un tag sin `expected_interval_ms` puede quedar `connected` indefinidamente.
- Cambio: forzar politica temporal explicita o perfil por defecto por tipo de tag.

2. Falta estado `disconnected` por ventana dura en evaluacion de tag.
- Cambio: agregar doble umbral (soft/hard).

3. Eventos `device.connection.*` no gobiernan aun toda derivacion de estado.
- Cambio: integrarlos como señal adicional en recomputo de `device_status`.

4. `reason` en API existe pero no totalmente estandarizado.
- Cambio: introducir `reason_code` estable.

## Estado de avance aplicado
1. `device_status` ahora prioriza `edge + connection + device_protocol_state` (signal `device.connection.*`).
2. `tag_status` ya no es la señal principal para lampara de `device`.
3. En HMI Live, la lampara de `device` se gobierna por `device.state` (con edge como guardrail), y `tag` queda en modo diagnostico.

## Plan de implementacion ordenado (TDD/DDD)
Fase A (contrato y dominio)
1. Tests dominio para `tag` con doble umbral (soft/hard).
2. Tests dominio para `device` con precedencia explicita y combinaciones limite.
3. Definir `reason_code` canonico.

Fase B (central)
1. Actualizar evaluadores y recomputo.
2. Persistir transiciones de `tag.status.*` (solo cambio).
3. Exponer `reason_code` en `/api/*/current`.

Fase C (UI)
1. Lamparas solo por `state`.
2. `quality` siempre separado.
3. Audit con filtros cerrados por tipo de evento.

Fase D (operacion)
1. Sweep periodico para expiraciones por tiempo sin depender de nuevos mensajes.
2. Metricas de flapping por entidad.
3. Runbook de diagnostico por `reason_code`.

## Criterios de aceptacion
1. Si se corta heartbeat, edge/device/tag cambian segun SLA sin refresco manual.
2. Si hay timeout de protocolo, aparece `device.connection.error` y la jerarquia converge a estado degradado.
3. `quality=Bad` no apaga/enciende lamparas por si sola.
4. No hay spam de eventos: solo transiciones.

## Politica especial: dispositivos on-demand (sin telemetria)
Aplicable a actuadores como impresoras compartidas (Windows spooler o TCP), donde no hay stream de tags.

Configuracion en `devices.metadata_json`:
```json
{
  "status_policy": {
    "mode": "on_demand",
    "stale_after_secs": 3600
  }
}
```

Reglas:
1. `mode=on_demand` evita depender de heartbeat/tag polling para el estado del `device`.
2. El estado se deriva de eventos operativos `device.connection.*` generados por ejecucion real de jobs.
3. Ultimo evento `connected` -> `connected`.
4. Ultimo evento `error|disconnected` -> `disconnected`.
5. Si no hay eventos recientes y `stale_after_secs` aplica -> `stale`.
6. Si `stale_after_secs` no se define, el estado no vence por tiempo (permanece hasta nuevo evento).
