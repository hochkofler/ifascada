import i18n from "i18next";
import { esTables } from "./locales/es";

export const TABLES_NS = "tables";

/**
 * Adds the "tables" namespace (DataTable chrome: toolbar, filters, pagination,
 * saved views, column dialog) to the global i18next instance. The app owns
 * initialization (its own lib/i18n.ts); this only registers the bundle.
 * Idempotent: safe to call repeatedly.
 */
export function registerTablesLocales(): void {
  if (!i18n.isInitialized || i18n.hasResourceBundle("es", TABLES_NS)) return;
  i18n.addResourceBundle("es", TABLES_NS, esTables, true, false);
}

// Import order may evaluate this lib before the app's i18n init. If i18next is
// already initialized, register now; otherwise wait for the init event.
if (i18n.isInitialized) {
  registerTablesLocales();
} else {
  i18n.on("initialized", () => {
    registerTablesLocales();
  });
}
