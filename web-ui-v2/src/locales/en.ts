export const en = {
  nav: {
    live: "Live",
    history: "History",
  },
  live: {
    title: "Live status",
    edgesOnline: "Edges online",
    site: "Site",
    siteError: "Failed to load site list",
    line: "Line",
    area: "Area",
    cell: "Cell",
    edge: "Edge",
    noData: "No tags for the current context",
    diagnostics: {
      reset: "Reset",
      sent: "Command sent, waiting for edge confirmation...",
      confirmedRecovered: "Reset confirmed: the edge reported again.",
      timedOutNoRecovery:
        "The command was sent, but the edge didn't confirm recovery within 30s. May require manual intervention.",
      error: "Failed to send the reset command.",
      recentEvents: "Recent events",
      eventsError: "Could not load the event history.",
      noEvents: "No recent events for this edge.",
    },
  },
  history: {
    title: "Historical query",
    tag: "Tag",
    pageSize: "Page size",
    valueFilter: "Value >",
    unit: "Unit",
    printSelected: "Print selected",
    selectedCount: "Selected",
  },
};
