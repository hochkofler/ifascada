import i18n from "i18next";
import { esNotifications } from "./locales/es";

export const NOTIFICATIONS_NS = "notifications";

/**
 * Adds the "notifications" namespace (drawer chrome) to the global i18next
 * instance. App owns initialization (its own lib/i18n.ts); this only registers
 * the bundle. Idempotent: safe to call repeatedly.
 */
export function registerNotificationsLocales(): void {
  if (!i18n.isInitialized || i18n.hasResourceBundle("es", NOTIFICATIONS_NS)) return;
  i18n.addResourceBundle("es", NOTIFICATIONS_NS, esNotifications, true, false);
}

// Import order may evaluate this lib before the app's i18n init. If i18next is
// already initialized, register now; otherwise wait for the init event.
if (i18n.isInitialized) {
  registerNotificationsLocales();
} else {
  i18n.on("initialized", () => {
    registerNotificationsLocales();
  });
}
