import { getNativeBridge } from "./native-bridge";

// Every upstream application command belongs to the isolated Molly plugin.
// Native plugin commands keep their own namespaces, using the parent's bridge.
export function invoke<T>(command: string, args?: Record<string, unknown>, options?: unknown): Promise<T> {
  const bridge = getNativeBridge()?.internals;
  if (!bridge) return Promise.reject(new Error("请在 MollyCloud 桌面客户端中打开内置 CC Switch。"));
  if (/^plugin:(process|updater|window)\|/.test(command)) {
    return Promise.reject(new Error("内置模块不能更新、退出或控制 MollyCloud 的原生窗口。"));
  }
  if (command === "open_provider_terminal") {
    return bridge.invoke<T>("launch_ccswitch_cli", { appType: args?.app, providerId: args?.providerId, cwd: args?.cwd });
  }
  if (command === "open_external") {
    try {
      const url = new URL(String(args?.url ?? ""));
      if (url.protocol !== "http:" && url.protocol !== "https:") throw new Error("protocol");
      return bridge.invoke<T>("open_url", { url: url.toString() });
    } catch {
      return Promise.reject(new Error("内置 CC Switch 只允许打开 HTTP 或 HTTPS 网页链接。"));
    }
  }
  const scoped = command.startsWith("plugin:") ? command : `plugin:molly-ccswitch|${command}`;
  return bridge.invoke<T>(scoped, args, options);
}

export function isTauri(): boolean {
  return Boolean(getNativeBridge());
}
