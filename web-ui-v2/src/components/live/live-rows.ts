import type { ColumnFiltersState } from "@tanstack/react-table";
import type { DeviceCurrent, EdgeCurrent, TagCurrent } from "@/lib/api-client";
import { edgeConnected, lampFromDeviceState } from "@/lib/connectivity";
import { formatValueWithUnit } from "@/lib/value-formatting";

export type Lamp = "good" | "warn" | "bad";

/**
 * Una fila de la grilla en vivo: un device, o uno de sus tags.
 *
 * Los campos de display van PLANOS y precalculados, no como una union discriminada con los DTO
 * anidados. La razon es el contrato del DataTable: `ColumnDefinition` pide `accessorKey: keyof T`,
 * y sobre una union `keyof` solo devuelve las claves comunes -- no habria forma de declarar una
 * columna "code" o "detail". Aplanar tambien elimina la necesidad de una funcion aparte que
 * derive el texto de cada celda: los campos SON el texto, asi que lo que se filtra y lo que se
 * ve no pueden desincronizarse.
 *
 * `kind` se conserva para que los renderers puedan diferenciar visualmente device de tag, y
 * `site`/`edgeCode` para poder abrir el panel de diagnostico desde cualquiera de las dos.
 */
export interface LiveRow {
  /**
   * `edge` es un edge SIN devices reportando. Se muestra como fila propia a proposito: un edge
   * que dejo de reportar del todo es justo el modo de falla que el operador necesita ver, y si
   * no apareciera, el Reset quedaria inalcanzable para los edges que mas lo necesitan.
   */
  kind: "device" | "edge" | "tag";
  id: string;
  lamp: Lamp;
  /** `device_code` o `tag_code`. */
  code: string;
  /** Edge al que pertenece; vacio en las filas de tag, que lo heredan del padre. */
  edge: string;
  /** Resumen de tags en un device; valor con unidad en un tag. */
  detail: string;
  /** Estado del device, o calidad del tag. */
  quality: string;
  /** ISO. Se formatea al renderizar, no aca, para que el filtro opere sobre el texto crudo. */
  lastSeen: string;
  site: string;
  edgeCode: string;
  /** Tags del device. Vacio en las filas de tag. */
  tags: LiveRow[];
}

/**
 * Identidad estable de fila.
 *
 * NO es un detalle: la pagina refresca cada 2500 ms (mas los nudges de SSE), y el id por defecto
 * de TanStack es el indice del array. Con el indice, cada refetch reasigna las filas y lo que el
 * operador tenia expandido se cierra -- o peor, queda abierto sobre OTRO device. El `device_code`
 * solo tampoco alcanza: se repite entre edges (en los datos reales hay dos `dev_scale_manual_1`,
 * uno en `edge-01` y otro en `edge-com-01`), asi que la clave incluye el edge.
 */
export function liveRowId(row: LiveRow): string {
  return row.id;
}

/** `getSubRows` del DataTable: solo los devices con tags tienen hijos. */
export function liveSubRows(row: LiveRow): LiveRow[] | undefined {
  return row.tags.length > 0 ? row.tags : undefined;
}

/** Semaforo de un tag segun su `quality.status`, con el edge como condicion previa. */
export function lampFromTag(tag: TagCurrent, edgeConn: boolean): Lamp {
  if (!edgeConn) return "bad";
  const status = String(tag.quality.status ?? "").toLowerCase();
  if (status === "good") return "good";
  if (status === "bad" || status === "error") return "bad";
  return "warn";
}

/** Resumen de tags de un device. Los contadores ya vienen en el DTO: cero consultas extra. */
export function tagSummary(device: DeviceCurrent): string {
  return `${String(device.tags_connected)} ok · ${String(device.tags_stale)} stale · ${String(device.tags_disconnected)} caidos`;
}

/**
 * Arma el arbol device -> tags. Los tags se agrupan por `edge_code|device_code` en el cliente:
 * `TagCurrent` ya trae ambos, asi que no hace falta un endpoint nuevo.
 */
export function buildLiveRows(
  devices: readonly DeviceCurrent[],
  edges: readonly EdgeCurrent[],
  tags: readonly TagCurrent[],
  nowMs: number = Date.now()
): LiveRow[] {
  const edgeByCode = new Map(edges.map((e) => [e.edge_code, e]));

  const tagsByDevice = new Map<string, TagCurrent[]>();
  for (const tag of tags) {
    const key = `${tag.edge_code}|${tag.device_code}`;
    const bucket = tagsByDevice.get(key);
    if (bucket) bucket.push(tag);
    else tagsByDevice.set(key, [tag]);
  }

  const deviceRows = devices
    .map((device): LiveRow => {
      const conn = edgeConnected(edgeByCode.get(device.edge_code), nowMs);
      const key = `${device.edge_code}|${device.device_code}`;
      const deviceTags = (tagsByDevice.get(key) ?? [])
        .slice()
        .sort((a, b) => a.tag_code.localeCompare(b.tag_code))
        .map((tag): LiveRow => ({
          kind: "tag",
          id: `tag:${tag.edge_code}|${tag.device_code}|${tag.tag_code}`,
          lamp: lampFromTag(tag, conn),
          code: tag.tag_code,
          edge: "",
          detail: formatValueWithUnit(tag.value),
          quality: tag.quality.status ?? "",
          lastSeen: tag.ts,
          site: tag.site_code,
          edgeCode: tag.edge_code,
          tags: [],
        }));
      return {
        kind: "device",
        id: `dev:${device.edge_code}|${device.device_code}`,
        lamp: lampFromDeviceState(device, conn),
        code: device.device_code,
        edge: device.edge_code,
        detail: tagSummary(device),
        quality: device.state,
        lastSeen: device.last_seen_at,
        site: device.site_code,
        edgeCode: device.edge_code,
        tags: deviceTags,
      };
    })
    .sort((a, b) => a.id.localeCompare(b.id));

  // Edges sin ninguna fila de device quedarian invisibles, y con ellos su Reset. Se agregan al
  // final como filas propias.
  const edgeCodesWithDevices = new Set(devices.map((d) => d.edge_code));
  const orphanEdges = edges
    .filter((e) => !edgeCodesWithDevices.has(e.edge_code))
    .map((e): LiveRow => ({
      kind: "edge",
      id: `edge:${e.edge_code}`,
      lamp: edgeConnected(e, nowMs) ? "good" : "bad",
      code: e.edge_code,
      // Vacio a proposito: en esta fila el codigo YA es el del edge, repetirlo en la columna
      // Edge seria ruido.
      edge: "",
      // El texto lo pone el renderer: este modulo es puro y no conoce i18n.
      detail: "",
      quality: e.status,
      lastSeen: e.last_seen_at,
      site: e.site_code,
      edgeCode: e.edge_code,
      tags: [],
    }))
    .sort((a, b) => a.id.localeCompare(b.id));

  return [...deviceRows, ...orphanEdges];
}

function matches(row: LiveRow, filters: ColumnFiltersState): boolean {
  return filters.every((f) => {
    const needle = String(f.value ?? "")
      .trim()
      .toLowerCase();
    if (!needle) return true;
    const field = (row as unknown as Record<string, unknown>)[f.id];
    return String(field ?? "")
      .toLowerCase()
      .includes(needle);
  });
}

/**
 * Aplica los filtros por columna sobre el arbol.
 *
 * El DataTable corre con `manualFiltering: true`: la fila de filtros reporta la intencion y la
 * pagina la aplica. Aca eso es una ventaja, porque la regla correcta para una jerarquia no es la
 * que aplicaria un filtro generico:
 *
 *   - si el DEVICE coincide, se muestra con TODOS sus tags (ya lo encontraste, queres ver todo);
 *   - si el device no coincide pero SI alguno de sus tags, se muestra el device con solo esos
 *     tags -- sin esto, filtrar por un tag esconderia la fila que lo contiene y no verias nada.
 */
export function filterLiveRows(rows: readonly LiveRow[], filters: ColumnFiltersState): LiveRow[] {
  const active = filters.filter((f) => String(f.value ?? "").trim() !== "");
  if (active.length === 0) return [...rows];

  return rows.flatMap((row): LiveRow[] => {
    if (row.kind !== "device") return matches(row, active) ? [row] : [];
    if (matches(row, active)) return [row];
    const hits = row.tags.filter((tag) => matches(tag, active));
    return hits.length > 0 ? [{ ...row, tags: hits }] : [];
  });
}
