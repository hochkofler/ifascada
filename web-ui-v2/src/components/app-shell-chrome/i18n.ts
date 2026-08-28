import i18n from "i18next";
import { esAppShell } from "./locales/es";

export const APP_SHELL_NS = "appShell";

/**
 * Agrega el namespace "appShell" (chrome compartido) a la instancia global de
 * i18next. La inicialización sigue siendo de la app (su lib/i18n.ts).
 * Idempotente: seguro de llamar en cada render de AppProviders.
 */
export function registerAppShellLocales(): void {
  if (!i18n.isInitialized || i18n.hasResourceBundle("es", APP_SHELL_NS)) return;
  i18n.addResourceBundle("es", APP_SHELL_NS, esAppShell, true, false);
}

// El orden de imports de main.tsx puede evaluar esta lib ANTES del init de la app
// (routeTree → __root → app-shell se importa antes que ./lib/i18n): si todavía no
// está inicializado, registramos al evento; si ya lo está, de una.
if (i18n.isInitialized) {
  registerAppShellLocales();
} else {
  i18n.on("initialized", () => {
    registerAppShellLocales();
  });
}
