import type { ReactElement } from "react";
import { BellIcon, Trash2Icon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Sheet,
  SheetContent,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { NOTIFICATIONS_NS } from "../i18n";
import { useMessageLog } from "../use-message-log";
import { MessageLogRow } from "./message-log-row";

/**
 * Header action that opens the session message log. Mount it via the AppShell
 * `headerActions` slot. The bell shows an unread-style count of session messages.
 */
export function MessageLogDrawer(): ReactElement {
  const { t } = useTranslation(NOTIFICATIONS_NS);
  const { messages, clear } = useMessageLog();
  const count = messages.length;

  return (
    <Sheet>
      <SheetTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="relative size-11 md:size-8"
          title={t("log.open")}
          aria-label={t("log.open")}
        >
          <BellIcon className="h-3.5 w-3.5" />
          {count > 0 && (
            <span className="absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-medium text-primary-foreground">
              {count > 99 ? "99+" : count}
            </span>
          )}
        </Button>
      </SheetTrigger>
      <SheetContent className="flex w-full flex-col gap-0 sm:max-w-md">
        <SheetHeader>
          <SheetTitle>{t("log.title")}</SheetTitle>
        </SheetHeader>
        {count === 0 ? (
          <p className="px-4 py-8 text-center text-sm text-muted-foreground">{t("log.empty")}</p>
        ) : (
          <ScrollArea className="flex-1 px-4">
            <ul>
              {messages.map((message) => (
                <MessageLogRow key={message.id} message={message} />
              ))}
            </ul>
          </ScrollArea>
        )}
        {count > 0 && (
          <SheetFooter className="border-t pt-4">
            <Button type="button" variant="outline" size="sm" className="gap-1.5" onClick={clear}>
              <Trash2Icon className="size-3.5" />
              {t("log.clear")}
            </Button>
          </SheetFooter>
        )}
      </SheetContent>
    </Sheet>
  );
}
