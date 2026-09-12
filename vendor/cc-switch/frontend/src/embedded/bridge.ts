export interface HostNavigation {
  source: "mollycloud";
  type: "navigate" | "provider-imported";
  providerId?: string;
  app?: string;
}

export function reportLoadError(): void {
  window.parent.postMessage({ source: "molly-ccswitch", type: "load-error" }, window.location.origin);
}

export function listenToHost(handler: (message: HostNavigation) => void): () => void {
  const listener = (event: MessageEvent<unknown>) => {
    if (event.source !== window.parent || event.origin !== window.location.origin) return;
    const message = event.data as Partial<HostNavigation> | null;
    if (!message || message.source !== "mollycloud" || !["navigate", "provider-imported"].includes(message.type ?? "")) return;
    if (message.providerId !== undefined && (typeof message.providerId !== "string" || message.providerId.length > 256)) return;
    if (message.app !== undefined && typeof message.app !== "string") return;
    handler(message as HostNavigation);
  };
  window.addEventListener("message", listener);
  window.parent.postMessage({ source: "molly-ccswitch", type: "ready" }, window.location.origin);
  return () => window.removeEventListener("message", listener);
}
