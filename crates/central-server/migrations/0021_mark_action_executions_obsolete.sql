-- `action_executions` esta obsoleta: ningun codigo la escribe.
--
-- Los resultados de acciones del edge se persisten en `operational_events` (ver
-- `insert_action_result` en crates/central-server/src/persistence/postgres.rs), con
-- event_type 'action.command.accepted' / 'action.command.rejected'. La UI ya los muestra en
-- el panel de diagnostico del edge, via GET /api/ops/events.
--
-- NO se elimina, y el motivo importa: conserva 4.562 filas de marzo a mayo de 2026 que
-- `operational_events` NO tiene -- esa tabla arranca en junio. Borrarla destruiria historial
-- de acciones que no existe en ningun otro lado.
--
-- El comentario existe porque la tabla es una trampa: quien la vea supone que ahi viven los
-- resultados de acciones, mira que no se escribe desde el 2026-08-20 y concluye que el
-- sistema esta roto. Eso ya paso una vez, el 2026-09-03, y costo una tarde de diagnostico.

COMMENT ON TABLE action_executions IS
    'OBSOLETA desde 2026-08-20: ningun codigo la escribe. Los resultados de acciones viven '
    'en operational_events (event_type action.command.*), visibles en la UI por '
    'GET /api/ops/events. Se conserva porque guarda historial de marzo a mayo de 2026 que '
    'operational_events no tiene. No usar para consultas nuevas.';
