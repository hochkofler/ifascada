import type { ReactElement } from "react";

export interface MessageToastTitleProps {
  title: string;
  /** Reference document number; rendered in brand red. Hidden when undefined. */
  docNum?: string | number;
}

/**
 * Toast title content. Centralizes the "docNum in brand red" convention so every
 * message renders the reference the same way (DRY). `text-primary` is the IFA
 * brand red in both light and dark themes.
 */
export function MessageToastTitle({ title, docNum }: MessageToastTitleProps): ReactElement {
  return (
    <span className="flex flex-wrap items-center gap-x-2">
      <span>{title}</span>
      {docNum !== undefined && <span className="font-semibold text-primary">#{docNum}</span>}
    </span>
  );
}
