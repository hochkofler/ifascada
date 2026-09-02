-- Ordenes de control fuera de banda para los edges.
--
-- El unico mecanismo de reinicio que habia viajaba por MQTT y entraba por el mismo event
-- loop del agente que se cuelga: el 2026-09-02 el reset se publico, el broker lo acepto y
-- lcc01 nunca lo ejecuto. Esta tabla es la cola que el supervisor consulta por HTTP, un
-- camino que no comparte destino con el agente.
--
-- Vive en Postgres y no en memoria para que una orden sobreviva a un reinicio del central,
-- que es un escenario perfectamente posible mientras se diagnostica un incidente.

CREATE TABLE IF NOT EXISTS edge_control_command (
    id           bigserial PRIMARY KEY,
    edge_code    text        NOT NULL,
    request_id   text        NOT NULL,
    kind         text        NOT NULL,
    reason       text,
    operator     text,
    requested_at timestamptz NOT NULL DEFAULT now(),
    -- Diagnostico, no control de flujo: distingue "el supervisor nunca pregunto"
    -- (supervisor caido, sin red, o un host que todavia corre run-edge.ps1) de "la recibio
    -- y no la confirmo" (murio ejecutandola). Se investigan distinto.
    delivered_at timestamptz,
    completed_at timestamptz,
    CONSTRAINT edge_control_command_edge_request_unique UNIQUE (edge_code, request_id)
);

-- La consulta del long-poll es siempre la misma y casi siempre no devuelve nada. El indice
-- parcial mantiene ese caso barato sin importar cuanto historial se acumule.
CREATE INDEX IF NOT EXISTS idx_edge_control_command_pending
    ON edge_control_command (edge_code, requested_at)
    WHERE completed_at IS NULL;
