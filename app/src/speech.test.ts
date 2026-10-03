import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { defaultSpeechConfig, getSpeechSettings, Pcm16Decoder, sameSpeechEndpoint, SpeechOutput, SpeechPlayer, speechConfigSchema } from './speech';
import { petDraftSchema } from './petSettings';
const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: ipc.invoke, Channel: class { onmessage?: (value: unknown) => void } }));
vi.mock('@tauri-apps/api/event', () => ({ emitTo: vi.fn(), listen: vi.fn() }));
class TestContext {
  static created: TestContext[] = [];
  currentTime = 1; destination = {}; closed = false;
  sources: Array<{ onended: (() => void) | null; start: ReturnType<typeof vi.fn>; stop: ReturnType<typeof vi.fn>; disconnect: ReturnType<typeof vi.fn>; buffer?: { duration: number } }> = [];
  constructor() { TestContext.created.push(this); }
  createGain() { return { gain: { value: 0 }, connect() {}, disconnect() {} }; }
  resume() { return Promise.resolve(); }
  close() { this.closed = true; return Promise.resolve(); }
  createBuffer(_channels: number, frames: number, rate: number) { return { duration: frames / rate, copyToChannel() {} }; }
  createBufferSource() { const source = { onended: null, connect() {}, disconnect: vi.fn(), start: vi.fn(), stop: vi.fn(), buffer: undefined }; this.sources.push(source); return source; }
  decodeAudioData() { return Promise.resolve({ duration: 0.1, length: 2400, sampleRate: 24000, numberOfChannels: 1, getChannelData: () => new Float32Array(2400).fill(0.08) }); }
}
beforeEach(() => { ipc.invoke.mockReset(); TestContext.created = []; vi.stubGlobal('AudioContext', TestContext); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('speech settings and playback', () => {
  it('migrates existing pet drafts without enabling speech or requiring a key', () => {
    const draft = petDraftSchema.parse({ model: { type: 'manifest', name: 'test' }, modelScale: 1, boundsPadding: {left:0,right:0,top:0,bottom:0}, debugBorder:false,debugModelBounds:false,params:{},auto:{},assistant:{enabled:true,provider:'mollycloud',model:'',persona:'',customBaseUrl:'',greetInterval:20} });
    expect(draft.speech).toEqual(defaultSpeechConfig);
    expect(draft.speech.enabled).toBe(false);
  });
  it('validates custom endpoints and recognizes a changed service or address', () => {
    for (const baseUrl of ['http://example.com/v1','https://user:pass@example.com','https://example.com?key=test']) expect(speechConfigSchema.safeParse({...defaultSpeechConfig,provider:'custom',baseUrl}).success).toBe(false);
    expect(speechConfigSchema.safeParse({...defaultSpeechConfig,provider:'custom',baseUrl:'http://localhost:8080/v1'}).success).toBe(true);
    expect(sameSpeechEndpoint(defaultSpeechConfig,{...defaultSpeechConfig,baseUrl:defaultSpeechConfig.baseUrl+'/'})).toBe(true);
    expect(sameSpeechEndpoint(defaultSpeechConfig,{...defaultSpeechConfig,provider:'custom'})).toBe(false);
  });
  it('only exposes whether a key is configured, without decoding any key in the renderer', async () => {
    ipc.invoke.mockResolvedValue({config:defaultSpeechConfig,apiKeyConfigured:true});
    expect(await getSpeechSettings()).toEqual({config:defaultSpeechConfig,apiKeyConfigured:true});
    expect(ipc.invoke).toHaveBeenCalledWith('get_speech_settings');
  });
  it('decodes PCM little endian across arbitrary byte boundaries', () => {
    const pcm = new Pcm16Decoder();
    expect([...pcm.decode(new Uint8Array([0]))]).toEqual([]);
    expect([...pcm.decode(new Uint8Array([128,255,127,0]))]).toEqual([-1,32767/32768]);
    expect([...pcm.decode(new Uint8Array([0]))]).toEqual([0]);
    expect(() => pcm.complete()).not.toThrow();
    pcm.decode(new Uint8Array([0])); expect(() => pcm.complete()).toThrow('不完整');
  });
  it('schedules continuous PCM and frees all sources when interrupted', async () => {
    const output = new SpeechOutput(24000,0.8);
    output.pcm(new Uint8Array(4800).fill(8)); output.pcm(new Uint8Array(4800).fill(8));
    const ctx = TestContext.created[0]!;
    expect(ctx.sources[0]!.start).toHaveBeenCalledWith(1.025);
    expect(ctx.sources[1]!.start.mock.calls[0]![0]).toBeCloseTo(1.125);
    const finished = output.finish(true); output.stop(); await finished;
    expect(ctx.closed).toBe(true); expect(ctx.sources.every(s=>s.stop.mock.calls.length===1)).toBe(true);
  });
  it('starts streaming playback before the synthesis request has finished', async () => {
    vi.useFakeTimers();
    let end!: (value: boolean) => void;
    const states: string[] = [];
    ipc.invoke.mockImplementation((command, args) => {
      if(command==='cancel_speech')return Promise.resolve();
      args.onAudio.onmessage({type:'start',format:'pcm16',sampleRate:24000,volume:0.8});
      args.onAudio.onmessage({type:'chunk',data:btoa(String.fromCharCode(0,8,0,8))});
      return new Promise(resolve=>{end=resolve;});
    });
    const player = new SpeechPlayer(s=>states.push(s.phase)); const playing = player.play('你好');
    expect(states[states.length - 1]).toBe('preparing');
    await vi.advanceTimersByTimeAsync(26);
    expect(states[states.length - 1]).toBe('playing'); expect(TestContext.created[0]!.sources).toHaveLength(1);
    TestContext.created[0]!.sources[0]!.onended?.(); end(true); await playing;
    expect(states[states.length - 1]).toBe('idle'); expect(TestContext.created[0]!.closed).toBe(true);
  });
  it('signals text reveal once at first buffer playback, and suppresses cancelled starts', async () => {
    vi.useFakeTimers();const started=vi.fn();
    const output=new SpeechOutput(24000,0.8,undefined,started);output.pcm(new Uint8Array(4800).fill(8));
    await vi.advanceTimersByTimeAsync(24);expect(started).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(2);expect(started).toHaveBeenCalledTimes(1);
    output.pcm(new Uint8Array(4800).fill(8));await vi.advanceTimersByTimeAsync(150);expect(started).toHaveBeenCalledTimes(1);output.stop();
    const late=vi.fn();const cancelled=new SpeechOutput(24000,0.8,undefined,late);cancelled.pcm(new Uint8Array(4800).fill(8));cancelled.stop();await vi.advanceTimersByTimeAsync(50);expect(late).not.toHaveBeenCalled();
  });
  it('does not reveal text during a provider leading-silence chunk', async () => {
    vi.useFakeTimers();const started=vi.fn();const output=new SpeechOutput(24000,0.8,undefined,started);
    output.pcm(new Uint8Array(4800));await vi.advanceTimersByTimeAsync(50);expect(started).not.toHaveBeenCalled();expect(TestContext.created[0]!.sources).toHaveLength(0);
    output.pcm(new Uint8Array(4800).fill(8));await vi.advanceTimersByTimeAsync(26);expect(started).toHaveBeenCalledTimes(1);output.stop();
  });
  it('buffers custom WAV until complete rather than decoding each network fragment', async () => {
    let end!: (value: boolean) => void;
    ipc.invoke.mockImplementation((_command,args) => {
      args.onAudio.onmessage({type:'start',format:'wav',sampleRate:24000,volume:0.8});
      args.onAudio.onmessage({type:'chunk',data:btoa('RIFF')});
      args.onAudio.onmessage({type:'chunk',data:btoa('mock-wave')});
      return new Promise(resolve=>{end=resolve;});
    });
    const playing = new SpeechPlayer().play('hello');
    expect(TestContext.created[0]!.sources).toHaveLength(0);
    end(true);
    for (let i=0;i<10&&!TestContext.created[0]!.sources.length;i++) await Promise.resolve();
    expect(TestContext.created[0]!.sources).toHaveLength(1);
    TestContext.created[0]!.sources[0]!.onended?.(); await playing;
    expect(TestContext.created[0]!.closed).toBe(true);
  });
  it('ignores late audio from a cancelled request and never cancels its replacement', async () => {
    const pending: Array<{args:any;resolve:(value:boolean)=>void}> = [];
    ipc.invoke.mockImplementation((command,args)=>command==='cancel_speech'?Promise.resolve():new Promise(resolve=>pending.push({args,resolve})));
    const player = new SpeechPlayer(); const first=player.play('first'); const second=player.play('second');
    pending[0]!.args.onAudio.onmessage({type:'start',format:'pcm16',sampleRate:24000,volume:0.8});
    expect(TestContext.created).toHaveLength(0);
    pending[0]!.resolve(true);pending[1]!.resolve(false);await Promise.all([first,second]);
    expect(ipc.invoke).toHaveBeenCalledWith('cancel_speech',{requestId:pending[0]!.args.requestId});
    expect(pending[0]!.args.requestId).not.toBe(pending[1]!.args.requestId);
  });
  it('keeps synthesis failures visible without retrying a charged request', async () => {
    const states: string[]=[];ipc.invoke.mockRejectedValue(new Error('模拟超时'));
    await expect(new SpeechPlayer(s=>states.push(s.phase)).play('你好')).rejects.toThrow('超时');
    expect(ipc.invoke).toHaveBeenCalledTimes(1);expect(states[states.length - 1]).toBe('error');
  });
});
