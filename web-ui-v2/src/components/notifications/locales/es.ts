/** Chrome strings for the message log drawer (namespace "notifications"). */
export const esNotifications = {
  log: {
    title: "Mensajes",
    open: "Ver mensajes",
    empty: "No hay mensajes en esta sesión.",
    clear: "Limpiar",
    copyAll: "Copiar error completo",
    correlation: "ID de seguimiento",
    code: "Código",
    autoTitle: {
      query: "No se pudo cargar información",
      mutation: "Una operación no se completó",
    },
    copyField: {
      title: "Título",
      description: "Descripción",
      docNum: "Documento",
      code: "Código",
      httpStatus: "HTTP Status",
      source: "Origen",
      timestamp: "Fecha",
      correlationId: "ID de seguimiento",
    },
  },
} as const;
