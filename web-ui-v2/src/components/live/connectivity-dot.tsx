/**
 * The "lamp" from web-ui's Live page (green/amber/red dot) -- a colored indicator, not a
 * Badge with text, since operators scan a dense grid of these at a glance. Styling is via
 * data-state so it can be themed centrally (globals.css) without a new dependency.
 */
export function ConnectivityDot({ state, title }: { state: "good" | "warn" | "bad"; title?: string }) {
  return (
    <span
      data-testid="connectivity-dot"
      data-state={state}
      title={title}
      className="inline-block h-2.5 w-2.5 rounded-full data-[state=good]:bg-emerald-500 data-[state=warn]:bg-amber-500 data-[state=bad]:bg-red-500"
    />
  );
}
