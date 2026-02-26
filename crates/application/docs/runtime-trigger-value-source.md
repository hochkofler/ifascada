# Runtime Trigger Value Source

## Cambio
Automations ya no deben depender del valor `display` formateado.
Ahora consumen `trigger_value` derivado del pipeline de tag.

## Regla
1. `pipeline.trigger.value_source = "canonical"` (default)
   - usa valor post `extract/scale/range` y pre `format`.
2. `pipeline.trigger.value_source = "display"`
   - usa valor final formateado.

Alias soportado: `pipeline.trigger_source`.

## Efecto
- MQTT telemetria sigue publicando `value` (display).
- Evaluacion de trigger (`consecutive_numeric`) usa `trigger_value`.
- Se elimina necesidad de parsear string display en triggers.

## Ejemplo
```json
{
  "pipeline": {
    "extract": "scale:compound",
    "format": "{value} {unit}",
    "trigger": { "value_source": "canonical" }
  }
}
```
