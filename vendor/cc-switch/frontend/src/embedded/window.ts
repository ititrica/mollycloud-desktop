// An embedded view has no native window lifecycle of its own.
const blocked = async (): Promise<void> => {
  throw new Error("内置 CC Switch 的窗口由 MollyCloud 管理。");
};

export function getCurrentWindow() {
  return {
    isMaximized: async () => false,
    onResized: async (handler: () => void) => {
      window.addEventListener("resize", handler);
      return () => window.removeEventListener("resize", handler);
    },
    onFocusChanged: async (handler: (event: { payload: boolean }) => void) => {
      const focus = () => handler({ payload: true });
      const blur = () => handler({ payload: false });
      window.addEventListener("focus", focus);
      window.addEventListener("blur", blur);
      return () => {
        window.removeEventListener("focus", focus);
        window.removeEventListener("blur", blur);
      };
    },
    setDecorations: blocked,
    minimize: blocked,
    toggleMaximize: blocked,
    close: blocked,
  };
}
