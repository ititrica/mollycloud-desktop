<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import PetraAssistant from "./components/PetraAssistant.vue";
import type { AssistantConfig } from "./contracts";
import { createMollyRenderer, type MollyRenderer, type MotionGroup } from "./live2d/renderer";
import { PetraBehaviorEngine } from "./petra/PetraBehaviorEngine";
import { desktopApi } from "./ipc";

const canvas = ref<HTMLCanvasElement | null>(null);
const state = ref<"loading" | "ready" | "error">("loading");
const errorMessage = ref("");
const bubbleMessage = ref("");
const assistantOpen = ref(false);
const interactive = ref(false);
let renderer: MollyRenderer | undefined;
let behavior: PetraBehaviorEngine | undefined;
let unlistenMotion: (() => void) | undefined;
let unlistenBubble: (() => void) | undefined;
let unlistenAssistantOpen: (() => void) | undefined;
let unlistenInteractive: (() => void) | undefined;
let unlistenAssistantConfig: (() => void) | undefined;
let bubbleTimer: number | undefined;
let greetingTimer: number | undefined;
let greetingRevision = 0;

function showBubble(message: string): void {
  bubbleMessage.value = message.slice(0, 180);
  if (bubbleTimer) window.clearTimeout(bubbleTimer);
  bubbleTimer = window.setTimeout(() => { bubbleMessage.value = ""; }, 9_000);
}

function scheduleGreeting(config: AssistantConfig): void {
  const revision = ++greetingRevision;
  if (greetingTimer) window.clearTimeout(greetingTimer);
  greetingTimer = undefined;
  if (!config.enabled || !config.model || !("__TAURI_INTERNALS__" in window)) return;
  greetingTimer = window.setTimeout(async () => {
    if (!assistantOpen.value) {
      try {
        const reply = await desktopApi.assistantChat([{
          role: "user",
          content: "请根据你的人设主动向用户打一个自然、简短、不超过30个汉字的招呼；不要提及这是自动任务。",
        }]);
        showBubble(reply.content);
        await behavior?.playRequested("Nod");
      } catch {
        // 主动问候失败时保持安静，避免打扰用户。
      }
    }
    if (revision === greetingRevision) scheduleGreeting(config);
  }, Math.max(5, Math.min(120, config.greet_interval)) * 60_000);
}

onMounted(async () => {
  try {
    if (!canvas.value) throw new Error("模型画布创建失败");
    renderer = await createMollyRenderer(canvas.value);
    behavior = new PetraBehaviorEngine(renderer, "normal");
    behavior.start();
    state.value = "ready";

    if (import.meta.env.DEV && new URLSearchParams(window.location.search).get("ui-preview")?.startsWith("assistant")) {
      interactive.value = true;
      assistantOpen.value = true;
    }

    if ("__TAURI_INTERNALS__" in window) {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenMotion = await listen<MotionGroup>("live2d-motion", (event) => {
        void behavior?.playRequested(event.payload);
      });
      unlistenBubble = await listen<string>("petra-bubble", (event) => {
        showBubble(event.payload);
      });
      unlistenAssistantOpen = await listen("petra-assistant-open", () => {
        interactive.value = true;
        assistantOpen.value = true;
      });
      unlistenInteractive = await listen<boolean>("overlay-interactive-changed", (event) => {
        interactive.value = event.payload;
        if (!event.payload) assistantOpen.value = false;
      });
      unlistenAssistantConfig = await listen<AssistantConfig>("assistant-config-changed", (event) => {
        scheduleGreeting(event.payload);
      });
      scheduleGreeting(await desktopApi.getAssistantConfig());
    }
  } catch (reason) {
    state.value = "error";
    errorMessage.value = reason instanceof Error ? reason.message : String(reason);
  }
});

async function startDragging(event: PointerEvent): Promise<void> {
  if (event.button !== 0 || !("__TAURI_INTERNALS__" in window)) return;
  behavior?.suspend(4_000);
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().startDragging();
}

function trackPointer(event: PointerEvent): void {
  renderer?.focus(event.clientX, event.clientY);
}

async function closeAssistant(): Promise<void> {
  assistantOpen.value = false;
  interactive.value = false;
  await desktopApi.setOverlayInteractive(false).catch(() => undefined);
}

onBeforeUnmount(() => {
  unlistenMotion?.();
  unlistenBubble?.();
  unlistenAssistantOpen?.();
  unlistenInteractive?.();
  unlistenAssistantConfig?.();
  if (bubbleTimer) window.clearTimeout(bubbleTimer);
  if (greetingTimer) window.clearTimeout(greetingTimer);
  behavior?.stop();
  renderer?.destroy();
});
</script>

<template>
  <main class="overlay-shell" @pointerdown="startDragging" @pointermove="trackPointer">
    <canvas ref="canvas" aria-label="Molly Live2D 桌宠" />
    <button v-if="interactive && !assistantOpen" class="petra-launcher" type="button" aria-label="打开 Molly 助手" @pointerdown.stop @click.stop="assistantOpen = true">✦</button>
    <PetraAssistant v-if="assistantOpen" @pointerdown.stop @close="closeAssistant" />
    <div v-if="bubbleMessage" class="petra-bubble">{{ bubbleMessage }}</div>
    <div v-if="state === 'loading'" class="overlay-status">正在唤醒 Molly…</div>
    <div v-else-if="state === 'error'" class="overlay-error" role="alert">{{ errorMessage }}</div>
  </main>
</template>

<style>
:root, html, body, #overlay { width: 100%; height: 100%; margin: 0; overflow: hidden; background: transparent !important; }
* { box-sizing: border-box; }
.overlay-body { min-width: 0; min-height: 0; user-select: none; }
.overlay-shell { position: relative; width: 100%; height: 100%; overflow: hidden; background: transparent; }
.overlay-shell canvas { position: absolute; inset: 0; width: 100%; height: 100%; background: transparent; }
.petra-launcher { position: absolute; z-index: 3; right: 48px; top: 82px; width: 46px; height: 46px; border: 1px solid rgba(20, 20, 20, .16); border-radius: 15px 15px 15px 5px; color: #fff; background: #4c49ff; box-shadow: 0 12px 30px rgba(76, 73, 255, .26); font-size: 20px; }
.petra-launcher:hover { transform: translateY(-2px); background: #151515; }
.petra-bubble { position: absolute; z-index: 2; left: 50%; top: 24px; width: min(330px, calc(100% - 50px)); transform: translateX(-50%); padding: 13px 16px; border: 1px solid rgba(200, 255, 22, 0.32); border-radius: 16px 16px 16px 5px; color: #f4f7ff; background: rgba(11, 13, 18, 0.88); box-shadow: 0 18px 42px rgba(0, 0, 0, 0.32); font: 13px/1.55 "Segoe UI", "Microsoft YaHei UI", sans-serif; backdrop-filter: blur(14px); animation: bubble-in 220ms ease-out; }
.overlay-status,
.overlay-error { position: absolute; left: 50%; bottom: 20px; transform: translateX(-50%); padding: 8px 12px; border-radius: 10px; color: #f4f7ff; background: rgba(11, 13, 18, 0.76); font: 12px/1.4 "Segoe UI", sans-serif; backdrop-filter: blur(10px); white-space: nowrap; }
.overlay-error { max-width: calc(100% - 40px); color: #ff9aab; white-space: normal; }
@keyframes bubble-in { from { opacity: 0; transform: translate(-50%, 8px) scale(0.97); } to { opacity: 1; transform: translate(-50%, 0) scale(1); } }
@media (prefers-reduced-motion: reduce) { .petra-bubble { animation: none; } }
</style>
