import type { ConnectionCurrent } from "@/lib/api-client";
import type { Lamp } from "@/components/live/live-rows";

/**
 * Una fila de la grilla de conexiones.
 *
 * Campos planos y precalculados por la misma razon que en la grilla en vivo: el
 * `ColumnDefinition` del DataTable pide `accessorKey: keyof T`, y tener el texto en la fila
 * evita que lo que se filtra y lo que se ve puedan desincronizarse.
 */
export interface ConnectionRow {
  id: string;
  lamp: Lamp;
  connectionId: string;
  edge: string;
  state: string;
  severity: string;
  message: string;
  lastChange: string;
  site: string;
}

/**
 * Semaforo de una conexion.
 *
 * Se decide por `severity` y no por `state`: `state` es texto libre del edge-agent y varia entre
 * drivers, mientras que `severity` es el eje que el backend ya normaliza a info/warn/error. Un
 * estado desconocido cae en `warn`, no en `good`: ante la duda no se pinta de sano algo que
 * podria estar fallando.
 */
export function lampFromConnection(connection: ConnectionCurrent): Lamp {
  const severity = connection.severity.toLowerCase();
  if (severity === "error" || severity === "critical") return "bad";
  if (severity === "warn" || severity === "warning") return "warn";
  if (severity === "info") {
    return connection.state.toLowerCase() === "connected" ? "good" : "warn";
  }
  return "warn";
}

/**
 * `connection_id` NO es unico: en produccion `conn-protocol-1` aparece bajo dos edges distintos.
 * La identidad real es edge + connection, la misma leccion que ya dieron `device_code` y las
 * filas de la grilla en vivo.
 */
export function connectionRowId(row: ConnectionRow): string {
  return row.id;
}

export function buildConnectionRows(connections: readonly ConnectionCurrent[]): ConnectionRow[] {
  return connections
    .map((c): ConnectionRow => ({
      id: `conn:${c.edge_code}|${c.connection_id}`,
      lamp: lampFromConnection(c),
      connectionId: c.connection_id,
      edge: c.edge_code,
      state: c.state,
      severity: c.severity,
      message: c.message,
      lastChange: c.last_change_at,
      site: c.site_code,
    }))
    .sort((a, b) => {
      // Las que fallan primero: son las que motivan abrir esta pantalla.
      const rank = (l: Lamp) => (l === "bad" ? 0 : l === "warn" ? 1 : 2);
      const byLamp = rank(a.lamp) - rank(b.lamp);
      return byLamp !== 0 ? byLamp : a.id.localeCompare(b.id);
    });
}
