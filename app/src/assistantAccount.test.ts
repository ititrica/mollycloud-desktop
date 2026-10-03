import { describe, expect, it, vi } from 'vitest';
import { accountAlerts, accountHealthSchema, AccountReminderMonitor } from './assistantAccount';
const now=Date.parse('2026-10-03T04:00:00Z');
const base=()=>accountHealthSchema.parse({user_id:1,currency:'USD',balance:10,subscriptions:[]});
const sub=(remaining=4,expiry=now+3600000,status='active')=>({id:'7',name:'Pro',status,expires_at:new Date(expiry).toISOString(),quotas:[{period:'daily' as const,limit_usd:10,used_usd:9.6,remaining_usd:0.4,remaining_percent:remaining,window_start:'2026-10-03T00:00:00Z'}]});
const memoryStorage=()=>{let value='{}';return {getItem:()=>value,setItem:(_key:string,next:string)=>{value=next;}};};
describe('account reminder thresholds',()=>{
  it('only alerts strictly below 5 USD and never treats unknown balance as zero',()=>{
    const health=base();for(const amount of [null,5,6]){health.balance=amount;expect(accountAlerts(health,now)).toHaveLength(0);}
    health.balance=4.999;expect(accountAlerts(health,now)[0]?.text).toContain('US$5');
  });
  it('checks each configured quota below 5 percent and expiry within 24 hours',()=>{
    const health=base();health.subscriptions=[sub()];expect(accountAlerts(health,now)).toHaveLength(2);
    health.subscriptions=[sub(5,now+24*3600000)];expect(accountAlerts(health,now)).toHaveLength(0);
    health.subscriptions=[sub(0,now-1),sub(0,now+3600000,'expired')];expect(accountAlerts(health,now)).toHaveLength(0);
  });
  it('does not invent a limit for an unlimited subscription',()=>{
    const health=base();health.subscriptions=[{...sub(),expires_at:null,quotas:[]}];expect(accountAlerts(health,now)).toHaveLength(0);
  });
  it('deduplicates successful reminders per account and keeps failed deliveries retryable',async()=>{
    let clock=now;const health=base();health.balance=1;const send=vi.fn().mockReturnValue(false);const fetch=vi.fn().mockResolvedValue(health);
    const monitor=new AccountReminderMonitor(()=>true,send,fetch,memoryStorage(),()=>clock);
    await monitor.check();await monitor.check();expect(send).toHaveBeenCalledTimes(2);
    send.mockReturnValue(true);await monitor.check();await monitor.check();expect(send).toHaveBeenCalledTimes(3);
    expect(send.mock.calls[0]?.[0]).toContain('\n');
    health.user_id='2';await monitor.check();expect(send).toHaveBeenCalledTimes(4);
    clock+=6*3600000;await monitor.check();expect(send).toHaveBeenCalledTimes(5);
  });
  it('discards a response predating logout and retries after busy conversations',async()=>{
    let ready=false;let resolve!:(value:ReturnType<typeof base>)=>void;const fetch=vi.fn(()=>new Promise<ReturnType<typeof base>>(r=>{resolve=r;}));const send=vi.fn(()=>true);
    const monitor=new AccountReminderMonitor(()=>ready,send,fetch,memoryStorage(),()=>now);
    await monitor.check();expect(fetch).not.toHaveBeenCalled();ready=true;const pending=monitor.check();monitor.reset();const health=base();health.balance=0;resolve(health);await pending;expect(send).not.toHaveBeenCalled();
  });
  it('network failures do not generate reminders or start paid AI calls',async()=>{
    const send=vi.fn(()=>true);const monitor=new AccountReminderMonitor(()=>true,send,async()=>{throw Error('offline');},memoryStorage(),()=>now);
    await monitor.check();expect(send).not.toHaveBeenCalled();
  });
});
