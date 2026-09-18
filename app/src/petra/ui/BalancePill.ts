import type { LogicalRect } from "../input/regions";
import { createCodexActivity, type ActivitySnapshot } from "./CodexActivity";
import { registerFloatingWindowReader } from "../assistant/FloatingWindowInfo";

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
  return `${whole}$`;
}

export function formatTodayTokens(value: unknown): string {
  const number = numericValue(value);
  return number == null || number < 0
    ? "暂无法获取今日用量"
    : `${Math.trunc(number).toLocaleString("en-US")} token`;
}

/** A small account popover positioned in window logical pixels, next to the head. */
export function createBalancePill(onGeometryChange: () => void) {
  const root = document.getElementById("balance-pill")!;
  const trigger = document.getElementById("balance-pill-trigger")!;
  const value = document.getElementById("balance-pill-value")!;
  const fullValue = document.getElementById("balance-pill-full-value")!;
  const dropdown = document.getElementById("balance-pill-usage")!;
  const tokens = document.getElementById("balance-pill-tokens")!;
  // Keep distance per second constant when task text changes the perimeter.
  // The path is centered on the trigger's 1px border, for either silhouette.
  const borderSizeObserver = new ResizeObserver(([entry]) => {
    if (!entry) return;
    const box = entry.borderBoxSize[0];
    const width = box?.inlineSize ?? trigger.offsetWidth;
    const height = box?.blockSize ?? trigger.offsetHeight;
    if (width <= 1 || height <= 1) return;
    const diameter = Math.min(width, height) - 1;
    const perimeter = 2 * Math.abs(width - height) + Math.PI * diameter;
    const pixelsPerSecond = root.classList.contains("balance-pill--capsule") ? 136 : 24.5;
    trigger.style.setProperty("--border-light-duration", `${(perimeter / pixelsPerSecond).toFixed(5)}s`);
  });
  borderSizeObserver.observe(trigger);
  let hasBalance = false;
  let lowBalance = false;
  let positioned = false;
  let open = false;
  let closeTimer: number | null = null;
  let previousMode: boolean | null = null;
  const activity = createCodexActivity(root, dropdown, () => {
    syncVisibility();
    onGeometryChange();
  });
  registerFloatingWindowReader(() => ({
    mode: activity.isActive ? "capsule" : "orb",
    balance: { compact: value.textContent ?? "--", full: fullValue.textContent ?? "--", low: lowBalance },
    todayTokens: tokens.textContent ?? "暂无法获取今日用量",
    activity: activity.info,
  }));

  function setOpen(next: boolean) {
    next &&= positioned && !activity.isActive && (activity.hasTasks || (hasBalance && !lowBalance));
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
    const hidden = (!hasBalance && !activity.hasTasks) || !positioned;
    root.classList.toggle("hidden", hidden);
    if (!hasBalance) {
      const label = activity.hasTasks ? "AI" : "--";
      if (value.textContent !== label) value.textContent = label;
    }
    trigger.setAttribute("aria-label", `${lowBalance ? "余额不足" : `账户余额 ${hasBalance ? fullValue.textContent : "暂无法获取"}`}${activity.description}${activity.isActive ? "" : "，展开查看详情"}`);
    if (activity.isActive) trigger.removeAttribute("aria-controls");
    else trigger.setAttribute("aria-controls", dropdown.id);
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
    updateActivity(snapshot: ActivitySnapshot | null) {
      activity.update(snapshot);
      if (activity.isActive || (lowBalance && !activity.hasTasks)) setOpen(false);
      if (activity.isActive) dropdown.hidden = true;
      syncVisibility();
      onGeometryChange();
    },
    update(account: OverlayAccount) {
      const numericBalance = numericValue(account.balance);
      const balance = formatOverlayBalance(account.balance);
      const compactBalance = formatCompactOverlayBalance(account.balance);
      hasBalance = balance != null;
      lowBalance = numericBalance != null && numericBalance < 1;
      root.classList.toggle("balance-pill--low", lowBalance);
      value.textContent = compactBalance ?? "--";
      value.style.fontSize = compactBalance && compactBalance.length > 4
        ? `${Math.max(7, 48 / compactBalance.length)}px` : "";
      fullValue.textContent = balance ?? "--";
      tokens.textContent = formatTodayTokens(account.today_tokens);
      if (lowBalance && !activity.hasTasks) setOpen(false);
      syncVisibility();
      onGeometryChange();
    },
    position(head: LogicalRect | null, visible: LogicalRect) {
      positioned = head != null;
      syncVisibility();
      if (!head || (!hasBalance && !activity.hasTasks)) return;

      const margin = 8;
      const width = trigger.offsetWidth;
      const height = trigger.offsetHeight;
      const headWidth = head.right - head.left;
      const headHeight = head.bottom - head.top;
      // Running tasks center the capsule above the head; idle keeps the original orb anchor.
      const targetLeft = activity.isActive ? (head.left + head.right - width) / 2
        : head.left - width - Math.max(12, Math.min(20, headWidth * 0.1));
      const targetTop = activity.isActive ? head.top - height - 14
        : head.top + Math.max(0, Math.min(6, headHeight * 0.04));
      const left = Math.round(Math.max(visible.left + margin, Math.min(
        targetLeft,
        visible.right - margin - width,
      )));
      const top = Math.round(Math.max(visible.top + margin, Math.min(
        targetTop,
        visible.bottom - margin - height,
      )));
      const leftStyle = `${left}px`;
      const topStyle = `${top}px`;
      if (previousMode !== null && previousMode !== activity.isActive && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        const from = root.getBoundingClientRect();
        root.getAnimations().forEach(animation => animation.cancel());
        root.animate([
          { transform: `translate(${from.left - left}px, ${from.top - top}px)` },
          { transform: "translate(0, 0)" },
        ], { duration: 220, easing: "cubic-bezier(0.2, 0.75, 0.25, 1)" });
      }
      previousMode = activity.isActive;
      if (root.style.left !== leftStyle) root.style.left = leftStyle;
      if (root.style.top !== topStyle) root.style.top = topStyle;

      if (open) {
        const gap = activity.isActive ? 8 : 0;
        const belowSpace = visible.bottom - margin - top - height - gap;
        const aboveSpace = top - visible.top - margin - gap;
        dropdown.style.maxHeight = activity.isActive ? `${Math.max(80, Math.floor(Math.max(belowSpace, aboveSpace)))}px` : "";
        // offsetHeight rounds fractional CSS pixels down on some display scales.
        const panelHeight = Math.ceil(parseFloat(getComputedStyle(dropdown).height) || dropdown.offsetHeight);
        // The normal expansion keeps the orb's top-right corner fixed and grows down-left.
        const panelLeft = Math.round(Math.max(visible.left + margin - left, Math.min(
          activity.isActive ? (width - dropdown.offsetWidth) / 2 : width - dropdown.offsetWidth,
          visible.right - margin - left - dropdown.offsetWidth,
        )));
        const panelLeftStyle = `${panelLeft}px`;
        if (dropdown.style.left !== panelLeftStyle) dropdown.style.left = panelLeftStyle;
        const above = activity.isActive ? belowSpace < panelHeight && aboveSpace > belowSpace
          : top + panelHeight > visible.bottom - margin;
        const panelTop = Math.round(Math.max(
          visible.top + margin - top,
          Math.min(activity.isActive ? (above ? -panelHeight - gap : height + gap)
            : (above ? height - panelHeight : 0), visible.bottom - margin - top - panelHeight),
        ));
        const panelTopStyle = `${panelTop}px`;
        if (dropdown.style.top !== panelTopStyle) dropdown.style.top = panelTopStyle;
        const direction = above ? "above" : "below";
        if (dropdown.dataset.direction !== direction) {
          dropdown.dataset.direction = direction;
          root.dataset.dropdownDirection = direction;
          onGeometryChange();
        }
      }
    },
  };
}
