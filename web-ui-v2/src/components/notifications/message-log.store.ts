import { create } from "zustand";
import type { SystemMessage } from "./types";

/** Ring buffer cap: keep the most recent N messages of the session in memory. */
const MAX_MESSAGES = 200;

interface MessageLogState {
  /** Newest first. */
  messages: SystemMessage[];
  push: (message: SystemMessage) => void;
  clear: () => void;
}

/**
 * Session message log. In-memory only (cleared on reload by design — this tracks
 * a single session). `notify` writes here via getState().push; the drawer reads
 * it reactively through useMessageLog.
 */
export const useMessageLogStore = create<MessageLogState>((set) => ({
  messages: [],
  push: (message) => {
    set((state) => ({
      messages: [message, ...state.messages].slice(0, MAX_MESSAGES),
    }));
  },
  clear: () => {
    set({ messages: [] });
  },
}));
