# web-ui-v2

Frontend React de ifascada: monitoreo en vivo e historico de tags de planta.

## Stack

React 19 + Vite + TypeScript estricto, TanStack Router/Query/Table, Tailwind 4 sobre los
primitivos shadcn de `src/components/ui`, i18next (es), Zustand para estado de UI transversal,
Zod para validar el borde HTTP.

Buena parte del chrome (notificaciones, cascaron, breadcrumb, tema, DataTable) esta cosechada de
las librerias compartidas de [ifahub](https://github.com/ifamasbun/ifahub); cada archivo dice de
donde vino y que se adapto.

## Comandos

```
npm run dev         # servidor de desarrollo (proxy /api -> 127.0.0.1:8088)
npm run typecheck   # tsc --noEmit
npm run lint        # oxlint
npm test            # vitest
npm run build       # typecheck + build de produccion
npm run format      # prettier
```

## Backend

Necesita el central-server. Con Docker, desde la raiz del repo:

```
POSTGRES_PASSWORD=postgres MQTT_PORT_EXTERNAL=41883 \
  docker compose -f docker-compose.scada.yml --profile central up -d
docker compose -f docker-compose.edge-sim.yml up -d   # telemetria simulada
```

`MQTT_PORT_EXTERNAL` es necesario porque el 51883 por defecto cae en un rango de puertos que
Windows reserva.

Los fixtures de `src/test/fixtures/` son respuestas reales de ese backend; el test de contrato
(`src/lib/api-schemas.contract.test.ts`) los valida contra los esquemas de Zod, asi que corre sin
necesidad de tener el servidor levantado.
