import type { AppId } from "@/lib/api";

export type KeyAction = "create" | "delete" | "group" | "configure" | "configure-enable";
export function requestKeyAction(action: KeyAction, app: AppId, keyId?: string, model?: string): void {
  window.parent.postMessage({ source: "molly-ccswitch", type: "key-action", action, app, keyId, model }, window.location.origin);
}

export interface MollyAccountKey {
  id: string; name: string; key: string; status: string;
  group: { id?: number | string; name?: string; platform?: string; rate_multiplier?: number | string };
  quota: number | string | null; quota_used: number | string | null;
  usage: { total_actual_cost?: number | string | null; today_actual_cost?: number | string | null } | null;
  provider_id: string | null; model: string; compatible: boolean; error: string | null;
}
export interface MollyKeySnapshot { agent: AppId; keys: MollyAccountKey[]; current_removed: boolean }

export function accountCost(value: unknown): string {
  const number = value == null || value === "" ? NaN : Number(value);
  return Number.isFinite(number) ? `$${number.toFixed(4)}` : "—";
}
