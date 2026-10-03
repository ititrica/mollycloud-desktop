// CSS HSL channels are required by both embedded Tailwind themes. Read Molly's
// shared tokens instead of maintaining another palette inside each tool.
export function applyEmbeddedTheme(root: HTMLElement, theme: "light" | "dark"): void {
  root.dataset.theme = theme;
  root.dataset.mollyEmbeddedTheme = "true";
  root.classList.toggle("dark", theme === "dark");
  root.classList.toggle("light", theme === "light");
  const style = getComputedStyle(root);
  const tokens: Record<string, string> = {
    background: "bg", foreground: "text", surface: "surface", muted: "surface-soft",
    "muted-foreground": "text-soft", border: "border", sidebar: "chrome",
    primary: "lime", "primary-foreground": "on-lime", ring: "lime-deep",
    "primary-hover": "lime-hover", "primary-pressed": "lime-pressed", "accent-border": "lime-border",
    accent: "lime-soft", "accent-foreground": "lime-deep",
    "accent-text": theme === "dark" ? "lime" : "lime-deep",
  };
  for (const [name, token] of Object.entries(tokens)) {
    const hex = style.getPropertyValue(`--color-${token}`).trim();
    if (!/^#[a-f\d]{6}$/i.test(hex)) continue;
    const [r, g, b] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255);
    const max = Math.max(r, g, b), min = Math.min(r, g, b), delta = max - min, l = (max + min) / 2;
    const saturation = delta === 0 ? 0 : delta / (1 - Math.abs(2 * l - 1));
    let hue = delta === 0 ? 0 : max === r ? ((g - b) / delta) % 6 : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4;
    hue = (hue * 60 + 360) % 360;
    root.style.setProperty(`--molly-${name}-hsl`, `${hue} ${saturation * 100}% ${l * 100}%`);
  }
}
