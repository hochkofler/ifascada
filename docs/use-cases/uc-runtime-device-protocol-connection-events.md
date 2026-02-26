# UC: Estado de conexión por protocolo a nivel device (sin mezclar `quality`)

## Objetivo
Diferenciar explícitamente:
- `tag.quality.*` (calidad de dato),
- `device.connection.*` (éxito/falla de comunicación de protocolo por dispositivo).

Esto evita diagnosticar desconexiones de protocolo usando únicamente `quality`.

## Flujo implementado
1. `ConnectionRuntime` detecta resultado de polling por tag:
- éxito: transición de device a `Connected` (si venía en error),
- error: transición de device a `Error`.
2. El edge publica evento MQTT en:
- `scada/{site}/edge/{edge}/device/conn/state`
3. Central ingiere ese evento y lo persiste en `operational_events` como:
- `device.connection.connected`
- `device.connection.error`
4. La vista `Audit` lo muestra en tiempo real y permite filtrarlo por `event_type`.

## Payload MQTT (schema v1)
```json
{
  "schema_version": 1,
  "source": "edge-agent",
  "connection_id": "conn_modbus_rtu_com10_1",
  "device_id": "dev_modbus_100",
  "tag_id": "tag_airborne_particle_pm1",
  "state": "Error",
  "reason": "modbus read timeout after 300 ms",
  "timestamp": "2026-02-23T15:00:00Z"
}
```

## Notas de diseño
- Se emite por transición de estado de protocolo por `device` (no por cada poll).
- No reemplaza `device.status.*`; complementa observabilidad operacional.
- Permite distinguir “conecté transporte” vs “dispositivo no responde por protocolo”.
