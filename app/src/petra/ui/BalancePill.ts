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

export function formatCompactOverlayBalance(value: unknown): string | null {
  const number = numericValue(value);
  if (number == null) return null;
  const whole = Math.floor(number);
  return `${Math.min(99, whole)}$`;
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
  const fullValue = document.getElementById("balance-pill-full-value")!;
  const dropdown = document.getElementById("balance-pill-usage")!;
  const tokens = document.getElementById("balance-pill-tokens")!;
  let hasBalance = false;
  let lowBalance = false;
  let positioned = false;
  let open = false;
  let requestId = 0;
  let closeTimer: number | null = null;

  function setOpen(next: boolean) {
    next &&= hasBalance && positioned && !lowBalance;
    if (next === open) return;
    open = next;
    trigger.setAttribute("aria-expanded", String(open));
    if (closeTimer != null) {
      window.clearTimeout(closeTimer);
      closeTimer = null;
    }
    if (open) {
      dropdown.hidden = false;
      void dropdown.offsetWidth;
      root.classList.add("balance-pill--open");
    } else {
      root.classList.remove("balance-pill--open");
      closeTimer = window.setTimeout(() => {
        closeTimer = null;
        if (!open) dropdown.hidden = true;
        onGeometryChange();
      }, 240);
    }
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
        const numericBalance = numericValue(account.balance);
        const balance = formatOverlayBalance(account.balance);
        const compactBalance = formatCompactOverlayBalance(account.balance);
        hasBalance = balance != null;
        lowBalance = numericBalance != null && numericBalance < 1;
        root.classList.toggle("balance-pill--low", lowBalance);
        value.textContent = compactBalance ?? "--";
        fullValue.textContent = balance ?? "--";
        tokens.textContent = formatTodayTokens(account.today_tokens);
        trigger.setAttribute("aria-label", lowBalance
          ? "余额不足"
          : `账户余额 ${balance ?? "暂无法获取"}，悬停查看今日消耗`);
        if (lowBalance) setOpen(false);
      } catch {
        if (request !== requestId) return;
        hasBalance = false;
        lowBalance = false;
        root.classList.remove("balance-pill--low");
        value.textContent = "--";
        fullValue.textContent = "--";
        tokens.textContent = "暂无法获取今日用量";
        trigger.setAttribute("aria-label", "暂无法获取余额");
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
      const headWidth = head.right - head.left;
      const headHeight = head.bottom - head.top;
      // Keep the orb just left of the hairline, at the upper-left side of the head.
      const left = Math.round(Math.max(visible.left + margin, Math.min(
        head.left - width - Math.max(12, Math.min(20, headWidth * 0.1)),
        visible.right - margin - width,
      )));
      const top = Math.round(Math.max(visible.top + margin, Math.min(
        head.top + Math.max(0, Math.min(6, headHeight * 0.04)),
        visible.bottom - margin - height,
      )));
      const leftStyle = `${left}px`;
      const topStyle = `${top}px`;
      if (root.style.left !== leftStyle) root.style.left = leftStyle;
      if (root.style.top !== topStyle) root.style.top = topStyle;

      if (open) {
        // The normal expansion keeps the orb's top-right corner fixed and grows down-left.
        const panelLeft = Math.round(Math.max(visible.left + margin - left, Math.min(
          width - dropdown.offsetWidth,
          visible.right - margin - left - dropdown.offsetWidth,
        )));
        const panelLeftStyle = `${panelLeft}px`;
        if (dropdown.style.left !== panelLeftStyle) dropdown.style.left = panelLeftStyle;
        const above = top + dropdown.offsetHeight > visible.bottom - margin;
        const panelTop = Math.round(Math.max(
          visible.top + margin - top,
          Math.min(above ? height - dropdown.offsetHeight : 0, visible.bottom - margin - top - dropdown.offsetHeight),
        ));
        const panelTopStyle = `${panelTop}px`;
        if (dropdown.style.top !== panelTopStyle) dropdown.style.top = panelTopStyle;
        const direction = above ? "above" : "below";
        if (dropdown.dataset.direction !== direction) {
          dropdown.dataset.direction = direction;
          onGeometryChange();
        }
      }
    },
  };
}
