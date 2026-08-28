/**
 * Textos del chrome compartido (namespace "appShell"), cosechados de
 * libs/app-shell/src/locales/es.ts de ifahub y podados a las claves que este tramo usa.
 * Las claves de sesion/cuenta/appSwitcher quedaron afuera a proposito: dependen de una
 * identidad OIDC que ifascada no tiene. Al traer el chrome de navegacion se suman
 * commandPalette y mobileMore.
 */
export const esAppShell = {
  chrome: {
    errorTitle: "Algo salio mal",
    errorFallback: "Error inesperado",
    retry: "Intentar de nuevo",
  },
  mobileMore: {
    trigger: "Mas opciones",
    title: "Opciones de la aplicacion",
  },
  theme: {
    toggleLabel: "Cambiar tema",
    light: "Claro",
    dark: "Oscuro",
    system: "Sistema",
  },
} as const;
