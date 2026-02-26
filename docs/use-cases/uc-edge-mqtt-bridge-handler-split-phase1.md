# UC: Edge MQTT Bridge Handler Split (Phase 1)

## Objetivo
Reducir acoplamiento de `run_mqtt_bridge` sin cambiar contratos MQTT ni payloads.

## Cambios
Se extrajeron handlers dedicados para tráfico inbound:
1. `handle_write_command_packet`
2. `handle_action_command_packet`
3. `handle_alert_ack_packet`
4. `handle_config_apply_packet`
5. `handle_control_reset_packet`

Implementación estructural:
1. `crates/edge-agent/src/mqtt_bridge.rs` queda como orquestador.
2. `crates/edge-agent/src/mqtt_bridge/handlers.rs` funciona como fachada de handlers.
3. `crates/edge-agent/src/mqtt_bridge/handlers/action.rs`
4. `crates/edge-agent/src/mqtt_bridge/handlers/write.rs`
5. `crates/edge-agent/src/mqtt_bridge/handlers/alert.rs`
6. `crates/edge-agent/src/mqtt_bridge/handlers/config.rs`
7. `crates/edge-agent/src/mqtt_bridge/handlers/control.rs`

## Alcance
1. Solo refactor interno del `edge-agent`.
2. Mismos topics, mismas estructuras JSON, misma semántica funcional.
3. `run_mqtt_bridge` queda como orquestador de routing/event-loop.

## Validación
1. `cargo check -p edge-agent`
2. `cargo test -p edge-agent`

## Resultado
Fase 1 queda más cercana al objetivo de arquitectura limpia:
1. responsabilidades separadas por caso de uso MQTT
2. menor complejidad del loop principal
3. handlers ya movidos a módulo dedicado, listos para seguir separando por dominio (write/action/config/alert/control) en iteraciones siguientes
