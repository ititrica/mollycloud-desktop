import type { Event, EventCallback, EventName, EventTarget, Options, UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "./core";
import { getNativeBridge } from "./native-bridge";

export type { Event, EventCallback, EventName, EventTarget, Options, UnlistenFn };

async function subscribe<T>(event: EventName, handler: EventCallback<T>, options?: Options, onlyOnce = false): Promise<UnlistenFn> {
  const bridge = getNativeBridge();
  const internals = bridge?.internals;
  const events = bridge?.events;
  if (!internals?.transformCallback || !internals.unregisterCallback || !events) {
    throw new Error("MollyCloud 桌面事件桥接不可用。");
  }
  const unregisterCallback = internals.unregisterCallback.bind(internals);
  let disposed = false;
  let callbackId: number;
  const cleanup = async (eventId: number) => {
    if (disposed) return;
    disposed = true;
    events.unregisterListener(event, eventId);
    unregisterCallback(callbackId);
    await internals.invoke("plugin:event|unlisten", { event, eventId });
  };
  callbackId = internals.transformCallback((response) => {
    if (disposed) return;
    const data = response as Event<T>;
    if (onlyOnce) void cleanup(data.id).catch(console.error);
    handler(data);
  });
  const target = typeof options?.target === "string"
    ? { kind: "AnyLabel", label: options.target }
    : options?.target ?? { kind: "Any" };
  try {
    const eventId = await internals.invoke<number>("plugin:event|listen", { event, target, handler: callbackId });
    return () => cleanup(eventId);
  } catch (error) {
    disposed = true;
    unregisterCallback(callbackId);
    throw error;
  }
}

export function listen<T>(event: EventName, handler: EventCallback<T>, options?: Options): Promise<UnlistenFn> {
  return subscribe(event, handler, options);
}

export function once<T>(event: EventName, handler: EventCallback<T>, options?: Options): Promise<UnlistenFn> {
  return subscribe(event, handler, options, true);
}

export function emit<T>(event: string, payload?: T): Promise<void> {
  return invoke("plugin:event|emit", { event, payload });
}

export function emitTo<T>(target: EventTarget | string, event: string, payload?: T): Promise<void> {
  return invoke("plugin:event|emit_to", { target: typeof target === "string" ? { kind: "AnyLabel", label: target } : target, event, payload });
}
