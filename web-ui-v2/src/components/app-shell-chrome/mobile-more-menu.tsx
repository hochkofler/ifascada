import type { ReactElement, ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { esAppShell } from "./locales/es";

export interface MobileMoreMenuProps {
  command?: ReactNode;
  notifications?: ReactNode;
  appSwitcher?: ReactNode;
  theme?: ReactNode;
}

export function MobileMoreMenu(props: MobileMoreMenuProps): ReactElement {
  const { t } = useTranslation("appShell");
  const actions = [
    ["command", props.command],
    ["notifications", props.notifications],
    ["apps", props.appSwitcher],
    ["theme", props.theme],
  ] as const;

  return (
    <Sheet>
      <SheetTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-11 shrink-0"
          aria-label={t("mobileMore.trigger", { defaultValue: esAppShell.mobileMore.trigger })}
        >
          <MoreHorizontal className="size-5" aria-hidden="true" />
        </Button>
      </SheetTrigger>
      <SheetContent side="right" className="w-[min(20rem,calc(100vw-1rem))]">
        <SheetHeader>
          <SheetTitle>
            {t("mobileMore.title", { defaultValue: esAppShell.mobileMore.title })}
          </SheetTitle>
        </SheetHeader>
        <div className="grid gap-2 px-4">
          {actions.map(([key, action]) =>
            action == null ? null : (
              <div
                key={key}
                data-more-action={key}
                className="flex min-h-11 items-center justify-between gap-3 rounded-md border p-2"
              >
                {action}
              </div>
            )
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}
