import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { Copy, KeyRound, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { AppId } from "@/lib/api";
import { providersApi } from "@/lib/api/providers";
import { isAdditiveAppId } from "@/config/appConfig";
import { accountCost, requestKeyAction, type MollyKeySnapshot } from "@/embedded/mollyKeys";
import { extractErrorMessage } from "@/utils/errorUtils";
import type { Provider } from "@/types";
import { usePiCurrentState } from "@/lib/query/pi";

export function MollyKeyDirectory({ appId, currentProviderId, providers = {}, onEdit }: { appId: AppId; currentProviderId: string; providers?: Record<string, Provider>; onEdit: (provider: Provider) => void }) {
  const client = useQueryClient();
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const { data, error, isLoading, isFetching, refetch, dataUpdatedAt } = useQuery({
    queryKey: ["molly-account-keys", appId],
    queryFn: () => invoke<MollyKeySnapshot>("sync_molly_key_providers", { agent: appId }),
    staleTime: 30_000, refetchInterval: 60_000, retry: false,
  });
  const { data: liveIds } = useQuery({
    queryKey: ["opencodeLiveProviderIds"], queryFn: () => providersApi.getOpenCodeLiveProviderIds(), enabled: appId === "opencode",
  });
  const { data: openclawIds } = useQuery({queryKey:["openclawLiveProviderIds"],queryFn:()=>providersApi.getOpenClawLiveProviderIds(),enabled:appId==="openclaw"});
  const { data: hermesIds } = useQuery({queryKey:["hermesLiveProviderIds"],queryFn:()=>providersApi.getHermesLiveProviderIds(),enabled:appId==="hermes"});
  const { data: piState } = usePiCurrentState(appId==="pi");
  const refreshLive = async () => {
    await Promise.all(["opencodeLiveProviderIds","openclawLiveProviderIds","hermesLiveProviderIds","pi"].map(name=>client.invalidateQueries({queryKey:[name]})));
    await client.invalidateQueries({queryKey:["providers",appId]});
  };
  const additive = isAdditiveAppId(appId);
  useEffect(() => {
    if (dataUpdatedAt || error) void client.invalidateQueries({queryKey:["providers",appId]});
  },[dataUpdatedAt,error,client,appId]);
  useEffect(() => { if (!copied) return; const timer = setTimeout(() => setCopied(null), 1800); return () => clearTimeout(timer); }, [copied]);
  const run = async (id: string, operation: () => Promise<unknown>, message?: string) => {
    if (busy) return;
    setBusy(id);
    try { await operation(); if (message) toast.success(message); }
    catch (e) { toast.error(extractErrorMessage(e)); }
    finally { setBusy(null); }
  };
  const shown = data?.keys.filter(key => `${key.name} ${key.group?.name ?? ""}`.toLowerCase().includes(query.trim().toLowerCase())) ?? [];
  return <section className="space-y-3 pb-4" aria-label="MollyCloud 账户密钥">
    <div className="flex flex-wrap items-center gap-2">
      <Button className="create-key-button" size="sm" onClick={() => requestKeyAction("create", appId)}><Plus className="mr-1 h-4 w-4" />创建密钥</Button>
      <Button variant="outline" size="sm" aria-label="复制 API 端点" onClick={() => void run("endpoint", () => invoke("copy_api_endpoint"), "API 端点已复制")}><code className="text-xs">https://mollycloud.cn/v1</code><Copy className="ml-2 h-3 w-3" /></Button>
      <Input value={query} onChange={e => setQuery(e.target.value)} aria-label="搜索账户密钥" placeholder="搜索密钥或分组…" className="h-8 min-w-0 max-w-64" />
      <Button variant="ghost" size="sm" aria-label="刷新账户密钥" disabled={isFetching} onClick={() => void refetch()}><RefreshCw className={`h-4 w-4 ${isFetching ? "animate-spin" : ""}`} /></Button>
    </div>
    {additive && <p className="text-xs text-muted-foreground">选择需要的密钥和模型后添加；工具原有配置会保留。</p>}
    {error && <div role="alert" className="text-sm text-destructive">账户密钥同步失败：{extractErrorMessage(error)}<Button size="sm" variant="ghost" onClick={() => void refetch()}>重试</Button></div>}
    {data?.current_removed && <p role="alert" className="text-sm text-amber-700 dark:text-amber-300">当前配置对应的账户密钥已删除，请选择其他密钥或 Official 配置。</p>}
    {isLoading && <p role="status" className="text-sm text-muted-foreground">正在读取账户密钥并准备配置…</p>}
    {!isLoading && !error && !data?.keys.length && <div className="molly-keys-empty rounded-lg border border-dashed p-6 text-center text-muted-foreground"><KeyRound className="mx-auto mb-2 h-5 w-5" /><p className="text-sm">还没有 API 密钥</p><Button size="sm" className="mt-3" onClick={() => requestKeyAction("create", appId)}>创建密钥</Button></div>}
    {shown.map(key => {
      const provider = key.provider_id ? providers[key.provider_id] : undefined;
      const mapped = provider?.meta?.mollyCodexMode === "mapped";
      const pending = provider?.meta?.mollyPendingApply === true;
      const enabled = Boolean(key.provider_id && (additive ? appId === "opencode" ? liveIds?.includes(key.provider_id) : appId === "openclaw" ? openclawIds?.includes(key.provider_id) : appId === "hermes" ? hermesIds?.includes(key.provider_id) : appId === "pi" ? piState?.enabledProviderIds.includes(key.provider_id) : providers[key.provider_id]?.meta?.liveConfigManaged === true : key.provider_id === currentProviderId));
      const active = key.status === "active";
      const quota = Number(key.quota);
      return <article key={key.id} className="molly-key-record rounded-lg border border-border bg-card p-4 space-y-3" data-key-id={key.id} data-provider-id={key.provider_id ?? undefined}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><strong className="break-all text-sm">{key.name || "未命名密钥"}</strong><span className="text-xs text-muted-foreground">{active ? "正常" : key.status}</span>{enabled && <span className="rounded bg-primary/15 px-2 py-0.5 text-xs text-primary">{additive ? "已添加" : "当前使用"}</span>}</div>
            <div className="mt-1 flex flex-wrap items-center gap-2"><code className="text-xs text-muted-foreground">{key.key}</code><Button variant="ghost" size="sm" className="h-6 px-2 key-copy-button" aria-label={`复制 ${key.name} 的密钥`} disabled={busy !== null} onClick={() => void run(key.id, async () => { await invoke("copy_api_key", { keyId: key.id }); setCopied(key.id); })}><Copy className="mr-1 h-3 w-3" />{copied === key.id ? "已复制" : "复制"}</Button></div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="ghost" className="h-8 w-8 p-0" aria-label={`编辑 ${key.name} 的供应商配置`} title="编辑供应商" data-key-action="configure" disabled={busy !== null || !key.compatible} onClick={() => void run(key.id, async () => {
              const id = key.provider_id ?? await invoke<string>("prepare_molly_key_provider", { agent: appId, keyId: key.id });
              const saved = provider ?? (await providersApi.getAll(appId))[id];
              if (!saved) throw new Error("供应商配置尚未加载，请刷新后重试。");
              onEdit(saved);
              await client.invalidateQueries({queryKey:["providers",appId]});
            })}><Pencil className="h-4 w-4" /></Button>
            {enabled && additive ? <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => void run(key.id, async () => { await providersApi.removeFromLiveConfig(key.provider_id!, appId); await refreshLive(); }, "已从工具配置移除")}>移除</Button> : <Button size="sm" disabled={!active || !key.compatible || Boolean(key.provider_id && !key.model) || (enabled && !pending) || (mapped && Boolean(key.error)) || busy !== null} onClick={() => {
              if (!key.provider_id) { requestKeyAction("configure-enable", appId, key.id, key.model); return; }
              void run(key.id, async () => { const result = await providersApi.switch(key.provider_id!, appId); await refreshLive(); result.warnings?.forEach(w => toast.warning(w)); }, additive ? "已添加到工具配置" : "已启用密钥");
            }}>{enabled ? pending ? "应用配置" : "已启用" : additive ? "选择并添加" : key.provider_id ? "启用" : "选择模型并启用"}</Button>}
            <Button className="key-delete-button" variant="ghost" size="sm" aria-label={`删除 ${key.name} 的密钥`} data-key-action="delete" onClick={() => requestKeyAction("delete", appId, key.id)}><Trash2 className="h-4 w-4 text-destructive" /></Button>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-xs text-muted-foreground">
          {appId === "codex" && <div className="flex items-center gap-2">
            <span className={!mapped ? "text-foreground" : undefined}>原生</span>
            <Switch aria-label={`${key.name} 的映射模式`} checked={mapped} disabled={busy !== null || !active || !key.compatible} onCheckedChange={checked => void run(key.id, async () => {
              if (!key.provider_id) await invoke("prepare_molly_key_provider", {agent:appId,keyId:key.id});
              await invoke("set_molly_key_mode", {agent:appId,keyId:key.id,mode:checked ? "mapped" : "native"});
              await client.invalidateQueries({queryKey:["providers",appId]});
              await refetch();
            }, "配置已保存，点击启用或应用配置后生效")} />
            <span className={mapped ? "text-foreground" : undefined}>映射</span>
            {pending && <span>待应用</span>}
          </div>}
          <Button className="key-group-trigger h-auto max-w-full whitespace-normal break-all text-left" size="sm" variant="outline" data-key-action="group" aria-label={`切换 ${key.name} 的分组`} onClick={() => requestKeyAction("group", appId, key.id)}>{key.group?.name || "未选择分组"}{key.group?.rate_multiplier != null && <span className="ml-2 shrink-0">{key.group.rate_multiplier}x</span>}</Button>
          <span>累计消费 {accountCost(key.usage?.total_actual_cost)} · 今日 {accountCost(key.usage?.today_actual_cost)}</span>
          {Number.isFinite(quota) && quota > 0 && <span>额度 {accountCost(key.quota_used)} / {accountCost(quota)}</span>}
          {key.model && <span className="break-all">模型 {key.model}</span>}
        </div>
        {(!key.compatible || key.error) && <p className="text-xs text-muted-foreground">{!key.compatible ? "当前分组不适用于此工具，可修改分组后使用。" : key.error}</p>}
      </article>;
    })}
    {data?.keys.length && !shown.length ? <p className="text-sm text-muted-foreground">没有匹配的密钥。</p> : null}
  </section>;
}
