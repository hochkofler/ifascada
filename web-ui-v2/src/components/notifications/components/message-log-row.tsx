import type { ReactElement } from "react";
import { CircleCheckIcon, CopyIcon, InfoIcon, OctagonXIcon, TriangleAlertIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatServerDateTime } from "@/lib/datetime";
import { formatMessageForCopy } from "../format-message-for-copy";
import { NOTIFICATIONS_NS } from "../i18n";
import type { MessageLevel, SystemMessage } from "../types";

const LEVEL_ICON: Record<MessageLevel, typeof InfoIcon> = {
  success: CircleCheckIcon,
  info: InfoIcon,
  warning: TriangleAlertIcon,
  error: OctagonXIcon,
};

const LEVEL_COLOR: Record<MessageLevel, string> = {
  success: "text-success",
  info: "text-info",
  warning: "text-warning",
  error: "text-destructive",
};

export interface MessageLogRowProps {
  message: SystemMessage;
}

/** One row in the session message log drawer. */
export function MessageLogRow({ message }: MessageLogRowProps): ReactElement {
  const { t } = useTranslation(NOTIFICATIONS_NS);
  const Icon = LEVEL_ICON[message.level];

  const copyFull = () => {
    void navigator.clipboard.writeText(
      formatMessageForCopy(message, {
        title: t("log.copyField.title"),
        description: t("log.copyField.description"),
        docNum: t("log.copyField.docNum"),
        code: t("log.copyField.code"),
        httpStatus: t("log.copyField.httpStatus"),
        source: t("log.copyField.source"),
        timestamp: t("log.copyField.timestamp"),
        correlationId: t("log.copyField.correlationId"),
      })
    );
  };

  return (
    <li className="flex gap-3 border-b py-3 text-sm last:border-b-0">
      <Icon className={`mt-0.5 size-4 shrink-0 ${LEVEL_COLOR[message.level]}`} />
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex flex-wrap items-center gap-x-2">
          <span className="font-medium">{message.title}</span>
          {message.docNum !== undefined && (
            <span className="font-semibold text-primary">#{message.docNum}</span>
          )}
        </div>
        {message.description !== undefined && message.description !== "" && (
          <p className="text-muted-foreground">{message.description}</p>
        )}
        {message.correlationId !== undefined && (
          <div className="text-xs text-muted-foreground">
            {t("log.correlation")}: <code className="font-mono">{message.correlationId}</code>
          </div>
        )}
        <div className="flex flex-wrap items-center gap-x-3 text-xs text-muted-foreground">
          <span>{formatServerDateTime(message.timestamp)}</span>
          {message.code !== undefined && (
            <span>
              {t("log.code")}: {message.code}
            </span>
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="ml-auto size-5"
            title={t("log.copyAll")}
            onClick={copyFull}
          >
            <CopyIcon className="size-3" />
          </Button>
        </div>
      </div>
    </li>
  );
}
