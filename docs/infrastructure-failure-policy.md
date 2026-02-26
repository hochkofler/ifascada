# Politica de Fallos de Infraestructura (Broker, Central, DB, Redis)

## Objetivo
Definir cuando un mensaje MQTT se considera "procesado" y cuando se mantiene para reintento sin perder datos operativos.

## Regla de oro
1. Un mensaje se confirma al broker (`ACK`) solo despues de persistir correctamente en Postgres.
2. Si falla parsing, validacion de esquema o persistencia, no se confirma (`no ACK`).
3. Redis nunca define consistencia; es solo acelerador realtime/cache.

## Configuracion recomendada en central
1. `CENTRAL_MQTT_CLEAN_SESSION=false`
2. `CENTRAL_MQTT_MANUAL_ACKS=true`
3. Suscripciones/editores de edge con `QoS 1`.

Con esto, si central cae, broker retiene mensajes QoS1 de la sesion durable y los reentrega al reconectar.

## Politica por componente

### Broker MQTT caido
1. Edge sigue operando.
2. Edge guarda en outbox local (store-and-forward).
3. Al recuperar broker, edge drena outbox respetando orden.
4. Edge no debe terminar proceso por corte MQTT; reintenta conexion con backoff.

### Central caido (broker vivo)
1. Broker conserva mensajes QoS1 para la sesion durable del central.
2. Al volver central, reprocesa backlog.
3. Se persiste:
   - `ts`: tiempo de origen del edge.
   - `received_at`: tiempo de recepcion/persistencia en central.
4. Central no debe terminar proceso por corte MQTT; reintenta conexion del consumer.

### Postgres caido
1. Central no puede persistir, por lo tanto no hace `ACK`.
2. Broker mantiene mensaje pendiente para reentrega.
3. Cuando Postgres vuelve, central procesa y recien confirma.

### Redis caido
1. No afecta consistencia historica ni estado fuente de verdad.
2. Solo se degrada realtime/cache temporal.
3. Central continua persistiendo en Postgres y confirmando broker.

## Validaciones minimas de mensaje
1. `schema_version` soportada.
2. Campos requeridos del contrato (`timestamp`, `tag_id`, `value`, etc.).
3. Si no cumple contrato, no se confirma al broker.

## Notas de operacion
1. `received_at` debe existir en `telemetry_ingest_events` y `telemetry_samples`.
2. Monitorear backlog del broker y profundidad de outbox edge.
3. No usar `clean_session=true` en central para produccion.
