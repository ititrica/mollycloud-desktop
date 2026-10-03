import { beforeEach,afterEach,describe,it,expect,vi } from 'vitest';
import { forwardImageRequest } from './imageWorkbench';
const native=vi.hoisted(()=>({invoke:vi.fn(),channels:[] as Array<{onmessage:(data:any)=>void}>}));
vi.mock('@tauri-apps/api/core',()=>({invoke:native.invoke,Channel:class{onmessage:(data:any)=>void=()=>{};constructor(){native.channels.push(this);}}}));
const request={url:'https://images.example.test/v1/images/generations',method:'POST',headers:{},bytes:null};
const base64=(bytes:Uint8Array)=>btoa(Array.from(bytes,byte=>String.fromCharCode(byte)).join(''));
const tick=async()=>{for(let i=0;i<5;i++)await Promise.resolve();};
beforeEach(()=>{native.invoke.mockReset();native.channels.length=0;vi.stubGlobal('window',{__TAURI_INTERNALS__:{}});});
afterEach(()=>vi.unstubAllGlobals());
describe('native image response handoff',()=>{
 it('keeps a multi-megabyte SSE body open after command completion until all channel bytes arrive',async()=>{
  const json=JSON.stringify({b64_json:'A'.repeat(4*1024*1024),type:'image_generation.completed'});
  const response=new TextEncoder().encode('data: '+json+'\n\ndata: [DONE]\n\n');
  const parts:Uint8Array[]=[];let closed=false,done=false;
  native.invoke.mockImplementation(async(command,args)=>{if(command==='image_request'){args.channel.onmessage({type:'headers',status:200,headers:{'content-type':'text/event-stream'}});args.channel.onmessage({type:'chunk',data:base64(response.subarray(0,49152))});}});
  const pending=forwardImageRequest(request,'large-image',data=>{const item=data as any;if(item.type==='end'){closed=true;done=true;}if(item.type==='chunk'&&!closed)parts.push(item.bytes);},new AbortController().signal);
  await tick();expect(done).toBe(false);
  for(let offset=49152;offset<response.length;offset+=49152)native.channels[0]!.onmessage({type:'chunk',data:base64(response.subarray(offset,offset+49152))});
  native.channels[0]!.onmessage({type:'end',bytes:response.length});await pending;
  expect(done).toBe(true);const received=new Uint8Array(parts.reduce((sum,part)=>sum+part.length,0));let at=0;for(const part of parts){received.set(part,at);at+=part.length;}expect(received.length).toBe(response.length);expect(received.every((byte,index)=>byte===response[index])).toBe(true);expect(JSON.parse(new TextDecoder().decode(received).split('\n')[0]!.slice(6)).b64_json.length).toBe(4*1024*1024);
 });
 it('rejects a mismatched byte count instead of closing a truncated stream as success',async()=>{
  native.invoke.mockImplementation(async(_command,args)=>{args.channel.onmessage({type:'headers',status:200,headers:{}});args.channel.onmessage({type:'chunk',data:'YWJj'});args.channel.onmessage({type:'end',bytes:100});});
  const sent:any[]=[];await expect(forwardImageRequest(request,'mismatch',data=>sent.push(data),new AbortController().signal)).rejects.toThrow('完整');expect(sent.some(data=>data.type==='end')).toBe(false);
 });
 it('allows cancelling while the last channel messages are still pending',async()=>{
  native.invoke.mockResolvedValue(undefined);const controller=new AbortController();const pending=forwardImageRequest(request,'cancel-tail',()=>{},controller.signal);await tick();controller.abort();await expect(pending).rejects.toThrow();expect(native.invoke).toHaveBeenCalledWith('cancel_image_request',{id:'cancel-tail'});
 });
 it('ignores late channel data after a native transfer failure',async()=>{
  native.invoke.mockRejectedValue(new Error('mock network failure'));const sent:object[]=[];
  await expect(forwardImageRequest(request,'failed',data=>sent.push(data),new AbortController().signal)).rejects.toThrow('network');
  native.channels[0]!.onmessage({type:'chunk',data:'YWJj'});native.channels[0]!.onmessage({type:'end',bytes:3});expect(sent).toHaveLength(0);
 });

});
