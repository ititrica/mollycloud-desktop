import { useState, useEffect } from "react";
export type Theme = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";
export function useTheme() {
  const [resolvedTheme, setResolved] = useState<ResolvedTheme>("light");
  useEffect(() => {
    const source = window.parent.document.documentElement;
    const apply = () => {
      const theme = source.dataset.theme === "dark" ? "dark" : "light";
      document.documentElement.dataset.theme = theme;
      document.documentElement.classList.toggle("dark", theme === "dark");
      setResolved(theme);
    };
    apply();
    const observer = new MutationObserver(apply);
    observer.observe(source, {attributes: true, attributeFilter: ["data-theme"]});
    return () => observer.disconnect();
  }, []);
  return {theme: "system" as Theme, resolvedTheme, setTheme: (_theme: Theme) => {}};
}
