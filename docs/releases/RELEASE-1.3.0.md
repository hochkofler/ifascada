# Release 1.3.0

Fecha: 2026-09-03
Versión: `1.3.0`
Alcance: solo runtime del edge (`edge-agent` y `edge-supervisor`). Sin cambios en central,
base de datos, UI ni contratos de tópicos MQTT.

## Resumen

La 1.2.0 dejó el sistema capaz de **reparar** rápido lo que un operador descubre. Esta
cierra la otra mitad: que el sistema **descubra solo** que un agente dejó de trabajar, y se
recupere sin que nadie mire.

Tres defectos, todos del mismo tipo: el edge no podía distinguir estar vivo de estar
funcionando.

## Qué cambia

### 1. El supervisor detecta un agente colgado, no solo uno que terminó

Hasta ahora el supervisor solo reaccionaba a que el proceso **terminara**. Uno que seguía
vivo sin hacer nada le resultaba idéntico a uno sano — exactamente lo que pasó en `lcc01` el
2026-09-02: proceso corriendo, tarea `Running`, puertos COM en `OK`, y 25 minutos de
silencio que terminaron porque una persona miró.

El agente escribe un latido **desde el bucle del bridge**, no desde una tarea aparte: lo que
hay que probar vivo es justo ese bucle. Un latido emitido desde otro lado podría seguir
latiendo con el bucle muerto, que es el fallo que esto existe para atrapar.

- Se escribe **como mucho** cada 5 s, con los milisegundos de epoch **dentro** del archivo
  (en Windows el `mtime` puede llegar tarde, y tener el número adentro deja saber *cuán*
  viejo es). La cadencia real está acotada por cuándo el bucle da una vuelta: en reposo eso
  es cada ~10 s (el keep-alive de MQTT), y en el peor caso 15 s (lo que el watchdog de
  sesión deja estacionar el `poll`). **Medido en producción el 2026-09-03: ~10 s en los dos
  hosts.** Muy lejos del umbral de 60 s, pero el número importa si alguna vez se ajusta ese
  umbral.
- El supervisor reinicia el agente si el latido pasa de **60 s**.
- **90 s de gracia desde cada lanzamiento**: sin eso, un agente recién arrancado —&nbsp;que
  todavía no escribió nada&nbsp;— sería reiniciado de inmediato, en un bucle indistinguible
  de un agente que no arranca.

La ruta del latido es un contrato entre los dos binarios; ambos la derivan de
`MQTT_OUTBOX_PATH`, con un test de cada lado. Configurable con `EDGE_HEARTBEAT_PATH`,
`EDGE_SUPERVISOR_HEARTBEAT_STALE_SECS` y `EDGE_SUPERVISOR_HEARTBEAT_GRACE_SECS`.

### 2. Watchdog de sesión MQTT en el edge

El bucle llamaba `event_loop.poll()` desnudo. Con un socket medio abierto, `poll()` puede
seguir teniendo éxito —&nbsp;sigue emitiendo `PingReq` salientes, porque escribir en el buffer
de un socket muerto no falla&nbsp;— y el brazo `Err`, única vía de reconexión, nunca se
dispara. Eso es el finding del 2026-08-18: **1 h 13 min de pérdida total de datos con el log
del edge en verde**.

Portado de `BrokerActivityWatch` del central, donde vive desde el 2026-08-25. Solo el
tráfico **entrante** cuenta como señal de vida. Tras `keep_alive × 1.5` de silencio se llama
a `event_loop.clean()` y el siguiente poll reconecta.

Queda **duplicado y no compartido**: `central-server` depende solo de `domain`, y un watchdog
de transporte no pertenece a esa capa. Las dos copias deben moverse juntas; el comentario de
cabecera lo dice.

### 3. El log deja de afirmar una entrega que no conoce

`mqtt publish ok` se registraba cuando el cliente **aceptaba** el mensaje en su canal, no
cuando el broker lo recibía. Es lo que hizo que aquella hora y cuarto de pérdida se viera
verde. Pasa a decir `mqtt publish queued`, que es lo único que ese `Ok` prueba.

Se agrega `broker_acked_total` al mensaje de salud, contando los `PUBACK` de QoS 1 que el
bucle ya recibía y descartaba. Eso sí es prueba de que el otro lado recibió algo: una brecha
que deja de crecer mientras se siguen encolando mensajes es un enlace trabado, visible sin
leer el log del broker en la otra máquina.

### 4. El chequeo de share de impresora no podía pasar nunca

`connection.check` ejecutaba `if exist "\\host\share"`, una prueba de **sistema de
archivos**. Un share de **impresora** no es una ruta de archivos, así que devolvía fallo para
impresoras que imprimen perfectamente: en el historial se ve `connection.check → Failed`
justo antes de que la impresión salga `Applied`.

Ahora prueba que el servidor de impresión responda por SMB. **No prueba** que el share exista
o acepte trabajos —&nbsp;eso solo se demuestra mandando uno, y un chequeo previo no debe
imprimir&nbsp;—; alcanzar el servidor es la afirmación más fuerte disponible sin gastar papel.
De paso deja de ser exclusivo de Windows, lo que sirve para Bolivia.

## Verificación

```
cargo test --workspace --lib     161 ok
cargo test -p edge-agent --bin    68 ok
cargo clippy                      sin advertencias nuevas
```

Dos tests fijan la garantía completa del punto 1: **un agente que no late se reinicia**, y
**uno que late se deja en paz**. El segundo importa tanto como el primero — un detector que
reinicia agentes sanos es peor que ninguno.

## Decisiones tomadas, con su motivo

- **Keepalive de sistema operativo (A2): descartado.** `rumqttc` no lo expone y exigiría un
  `Transport` propio. El watchdog de sesión, ahora en los dos lados, cubre la detección que
  A2 buscaba.
- **Last Will (A3): diferido.** La opción tentadora —&nbsp;publicarlo en `health/runtime` para
  reusar la ingesta existente&nbsp;— es una trampa: central actualiza `last_seen_at` con cada
  mensaje de salud, así que el testamento dejaría al edge marcado como *recién visto* justo
  al morir, enmascarando la detección de silencio. Hacerlo bien pide un tópico propio y algo
  que reaccione al aviso; se diseña junto con la alerta activa.
- **Gate de arranque en Windows (B4): sin sentido ya.** Con el deadlock de la 1.2.0 arreglado,
  un agente que arranca antes que su broker simplemente reintenta.

## Cambios que rompen

Ninguno. Sin cambios de configuración obligatorios, de esquema ni de contratos MQTT. Las tres
variables nuevas del latido tienen valores por defecto que funcionan en las instalaciones
actuales sin tocar `edge.env`.

`broker_acked_total` se agrega al payload de salud; central lo ignora sin quejarse (ninguna
estructura usa `deny_unknown_fields`, verificado).

## Despliegue

**Cambian los dos binarios**, y `update-edge.ps1` solo reemplaza `edge-agent.exe`. Este
release se instala copiando ambos y reiniciando la tarea, **sin re-registrarla** — así no
hace falta la contraseña de la cuenta en `lcc02`, que corre como `user`.

Orden: detener la tarea → matar el supervisor (el Job Object se lleva al agente) → copiar los
dos binarios verificando SHA-256 contra el manifiesto → arrancar la tarea.

Ventana ciega esperada: ~15 s por host, como en la 1.2.0.

## Verificación posterior al despliegue

1. Los dos procesos corriendo, con el agente colgando del supervisor.
2. Aparece el archivo de latido junto al outbox y su marca avanza.
3. En el log del supervisor: `wedge detection: heartbeat at ... (stale after 60s, 90s of grace)`.
4. El edge sigue reportando en `/api/edges/current`.

## Rollback

Restaurar `edge-agent.exe` y `edge-supervisor.exe` desde
`C:\Program Files\ifascada\edge\releases\` y reiniciar la tarea. El latido que quede huérfano
es inofensivo: un supervisor 1.2.0 no lo mira.

## Referencias

- `docs/finding-mqtt-client-stale-session-detection.md` — el fallo silencioso que el punto 2
  cierra del lado edge.
- `docs/infrastructure-failure-policy.md` — actualizada: ahora dice que al llenarse el outbox
  se descartan pesadas.
- `docs/releases/RELEASE-1.2.0.md` — el supervisor y el canal de control que esto extiende.
