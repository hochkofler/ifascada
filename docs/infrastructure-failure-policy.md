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

#### Que pasa cuando el outbox se llena (documentado 2026-09-03)

Los puntos de arriba describen el caso feliz. El outbox tiene un techo
(`MQTT_OUTBOX_MAX_MESSAGES`, 10.000 por defecto) y **al llegar a el se descartan mensajes**.
Conviene decirlo explicitamente porque "drena respetando orden" se lee como si nunca se
perdiera nada:

- El outbox distingue **dos clases**: `Ack` y `Audit`. **Toda la telemetria de tags viaja
  como `Audit`** -- las pesadas incluidas.
- Al llegar al techo, para hacer lugar se borra el `Audit` **mas viejo**. Es decir, se
  pierden las lecturas mas antiguas primero.
- Si no queda ningun `Audit` que borrar, se **rechaza el `Audit` entrante** para preservar
  la prioridad de los `Ack`.

Priorizar los `Ack` es deliberado: son la confirmacion de comandos y su perdida deja a
central creyendo que una orden nunca se ejecuto. Pero la consecuencia hay que tenerla
presente: **una caida de broker lo bastante larga como para acumular 10.000 mensajes empieza
a costar pesadas**, y el sistema no avisa cuando eso ocurre.

A 10.000 mensajes y con el ritmo de una planta activa, ese techo esta lejos en un corte de
horas y cerca en uno de dias. Si alguna vez importa, la palanca es
`MQTT_OUTBOX_MAX_MESSAGES`, acotada por el disco del edge.

#### El drenado no puede bloquear el event loop

Desde el 2026-09-03 (release 1.2.0) el drenado del outbox usa una variante de publicacion
que **nunca espera** por espacio en el canal del cliente. Antes podia bloquearse antes de
`event_loop.poll()`, que es lo unico que vacia ese canal: un interbloqueo permanente, sin
logs y sin reconexion. Ver `docs/releases/RELEASE-1.2.0.md`.

Ademas, **no se drena mientras no haya sesion con el broker**: aceptar un mensaje en el
cliente lo saca del SQLite durable y lo deja en una cola en memoria sin techo, que es lo
contrario de lo que el outbox existe para lograr.

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
