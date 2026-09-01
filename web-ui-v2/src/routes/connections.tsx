import { createFileRoute } from "@tanstack/react-router";
import { ConnectionsPage } from "@/components/connections/connections-page";

export const Route = createFileRoute("/connections")({
  component: ConnectionsPage,
  staticData: { breadcrumb: "Conexiones" },
});
