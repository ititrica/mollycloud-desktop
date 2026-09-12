import { vi } from "vitest";
import type { BridgeWindow, NativeInternals } from "./native-bridge";

export function mockEmbeddedWindow(native?: Pick<NativeInternals, "invoke">, nativePresent = true) {
  let id = 100;
  const callbacks = new Map<number, (response: unknown) => void>();
  const parentInternals = {
    invoke: native?.invoke ?? vi.fn().mockResolvedValue(42),
    transformCallback: vi.fn((callback: (response: unknown) => void) => {
      callbacks.set(++id, callback);
      return id;
    }),
    unregisterCallback: vi.fn((callbackId: number) => { callbacks.delete(callbackId); }),
  };
  const parentEvents = { unregisterListener: vi.fn() };
  const childInternals = { invoke: vi.fn(), transformCallback: vi.fn(), unregisterCallback: vi.fn() };
  const childEvents = { unregisterListener: vi.fn() };
  const parent = { location: { origin: "http://localhost:24320", search: "?ui-preview=console" } } as BridgeWindow;
  Object.defineProperties(parent, {
    parent: { value: parent },
    __TAURI_INTERNALS__: { value: nativePresent ? parentInternals : undefined },
    __TAURI_EVENT_PLUGIN_INTERNALS__: { value: nativePresent ? parentEvents : undefined },
    isTauri: { value: nativePresent },
  });
  const child = { location: { origin: "http://localhost:24320", search: "?ui-preview=ccswitch" } } as BridgeWindow;
  Object.defineProperties(child, {
    parent: { value: parent },
    __TAURI_INTERNALS__: { value: nativePresent ? childInternals : undefined },
    __TAURI_EVENT_PLUGIN_INTERNALS__: { value: nativePresent ? childEvents : undefined },
    isTauri: { value: nativePresent },
  });
  vi.stubGlobal("window", child);
  return { child, parent, callbacks, parentInternals, parentEvents, childInternals, childEvents };
}
