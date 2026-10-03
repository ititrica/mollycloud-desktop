import {describe,it,expect,vi} from 'vitest';
import {buildMessages} from './AssistantClient';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
describe('local account privacy and token boundary',()=>{
 it('excludes local queries and reminders from conversational model input',()=>{
  const input=buildMessages([{role:'user',content:'查余额',local_account:true},{role:'assistant',content:'private balance 3.72',local_account:true},{role:'user',content:'你好'}],'',[]);
  expect(input.some(m=>m.content?.includes('private balance'))).toBe(false);expect(input.some(m=>m.content==='查余额')).toBe(false);expect(input.some(m=>m.content==='你好')).toBe(true);
 });
});
