# Configuración del Edge Agent

El Edge Agent utiliza un sistema de configuración jerárquico y dinámico.

## Jerarquía de Configuración

El agente busca archivos de configuración en la carpeta `config/` (local al ejecutable o en `crates/edge-agent/config` durante el desarrollo). El orden de prioridad es:

1. **Variables de Entorno**: Prefijo `SCADA__` (ej. `SCADA__MQTT__HOST=10.0.0.1`).
2. **Configuración de Prueba**: Si se define la variable `RUN_MODE=test`, se cargará `config/test.json`.
3. **Última Conocida (`last_known.json`)**: Este archivo es actualizado automáticamente por el **Config Manager** cuando recibe actualizaciones del Servidor Central vía MQTT.
4. **Predeterminada (`default.toml`)**: Configuración base mínima para el arranque.

## Sincronización Remota

El agente se suscribe al tópico MQTT `scada/config/{agent_id}`. Cuando el Servidor Central publica una nueva configuración:
1. El agente la recibe.
2. La guarda en `config/last_known.json`.
3. Realiza un **Hot Reload** (recarga en caliente) de los tags y automatizaciones sin reiniciar el proceso.

## Estructura del Archivo

```toml
agent_id = "planta-1"

[mqtt]
host = "localhost"
port = 1883

[[tags]]
id = "tag_vibracion"
driver = "Modbus"
driver_config = { slave_id = 1, address = 100 }
update_mode = { type = "Polling", interval_ms = 1000 }
```
