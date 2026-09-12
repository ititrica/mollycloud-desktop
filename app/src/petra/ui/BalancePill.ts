import type { LogicalRect } from "../input/regions";

export interface OverlayAccount {
  balance?: unknown;
  today_tokens?: unknown;
}

function numericValue(value: unknown): number | null {
  if (typeof value !== "number" && (typeof value !== "string" || !value.trim())) return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

export function formatOverlayBalance(value: unknown): string | null {
  const number = numericValue(value);
  return number == null ? null : `${number.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}$`;
}

export function formatTodayTokens(value: unknown): string {
  const number = numericValue(value);
  return number == null || number < 0
    ? "暂无法获取今日用量"
    : `${Math.trunc(number).toLocaleString("en-US")} token`;
}

/** A small account popover positioned in window logical pixels, next to the head. */
export function createBalancePill(loadAccount: () => Promise<OverlayAccount>, onGeometryChange: () => void) {
  const root = document.getElementById("balance-pill")!;
  const trigger = document.getElementById("balance-pill-trigger")!;
  const value = document.getElementById("balance-pill-value")!;
  const dropdown = document.getElementById("balance-pill-usage")!;
  const tokens = document.getElementById("balance-pill-tokens")!;
  let hasBalance = false;
  let positioned = false;
  let open = false;
  let requestId = 0;

  function setOpen(next: boolean) {
    next &&= hasBalance && positioned;
    if (next === open) return;
    open = next;
    trigger.setAttribute("aria-expanded", String(open));
    dropdown.hidden = !open;
    onGeometryChange();
  }

  function syncVisibility() {
    const hidden = !hasBalance || !positioned;
    root.classList.toggle("hidden", hidden);
    if (hidden) setOpen(false);
  }

  root.addEventListener("pointerenter", () => setOpen(true));
  root.addEventListener("pointerleave", () => {
    if (!root.contains(document.activeElement)) setOpen(false);
  });
  root.addEventListener("focusin", () => setOpen(true));
  root.addEventListener("focusout", (event) => {
    if (!root.contains(event.relatedTarget as Node | null) && !root.matches(":hover")) setOpen(false);
  });
  root.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      setOpen(false);
    }
  });
  trigger.addEventListener("click", () => setOpen(true));
  root.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
  });

  return {
    get isOpen() { return open; },
    async refresh() {
      const request = ++requestId;
      try {
        const account = await loadAccount();
        if (request !== requestId) return;
        const balance = formatOverlayBalance(account.balance);
        hasBalance = balance != null;
        value.textContent = balance ?? "--";
        tokens.textContent = formatTodayTokens(account.today_tokens);
      } catch {
        if (request !== requestId) return;
        hasBalance = false;
        value.textContent = "--";
        tokens.textContent = "暂无法获取今日用量";
      }
      syncVisibility();
      onGeometryChange();
    },
    position(head: LogicalRect | null, visible: LogicalRect) {
      positioned = head != null;
      syncVisibility();
      if (!head || !hasBalance) return;

      const margin = 8;
      const width = trigger.offsetWidth;
      const height = trigger.offsetHeight;
      // Tuck the capsule into the head's upper-left corner, beside the curved hairline.
      const left = Math.round(Math.max(visible.left + margin, Math.min(
        head.left - width + Math.min(24, (head.right - head.left) * 0.2),
        visible.right - margin - width,
      )));
      const top = Math.round(Math.max(visible.top + margin, Math.min(
        head.top - height + 4,
        visible.bottom - margin - height,
      )));
      const leftStyle = `${left}px`;
      const topStyle = `${top}px`;
      if (root.style.left !== leftStyle) root.style.left = leftStyle;
      if (root.style.top !== topStyle) root.style.top = topStyle;

      if (open) {
        // Align the wider usage panel to the capsule's right edge, away from the face.
        const panelLeft = Math.round(Math.max(visible.left + margin - left, Math.min(
          width - dropdown.offsetWidth,
          visible.right - margin - left - dropdown.offsetWidth,
        )));
        const panelLeftStyle = `${panelLeft}px`;
        if (dropdown.style.left !== panelLeftStyle) dropdown.style.left = panelLeftStyle;
        const above = top + height + dropdown.offsetHeight > visible.bottom - margin;
        const direction = above ? "above" : "below";
        if (dropdown.dataset.direction !== direction) {
          dropdown.dataset.direction = direction;
          onGeometryChange();
        }
      }
    },
  };
}
