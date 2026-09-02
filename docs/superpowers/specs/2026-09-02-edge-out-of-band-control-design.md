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
   orden. Un único camino, siempre funciona, con hasta 10 s de latencia. Se descartó el
   esquema mixto (MQTT + fuera de banda) porque puede reiniciar dos veces, y los dos botones
   separados porque obligan al operador a entender una distinción que es nuestra, no suya.
2. **Windows primero, Bolivia después.** `lcc01` y `lcc02` son los que fallaron y están a
   mano. Bolivia entra cuando el supervisor haya rodado unos días.
3. **Un binario, no un script.** El requisito es independencia del sistema operativo y en
   Bolivia no hay PowerShell.

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
2. **Sondeo de órdenes.** Cada 10 s pregunta a central si hay una orden pendiente para su
   edge.
3. **Ejecución y confirmación.** Si la hay: mata al hijo, lo relanza, y le confirma a
   central que se ejecutó.

### Configuración

El supervisor lee el mismo `edge.env` que hoy carga `run-edge.ps1`, y de ahí toma lo que ya
está definido para el agente:

| Variable | Uso | Ya existe |
|---|---|---|
| `EDGE_CONFIG_URL` | Base de la API de central | Sí |
| `EDGE_ENROLL_TOKEN` | Autenticación del sondeo | Sí |
| `EDGE_AGENT` | Identifica al edge ante central | Sí |
| `EDGE_SUPERVISOR_POLL_SECS` | Cadencia del sondeo, por defecto 10 | No |
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

## Transporte: HTTP a central

No MQTT. El punto entero es no compartir destino con el event loop que se cuelga.

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

- `POST /api/edge/control/pending` — cuerpo `{ edge_id, enrollment_token }`. Devuelve la
  orden pendiente más antigua o vacío. Marca `delivered_at` si estaba en null.
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

La ventana de 30 s sigue alcanzando: 10 s de sondeo del supervisor + reinicio del agente
entra cómodo.

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
| Central caído cuando el supervisor sondea | El sondeo falla, se registra en debug y se reintenta a los 10 s. El agente sigue corriendo. |
| El supervisor muere ejecutando la orden | La orden queda con `delivered_at` y sin `completed_at`. Al volver, la vuelve a tomar y la ejecuta. Un reinicio de más es aceptable; uno de menos no. |
| El agente no arranca tras el reinicio | El supervisor lo reintenta cada 5 s, igual que hoy. La UI reporta `timed-out-no-recovery` porque `last_seen_at` no avanza. |
| Dos órdenes encoladas para el mismo edge | Se sirven de a una, la más antigua primero. |
| El supervisor no corre | No hay quien sondee. `delivered_at` en null lo delata. |

## Pruebas

- **Lógica de decisión, en funciones puras:** ¿hay orden pendiente?, ¿esta ya se confirmó?,
  ¿qué se responde ante cada forma de respuesta de central? Sin red y sin procesos.
- **Ciclo de vida del hijo:** el supervisor lanza un ejecutable de prueba, lo mata, verifica
  que se relanzó. En Windows, verificar además que cerrar el Job Object se lleva al hijo.
- **Endpoints de central:** pendiente devuelve la más antigua, marca `delivered_at`, el ack
  es idempotente, y `/api/edges/reset` encola en vez de publicar.
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
  agente. Se mitiga manteniéndolo deliberadamente chico y sin estado propio: sondear, matar,
  lanzar. Toda la lógica que pueda vivir en central, vive en central.
- **Latencia de 10 s** en el caso común, contra el reinicio casi inmediato del MQTT. Es el
  precio de la decisión 1 y se consideró aceptable.
- El token compartido, ya anotado arriba.
