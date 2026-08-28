/**
 * Cosechado de libs/notifications/src/notify.test.ts de ifahub. Unica adaptacion: el runner
 * (node:test + node:assert -> vitest, que es el de este proyecto).
 */
import { expect, test } from "vitest";
import { buildToastDescription, notify } from "./notify";
import { useMessageLogStore } from "./message-log.store";

/** Waits past the setTimeout(0) macrotask notify.logApiError() defers its push to. */
function nextMacrotask(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

test("logApiError() records a session-log entry (no component wired notify.apiError for it)", async () => {
  useMessageLogStore.getState().clear();

  notify.logApiError(new Error("boom"), { titleKey: "notifications:log.autoTitle.query" });
  await nextMacrotask();

  const messages = useMessageLogStore.getState().messages;
  expect(messages.length).toBe(1);
  expect(messages[0]?.level).toBe("error");
});

test("logApiError() is a no-op when the SAME error was already reported via notify.apiError()", async () => {
  useMessageLogStore.getState().clear();
  const error = new Error("already handled by the component");

  // The component calls notify.apiError() (its own toast + a specific title) first...
  notify.apiError(error, "notifications:log.autoTitle.mutation");
  // ...then TanStack Query's global mutationCache.onError safety net fires for the same error.
  notify.logApiError(error, { titleKey: "notifications:log.autoTitle.mutation" });
  await nextMacrotask();

  // Only the component's rich entry survives — no duplicate generic entry.
  expect(useMessageLogStore.getState().messages.length).toBe(1);
});

test("logApiError() does NOT suppress a different, unrelated error", async () => {
  useMessageLogStore.getState().clear();

  notify.apiError(new Error("reported one"), "notifications:log.autoTitle.mutation");
  notify.logApiError(new Error("a completely different failure"), {
    titleKey: "notifications:log.autoTitle.query",
  });
  await nextMacrotask();

  expect(useMessageLogStore.getState().messages.length).toBe(2);
});

// Mathias, 2026-08-27, on a real timeout toast that told him to "copy the correlation ID"
// without ever showing it: "debería venir con el id en el mensaje".
test("buildToastDescription() appends the correlation id to the description when one exists", () => {
  expect(
    buildToastDescription({
      description: "La solicitud excedió 30000 ms",
      correlationId: "abc-123",
    })
  ).toBe("La solicitud excedió 30000 ms · ID: abc-123");
});

test("buildToastDescription() falls back to just the id when there is no description", () => {
  expect(buildToastDescription({ description: undefined, correlationId: "abc-123" })).toBe(
    "ID: abc-123"
  );
});

// A genuine client-side timeout/network failure never got a response — there is no
// correlationId to show, and this must not invent one or claim there is text about it.
test("buildToastDescription() leaves the description untouched when there is no correlation id", () => {
  expect(
    buildToastDescription({
      description: "La solicitud excedió 30000 ms",
      correlationId: undefined,
    })
  ).toBe("La solicitud excedió 30000 ms");
  expect(buildToastDescription({ description: undefined, correlationId: undefined })).toBe(
    undefined
  );
});
