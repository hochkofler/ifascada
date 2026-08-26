export const es = {
  nav: {
    live: "En vivo",
    history: "Histórico",
  },
  live: {
    title: "Estado en vivo",
    edgesOnline: "Edges en línea",
    site: "Sitio",
    siteError: "No se pudo cargar la lista de sitios",
    line: "Línea",
    area: "Área",
    cell: "Celda",
    edge: "Edge",
    noData: "Sin tags para el contexto actual",
    diagnostics: {
      reset: "Reset",
      sent: "Comando enviado, esperando confirmación del edge...",
      confirmedRecovered: "Reset confirmado: el edge volvió a reportar.",
      timedOutNoRecovery:
        "El comando se envió, pero el edge no confirmó recuperación en 30s. Puede requerir intervención manual.",
      error: "Error al enviar el comando de reset.",
      recentEvents: "Eventos recientes",
      eventsError: "No se pudo cargar el historial de eventos.",
      noEvents: "Sin eventos recientes para este edge.",
    },
  },
  history: {
    title: "Consulta histórica",
    tag: "Tag",
    pageSize: "Tamaño de página",
    valueFilter: "Valor >",
    unit: "Unidad",
    printSelected: "Imprimir seleccionados",
    selectedCount: "Seleccionados",
  },
};
