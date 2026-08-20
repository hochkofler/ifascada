import { Fragment, type ReactNode, type JSX } from "react";
import { Link } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import type { DocumentRef } from "./types";
import { useDocumentRoute } from "./context/document-route-context";

interface DetailLinkCellProps {
  docRef: DocumentRef;
  /** Accessible name for the link, e.g. "Nº documento 1024". */
  label?: string;
  children: ReactNode;
}

/**
 * Renders a cell value as a link to a document's detail view (SAP Fiori-style:
 * accent-colored text + chevron). Falls back to plain text when no route resolver
 * is mounted or the document type is unknown.
 */
export function DetailLinkCell({ docRef, label, children }: DetailLinkCellProps): JSX.Element {
  const resolve = useDocumentRoute();
  const target = resolve?.(docRef) ?? null;

  if (!target) return <Fragment>{children}</Fragment>;

  return (
    <Link
      to={target.to}
      params={target.params}
      aria-label={label}
      className="inline-flex items-center gap-0.5 font-medium text-primary underline-offset-4 hover:underline"
    >
      {children}
      <ChevronRight className="h-3.5 w-3.5 shrink-0 opacity-70" aria-hidden="true" />
    </Link>
  );
}
