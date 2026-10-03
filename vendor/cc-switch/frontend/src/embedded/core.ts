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
  if (["sync_molly_key_providers", "prepare_molly_key_provider", "set_molly_key_mode", "copy_api_key", "copy_api_endpoint"].includes(command)) {
    return bridge.invoke<T>(command, args);
  }
  if (["register_ccswitch_web_import", "unregister_ccswitch_web_import", "ccswitch_web_import_handler"].includes(command)) {
    return bridge.invoke<T>(command, args);
  }
  if ((command === "queryProviderUsage" || command === "testUsageScript") &&
      typeof args?.providerId === "string" && args.providerId.startsWith("molly-")) {
    return bridge.invoke<T | null>("query_molly_provider_usage", {
      agent: args.app, providerId: args.providerId,
    }).then((result) => result ?? bridge.invoke<T>(`plugin:molly-ccswitch|${command}`, args, options)) as Promise<T>;
  }
  if (command === "open_external") {
    try {
      const url = new URL(String(args?.url ?? ""));
      // Repair links on cards imported before the website/API split.
      if (url.hostname === "mollycloud.cn" && url.pathname.replace(/\/+$/, "") === "/v1") {
        url.pathname = "/";
      }
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
