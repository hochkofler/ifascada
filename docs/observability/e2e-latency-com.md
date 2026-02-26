# E2E Latency Measurement (COM -> Edge -> Central -> SSE)

Este flujo mide automáticamente latencia con puerto COM real en 3 hitos:

1. `send_ts` (simulador serial escribe frame en COM)
2. `db_ts` (central persiste evento en `telemetry_ingest_events.ts`)
3. `sse_recv_ts` (cliente SSE recibe evento desde `/api/stream/events`)

## Scripts

- `scripts/mock-scale-com.ps1`
  - Ahora soporta `-LogPath` y guarda JSONL con `ts` y `frame`.
- `scripts/e2e-latency-com.ps1`
  - Ejecuta simulación serial + listener SSE + consulta SQL + reporte p50/p95.

## Ejecución

```powershell
powershell -ExecutionPolicy Bypass -File scripts/e2e-latency-com.ps1 `
  -ApiBase "http://192.168.103.70:8088" `
  -PgDsn "host=192.168.103.70 port=55432 user=postgres dbname=rustscada sslmode=disable" `
  -Site "plant-a" `
  -Edge "edge-com-01" `
  -Tag "tag_scale_manual_compound" `
  -WriteComPort "COM8" `
  -Samples 20 `
  -IntervalMs 1000
```

## Salida

En `data/e2e-latency/`:

- `send-<run>.jsonl`: timestamps del simulador.
- `sse-<run>.jsonl`: timestamps de recepción SSE.
- `report-<run>.json`: resumen + detalle por muestra.

Campos clave del resumen:

- `send_to_payload_ms`: tiempo simulador -> `payload.timestamp` (generado por edge).
- `send_to_db_ms`: tiempo simulador -> persistencia DB.
- `send_to_sse_ms`: tiempo simulador -> llegada a cliente SSE.

## Nota importante

`send_to_sse_ms` mide llegada a cliente SSE, no el tiempo exacto de paint/render del navegador.
Para medir render estricto en Web UI, usar browser tracing (Chrome Performance/Playwright).
