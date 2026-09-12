import type { ReactNode } from "react";
import type { UpdateInfo } from "@/lib/updater";

// Embedded modules ship with MollyCloud. Never run the upstream updater.
const value = {
  hasUpdate: false,
  updateInfo: null as UpdateInfo | null,
  isChecking: false,
  error: null as string | null,
  isDismissed: true,
  dismissUpdate: () => undefined,
  resetDismiss: () => undefined,
  checkUpdate: async (): Promise<boolean> => {
    throw new Error("内置 CC Switch 随 MollyCloud 更新，请使用控制台的应用更新功能。");
  },
};
export function UpdateProvider({ children }: { children: ReactNode }) { return <>{children}</>; }
export function useUpdate() { return value; }
