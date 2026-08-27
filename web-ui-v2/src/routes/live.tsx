import { createFileRoute } from "@tanstack/react-router";
import { LivePage } from "@/components/live/live-page";

export const Route = createFileRoute("/live")({
  component: LivePage,
});
