import React, {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";

function syncMollyAccent(root: HTMLElement): void {
  const style = getComputedStyle(root);
  const tokens = {lime:"lime", "on-lime":"on-lime", "lime-deep":"lime-deep", "lime-soft":"lime-soft", "lime-text":root.classList.contains("dark") ? "lime" : "lime-deep"};
  for (const [name, token] of Object.entries(tokens)) {
    const hex = style.getPropertyValue(`--color-${token}`).trim();
    if (!/^#[a-f\d]{6}$/i.test(hex)) continue;
    const [r,g,b] = [1,3,5].map(i => parseInt(hex.slice(i,i+2),16)/255);
    const max = Math.max(r,g,b), min = Math.min(r,g,b), delta = max-min, l = (max+min)/2;
    const sat = delta === 0 ? 0 : delta / (1-Math.abs(2*l-1));
    let h = delta === 0 ? 0 : max === r ? ((g-b)/delta)%6 : max === g ? (b-r)/delta+2 : (r-g)/delta+4;
    h = (h*60+360)%360;
    root.style.setProperty(`--molly-${name}-hsl`,`${h} ${sat*100}% ${l*100}%`);
  }
}

type Theme = "light" | "dark" | "system";
const followsHostTheme = typeof window !== "undefined" && window.parent !== window;

interface ThemeProviderProps {
  children: React.ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
}

interface ThemeContextValue {
  theme: Theme;
  followsHostTheme: boolean;
  setTheme: (theme: Theme) => void;
}

const ThemeProviderContext = createContext<ThemeContextValue | undefined>(
  undefined,
);

export function ThemeProvider({
  children,
  defaultTheme = "system",
  storageKey = "cc-switch-theme",
}: ThemeProviderProps) {
  const getInitialTheme = () => {
    // This embedded view shares the host origin; the host owns its appearance.
    if (followsHostTheme) return window.parent.document.documentElement.dataset.theme === "dark" ? "dark" : "light";
    if (typeof window === "undefined") {
      return defaultTheme;
    }

    const stored = window.localStorage.getItem(storageKey) as Theme | null;
    if (stored === "light" || stored === "dark" || stored === "system") {
      return stored;
    }

    return defaultTheme;
  };

  const [theme, setThemeState] = useState<Theme>(getInitialTheme);

  useEffect(() => {
    if (!followsHostTheme) return;
    const host = window.parent.document.documentElement;
    const sync = () => setThemeState(host.dataset.theme === "dark" ? "dark" : "light");
    const observer = new MutationObserver(sync);
    observer.observe(host, { attributes: true, attributeFilter: ["data-theme"] });
    sync();
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    if (!followsHostTheme) window.localStorage.setItem(storageKey, theme);
  }, [theme, storageKey]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    const root = window.document.documentElement;
    root.classList.remove("light", "dark");

    if (theme === "system") {
      const isDark =
        window.matchMedia &&
        window.matchMedia("(prefers-color-scheme: dark)").matches;
      root.classList.add(isDark ? "dark" : "light");
      return;
    }

    root.classList.add(theme);
    root.dataset.theme = theme;
    syncMollyAccent(root);
  }, [theme]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => {
      if (theme !== "system") {
        return;
      }

      const root = window.document.documentElement;
      root.classList.toggle("dark", mediaQuery.matches);
      root.classList.toggle("light", !mediaQuery.matches);
    };

    if (theme === "system") {
      handleChange();
    }

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [theme]);

  // Sync native window theme (Windows/macOS title bar)
  useEffect(() => {
    if (typeof window === "undefined" || followsHostTheme) {
      return;
    }

    let isCancelled = false;

    const updateNativeTheme = async (nativeTheme: string) => {
      if (isCancelled) return;
      try {
        await invoke("set_window_theme", { theme: nativeTheme });
      } catch (e) {
        // Ignore errors (e.g., when not running in Tauri)
        console.debug("Failed to set native window theme:", e);
      }
    };

    // When "system", pass "system" so Tauri uses None (follows OS theme natively).
    // This keeps the WebView's prefers-color-scheme in sync with the real OS theme,
    // allowing effect #3's media query listener to fire on system theme changes.
    if (theme === "system") {
      updateNativeTheme("system");
    } else {
      updateNativeTheme(theme);
    }

    return () => {
      isCancelled = true;
    };
  }, [theme]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme,
      followsHostTheme,
      setTheme: (nextTheme: Theme) => {
        if (followsHostTheme || nextTheme === theme) return;
        setThemeState(nextTheme);
      },
    }),
    [theme],
  );

  return (
    <ThemeProviderContext.Provider value={value}>
      {children}
    </ThemeProviderContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeProviderContext);
  if (context === undefined) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
