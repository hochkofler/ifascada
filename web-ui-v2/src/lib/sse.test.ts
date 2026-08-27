import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { subscribeSse, type RtEvent } from "./sse";

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  url: string;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  listeners: Record<string, ((ev: MessageEvent) => void)[]> = {};
  closed = false;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  addEventListener(type: string, handler: (ev: MessageEvent) => void) {
    (this.listeners[type] ??= []).push(handler);
  }
  close() {
    this.closed = true;
  }
  emit(type: "message" | "runtime", data: unknown) {
    const ev = { data: JSON.stringify(data) } as MessageEvent;
    if (type === "message") this.onmessage?.(ev);
    for (const h of this.listeners[type] ?? []) h(ev);
  }
}

describe("subscribeSse", () => {
  beforeEach(() => {
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("connects to /api/stream/events with query params from options", () => {
    subscribeSse(() => {}, { site: "plant-a", line: "line-main", excludeRaw: true });
    const es = FakeEventSource.instances[0];
    expect(es.url).toContain("/api/stream/events?");
    expect(es.url).toContain("site=plant-a");
    expect(es.url).toContain("line=line-main");
    expect(es.url).toContain("exclude_raw=true");
  });

  it("connects with no query string when no options are given", () => {
    subscribeSse(() => {});
    expect(FakeEventSource.instances[0].url).toBe("/api/stream/events");
  });

  it("delivers events from both the bare message event and the named 'runtime' event", () => {
    const received: RtEvent[] = [];
    subscribeSse((evt) => received.push(evt));
    const es = FakeEventSource.instances[0];
    const payload = { event_type: "telemetry", site: "plant-a", agent: "edge-mix-1", payload: { v: 1 }, published_at: "2026-08-26T18:00:00Z" };
    es.emit("message", payload);
    es.emit("runtime", { ...payload, event_type: "runtime-variant" });
    expect(received).toHaveLength(2);
    expect(received[0].event_type).toBe("telemetry");
    expect(received[1].event_type).toBe("runtime-variant");
  });

  it("stamps received_at_ms on delivery", () => {
    const received: RtEvent[] = [];
    subscribeSse((evt) => received.push(evt));
    const es = FakeEventSource.instances[0];
    es.emit("message", { event_type: "x", site: "s", agent: "a", payload: {}, published_at: "2026-08-26T18:00:00Z" });
    expect(received[0].received_at_ms).toBeTypeOf("number");
  });

  it("ignores malformed JSON payloads instead of throwing", () => {
    const received: RtEvent[] = [];
    subscribeSse((evt) => received.push(evt));
    const es = FakeEventSource.instances[0];
    expect(() => es.onmessage?.({ data: "not json" } as MessageEvent)).not.toThrow();
    expect(received).toHaveLength(0);
  });

  it("returns an unsubscribe function that closes the EventSource", () => {
    const unsubscribe = subscribeSse(() => {});
    const es = FakeEventSource.instances[0];
    expect(es.closed).toBe(false);
    unsubscribe();
    expect(es.closed).toBe(true);
  });
});
