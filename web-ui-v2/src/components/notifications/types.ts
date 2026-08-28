/** Severity of a system message. Drives icon, color and default duration. */
export type MessageLevel = "success" | "error" | "warning" | "info";

/**
 * A single system message as recorded in the session log. The text fields are
 * already resolved (i18n applied) so the log stays readable for support; the
 * `titleKey`/`params` are kept too in case a consumer wants to re-render it.
 */
export interface SystemMessage {
  /** Stable id (crypto.randomUUID). */
  id: string;
  level: MessageLevel;
  /** Resolved title text. */
  title: string;
  /** i18n key the title came from (when applicable). */
  titleKey?: string;
  /** Resolved description text (e.g. a server-provided error message). */
  description?: string;
  /** Interpolation params used to resolve the title/description. */
  params?: Record<string, unknown>;
  /**
   * Reference document number (SAP DocNum / created id). Rendered in brand red.
   * Only present when the operation actually produced one.
   */
  docNum?: string | number;
  /** Backend error code (e.g. SAP_ERROR, HTTP_ERROR) when this came from an ApiError. */
  code?: string;
  /** Backend correlation id — the handle support uses to trace the request in logs. */
  correlationId?: string;
  /** HTTP status when this came from an ApiError. */
  httpStatus?: number;
  /** Logical origin (feature/route) where the message was raised. */
  source?: string;
  /** ISO timestamp of when the message was raised. */
  timestamp: string;
}

/** Options accepted by the notify helpers. */
export interface NotifyOptions {
  /** Already-resolved description text. */
  description?: string;
  /** i18n key for the description; resolved with `params`. Use instead of `description`. */
  descriptionKey?: string;
  /** Interpolation params for the title key (and `descriptionKey`). */
  params?: Record<string, unknown>;
  /** Reference document number. Rendered in brand red; omitted when undefined. */
  docNum?: string | number;
  /** Logical origin (feature/route) for the session log. */
  source?: string;
  /** Override the level's default display duration (ms). */
  duration?: number;
}
