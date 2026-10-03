import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref } from 'vue';
import { migrateLegacyClock, useSystemClock } from '../../vendor/netspeed-dynamic/frontend/systemClock';
const fixture = vi.hoisted(() => ({ data: new Map<string,string>(), calls: [] as string[], fail: '' }));
vi.mock('../../vendor/netspeed-dynamic/frontend/bridge', () => ({
  storage: { getItem:(key:string)=>fixture.data.get(key) ?? null, setItem:(key:string,v:string)=>fixture.data.set(key,v), removeItem:(key:string)=>fixture.data.delete(key) },
  invoke:vi.fn(async (name:string)=>{ fixture.calls.push(name); if(fixture.fail===name) throw new Error('service unavailable'); }),
  emit:vi.fn(async()=>{}), listen:vi.fn(async()=>()=>{}),
}));
import { counterDelta, readTraffic } from '../../vendor/netspeed-dynamic/frontend/traffic';
import { emit, storage, invoke } from '../../vendor/netspeed-dynamic/frontend/bridge';
import { islandDefaults, saveIslandMode, saveIslandSetting, readIslandSettings, readIslandMode, readSlots } from './netspeed';
beforeEach(()=>{fixture.data.clear();fixture.calls=[];fixture.fail='';vi.mocked(emit).mockReset();});
afterEach(()=>vi.useRealTimers());
describe('island monitoring and settings',()=>{
  it('selects system time as an independent mode and persists time inside custom slots',async()=>{
    expect(readIslandSettings().system_time).toBe(false);
    await saveIslandMode('time',['speed','resource',null]);
    expect(readIslandMode(readIslandSettings())).toBe('time');
    expect(readIslandSettings()).toMatchObject({system_time:true,music_ctrl:false,sys_resource:false,fps_monitor:false,custom_display:false});
    expect(emit).toHaveBeenLastCalledWith('control-mode',{mode:'time',slots:['speed','resource',null]});
    await saveIslandMode('custom',['time','speed','fps']);
    expect(readIslandMode(readIslandSettings())).toBe('custom');
    expect(readSlots()).toEqual(['time','speed','fps']);
    expect(readIslandSettings().system_time).toBe(false);
    expect(invoke).toHaveBeenLastCalledWith('toggle_fps_plugin',{enable:true});
  });
  it('restores clock mode if delivery of a replacement mode fails',async()=>{
    await saveIslandMode('time',['time',null,null]);
    vi.mocked(emit).mockRejectedValueOnce(new Error('event unavailable'));
    await expect(saveIslandMode('custom',['speed','fps',null])).rejects.toThrow();
    expect(readIslandMode(readIslandSettings())).toBe('time');
    expect(readSlots()).toEqual(['time',null,null]);
    expect(invoke).toHaveBeenLastCalledWith('toggle_fps_plugin',{enable:false});
  });
  it('migrates an additional legacy clock into a combination without discarding its active content',()=>{
    fixture.data.set('nsd_show_system_time','true');fixture.data.set('nsd_sys_resource','true');
    expect(readIslandMode(readIslandSettings())).toBe('custom');
    expect(readSlots()).toEqual(['resource','time',null]);
    expect(fixture.data.has('nsd_show_system_time')).toBe(false);
    fixture.data.set('nsd_show_system_time','true');fixture.data.set('nsd_custom_slots','["cover","speed","fps"]');
    migrateLegacyClock(storage);
    expect(readSlots()).toEqual(['cover','speed','time']);
    expect(readIslandSettings().music_ctrl).toBe(true);
  });
  it('leaves disabled legacy clocks unchanged and retries failed preference migration',()=>{
    fixture.data.set('nsd_show_system_time','false');fixture.data.set('nsd_sys_resource','true');
    expect(readIslandMode(readIslandSettings())).toBe('resource');
    fixture.data.set('nsd_show_system_time','true');
    migrateLegacyClock({...storage,setItem:()=>{throw new Error('storage unavailable');}});
    expect(fixture.data.get('nsd_show_system_time')).toBe('true');
    expect(fixture.data.get('nsd_custom_display')).toBeUndefined();
    expect(fixture.data.get('nsd_sys_resource')).toBe('true');
  });
  it('reads local system time after clock changes and stops ticking when hidden or disposed',async()=>{
    vi.useFakeTimers();vi.setSystemTime(new Date(2026,9,2,23,59,59));
    const scope=effectScope(),active=ref(true);
    const clock=scope.run(()=>useSystemClock(active))!;
    expect(clock.text.value).toBe('23:59:59');
    vi.advanceTimersByTime(1000);expect(clock.text.value).toBe('00:00:00');
    vi.setSystemTime(new Date(2026,9,3,8,5,0));vi.advanceTimersByTime(1000);
    expect(clock.text.value).toBe('08:05:01');
    active.value=false;await nextTick();expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(3000);expect(clock.text.value).toBe('08:05:01');
    active.value=true;await nextTick();expect(clock.text.value).toBe('08:05:04');
    scope.stop();expect(vi.getTimerCount()).toBe(0);
  });
  it('does not count traffic from before launch or after a network counter reset',()=>{
    expect(counterDelta(null,[120000,55000])).toEqual([0,0]);
    expect(counterDelta([120000,55000],[100,70])).toEqual([0,0]);
    expect(counterDelta([100,70],[450,80])).toEqual([350,10]);
  });
  it('recovers from corrupt traffic history and discards invalid records',()=>{
    fixture.data.set('nsd_traffic_stats','broken'); expect(readTraffic()).toEqual({});
    fixture.data.set('nsd_traffic_stats',JSON.stringify({'2026-10-02':{up:10,down:100},'2026-10-01':{up:-1,down:4},bad:{up:4,down:4}}));
    expect(readTraffic()).toEqual({'2026-10-02':{up:10,down:100}});
  });
  it('keeps optional service switches off if their port or helper is unavailable',async()=>{
    fixture.fail='configure_activity_api';
    await expect(saveIslandSetting('activity_api',true,islandDefaults)).rejects.toThrow();
    expect(readIslandSettings().activity_api).toBe(false);
    fixture.fail='toggle_taskbar_plugin';
    await expect(saveIslandSetting('taskbar_plugin',true,islandDefaults)).rejects.toThrow();
    expect(readIslandSettings().taskbar_plugin).toBe(false);
  });
  it('preserves the selected mode when FPS cannot start',async()=>{
    fixture.data.set('nsd_music_ctrl','true'); fixture.fail='toggle_fps_plugin';
    await expect(saveIslandMode('fps',[])).rejects.toThrow();
    expect(readIslandSettings().music_ctrl).toBe(true); expect(readIslandSettings().fps_monitor).toBe(false);
  });
  it('enables custom FPS only when present and shuts it down for network mode',async()=>{
    await saveIslandMode('custom',['speed','fps',null]);
    expect(readIslandSettings().custom_display).toBe(true);
    await saveIslandMode('speed',[]);
    expect(readIslandSettings().custom_display).toBe(false);
    expect(fixture.calls).toEqual(['toggle_fps_plugin','toggle_fps_plugin']);
  });
});
