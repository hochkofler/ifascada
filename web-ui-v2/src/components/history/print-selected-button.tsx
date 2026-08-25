import { useState } from "react";
import { useTranslation } from "react-i18next";
import { postEdgeAction, type TagCurrent } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
import type { HistoryRow } from "./history-columns";
import { findPrintDeviceCommand, findPrintPersistAction } from "./print-metadata";

/**
 * Ported from web-ui/app/history/page.tsx's `executePrintSelected`. This is deliberately NOT
 * the brief's simplified sketch (a hardcoded `device.command print` payload): printing must use
 * the selected tag's own configured automation, read out of `metadata_json.automations` via
 * `findPrintDeviceCommand`/`findPrintPersistAction` -- a tag with no such automation can't print
 * at all (the button stays disabled), and a tag whose automation includes a `print.persist`
 * follow-up gets that sent too, after the print command.
 */
export function PrintSelectedButton({
  selectedRows,
  tag,
  fallbackSite,
}: {
  selectedRows: HistoryRow[];
  tag: TagCurrent | undefined;
  fallbackSite: string;
}) {
  const { t } = useTranslation();
  const [printing, setPrinting] = useState(false);
  const [message, setMessage] = useState("");

  const printCommandPayload = tag ? findPrintDeviceCommand(tag.metadata_json) : null;
  const printPersistAction = tag ? findPrintPersistAction(tag.metadata_json) : null;

  async function handlePrint() {
    if (!tag) {
      setMessage("No tag selected.");
      return;
    }
    if (!printCommandPayload) {
      setMessage("Selected tag has no print automation (device.command print).");
      return;
    }
    if (selectedRows.length === 0) {
      setMessage("Select at least one historical row.");
      return;
    }

    setPrinting(true);
    setMessage("");
    try {
      const selectedItems = [...selectedRows].sort(
        (a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime()
      );
      const site = tag.site_code || fallbackSite;
      const bufferId = `ui:${tag.tag_code}:${String(Date.now())}`;

      for (const row of selectedItems) {
        await postEdgeAction(
          site,
          tag.edge_code,
          "buffer.weights.accumulate",
          {
            buffer_id: bufferId,
            measurement_device_id: tag.device_code,
            measurement_device_name: tag.device_code,
            max_items: Math.max(500, selectedItems.length + 10),
            only_positive: false,
            trigger: {
              tag_id: tag.tag_code,
              device_id: tag.device_code,
              device_name: tag.device_code,
              value: row.value,
              timestamp: row.ts,
            },
          },
          { source: "web-ui-v2", target: "edge" }
        );
      }

      const payload = JSON.parse(JSON.stringify(printCommandPayload)) as Record<string, unknown>;
      const argsRaw = payload.args;
      const args = argsRaw && typeof argsRaw === "object" ? { ...(argsRaw as Record<string, unknown>) } : {};
      args.mode = "from_buffer";
      args.buffer_id = bufferId;
      args.clear_after_print = true;
      payload.args = args;
      payload.measurement_device_id = tag.device_code;
      payload.measurement_device_name = tag.device_code;
      payload.trigger = {
        tag_id: tag.tag_code,
        device_id: tag.device_code,
        device_name: tag.device_code,
      };
      if (!payload.command) payload.command = "print";

      await postEdgeAction(site, tag.edge_code, "device.command", payload, {
        source: "web-ui-v2",
        target: "edge",
      });

      if (printPersistAction) {
        await postEdgeAction(
          site,
          tag.edge_code,
          "print.persist",
          {
            ...printPersistAction.payload,
            buffer_id: bufferId,
            selected_count: selectedItems.length,
            tag_code: tag.tag_code,
          },
          { source: "web-ui-v2", target: "central" }
        );
      }

      setMessage(`Print command sent. samples=${String(selectedItems.length)} buffer=${bufferId}`);
    } catch (e) {
      setMessage(`Print failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setPrinting(false);
    }
  }

  return (
    <div className="flex items-center gap-2">
      <Button
        disabled={printing || selectedRows.length === 0 || !printCommandPayload}
        onClick={() => {
          void handlePrint();
        }}
      >
        {printing ? "…" : t("history.printSelected")}
      </Button>
      {message && <span className="text-xs text-muted-foreground">{message}</span>}
    </div>
  );
}
