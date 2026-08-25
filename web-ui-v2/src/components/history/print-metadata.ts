/**
 * Print automation lookup, ported from web-ui/app/history/page.tsx's `extractActions` /
 * `findPrintDeviceCommand` / `findPrintPersistAction`. A tag's `metadata_json.automations`
 * array (see `TagCurrent.metadata_json` from Task 5's api-client) can carry one or more
 * automation definitions; printing must use the selected tag's OWN configured
 * `device.command print` payload template (and, if present, follow it up with a
 * `print.persist` action) rather than a hardcoded command -- see print-selected-button.tsx.
 */

export type DeviceCommandAction = {
  action_type: string;
  payload: Record<string, unknown>;
};

function extractActions(meta: Record<string, unknown> | undefined): DeviceCommandAction[] {
  const autos = meta?.automations;
  if (!Array.isArray(autos)) return [];
  const out: DeviceCommandAction[] = [];
  for (const a of autos) {
    if (!a || typeof a !== "object") continue;
    const obj = a as Record<string, unknown>;
    if (obj.enabled === false) continue;
    const actions = Array.isArray(obj.actions) ? obj.actions : obj.action ? [obj.action] : [];
    for (const act of actions) {
      if (!act || typeof act !== "object") continue;
      const actObj = act as Record<string, unknown>;
      const actionType = String(actObj.action_type || "");
      const payload =
        actObj.payload && typeof actObj.payload === "object"
          ? (actObj.payload as Record<string, unknown>)
          : {};
      out.push({ action_type: actionType, payload });
    }
  }
  return out;
}

/** The tag's configured `device.command print`/`print.escpos` payload template, or `null`. */
export function findPrintDeviceCommand(
  meta: Record<string, unknown> | undefined
): Record<string, unknown> | null {
  for (const a of extractActions(meta)) {
    if (a.action_type !== "device.command") continue;
    const cmd = String(a.payload.command || "").toLowerCase();
    if (cmd === "print" || cmd === "print.escpos") return a.payload;
  }
  return null;
}

/** The tag's configured `print.persist` follow-up action, or `null` if it has none. */
export function findPrintPersistAction(
  meta: Record<string, unknown> | undefined
): DeviceCommandAction | null {
  for (const a of extractActions(meta)) {
    if (a.action_type === "print.persist") return a;
  }
  return null;
}
