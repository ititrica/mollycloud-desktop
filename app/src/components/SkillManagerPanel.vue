<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton } from "naive-ui";
const props = defineProps<{ preview: boolean }>();
const emit = defineEmits<{ modal: [open: boolean] }>();
const frame = ref<HTMLIFrameElement | null>(null);
const ready = ref(false);
const failed = ref(false);
const generation = ref(0);
let timer: number | undefined;
const url = computed(() => `${props.preview ? "/skills-manager" : "/molly-plugins/skills"}/index.html?embedded=1${props.preview ? '&ui-preview=skills' : ''}`);
function receive(event: MessageEvent) {
  if (event.source !== frame.value?.contentWindow || event.origin !== location.origin || event.data?.source !== "molly-skills") return;
  if (event.data.type === "ready") { ready.value = true; failed.value = false; window.clearTimeout(timer); }
  if (event.data.type === "modal" && typeof event.data.open === "boolean") emit("modal", event.data.open);
}
function start() { window.clearTimeout(timer); timer = window.setTimeout(() => { if (!ready.value) failed.value = true; }, 20000); }
function reload() { ready.value = false; failed.value = false; generation.value++; emit("modal", false); start(); }
onMounted(() => {
  window.addEventListener("message", receive); start();
});
onBeforeUnmount(() => { window.removeEventListener("message", receive); window.clearTimeout(timer); emit("modal", false); });
</script>
<template>
  <section class="skills-panel" aria-label="Skill 管理器">
    <iframe :key="generation" ref="frame" class="skills-frame" :src="url" title="Skill 管理器" @error="failed = true" />
    <div v-if="!ready" class="skills-load-state" role="status">
      <p>{{ failed ? 'Skill 管理器暂时无法加载' : '正在加载 Skill 管理器…' }}</p>
      <n-button v-if="failed" secondary @click="reload">重新加载</n-button>
    </div>
  </section>
</template>
