import { emitTo } from "@tauri-apps/api/event";
import { getSpeechSettings, SpeechPlayer, type SpeechStatus } from "../../speech";
import { loadSettings } from "../utils/settings";

let generation = 0;
let revealPending: (() => void) | undefined;
const player = new SpeechPlayer(status => {
  void emitTo("console", "assistant-speech-state", status).catch(() => {});
});
export function stopAssistantSpeech() {
  ++generation;
  // Cancelling speech must never discard an already generated reply.
  revealPending?.(); revealPending = undefined; player.stop();
}
/** Resolves at the first audible buffer start; remaining speech continues in background. */
export async function speakAssistant(text: string, showText: () => void = () => {}) {
  stopAssistantSpeech(); const current = generation;
  let displayed = false, resolveVisible!: () => void;
  const visible = new Promise<void>(resolve => { resolveVisible = resolve; });
  const reveal = () => {
    if (displayed) return; displayed = true;
    if (revealPending === reveal) revealPending = undefined;
    try { showText(); } finally { resolveVisible(); }
  };
  revealPending = reveal;
  void (async () => {
    try {
      const settings = await getSpeechSettings();
      if (current !== generation) return;
      if (!loadSettings().assistant.enabled || !settings.config.enabled) { reveal(); return; }
      await player.play(text, undefined, false, reveal);
      if (current === generation) reveal();
    } catch (reason) {
      if (current === generation) {
        const status: SpeechStatus = { phase: "error", error: reason instanceof Error ? reason.message : String(reason) };
        void emitTo("console", "assistant-speech-state", status).catch(() => {});
        reveal();
      }
    }
  })();
  await visible;
}
window.addEventListener("pagehide", stopAssistantSpeech);
