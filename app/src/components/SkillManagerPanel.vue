<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton } from "naive-ui";
const props = defineProps<{ preview: boolean }>();
const emit = defineEmits<{ modal: [open: boolean] }>();
const frame = ref<HTMLIFrameElement | null>(null);
const ready = ref(false);
const failed = ref(false);
const generation = ref(0);
const panel = ref<HTMLElement | null>(null);
const height = ref<number>();
let observer: ResizeObserver | undefined;
let timer: number | undefined;
const url = computed(() => `/skills-manager/index.html?embedded=1${props.preview ? '&ui-preview=skills' : ''}`);
function receive(event: MessageEvent) {
  if (event.source !== frame.value?.contentWindow || event.origin !== location.origin || event.data?.source !== "molly-skills") return;
  if (event.data.type === "ready") { ready.value = true; failed.value = false; window.clearTimeout(timer); }
  if (event.data.type === "modal" && typeof event.data.open === "boolean") emit("modal", event.data.open);
}
function start() { window.clearTimeout(timer); timer = window.setTimeout(() => { if (!ready.value) failed.value = true; }, 20000); }
function reload() { ready.value = false; failed.value = false; generation.value++; emit("modal", false); start(); }
function fit() {
  if (!panel.value?.getClientRects().length) return;
  const workspace = panel.value.closest<HTMLElement>(".workspace");
  const gutter = panel.value.parentElement ? parseFloat(getComputedStyle(panel.value.parentElement).paddingBottom) || 0 : 0;
  height.value = Math.max(220, (workspace?.getBoundingClientRect().bottom ?? window.innerHeight) - panel.value.getBoundingClientRect().top - (workspace?.scrollTop ?? 0) - gutter);
}
onMounted(() => {
  window.addEventListener("message", receive); window.addEventListener("resize", fit); start(); fit();
  observer = new ResizeObserver(fit);
  const workspace = panel.value?.closest(".workspace");
  if (workspace) observer.observe(workspace);
  if (panel.value?.parentElement) observer.observe(panel.value.parentElement);
});
onBeforeUnmount(() => { window.removeEventListener("message", receive); window.removeEventListener("resize", fit); observer?.disconnect(); window.clearTimeout(timer); emit("modal", false); });
</script>
<template>
  <section ref="panel" class="skills-panel" :style="height ? { height: `${height}px` } : undefined" aria-label="Skill 管理器">
    <iframe :key="generation" ref="frame" class="skills-frame" :src="url" title="Skill 管理器" @error="failed = true" />
    <div v-if="!ready" class="skills-load-state" role="status">
      <p>{{ failed ? 'Skill 管理器暂时无法加载' : '正在加载 Skill 管理器…' }}</p>
      <n-button v-if="failed" secondary @click="reload">重新加载</n-button>
    </div>
  </section>
</template>
