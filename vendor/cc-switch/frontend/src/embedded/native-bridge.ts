export type NativeInternals = {
  invoke<T>(command: string, args?: Record<string, unknown>, options?: unknown): Promise<T>;
  transformCallback?(callback: (response: unknown) => void, once?: boolean): number;
  unregisterCallback?(callbackId: number): void;
};

export type NativeBridge = {
  internals: NativeInternals;
  events?: { unregisterListener(event: string, eventId: number): void };
};

export type BridgeWindow = {
  readonly parent: BridgeWindow;
  readonly location: { origin: string; search: string };
  readonly __TAURI_INTERNALS__?: NativeInternals;
  readonly __TAURI_EVENT_PLUGIN_INTERNALS__?: NativeBridge["events"];
};

const currentWindow = () => window as unknown as BridgeWindow;
let preview: { owner: BridgeWindow; bridge: NativeBridge } | undefined;

// Tauri also injects non-writable globals into iframes. Never replace them:
// callbacks and IPC must share the parent WebView's callback registry.
export function getParentNativeBridge(scope: BridgeWindow = currentWindow()): NativeBridge | undefined {
  try {
    const parent = scope.parent;
    if (parent === scope || parent.location.origin !== scope.location.origin) return undefined;
    if (typeof parent.__TAURI_INTERNALS__?.invoke !== "function") return undefined;
    return { internals: parent.__TAURI_INTERNALS__, events: parent.__TAURI_EVENT_PLUGIN_INTERNALS__ };
  } catch {
    return undefined;
  }
}

export function canPreview(scope: BridgeWindow = currentWindow()): boolean {
  try {
    return scope.parent !== scope && scope.parent.location.origin === scope.location.origin
      && !getParentNativeBridge(scope) && !scope.__TAURI_INTERNALS__
      && new URLSearchParams(scope.location.search).get("ui-preview") === "ccswitch"
      && new URLSearchParams(scope.parent.location.search).get("ui-preview") === "console";
  } catch {
    return false;
  }
}

export function installPreviewBridge(bridge: NativeBridge): void {
  const owner = currentWindow();
  if (!canPreview(owner)) throw new Error("界面预览只能从 MollyCloud 控制台的预览模式打开。");
  preview = { owner, bridge };
}

export function getNativeBridge(): NativeBridge | undefined {
  const scope = currentWindow();
  return getParentNativeBridge(scope)
    ?? (preview?.owner === scope && canPreview(scope) ? preview.bridge : undefined);
}
