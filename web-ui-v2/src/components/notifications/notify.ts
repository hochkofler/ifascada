import { createElement, type ReactNode } from "react";
import i18n from "i18next";
import { toast } from "sonner";
import { ApiError } from "@/lib/api-error";
import { MessageToastTitle } from "./components/message-toast";
import { useMessageLogStore } from "./message-log.store";
import { DURATION_BY_LEVEL } from "./durations";
import type { MessageLevel, NotifyOptions, SystemMessage } from "./types";

/** Resolve an i18n key against the global instance; falls back to the key itself. */
function translate(key: string, params?: Record<string, unknown>): string {
  return i18n.t(key, params === undefined ? undefined : { ...params });
}

/** Fire the right sonner method for the level (explicit switch keeps types happy). */
function fire(
  level: MessageLevel,
  content: ReactNode,
  options: { description?: string; duration: number }
): void {
  switch (level) {
    case "success":
      toast.success(content, options);
      break;
    case "error":
      toast.error(content, options);
      break;
    case "warning":
      toast.warning(content, options);
      break;
    case "info":
      toast.info(content, options);
      break;
  }
}

/**
 * The toast's description line — the message's own description with the correlation id appended
 * when one exists, so support has something to reference right from the toast itself, not only
 * from the session log drawer. Mathias, 2026-08-27, on a real timeout toast that told him to
 * "copy the correlation ID" without showing it anywhere: "debería venir con el id en el mensaje".
 *
 * A `correlationId` only exists once the server actually received and logged the request
 * (`GlobalExceptionFilter` mints it) — a genuine client-side timeout/network failure (the request
 * never got a response at all, see `client.ts`'s `AbortSignal.timeout` handling) has none to
 * show, so this only appends when one is actually present rather than inventing text about it.
 */
export function buildToastDescription(
  message: Pick<SystemMessage, "description" | "correlationId">
): string | undefined {
  if (message.correlationId === undefined) return message.description;
  const idSuffix = `ID: ${message.correlationId}`;
  return message.description ? `${message.description} · ${idSuffix}` : idSuffix;
}

/** Record a message in the session log and surface it as a toast. */
function emit(message: SystemMessage, duration: number): SystemMessage {
  useMessageLogStore.getState().push(message);
  fire(
    message.level,
    createElement(MessageToastTitle, { title: message.title, docNum: message.docNum }),
    {
      description: buildToastDescription(message),
      duration,
    }
  );
  return message;
}

function show(level: MessageLevel, titleKey: string, opts?: NotifyOptions): SystemMessage {
  const description = opts?.descriptionKey
    ? translate(opts.descriptionKey, opts.params)
    : opts?.description;
  const message: SystemMessage = {
    id: crypto.randomUUID(),
    level,
    title: translate(titleKey, opts?.params),
    titleKey,
    description,
    params: opts?.params,
    docNum: opts?.docNum,
    source: opts?.source,
    timestamp: new Date().toISOString(),
  };
  return emit(message, opts?.duration ?? DURATION_BY_LEVEL[level]);
}

interface ExtractedApiError {
  status?: number;
  code?: string;
  correlationId?: string;
  description?: string;
}

/** Pull the support-relevant fields out of an unknown thrown value. */
function extractApiError(error: unknown): ExtractedApiError {
  if (error instanceof ApiError) {
    const parsed = parseErrorBody(error.body);
    return {
      status: error.status,
      code: parsed.code,
      correlationId: parsed.correlationId,
      description: error.userMessage,
    };
  }
  if (error instanceof Error) {
    return { description: error.message };
  }
  return {};
}

/** Read `code`/`correlationId` from the backend's uniform error body (best effort). */
function parseErrorBody(body: string): { code?: string; correlationId?: string } {
  try {
    const json: unknown = JSON.parse(body);
    if (json !== null && typeof json === "object") {
      const obj = json as Record<string, unknown>;
      return {
        code: typeof obj.code === "string" ? obj.code : undefined,
        correlationId: typeof obj.correlationId === "string" ? obj.correlationId : undefined,
      };
    }
  } catch {
    // body is not JSON: nothing to extract
  }
  return {};
}

export type ApiErrorNotifyOptions = Pick<NotifyOptions, "params" | "source" | "duration">;

/**
 * Errors already recorded by `notify.apiError()` in this tick. `logApiError()` — the automatic
 * safety-net logger wired into TanStack Query's global cache — checks this so a mutation a
 * component already reports (its own toast + a specific title) doesn't also get a second,
 * generic entry in the session log.
 */
const alreadyReported = new WeakSet<object>();

function markReported(error: unknown): void {
  if (typeof error === "object" && error !== null) alreadyReported.add(error);
}

function wasReported(error: unknown): boolean {
  return typeof error === "object" && error !== null && alreadyReported.has(error);
}

export interface LogApiErrorOptions {
  /** i18n key for the generic title (query vs mutation phrasing differ). */
  titleKey: string;
  /** Logical origin — e.g. the TanStack Query key — for the session log. */
  source?: string;
}

/**
 * Centralized system-message API. Every user-facing message goes through here so
 * that duration, the docNum-in-red convention and the session log stay DRY.
 */
export const notify = {
  success: (titleKey: string, opts?: NotifyOptions): SystemMessage =>
    show("success", titleKey, opts),
  error: (titleKey: string, opts?: NotifyOptions): SystemMessage => show("error", titleKey, opts),
  warning: (titleKey: string, opts?: NotifyOptions): SystemMessage =>
    show("warning", titleKey, opts),
  info: (titleKey: string, opts?: NotifyOptions): SystemMessage => show("info", titleKey, opts),

  /**
   * Error helper for failed API calls: extracts the readable message, the backend
   * `code` and the `correlationId` (support handle) from an ApiError, logs them,
   * and shows an error toast titled by `fallbackTitleKey`.
   */
  apiError: (
    error: unknown,
    fallbackTitleKey: string,
    opts?: ApiErrorNotifyOptions
  ): SystemMessage => {
    markReported(error);
    const extracted = extractApiError(error);
    const message: SystemMessage = {
      id: crypto.randomUUID(),
      level: "error",
      title: translate(fallbackTitleKey, opts?.params),
      titleKey: fallbackTitleKey,
      description: extracted.description,
      params: opts?.params,
      code: extracted.code,
      correlationId: extracted.correlationId,
      httpStatus: extracted.status,
      source: opts?.source,
      timestamp: new Date().toISOString(),
    };
    return emit(message, opts?.duration ?? DURATION_BY_LEVEL.error);
  },

  /**
   * Safety-net logger for TanStack Query's global `queryCache`/`mutationCache` `onError`: records
   * every API failure in the session log — with NO toast — so it is never silently lost from the
   * notifications panel, even when no component wired `notify.apiError()` for it (queries never
   * do; a mutation might not either — see query-client.ts). Deferred one macrotask: TanStack Query
   * always runs the cache-level `onError` BEFORE a mutation's own `onError`, so a synchronous push
   * here would race ahead of — and duplicate — a richer `notify.apiError()` the component is about
   * to raise for the same error. The `setTimeout(0)` lets that synchronous chain finish first; if
   * it already reported the error via `alreadyReported`, this is a no-op.
   */
  logApiError: (error: unknown, opts: LogApiErrorOptions): void => {
    setTimeout(() => {
      if (wasReported(error)) return;
      const extracted = extractApiError(error);
      const message: SystemMessage = {
        id: crypto.randomUUID(),
        level: "error",
        title: translate(opts.titleKey),
        titleKey: opts.titleKey,
        description: extracted.description,
        code: extracted.code,
        correlationId: extracted.correlationId,
        httpStatus: extracted.status,
        source: opts.source,
        timestamp: new Date().toISOString(),
      };
      useMessageLogStore.getState().push(message);
    }, 0);
  },
};
