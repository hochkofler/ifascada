import { formatServerDateTime } from "@/lib/datetime";
import type { SystemMessage } from "./types";

export interface CopyFieldLabels {
  title: string;
  description: string;
  docNum: string;
  code: string;
  httpStatus: string;
  source: string;
  timestamp: string;
  correlationId: string;
}

/** Builds the plain-text block copied to the clipboard for "copy full error". */
export function formatMessageForCopy(message: SystemMessage, labels: CopyFieldLabels): string {
  const lines: string[] = [`${labels.title}: ${message.title}`];
  if (message.description !== undefined && message.description !== "") {
    lines.push(`${labels.description}: ${message.description}`);
  }
  if (message.docNum !== undefined) {
    lines.push(`${labels.docNum}: #${message.docNum}`);
  }
  if (message.code !== undefined) {
    lines.push(`${labels.code}: ${message.code}`);
  }
  if (message.httpStatus !== undefined) {
    lines.push(`${labels.httpStatus}: ${message.httpStatus}`);
  }
  if (message.source !== undefined && message.source !== "") {
    lines.push(`${labels.source}: ${message.source}`);
  }
  lines.push(`${labels.timestamp}: ${formatServerDateTime(message.timestamp)}`);
  if (message.correlationId !== undefined) {
    lines.push(`${labels.correlationId}: ${message.correlationId}`);
  }
  return lines.join("\n");
}
