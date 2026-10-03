import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query';
import { MollyKeyDirectory } from '../components/providers/MollyKeyDirectory';
import type { Provider } from '@/types';
import type { AppId } from '@/lib/api';
const native=vi.hoisted(()=>vi.fn());
const notice=vi.hoisted(()=>({success:vi.fn(),error:vi.fn(),warning:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke:native}));
vi.mock('sonner',()=>({toast:notice}));
const key={id:'42',name:'Account key',key:'sk-••••1234',status:'active',group:{name:'OpenAI',platform:'openai',rate_multiplier:1},quota:10,quota_used:1,usage:{total_actual_cost:1.2345,today_actual_cost:null},provider_id:'molly-mock',model:'available-model',compatible:true,error:null};
const provider:Provider={id:'molly-mock',name:'Account key',settingsConfig:{auth:{},config:'model = "available-model"'},meta:{mollyKeyId:'42',mollyCodexMode:'native'}};
const edit=vi.fn();
function mount(app:AppId='codex',current='codex-official',entry=provider) {
  const client=new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}});
  render(<QueryClientProvider client={client}><MollyKeyDirectory appId={app} currentProviderId={current} providers={{[entry.id]:entry}} onEdit={edit}/></QueryClientProvider>);
  return client;
}
beforeEach(()=>{native.mockReset();vi.clearAllMocks();native.mockImplementation(async command=>command==='sync_molly_key_providers'?{agent:'codex',keys:[key],current_removed:false}:command==='get_opencode_live_provider_ids'?[]:{warnings:[]});});
afterEach(()=>{cleanup();vi.restoreAllMocks();});
describe('integrated account-key management',()=>{
  it('refreshes the local Official choice even when account networking fails',async()=>{
    let official=false;
    native.mockImplementation(async command=>{if(command==='sync_molly_key_providers'){official=true;throw new Error('mock account offline');}return official?'codex-official':'';});
    function Current(){const {data}=useQuery({queryKey:['providers','codex'],queryFn:()=>native('get_current_provider',{app:'codex'})});return <span>{data||'unconfigured'}</span>;}
    const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
    render(<QueryClientProvider client={client}><Current/><MollyKeyDirectory appId="codex" currentProviderId="" onEdit={edit}/></QueryClientProvider>);
    await screen.findByText('codex-official');await screen.findByText(/mock account offline/);
    expect(native.mock.calls.filter(([command])=>command==='get_current_provider').length).toBeGreaterThan(1);
    expect(native.mock.calls.some(([command])=>command==='switch_provider')).toBe(false);client.clear();
  });
  it('prepares choices without activating a key, then activates only on click',async()=>{
    const client=mount();await screen.findByText('Account key');
    expect(native.mock.calls.some(([command])=>command==='switch_provider')).toBe(false);
    fireEvent.click(screen.getByText('启用'));
    await waitFor(()=>expect(native).toHaveBeenCalledWith('switch_provider',{id:'molly-mock',app:'codex'}));
    client.clear();
  });
  it('leaves OpenCode empty and requests model selection before manual addition',async()=>{
    native.mockImplementation(async command=>command==='sync_molly_key_providers'?{agent:'opencode',keys:[{...key,provider_id:null,model:''}],current_removed:false}:[]);
    const post=vi.spyOn(window.parent,'postMessage');const client=mount('opencode');await screen.findByText('Account key');
    expect(native.mock.calls.some(([command])=>command==='switch_provider'||command==='add_provider')).toBe(false);
    fireEvent.click(screen.getByText('选择并添加'));
    expect(post).toHaveBeenCalledWith({source:'molly-ccswitch',type:'key-action',action:'configure-enable',app:'opencode',keyId:'42',model:''},window.location.origin);
    client.clear();
  });
  it('copies by key ID without sending credentials through the bridge',async()=>{
    const client=mount();await screen.findByText('Account key');fireEvent.click(screen.getByLabelText('复制 Account key 的密钥'));
    await screen.findByText('已复制');expect(native).toHaveBeenCalledWith('copy_api_key',{keyId:'42'});
    expect(JSON.stringify(native.mock.calls)).not.toContain('sk-');client.clear();
  });
  it('routes create, grouping and delete into host dialogs with IDs only',async()=>{
    const post=vi.spyOn(window.parent,'postMessage');const client=mount();await screen.findByText('Account key');
    fireEvent.click(screen.getByText('创建密钥'));fireEvent.click(screen.getByLabelText('切换 Account key 的分组'));fireEvent.click(screen.getByLabelText('删除 Account key 的密钥'));
    expect(post.mock.calls.map(([data])=>data.action)).toEqual(['create','group','delete']);
    expect(JSON.stringify(post.mock.calls)).not.toContain('sk-');client.clear();
  });
  it('opens the full provider editor including the current key without rewriting configuration',async()=>{
    const post=vi.spyOn(window.parent,'postMessage');const client=mount('codex','molly-mock');await screen.findByText('Account key');
    fireEvent.click(screen.getByLabelText('编辑 Account key 的供应商配置'));
    await waitFor(()=>expect(edit).toHaveBeenCalledWith(provider));expect(post).not.toHaveBeenCalled();expect(native.mock.calls.some(([c])=>c==='switch_provider')).toBe(false);client.clear();
  });
  it('changes only the selected key mode and waits for explicit apply',async()=>{
    const client=mount('codex','molly-mock');await screen.findByText('Account key');
    fireEvent.click(screen.getByRole('switch',{name:'Account key 的映射模式'}));
    await waitFor(()=>expect(native).toHaveBeenCalledWith('set_molly_key_mode',{agent:'codex',keyId:'42',mode:'mapped'}));
    expect(native.mock.calls.some(([c])=>c==='switch_provider')).toBe(false);client.clear();
  });
  it('allows applying a prepared change to the currently enabled key',async()=>{
    const client=mount('codex','molly-mock',{...provider,meta:{...provider.meta,mollyPendingApply:true}});await screen.findByText('Account key');
    fireEvent.click(screen.getByText('应用配置'));await waitFor(()=>expect(native).toHaveBeenCalledWith('switch_provider',{id:'molly-mock',app:'codex'}));client.clear();
  });
  it('prepares a missing provider only after an explicit edit action',async()=>{
    native.mockImplementation(async command=>command==='sync_molly_key_providers'?{agent:'opencode',keys:[{...key,provider_id:null}],current_removed:false}:command==='prepare_molly_key_provider'?'molly-mock':command==='get_providers'?{'molly-mock':provider}:[]);
    const client=mount('opencode');await screen.findByText('Account key');expect(native.mock.calls.some(([c])=>c==='prepare_molly_key_provider')).toBe(false);
    fireEvent.click(screen.getByLabelText('编辑 Account key 的供应商配置'));await waitFor(()=>expect(edit).toHaveBeenCalledWith(provider));expect(native.mock.calls.some(([c])=>c==='switch_provider')).toBe(false);expect(screen.queryByRole('switch')).toBeNull();client.clear();
  });
  it('keeps the key visible if activation fails and preserves unknown usage',async()=>{
    native.mockImplementation(async command=>{if(command==='switch_provider')throw new Error('mock configuration failure');return {agent:'codex',keys:[key],current_removed:false};});
    const client=mount();await screen.findByText('Account key');fireEvent.click(screen.getByText('启用'));
    await waitFor(()=>expect(notice.error).toHaveBeenCalledWith('mock configuration failure'));
    expect(screen.getByText('Account key')).toBeTruthy();expect(screen.getByText(/今日 —/)).toBeTruthy();client.clear();
  });
});
