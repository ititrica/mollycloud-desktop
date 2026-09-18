import { useEffect } from "react";
export function useModalBoundary() {
  useEffect(() => {
    let active: HTMLElement | null = null;
    let previous: HTMLElement | null = null;
    const focusable = () => active ? Array.from(active.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]')).filter(el => el.getClientRects().length > 0) : [];
    const observer = new MutationObserver(() => {
      const dialogs = Array.from(document.querySelectorAll<HTMLElement>(".fixed.inset-0")).filter(el => el.getClientRects().length > 0 && el.querySelector("button"));
      const next = dialogs.at(-1) ?? null;
      if (next === active) return;
      if (!active && next) previous = document.activeElement as HTMLElement;
      active = next;
      if (active) { active.setAttribute("role", "dialog"); active.setAttribute("aria-modal", "true"); active.setAttribute("aria-label", active.querySelector("h2,h3")?.textContent || "技能操作"); focusable()[0]?.focus(); }
      else { previous?.isConnected && previous.focus(); previous = null; }
      parent.postMessage({source: "molly-skills", type: "modal", open: !!active}, location.origin);
    });
    observer.observe(document.body, {childList: true, subtree: true});
    const keydown = (event: KeyboardEvent) => {
      if (!active) return;
      if (event.key === "Escape") {
        const close = Array.from(active.querySelectorAll<HTMLButtonElement>("button")).find(button => !button.disabled && /^(取消|Cancel|关闭|Close)$/.test(button.textContent?.trim() || ""));
        if (close) { event.preventDefault(); event.stopImmediatePropagation(); close.click(); }
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusable();
      const index = items.indexOf(document.activeElement as HTMLElement);
      if (event.shiftKey && index <= 0) { event.preventDefault(); items.at(-1)?.focus(); }
      else if (!event.shiftKey && (index === items.length - 1 || index < 0)) { event.preventDefault(); items[0]?.focus(); }
    };
    document.addEventListener("keydown", keydown, true);
    return () => { observer.disconnect(); document.removeEventListener("keydown", keydown, true); parent.postMessage({source: "molly-skills", type: "modal", open: false}, location.origin); };
  }, []);
}
