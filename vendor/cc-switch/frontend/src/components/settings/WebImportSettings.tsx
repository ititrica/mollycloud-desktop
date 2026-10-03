import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { emit } from "@tauri-apps/api/event";

export function WebImportSettings() {
  const [handler, setHandler] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pastedLink, setPastedLink] = useState("");

  const refresh = async () => {
    setHandler(await invoke<string | null>("ccswitch_web_import_handler"));
  };
  useEffect(() => { void refresh().catch(() => undefined); }, []);

  const change = async (action: "register_ccswitch_web_import" | "unregister_ccswitch_web_import") => {
    setBusy(true);
    try {
      await invoke(action);
      await refresh();
      toast.success(action === "register_ccswitch_web_import" ? "已启用 CC Switch 网页导入" : "已解除 MollyCloud 网页导入关联");
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-2 border-t border-border/50 pt-4">
      <p className="text-sm font-medium">网页导入</p>
      <p className="text-xs text-muted-foreground">主动关联后，其他网页中的 ccswitch:// 导入链接会交给 MollyCloud，先预览并等待你确认。已有独立版关联不会被自动覆盖。</p>
      <p className="break-all text-xs text-muted-foreground">{handler ? `当前系统关联：${handler}` : "当前未检测到系统关联"}</p>
      <div className="flex gap-2">
        <Button variant="outline" disabled={busy} onClick={() => void change("register_ccswitch_web_import")}>关联网页导入</Button>
        <Button variant="outline" disabled={busy || !handler} onClick={() => void change("unregister_ccswitch_web_import")}>解除 Molly 关联</Button>
      </div>
      <div className="flex gap-2">
        <Input aria-label="CC Switch 导入链接" value={pastedLink} onChange={(event) => setPastedLink(event.target.value)} placeholder="ccswitch://v1/import?..." />
        <Button variant="outline" disabled={!pastedLink.trim()} onClick={() => { void (async () => {
          try {
            const request = await invoke("parse_deeplink", { url: pastedLink.trim() });
            await emit("deeplink-import", request);
            setPastedLink("");
          } catch (error) { toast.error(String(error)); }
        })(); }}>预览链接</Button>
      </div>
    </div>
  );
}
