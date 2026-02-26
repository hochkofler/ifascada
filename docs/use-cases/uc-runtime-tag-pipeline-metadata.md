# UC: Runtime Tag Pipeline by Metadata

## Objetivo
Activar transformaciones y validaciones de tag en capa `application` usando solo `tags.metadata_json`, manteniendo comportamiento previo por defecto.

## Contrato
El central incluye `metadata_json` por tag en runtime signed config (`/api/edge/config/runtime`).
El edge propaga ese metadata al dominio `Tag.metadata`.
`application::runtime::tag_pipeline` evalúa:

1. `pipeline.scale`
- `factor` (o alias `slope`)
- `offset` (o alias `intercept`)

2. `pipeline.range` (o `pipeline.validate.range`)
- `min`
- `max`

3. `pipeline.extract`
- `scale:compound` (o `compound_json`) para parsear payload compound JSON:
  - espera campos `value`, `unit`, `raw`

4. `pipeline.format`
- plantilla de display con placeholders:
  - `{value}`
  - `{unit}`
  - `{raw}`
- `pipeline.trim` opcional para compactar espacios.

## Política de calidad
1. Sin config de pipeline: passthrough + `Quality=Good`.
2. Valor no numérico con regla numérica: `Quality=Bad(ValidationFailed)`.
3. Valor fuera de rango: `Quality=Bad(OutOfRange)`.

## Ejemplo
```json
{
  "pipeline": {
    "extract": "scale:compound",
    "scale": { "factor": 0.1, "offset": 0.0 },
    "range": { "min": 0.0, "max": 100.0 },
    "format": "{value} {unit}",
    "trim": true
  }
}
```

Entrada `157` -> salida `15.7`.
Entrada `{"raw":"+ 12.300 g","unit":"g","value":12.3}` -> salida `"12.3 g"`.

## Aplicación operativa (tag de balanza manual)
SQL listo:
- `scripts/sql-configure-scale-display-pipeline.sql`

Aplicar:
```powershell
psql "$env:CENTRAL_PG_DSN" -v ON_ERROR_STOP=1 -f "scripts/sql-configure-scale-display-pipeline.sql"
```

Para que el edge tome la config actualizada:
1. esperar polling de config, o
2. publicar `config/apply`, o
3. reiniciar edge.

## Compatibilidad
No rompe tags existentes:
1. Si `metadata_json.pipeline` no existe, el runtime mantiene la lógica actual.
2. No cambia contratos MQTT/API de telemetría.
