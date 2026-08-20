import { createContext, useContext, type ReactNode, type JSX } from "react";
import type { DocumentRef } from "../types";

/** Resolved navigation target for a document reference. `to` is a TanStack Router path. */
export interface DocumentRouteTarget {
  to: string;
  params: Record<string, string>;
}

/** Maps a document reference to its detail route. Returns null when the type is unknown. */
export type DocumentRouteResolver = (ref: DocumentRef) => DocumentRouteTarget | null;

const DocumentRouteContext = createContext<DocumentRouteResolver | null>(null);

export function DocumentRouteProvider({
  resolve,
  children,
}: {
  resolve: DocumentRouteResolver;
  children: ReactNode;
}): JSX.Element {
  return <DocumentRouteContext.Provider value={resolve}>{children}</DocumentRouteContext.Provider>;
}

/** Returns the document-route resolver, or null when no provider is mounted. */
export function useDocumentRoute(): DocumentRouteResolver | null {
  return useContext(DocumentRouteContext);
}
