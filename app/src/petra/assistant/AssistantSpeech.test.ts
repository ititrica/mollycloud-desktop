import { beforeAll,beforeEach,afterAll,describe,it,expect,vi } from 'vitest';
const mock=vi.hoisted(()=>({settings:vi.fn(),play:vi.fn(),stop:vi.fn(),emit:vi.fn()}));
vi.mock('../../speech',()=>({getSpeechSettings:mock.settings,SpeechPlayer:class{stop(){mock.stop();}play(...args:unknown[]){return mock.play(...args);}}}));
vi.mock('@tauri-apps/api/event',()=>({emitTo:mock.emit}));
vi.mock('../utils/settings',()=>({loadSettings:()=>({assistant:{enabled:true}})}));
let speech:typeof import('./AssistantSpeech');
beforeAll(async()=>{vi.stubGlobal('window',{addEventListener:vi.fn()});mock.emit.mockResolvedValue(undefined);speech=await import('./AssistantSpeech');});
beforeEach(()=>{speech.stopAssistantSpeech();mock.settings.mockReset().mockResolvedValue({config:{enabled:true}});mock.play.mockReset();mock.emit.mockClear().mockResolvedValue(undefined);});
afterAll(()=>vi.unstubAllGlobals());
const flush=async()=>{for(let i=0;i<6;i++)await Promise.resolve();};
describe('assistant text and audio start',()=>{
 it('holds text until audio starts and releases it before audio ends',async()=>{
  let started!:()=>void,finished!:()=>void;mock.play.mockImplementation((_text,_preview,_saved,onStart)=>{started=onStart;return new Promise<void>(resolve=>{finished=resolve;});});
  const reveal=vi.fn();const pending=speech.speakAssistant('reply',reveal);await flush();expect(reveal).not.toHaveBeenCalled();started();await pending;expect(reveal).toHaveBeenCalledTimes(1);finished();await flush();expect(reveal).toHaveBeenCalledTimes(1);
 });
 it('shows text immediately when speech is disabled',async()=>{mock.settings.mockResolvedValue({config:{enabled:false}});const reveal=vi.fn();await speech.speakAssistant('reply',reveal);expect(reveal).toHaveBeenCalledTimes(1);expect(mock.play).not.toHaveBeenCalled();});
 it('preserves text and reports a synthesis failure',async()=>{mock.play.mockRejectedValue(new Error('模拟合成失败'));const reveal=vi.fn();await speech.speakAssistant('reply',reveal);expect(reveal).toHaveBeenCalledTimes(1);expect(mock.emit).toHaveBeenCalledWith('console','assistant-speech-state',{phase:'error',error:'模拟合成失败'});});
 it('stopping during preparation reveals buffered text and ignores late starts',async()=>{
  let started!:()=>void;mock.play.mockImplementation((_t,_p,_s,start)=>{started=start;return new Promise(()=>{});});const reveal=vi.fn();const pending=speech.speakAssistant('reply',reveal);await flush();speech.stopAssistantSpeech();await pending;started();expect(reveal).toHaveBeenCalledTimes(1);
 });
});
