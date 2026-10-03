import { Channel, invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

export const defaultSpeechConfig = { enabled: false, provider: 'mimo' as const, baseUrl: 'https://api.xiaomimimo.com/v1', model: 'mimo-v2.5-tts', voice: '冰糖', volume: 0.8 };
export const speechConfigSchema = z.object({
  enabled: z.boolean(), provider: z.enum(['mimo', 'custom']), baseUrl: z.string().max(2048),
  model: z.string().trim().min(1).max(160), voice: z.string().trim().min(1).max(160), volume: z.number().finite().min(0).max(1),
}).superRefine((config, ctx) => {
  if (config.provider !== 'custom') return;
  try {
    const url = new URL(config.baseUrl);
    if (!(url.protocol === 'https:' || url.protocol === 'http:' && ['localhost','127.0.0.1','[::1]'].includes(url.hostname)) || url.username || url.password || url.search || url.hash) throw new Error();
  } catch { ctx.addIssue({ code: 'custom', message: '请输入有效的 HTTPS 语音端点（本机可用 HTTP）', path: ['baseUrl'] }); }
});
export type SpeechConfig = z.infer<typeof speechConfigSchema>;
export interface SpeechSettings { config: SpeechConfig; apiKeyConfigured: boolean }
export interface SpeechUpdate { config: SpeechConfig; apiKey?: string; clearApiKey?: boolean }
export type SpeechStatus = { phase: 'idle' | 'preparing' | 'playing' | 'error'; error?: string };
export type SpeechAudio = { type: 'start'; format: 'pcm16' | 'wav'; sampleRate: number; volume: number } | { type: 'chunk'; data: string };
export function sameSpeechEndpoint(a: SpeechConfig, b: SpeechConfig) { return a.provider === b.provider && a.baseUrl.trim().replace(/\/+$/, '') === b.baseUrl.trim().replace(/\/+$/, ''); }
export function speechText(value: string): string {
  return value.replace(/```[\s\S]*?```/g, '').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/https?:\/\/\S+/g, '').replace(/(?:^|\n)\s*#{1,6}\s*/g, '\n').replace(/[*_`~]/g, '').trim().slice(0, 4000);
}
export async function getSpeechSettings(): Promise<SpeechSettings> {
  const raw = await invoke<SpeechSettings>('get_speech_settings');
  return { config: speechConfigSchema.parse(raw.config), apiKeyConfigured: raw.apiKeyConfigured === true };
}
export async function saveSpeechSettings(update: SpeechUpdate): Promise<SpeechSettings> { return invoke('save_speech_settings', { update: { ...update, config: speechConfigSchema.parse(update.config) } }); }

/** The provider may split a 16-bit sample across network chunks. */
export class Pcm16Decoder {
  private pending: number | undefined;
  decode(bytes: Uint8Array): Float32Array<ArrayBuffer> {
    const offset = this.pending === undefined ? 0 : 1;
    const data = new Uint8Array(bytes.length + offset);
    if (offset) data[0] = this.pending!;
    data.set(bytes, offset);
    const count = Math.floor(data.length / 2);
    this.pending = data.length % 2 ? data[data.length - 1] : undefined;
    const floats = new Float32Array(count), view = new DataView(data.buffer);
    for (let i = 0; i < count; i++) floats[i] = view.getInt16(i * 2, true) / 32768;
    return floats;
  }
  complete() { if (this.pending !== undefined) throw new Error('语音 PCM 数据不完整'); }
}
export class SpeechOutput {
  private context: AudioContext;
  private gain: GainNode;
  private decoder = new Pcm16Decoder();
  private sources = new Set<AudioBufferSourceNode>();
  private nextTime = 0;
  private resolveFinished?: () => void;
  private stopped = false;
  private started = false;
  private waitingForSound = true;
  private startTimer?: ReturnType<typeof setTimeout>;
  readonly ready: Promise<void>;
  constructor(private sampleRate: number, private volume: number, factory = () => new AudioContext(), private onStart = () => {}) {
    this.context = factory(); this.gain = this.context.createGain(); this.gain.gain.value = volume; this.gain.connect(this.context.destination);
    this.ready = new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('音频播放未能启动，请点击试听或重新打开 Molly 助手')), 4000);
      void this.context.resume().then(() => { clearTimeout(timer); resolve(); }, () => { clearTimeout(timer); reject(new Error('无法启动音频播放')); });
    });
    void this.ready.catch(() => {});
  }
  pcm(bytes: Uint8Array) {
    if (this.stopped) return;
    let samples = this.decoder.decode(bytes); if (!samples.length) return;
    if (this.waitingForSound && this.volume > 0) {
      const first = samples.findIndex(sample => Math.abs(sample) > 0.001);
      if (first < 0) return;
      samples = samples.subarray(first);
    }
    this.waitingForSound = false;
    const buffer = this.context.createBuffer(1, samples.length, this.sampleRate); buffer.copyToChannel(samples, 0); this.schedule(buffer);
  }
  private schedule(buffer: AudioBuffer, offset = 0) {
    if (this.stopped) return;
    const source = this.context.createBufferSource(); source.buffer = buffer; source.connect(this.gain);
    const start = Math.max(this.context.currentTime + 0.025, this.nextTime);
    if (start - this.context.currentTime > 180) throw new Error('语音队列过长，请缩短回复');
    this.nextTime = start + buffer.duration - offset; this.sources.add(source);
    source.onended = () => { source.disconnect(); this.sources.delete(source); if (!this.sources.size) this.resolveFinished?.(); };
    if (offset > 0) source.start(start, offset); else source.start(start);
    if (!this.started && this.startTimer === undefined) {
      this.started = true;
      void this.ready.then(() => {
        if (this.stopped) return;
        this.startTimer = setTimeout(() => { if (!this.stopped) this.onStart(); }, Math.max(0, Math.ceil((start - this.context.currentTime) * 1000)));
      }).catch(() => {});
    }
  }
  async wav(bytes: Uint8Array) {
    await this.ready;
    let buffer: AudioBuffer;
    try { buffer = await this.context.decodeAudioData(bytes.slice().buffer); }
    catch { throw new Error('自定义接口的音频无法播放，请确认支持 WAV 输出'); }
    let offset = 0;
    if (this.waitingForSound && this.volume > 0) {
      const channels = Array.from({ length: buffer.numberOfChannels }, (_, index) => buffer.getChannelData(index));
      let first = -1;
      for (let frame = 0; frame < buffer.length; frame++) { if (channels.some(channel => Math.abs(channel[frame] ?? 0) > 0.001)) { first = frame; break; } }
      if (first < 0) throw new Error('语音服务返回静音音频，请检查音色或接口');
      offset = first / buffer.sampleRate;
    }
    this.waitingForSound = false;
    this.schedule(buffer, offset);
  }
  async finish(pcm: boolean) {
    if (pcm) { this.decoder.complete(); if (this.waitingForSound && this.volume > 0) throw new Error('语音服务返回静音音频，请检查音色或接口'); } await this.ready;
    if (!this.stopped && this.sources.size) await new Promise<void>(resolve => { this.resolveFinished = resolve; });
  }
  stop() {
    if (this.stopped) return; this.stopped = true; clearTimeout(this.startTimer);
    for (const source of this.sources) { source.onended = null; source.stop(); source.disconnect(); }
    this.sources.clear(); this.resolveFinished?.(); this.gain.disconnect(); void this.context.close();
  }
}

export class SpeechPlayer {
  private active?: { id: string; output?: SpeechOutput; chunks: Uint8Array[]; bytes: number; format?: 'pcm16' | 'wav'; failed?: Error };
  constructor(private status: (status: SpeechStatus) => void = () => {}) {}
  stop() {
    const current = this.active; this.active = undefined;
    current?.output?.stop();
    if (current) void invoke('cancel_speech', { requestId: current.id }).catch(() => {});
    this.status({ phase: 'idle' });
  }
  async play(value: string, preview?: SpeechUpdate, useSavedKey = false, onStart?: () => void): Promise<void> {
    this.stop(); const text = speechText(value); if (!text) return;
    const current: NonNullable<SpeechPlayer['active']> = { id: crypto.randomUUID(), chunks: [], bytes: 0 };
    this.active = current; this.status({ phase: 'preparing' });
    const channel = new Channel<SpeechAudio>();
    channel.onmessage = message => {
      if (this.active !== current || current.failed) return;
      try {
        if (message.type === 'start') {
          if (current.output || !['pcm16','wav'].includes(message.format) || message.sampleRate !== 24000) throw new Error('语音流格式无效');
          current.format = message.format; current.output = new SpeechOutput(message.sampleRate, message.volume, undefined, () => {
            if (this.active !== current || current.failed) return;
            this.status({ phase: 'playing' }); onStart?.();
          });
        } else {
          if (!current.output) throw new Error('语音流缺少格式信息');
          const data = Uint8Array.from(atob(message.data), ch => ch.charCodeAt(0)); current.bytes += data.length;
          if (current.bytes > 24 * 1024 * 1024) throw new Error('音频长度超出限制');
          if (current.format === 'pcm16') { current.output.pcm(data); }
          else current.chunks.push(data);
        }
      } catch (reason) {
        current.failed = reason instanceof Error ? reason : new Error('语音音频无法解码'); current.output?.stop();
        void invoke('cancel_speech', { requestId: current.id }).catch(() => {});
      }
    };
    let failed = false;
    try {
      const completed = await invoke<boolean>('synthesize_speech', { requestId: current.id, text, onAudio: channel, preview: preview ?? null, useSavedKey });
      if (this.active !== current) return;
      if (current.failed) throw current.failed;
      if (!completed) return;
      if (!current.output || !current.bytes) throw new Error('语音服务未返回音频');
      if (current.format === 'wav') {
        const bytes = new Uint8Array(current.bytes); let offset = 0;
        for (const chunk of current.chunks) { bytes.set(chunk, offset); offset += chunk.length; }
        current.chunks = []; await current.output.wav(bytes); if (this.active !== current) return;
      }
      await current.output.finish(current.format === 'pcm16');
    } catch (reason) {
      if (this.active === current) { failed = true; this.status({ phase: 'error', error: reason instanceof Error ? reason.message : String(reason) }); throw reason; }
    } finally {
      current.output?.stop();
      if (this.active === current) { this.active = undefined; if (!failed) this.status({ phase: 'idle' }); }
    }
  }
}
