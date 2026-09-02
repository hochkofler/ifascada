# Control de Edge Fuera de Banda

**Estado:** Pendiente de aprobación — 2026-09-02.

## Problema

El único mecanismo para reiniciar un edge desde central viaja por MQTT:
`POST /api/edges/reset` publica en `scada/{site}/edge/{agent}/control/reset`, y el edge lo
recibe como un `Incoming::Publish` dentro de `event_loop.poll()`.

Ese es exactamente el punto que falla. El 2026-09-02, tras el apagón, `lcc01` quedó con el
event loop bloqueado y el socket en `CLOSE_WAIT`. El reset se publicó a las 11:50:05, el
broker lo aceptó, y el agente nunca lo ejecutó. La recuperación fue por SSH.

Es una dependencia circular: **el mecanismo de reparación solo funciona cuando no hace
falta.** Ver `docs/finding-lcc01-bala11-13-silent-serial.md` y la auditoría del 2026-09-02
(hallazgo C1).

## Objetivo

Que un operador pueda reiniciar un edge desde la UI y que eso funcione **también cuando el
agente está colgado**, sin abrir una terminal, y con el mismo procedimiento en Windows y en
Linux.

## No objetivos

- El latido en disco y el reinicio automático sin intervención humana (punto 3 del plan).
- El watchdog de sesión MQTT del edge (punto 4).
- Cerrar la ventana de "borrar al encolar, no al entregar" (punto 6).
- Reemplazar el `update-edge.ps1` existente. Este diseño debe **convivir** con él.

## Decisiones tomadas

1. **Solo fuera de banda.** `/api/edges/reset` deja de publicar en MQTT y solo encola la
   orden. Un único camino y siempre funciona. Se descartó el
   esquema mixto (MQTT + fuera de banda) porque puede reiniciar dos veces, y los dos botones
   separados porque obligan al operador a entender una distinción que es nuestra, no suya.
2. **Windows primero, Bolivia después.** `lcc01` y `lcc02` son los que fallaron y están a
   mano. Bolivia entra cuando el supervisor haya rodado unos días.
3. **Un binario, no un script.** El requisito es independencia del sistema operativo y en
   Bolivia no hay PowerShell.
4. **Long-poll, no sondeo repetido.** El supervisor deja un pedido esperando en central en
   vez de preguntar cada N segundos. En reposo no genera tráfico ni consultas, y la orden
   llega en menos de un segundo. Se descartó el sondeo repetido porque hacer un pedido cada
   10 s para que el 99,99% devuelvan "nada" es desperdicio evitable; y se descartaron SSE,
   WebSocket y MQTT por lo que se explica en «Transporte».

## Arquitectura

Se introduce un proceso nuevo entre el gestor del sistema operativo y el agente:

```
Windows:  Tarea programada `ifascada-edge`  ──lanza──>  edge-supervisor  ──lanza──>  edge-agent
Linux:    systemd `ifascada-edge.service`   ──lanza──>  edge-supervisor  ──lanza──>  edge-agent
```

El supervisor es lo que la tarea y systemd supervisan; el agente pasa a ser su hijo. Ese
cambio de jerarquía es lo que da un lugar desde donde actuar sobre un agente colgado: el
supervisor no comparte su event loop, ni su proceso, ni su suerte.

En Windows reemplaza a `run-edge.ps1`, cuyo bucle `while` bloquea en la llamada al
ejecutable y por eso solo reacciona a que el proceso *termine* — un agente colgado pero vivo
le resulta indistinguible de uno sano.

### Crate nuevo: `crates/edge-supervisor`

Miembro del workspace, binario `edge-supervisor`. Tres responsabilidades:

1. **Ciclo de vida del hijo.** Lanza `edge-agent`, lo relanza si termina, con la misma
   espera de 5 s que hoy tiene `run-edge.ps1`. Hereda el `edge.env` igual que hoy.
2. **Espera de órdenes.** Mantiene un pedido abierto contra central, que se resuelve en
   cuanto aparece una orden para su edge o vence a los ~25 s, y se vuelve a lanzar.
3. **Ejecución y confirmación.** Si la hay: mata al hijo, lo relanza, y le confirma a
   central que se ejecutó.

### Configuración

El supervisor lee el mismo `edge.env` que hoy carga `run-edge.ps1`, y de ahí toma lo que ya
está definido para el agente:

| Variable | Uso | Ya existe |
|---|---|---|
| `EDGE_CONFIG_URL` | Base de la API de central | Sí |
| `EDGE_ENROLL_TOKEN` | Autenticación del pedido | Sí |
| `EDGE_AGENT` | Identifica al edge ante central | Sí |
| `EDGE_SUPERVISOR_WAIT_SECS` | Cuánto espera central antes de responder vacío, por defecto 25 | No |
| `EDGE_SUPERVISOR_AGENT_PATH` | Ruta del ejecutable del agente | No |

Las tres primeras se reusan tal cual, así que un host ya instalado no necesita configuración
nueva salvo las dos últimas, y ambas tienen valor por defecto razonable
(`EDGE_SUPERVISOR_AGENT_PATH` resuelve a `edge-agent[.exe]` junto al propio supervisor).

Si `EDGE_CONFIG_URL` o `EDGE_ENROLL_TOKEN` faltan, el supervisor **no aborta**: sigue
cumpliendo el ciclo de vida del hijo y registra una advertencia por arranque diciendo que el
control remoto está deshabilitado. Perder el canal de control no debe costar el agente.

### El hijo debe morir con el padre

Requisito duro, no un detalle de implementación: si el supervisor muere y el agente
sobrevive, el siguiente arranque tendría dos agentes con el mismo `client_id` peleando por
los mismos puertos serie.

- **Windows:** el hijo se asigna a un Job Object con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- **Linux:** grupo de procesos propio; la unit usa `KillMode=control-group`.

Esto además es lo que hace que el updater existente siga funcionando (ver más abajo).

## Transporte: long-poll HTTP a central

No MQTT. El punto entero es no compartir destino con el event loop que se cuelga.

### Por qué long-poll y no una conexión persistente

Central ya expone SSE (`/api/ops/events/stream`, `/api/stream/events`), así que un push
clásico sería infraestructura disponible. Se descarta igual, y la razón es específica de
este sistema: **el modo de fallo que ya sufrió dos veces es una conexión de larga vida que
ambos extremos creen viva y está muerta.** El 2026-08-18 fueron 1 h 13 min de pérdida total
de datos con el log del edge en verde; el 2026-09-02, 25 minutos. Los hallazgos A1, A2 y A4
de la auditoría siguen abiertos del lado edge.

Apoyar el mecanismo de recuperación sobre una conexión de ese tipo lo haría heredar la clase
de fallo de la que existe para recuperarnos, y obligaría a construirle latido, watchdog de
actividad, reconexión y backoff — la maquinaria que central tiene en `BrokerActivityWatch` y
el edge no. Es demasiada superficie en el único componente que no puede fallar.

El long-poll no tiene ese problema, y la diferencia es exactamente ésta:

| | SSE / WebSocket / MQTT | Long-poll |
|---|---|---|
| La conexión *debería* durar | para siempre | ~25 s |
| Que se corte es | una anomalía a detectar | lo normal, ocurre siempre |
| Si muere en silencio | nadie se entera | el timeout la cierra igual y se rehace |
| Necesita latido y watchdog | sí | no |

Morir es su comportamiento normal, así que no hay nada que detectar.

**Propiedad que lo hace seguro:** si el aviso interno de central no llegara —&nbsp;por un
reinicio, por un despliegue, por lo que sea&nbsp;— el pedido vence igual a los 25 s y vuelve
a consultar la base. La corrección **no depende de que el push funcione**: el aviso solo
mejora la latencia. En el peor caso degrada a un sondeo de 25 s, nunca a silencio.

**Descartado explícitamente:** que central abra la conexión hacia el edge. Invierte la
dirección y exige que el edge acepte conexiones entrantes, con firewall y NAT en el medio.
`lcc02` ni siquiera acepta SSH — comprobado el 2026-09-02. En redes de planta la dirección
edge→central es la que funciona.

Ya existe el precedente exacto en `crates/edge-agent/src/bootstrap.rs`: el agente sondea
`POST /api/edge/config/check` con `edge_id` en el cuerpo y la cabecera
`x-enrollment-token`, sobre un `reqwest::Client` con timeout de 8 s. El supervisor copia esa
forma, y con ella reusa la autenticación que ya existe (`EDGE_ENROLL_TOKEN` contra
`state.edge_cfg.enroll_token`) en vez de inventar un esquema nuevo.

**Limitación heredada, aceptada:** el token de enrolamiento es un secreto compartido por
todo el parque, así que un edge podría sondear las órdenes de otro. Es el modelo de
autenticación que ya rige `config/check` y `config/runtime`; este diseño no lo empeora ni
pretende arreglarlo. Queda anotado como deuda.

## Central

### Migración `0020_edge_control_command.sql`

```
edge_control_command
  id            bigserial primary key
  edge_code     text        not null
  request_id    text        not null
  kind          text        not null   -- 'restart' por ahora
  reason        text
  operator      text
  requested_at  timestamptz not null default now()
  delivered_at  timestamptz
  completed_at  timestamptz
  unique (edge_code, request_id)
```

En Postgres y no en memoria, para que una orden sobreviva a un reinicio del central — que es
un escenario perfectamente posible mientras se diagnostica un incidente.

Una orden se considera **pendiente** si `completed_at is null`. `delivered_at` es
diagnóstico: distingue "el supervisor nunca preguntó" (supervisor caído o sin red) de "la
recibió y no la confirmó" (murió ejecutándola), y esas dos cosas se investigan distinto.

### Endpoints

- `POST /api/edge/control/pending` — cuerpo `{ edge_id, enrollment_token }`. **Retiene el
  pedido.** Primero consulta la base: si hay orden pendiente responde en el acto. Si no,
  espera con `tokio::select!` entre un `tokio::sync::Notify` por edge y un timeout de 25 s;
  al despertar por cualquiera de los dos vuelve a consultar la base y responde con la orden
  o vacío. Marca `delivered_at` si estaba en null.

  El `Notify` vive en memoria y se dispara al insertar una orden. No es la fuente de verdad:
  el timeout garantiza que la base se reconsulte pase lo que pase.
- `POST /api/edge/control/ack` — cuerpo `{ edge_id, enrollment_token, request_id }`. Marca
  `completed_at`. Idempotente: reconfirmar una orden ya confirmada responde OK sin cambiar
  nada.

### `POST /api/edges/reset` (existente, cambia por dentro)

Deja de publicar en MQTT y pasa a insertar una fila en `edge_control_command`. El cuerpo de la
petición no cambia. En la respuesta se **elimina** el campo `topic` de `EdgeResetResponse`:
nombraba un tópico MQTT que ya no se publica, y dejarlo con un valor inventado sería mentir.
`accepted` y `request_id` se mantienen.

**Efecto colateral deseado:** hoy el endpoint devuelve `503` si `state.mqtt_cmd` es `None`.
Con el cambio, reiniciar un edge deja de depender de que el propio central tenga sesión MQTT
viva.

## UI

Sin cambios de comportamiento. `use-edge-reset.ts` manda el comando, sondea 15 veces cada
2 s comparando `last_seen_at` contra el valor previo, y distingue `confirmed-recovered` de
`timed-out-no-recovery`. Esa lógica sigue siendo correcta con el transporte nuevo: mide
recuperación real, no entrega del mensaje.

La ventana de 30 s queda holgada: la orden llega al supervisor en menos de un segundo, y lo
único que consume tiempo es el reinicio del agente.

El único ajuste es quitar `topic` del tipo de la respuesta en `api-schemas`.

## Interacción con `update-edge.ps1`

**Restricción crítica.** El updater existente (`docs/superpowers/specs/2026-08-10-windows-edge-safe-updater-design.md`)
detiene la tarea programada y además mata "cualquier `edge-agent.exe` cuya ruta sea el
objetivo de instalación". Con un supervisor en el medio, ese segundo paso pelearía con él:
mataría al hijo y el supervisor lo relanzaría de inmediato, sobre un binario que el updater
está por reemplazar.

Lo que lo resuelve es el requisito de "el hijo muere con el padre": el updater detiene la
tarea → muere el supervisor → el Job Object se lleva al agente. El paso de matar procesos
por ruta queda como red de seguridad y no encuentra nada que matar.

**Verificación obligatoria antes del rollout:** correr el updater contra un host con
supervisor instalado y confirmar que el reemplazo del binario y el rollback siguen pasando
sus pruebas de contrato. Si el paso de "matar por ruta" resulta problemático, se ajusta el
updater, no el supervisor.

El updater no toca `DataRoot`, así que el outbox y la secuencia de tickets se preservan
igual que hoy.

## Reinicio: matar y relanzar

Sin apagado ordenado. Un agente colgado no responde a un pedido ordenado — es justo el caso
a cubrir. Y no hace falta: el outbox y la secuencia de tickets son SQLite duraderos, y el
agente ya tolera reinicios duros; es el procedimiento que se aplicó a mano el 2026-09-02.

## Modos de fallo

| Situación | Qué pasa |
|---|---|
| Central caído o reiniciándose | El pedido falla o se corta; se registra en debug y se relanza tras una espera corta con backoff. El agente sigue corriendo. |
| El `Notify` no llega (reinicio de central, despliegue) | El pedido vence a los 25 s, reconsulta la base y encuentra la orden igual. Solo se pierde latencia. |
| Un intermediario corta conexiones ociosas antes de 25 s | El pedido termina antes de tiempo y se relanza. Indistinguible del vencimiento normal. Si pasara seguido, se baja `EDGE_SUPERVISOR_WAIT_SECS`. |
| El supervisor muere ejecutando la orden | La orden queda con `delivered_at` y sin `completed_at`. Al volver, la vuelve a tomar y la ejecuta. Un reinicio de más es aceptable; uno de menos no. |
| El agente no arranca tras el reinicio | El supervisor lo reintenta cada 5 s, igual que hoy. La UI reporta `timed-out-no-recovery` porque `last_seen_at` no avanza. |
| Dos órdenes encoladas para el mismo edge | Se sirven de a una, la más antigua primero. |
| El supervisor no corre | No hay quien espere la orden. `delivered_at` en null lo delata. |

## Pruebas

- **Lógica de decisión, en funciones puras:** ¿hay orden pendiente?, ¿esta ya se confirmó?,
  ¿qué se responde ante cada forma de respuesta de central? Sin red y sin procesos.
- **Ciclo de vida del hijo:** el supervisor lanza un ejecutable de prueba, lo mata, verifica
  que se relanzó. En Windows, verificar además que cerrar el Job Object se lleva al hijo.
- **Endpoints de central:** pendiente devuelve la más antigua, marca `delivered_at`, el ack
  es idempotente, y `/api/edges/reset` encola en vez de publicar.
- **Long-poll:** con la base vacía el pedido no responde de inmediato; insertar una orden
  mientras está esperando lo despierta y responde con ella; sin `Notify` alguno, vence y
  responde vacío dentro del plazo. Esa última prueba es la que fija la propiedad de que la
  corrección no depende del aviso.
- **Contrato con la UI:** los tests existentes de `api-client` y `edge-actions` cubren la
  forma de la petición; se ajustan si cambia `EdgeResetResponse`.

## Rollout

1. Release con supervisor, central y UI. El agente ya lleva los arreglos del punto 1.
2. `lcc01` primero: instalar el supervisor, verificar que el updater sigue funcionando,
   probar el botón de la UI contra un agente sano y contra uno colgado a propósito.
3. `lcc02` a los pocos días.
4. Bolivia después, con la unit de systemd ajustada.

Durante la transición conviven hosts con `run-edge.ps1` y hosts con supervisor. Los primeros
simplemente no responden a las órdenes encoladas, y eso se ve como `delivered_at` en null —
no como un fallo silencioso.

## Riesgos abiertos

- **El supervisor se vuelve un punto único nuevo.** Si se cuelga él, nadie reinicia al
  agente. Se mitiga manteniéndolo deliberadamente chico y sin estado propio: esperar, matar,
  lanzar. Toda la lógica que pueda vivir en central, vive en central.
- **Central sostiene una conexión abierta por edge.** Con el parque actual son tres, y axum
  las maneja sin esfuerzo. A escalas mucho mayores habría que revisar límites de descriptores
  y de conexiones del reverse proxy, si llega a haber uno delante.
- El token compartido, ya anotado arriba.
