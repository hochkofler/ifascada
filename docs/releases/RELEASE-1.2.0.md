# Release 1.2.0

Fecha: 2026-09-03
Versión: `1.2.0`
Alcance: `edge-agent`, **binario nuevo `edge-supervisor`**, `central-server` (con migración
de base), `web-ui-v2` e instalador del edge.

> Las dos notas anteriores de esta serie están en inglés. Esta va en español, como el resto
> de la documentación operativa que usa el equipo de planta.

## Resumen

Cierra dos defectos que se manifestaron juntos el 2026-09-02, tras un corte de luz, dejando
a `lcc01` mudo 25 minutos con todos los indicadores en verde:

1. El bridge MQTT del edge podía quedar en **deadlock permanente** al drenar su outbox
   después de una caída larga del broker. Sin logs, sin error, sin reconexión.
2. El **único** mecanismo para reiniciar un edge desde central viajaba por MQTT y entraba
   por ese mismo event loop, así que el reset solo funcionaba con un agente que no lo
   necesitaba.

## Causa raíz

### El deadlock

El loop del bridge llamaba `flush_pending` **antes** de `event_loop.poll()`, y `poll()` es
lo único que vacía el canal de peticiones del cliente, acotado a 100. Con el broker caído el
outbox se llena; al volver, dos vueltas del loop llenan el canal (50 + 50) y el siguiente
`publish().await` queda esperando un espacio que solo `poll()` puede liberar. Como el
bloqueo ocurre antes del `poll()`, nadie lo libera nunca.

Evidencia del incidente: proceso vivo, tarea `Running`, puertos COM en `OK`, y el socket
MQTT clavado en `CLOSE_WAIT` — nadie leyó el FIN porque nadie volvió a llamar `poll()`.

Es un mecanismo **distinto** del documentado en
`docs/finding-mqtt-client-stale-session-detection.md` (2026-08-18): aquel dejaba al edge
logueando «publish ok» durante toda la falla; en un deadlock no hay logs de ningún tipo.
Ese finding sigue abierto del lado edge.

### El control circular

`POST /api/edges/reset` publicaba en `scada/{site}/edge/{agent}/control/reset`, que el
agente consume dentro de `event_loop.poll()`. El log de central del 2026-09-02 muestra el
reset publicado a las 11:50:05 y aceptado por el broker; `lcc01` nunca lo ejecutó.

## Qué cambia

### 1. `edge-agent` — el deadlock

`flush_pending` pasa a usar una variante que nunca espera por espacio en el canal. El nuevo
`PublishAttempt` distingue «canal lleno» de un error real: lo primero es una postergación
normal (la fila sigue en el outbox), no una anomalía que merezca un `warn` en cada
reconexión. Cubre las dos rutas al mismo bloqueo — el flush del loop y el de
`publish_with_outbox`, que los handlers invocan también sobre el loop.

Además el outbox **no se drena mientras no haya sesión con el broker**. `flush_pending`
borra la fila apenas el cliente *acepta* el mensaje, y `rumqttc` guarda lo aceptado-pero-no-
entregado en una lista en memoria sin techo; drenar con la sesión caída sacaba mensajes del
SQLite durable y los metía en memoria volátil, debilitando la garantía para la que existe el
outbox justo en la condición para la que existe.

### 2. `edge-supervisor` — binario nuevo

Es lo que la tarea programada lanza ahora; el `edge-agent` pasa a ser su hijo. Reemplaza a
`run-edge.ps1`, cuyo bucle bloqueaba dentro de la llamada al hijo y solo despertaba cuando
el proceso **terminaba** — un agente colgado pero vivo le resultaba idéntico a uno sano.

Mantiene un long-poll HTTP contra central que se resuelve en cuanto hay una orden o vence a
los ~25 s. En reposo no genera tráfico ni consultas. No es una conexión persistente: se
cierra cada 25 s por diseño, así que morir es su comportamiento normal y no necesita latido
ni watchdog — que es justo la maquinaria cuya ausencia causó el incidente de agosto.

En Windows el hijo se asigna a un Job Object con `KILL_ON_JOB_CLOSE`: si el supervisor
muere, el kernel se lleva al agente. De eso depende que `update-edge.ps1` siga funcionando.

### 3. `central-server` — la cola de órdenes

`/api/edges/reset` deja de publicar en MQTT y encola en la tabla nueva
`edge_control_command` (migración `0020`). Dos endpoints nuevos, `/api/edge/control/pending`
(long-poll) y `/api/edge/control/ack`, autenticados con el `EDGE_ENROLL_TOKEN` que ya
existía.

El aviso interno (`Notify`) **no es la fuente de verdad**: el vencimiento del long-poll
vuelve a consultar la base pase lo que pase, así que un reinicio de central o un aviso
perdido cuesta latencia y nunca corrección. Hay un test dedicado a fijar esa propiedad.

Efecto colateral deseado: reiniciar un edge ya no depende de que el propio central tenga
sesión MQTT viva (antes devolvía `503` sin `mqtt_cmd`).

### 4. `web-ui-v2`

Sin cambios de comportamiento. Solo se elimina `topic` del tipo de respuesta: nombraba un
tópico MQTT que ya no se publica.

## Verificación realizada

```
cargo test --workspace --lib          → 146 pasan, 0 fallan
cargo test -p edge-agent --bin        →  52 pasan, 0 fallan
Invoke-Pester update-edge.Tests.ps1   →  20 pasan, 0 fallan
npx tsc --noEmit (web-ui-v2)          → limpio
vitest (edge-actions, diagnostics)    →  12 pasan
cargo clippy                          → sin advertencias nuevas
cargo build --release --workspace     → los tres binarios compilan
build-edge-package.ps1                → ejecutado de punta a punta, paquete correcto
```

El test que reproduce el deadlock lo hace **sin broker ni hardware**: un cliente con canal
chico que nadie poll'ea y un outbox con más filas que la capacidad. Antes se colgaba los
5 s del timeout; ahora retorna en 0,05 s.

## Lo que NO se pudo verificar

Dos cosas, y ambas hay que hacerlas antes de dar el despliegue por bueno:

1. **Los tests de contrato contra Postgres.** Los cinco escenarios nuevos de
   `api_edge_control_contract_tests.rs` compilan pero no se ejecutaron: el
   `CENTRAL_PG_DSN` configurado apunta a un host que no responde desde la máquina de
   desarrollo. Los tests de contrato preexistentes fallan igual y por lo mismo.
2. **`update-edge.ps1` contra un host con supervisor instalado.** Las 20 pruebas Pester
   cubren la lógica del script, no su interacción con un supervisor y una tarea reales. La
   teoría dice que funciona (se detiene la tarea → muere el supervisor → el Job Object se
   lleva al agente, y el paso de «matar por ruta» no encuentra nada), pero hay que verlo.

## Orden de despliegue — importa

En cuanto central esté desplegado, `/api/edges/reset` **solo encola**. Un host que siga con
`run-edge.ps1` no tiene quién lea esa cola: el botón de la UI deja de hacer nada para él y
el operador ve `timed-out-no-recovery`. No falla en silencio —&nbsp;se ve como `delivered_at`
en null&nbsp;— pero hay que saberlo antes.

Por eso central, la UI y el supervisor de `lcc01` van en la misma ventana:

1. Aplicar la migración en la base de `lcc01`. **`docker-entrypoint-initdb.d` solo corre
   sobre una base vacía**, así que en una base existente hay que aplicarla a mano:
   ```
   docker exec -i ifascada-timescaledb psql -U postgres -d rustscada \
     -f /migrations/0020_edge_control_command.sql
   ```
   Es idempotente (`CREATE TABLE IF NOT EXISTS`).
2. Reconstruir y desplegar `central-server` y `web-ui-v2`.
3. Instalar el supervisor en `lcc01` con **`install-edge.ps1`**, no con el updater (ver
   «Cambios que rompen»).
4. Probar el botón de la UI contra un agente sano y contra uno colgado a propósito.
5. `lcc02` a los pocos días. **Queda sin botón de reinicio hasta entonces**; se recupera por
   el procedimiento manual de siempre.
6. Bolivia (systemd) después, con la unit ajustada. No entra en este release.

## Cambios que rompen

1. **`EdgeResetResponse.topic` se elimina.** Cualquier consumidor de la API que lo lea deja
   de encontrarlo. La UI ya está actualizada.
2. **`run-edge.ps1` desaparece.** `install-edge.ps1` lo borra al instalar: una copia
   huérfana es una trampa, porque correrla a mano levantaría un segundo agente junto al
   supervisado, ambos peleando por los mismos puertos COM y el mismo `client_id`.
3. **El supervisor se instala, no se actualiza.** `update-edge.ps1` valida que
   `manifest.binary.path` sea exactamente `bin/edge-agent.exe` y solo reemplaza ese archivo.
   Cambiar el supervisor exige reinstalar con `install-edge.ps1`. El manifiesto igual
   declara su SHA-256 bajo la clave `supervisor`, para poder verificar qué quedó desplegado.
   Decisión registrada en el spec, con su razonamiento.

## Rollback

- **Edge:** `update-edge.ps1` sigue haciendo rollback automático del agente ante un chequeo
  de salud fallido. Para volver a `run-edge.ps1`, reinstalar desde el paquete 1.1.2.
- **Central:** volver a la imagen anterior. La tabla `edge_control_command` puede quedarse
  donde está —&nbsp;no la lee nadie más&nbsp;— pero el `/api/edges/reset` viejo vuelve a
  publicar por MQTT, así que un edge con supervisor y un central viejo pierden el canal de
  control hasta que se alineen.
- **Migración:** no hay `DOWN`. Es una tabla nueva y aislada; dejarla es inocuo.

## Referencias

- `docs/superpowers/specs/2026-09-02-edge-out-of-band-control-design.md` — diseño y
  decisiones del canal de control.
- `docs/finding-mqtt-client-stale-session-detection.md` — el otro mecanismo de fallo
  silencioso, todavía abierto del lado edge.
- `docs/infrastructure-failure-policy.md` — la política que el deadlock incumplía.
