import { Component, type ErrorInfo, type ReactNode } from "react";
import i18n from "i18next";
import { formatMessageForCopy, notify, type SystemMessage } from "@/components/notifications";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  /**
   * Built in componentDidCatch() via notify.apiError() — a real, readable message (ApiError's
   * userMessage, not its raw `.message`) plus `code`/`correlationId`, already pushed to the
   * session log. This screen unmounts the ENTIRE app (Toaster and the notifications bell
   * included, since ErrorBoundary wraps everything — see app-providers.tsx), so it's the only
   * place left where the user can see or copy that correlationId to hand it to Sistemas.
   */
  message: SystemMessage | null;
}

const COPY_FIELD_LABELS_NS = "notifications:log.copyField";

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null, message: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error, message: null };
  }

  override componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    console.error("[error-boundary]", error, errorInfo.componentStack);
    // No toast will actually render here (Toaster is unmounted along with the rest of the app),
    // but the session-log push is a plain Zustand store write — it survives regardless.
    const message = notify.apiError(error, "appShell:chrome.errorTitle");
    this.setState({ message });
  }

  private copyDetails = (): void => {
    const { message } = this.state;
    if (!message) return;
    void navigator.clipboard.writeText(
      formatMessageForCopy(message, {
        title: i18n.t(`${COPY_FIELD_LABELS_NS}.title`),
        description: i18n.t(`${COPY_FIELD_LABELS_NS}.description`),
        docNum: i18n.t(`${COPY_FIELD_LABELS_NS}.docNum`),
        code: i18n.t(`${COPY_FIELD_LABELS_NS}.code`),
        httpStatus: i18n.t(`${COPY_FIELD_LABELS_NS}.httpStatus`),
        source: i18n.t(`${COPY_FIELD_LABELS_NS}.source`),
        timestamp: i18n.t(`${COPY_FIELD_LABELS_NS}.timestamp`),
        correlationId: i18n.t(`${COPY_FIELD_LABELS_NS}.correlationId`),
      })
    );
  };

  private retry = (): void => {
    this.setState({ hasError: false, error: null, message: null });
  };

  override render(): ReactNode {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;

      const { message } = this.state;
      return (
        <div className="flex min-h-screen items-center justify-center">
          <div className="max-w-md text-center">
            <h2 className="text-lg font-semibold text-destructive">
              {i18n.t("appShell:chrome.errorTitle")}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {message?.description ?? i18n.t("appShell:chrome.errorFallback")}
            </p>
            {message?.correlationId !== undefined && (
              <p className="mt-2 text-xs text-muted-foreground">
                {i18n.t("notifications:log.correlation")}:{" "}
                <code className="font-mono">{message.correlationId}</code>
              </p>
            )}
            <div className="mt-4 flex items-center justify-center gap-4">
              <button
                type="button"
                onClick={this.retry}
                className="text-sm text-primary underline-offset-4 hover:underline"
              >
                {i18n.t("appShell:chrome.retry")}
              </button>
              {message && (
                <button
                  type="button"
                  onClick={this.copyDetails}
                  className="text-sm text-primary underline-offset-4 hover:underline"
                >
                  {i18n.t("notifications:log.copyAll")}
                </button>
              )}
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
