import { useMessageLogStore } from "./message-log.store";
import type { SystemMessage } from "./types";

export interface UseMessageLogResult {
  /** Session messages, newest first. */
  messages: SystemMessage[];
  /** Remove every message from the session log. */
  clear: () => void;
}

/** Reactive read access to the session message log. */
export function useMessageLog(): UseMessageLogResult {
  const messages = useMessageLogStore((state) => state.messages);
  const clear = useMessageLogStore((state) => state.clear);
  return { messages, clear };
}
