import { targetPlatform } from "./platform.mjs";
import { regressionBrowserPath } from "./browser-path.mjs";
// Isolated desktop UI regression check; requires the local Vite dev server.
// No real login, credentials, clipboard, or desktop pet state are used.
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const isWindows = targetPlatform() === 'windows';
const navCount = isWindows ? 7 : 6;
const trayName = isWindows ? '托盘' : '菜单栏';
const baseUrl = process.env.MOLLY_UI_URL || 'http://localhost:24320';
const appVersion = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8')).version;
const previewUpdateVersion = appVersion.replace(/\d+$/, patch => String(Number(patch) + 1));
const productionSmoke = process.argv.includes('--production');
const darkMode = process.argv.includes('--dark');
const appearanceOnly = process.argv.includes('--appearance');
const motionOnly = process.argv.includes('--motion');
const netspeedOnly = process.argv.includes('--netspeed');
const paymentsOnly = process.argv.includes('--keys-payments');
const speechOnly = process.argv.includes('--speech');
const subscriptionsOnly = process.argv.includes('--subscriptions');
const clockOnly = process.argv.includes('--island-clock');
if (!isWindows && (netspeedOnly || clockOnly)) throw new Error('macOS 不包含灵动岛；此专项只适用于 Windows。');
const outputDir = resolve(import.meta.dirname, '../../artifacts/design-review', clockOnly ? 'island-clock' : motionOnly ? 'motion' : appearanceOnly ? 'appearance' : speechOnly ? `speech/${darkMode ? 'dark' : 'light'}` : subscriptionsOnly ? `subscriptions/${darkMode ? 'dark' : 'light'}` : darkMode ? 'dark' : 'light');
await mkdir(outputDir, { recursive: true });
const profile = await mkdtemp(join(tmpdir(), 'molly-design-check-'));
const browser = spawn(regressionBrowserPath(), [
  '--headless=new', '--disable-gpu', '--disable-partial-raster', '--no-first-run', '--no-default-browser-check',
  '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
const endpoint = await new Promise((resolve, reject) => {
  let log = '';
  const timer = setTimeout(() => reject(new Error('Browser startup timed out')), 15000);
  browser.on('error', reject);
  browser.stderr.on('data', chunk => {
    log += String(chunk);
    const match = log.match(/DevTools listening on (ws:\/\/[^\s]+)/);
    if (match) { clearTimeout(timer); resolve(match[1]); }
  });
});
const socket = new WebSocket(endpoint);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let nextId = 0;
const pending = new Map();
const runtimeErrors = [];
const contexts = new Map();
socket.onmessage = event => {
  const message = JSON.parse(event.data);
  if (message.method === 'Runtime.executionContextCreated') contexts.set(`${message.sessionId}-${message.params.context.id}`, { ...message.params.context, session: message.sessionId });
  if (message.method === 'Runtime.executionContextDestroyed') contexts.delete(`${message.sessionId}-${message.params.executionContextId}`);
  if (message.method === 'Runtime.executionContextsCleared') for (const [key, value] of contexts) if (value.session === message.sessionId) contexts.delete(key);
  if (message.method === 'Target.attachedToTarget') void call('Runtime.enable', {}, message.params.sessionId);
  if (message.method === 'Runtime.exceptionThrown') runtimeErrors.push(message.params.exceptionDetails);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  clearTimeout(request.timer);
  if (message.error) request.reject(new Error(JSON.stringify(message.error)));
  else request.resolve(message.result);
};
function call(method, params = {}, sessionId) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Timed out: ${method}`)); }, 15000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
}
const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
const page = (method, params) => call(method, params, sessionId);
async function evaluate(expression, context) {
  const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, ...(context ? { contextId: context.id } : {}) }, context?.session ?? sessionId);
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function ready(selector) {
  for (let i = 0; i < 80; i++) {
    if (await evaluate(`Boolean(document.querySelector(${JSON.stringify(selector)}))`)) {
      await evaluate('document.fonts.ready.then(() => true)');
      await pause(220);
      return;
    }
    await pause(100);
  }
  throw new Error(`Page did not render ${selector}`);
}
async function waitFor(expression, description) {
  for (let i = 0; i < 150; i++) {
    if (await evaluate(expression)) { await pause(200); return; }
    await pause(100);
  }
  throw new Error(`Timed out: ${description}`);
}
async function screenshot(name) {
  await settleAnimations();
  // Flush the headless compositor after viewport and sidebar changes.
  await page('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  const { data } = await page('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  await writeFile(join(outputDir, `${name}.png`), Buffer.from(data, 'base64'));
}
async function settleAnimations() {
  await evaluate(`(async () => {
    // Out-in transitions start a second animation after the first has finished.
    for (let pass=0;pass<10;pass++) {
      await new Promise(resolve => requestAnimationFrame(resolve));
      const running=document.getAnimations().filter(animation => animation.playState==='running' && Number.isFinite(animation.effect?.getComputedTiming().endTime));
      if (!running.length) return;
      await Promise.all(running.map(animation => animation.finished.catch(() => {})));
    }
  })()`);
}
const failures = [];
const report = [];
function check(condition, message) { if (!condition) failures.push(message); }
async function mutate(code, root = '.app-shell') {
  await evaluate(`(() => { const state = document.querySelector('${root}').__vueParentComponent.setupState; ${code} })()`);
  await pause(240);
}
async function key(key, code, virtualKey) {
  await page('Input.dispatchKeyEvent', { type: 'keyDown', key, code, windowsVirtualKeyCode: virtualKey, text: key === 'Enter' ? '\r' : key === ' ' ? ' ' : '' });
  await page('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: virtualKey });
  await pause(150);
}
const names = ['overview', 'subscriptions', 'keys', 'usage', 'assistant', 'ccswitch', 'images', 'skills', 'recharge', ...(isWindows ? ['netspeed'] : [])];
async function select(index) {
  const target = names[index];
  if (['overview', 'subscriptions', 'usage', 'recharge'].includes(target)) {
    await evaluate(`document.querySelector('.nav-item[data-page="overview"]').click()`);
    await pause(50);
    await evaluate(`document.querySelector('.overview-subnav [data-page="${target}"]').click()`);
  } else {
    await evaluate(`document.querySelector('.nav-item[data-page="${target}"]').click()`);
  }
  await settleAnimations();
}
async function verifySkills(width, height) {
  await waitFor(`!document.querySelector('.skills-load-state') && Boolean(document.querySelector('.skills-frame')?.contentDocument.querySelector('.molly-skills-tabs'))`, 'Skill Manager ready');
  const child = expression => evaluate(`(() => {const frame=document.querySelector('.skills-frame'); const d=frame.contentDocument; const w=frame.contentWindow; return (${expression});})()`);
  for (const route of ['/', '/my-skills', '/install', '/global-workspace', '/organizer', '/backup', '/settings']) {
    await child(`d.querySelector('a[href="#${route}"]').click()`);
    await pause(350);
    const metrics=await child(`(() => {const frameRect=frame.getBoundingClientRect(); const content=d.querySelector('.molly-skills-content'); return {route:w.location.hash || '#/', width:w.innerWidth, height:w.innerHeight, frameBottom:frameRect.bottom, outerScroll:document.querySelector('.workspace').scrollHeight > document.querySelector('.workspace').clientHeight+1, overflow:d.documentElement.scrollWidth>w.innerWidth || content.scrollWidth>content.clientWidth+1, theme:d.documentElement.dataset.theme, font:w.getComputedStyle(d.body).fontFamily, bg:w.getComputedStyle(d.body).backgroundColor, text:d.body.textContent.slice(0,180)};})()`);
    check(metrics.route === '#'+route && !metrics.overflow && !metrics.outerScroll && metrics.frameBottom <= height, `Skills content bounds ${route} ${width}`);
    check(await child(`!d.querySelector('.app-page-title,.app-page-subtitle,.molly-page-title,.molly-page-subtitle')`), `Skill page introductions removed ${route} ${width}`);
    check(metrics.theme === (darkMode ? 'dark' : 'light') && metrics.font.includes('Microsoft YaHei') && metrics.bg === (darkMode ? 'rgb(34, 37, 31)' : 'rgb(255, 255, 255)'), `Skills shared theme ${route} ${width}`);
    report.push({page:'skills-'+route,width,height,...metrics});
    await screenshot(`skills-${route === '/' ? 'home' : route.slice(1)}-${width}`);
  }
  await child(`d.querySelector('a[href="#/organizer"]').click()`);
  await pause(200);
  await child(`[...d.querySelectorAll('button')].find(b=>b.textContent.trim()==='新建预设').click()`);
  await waitFor(`document.querySelector('.skills-frame').contentDocument.querySelector('[role="dialog"]') && document.querySelector('.sidebar').inert`, 'Skills modal host boundary');
  check(await child(`!document.querySelector('.app-shell').inert && !frame.closest('[inert]') && d.querySelector('[role="dialog"]').contains(d.activeElement)`), `Skills modal remains interactive and focused ${width}`);
  await child(`(() => {const input=d.querySelector('[role="dialog"] input'); Object.getOwnPropertyDescriptor(w.HTMLInputElement.prototype,'value').set.call(input,'Preview test'); input.dispatchEvent(new w.Event('input',{bubbles:true}));})()`);
  await pause(100);
  await child(`[...d.querySelector('[role="dialog"]').querySelectorAll('button')].find(b=>b.textContent.trim()==='创建').click()`);
  await waitFor(`document.querySelector('.skills-frame').contentDocument.querySelector('[role="alert"]')?.textContent.includes('桌面端')`, 'Skills preview refuses writes');
  await screenshot(`skills-preview-write-error-${width}`);
  await child(`[...d.querySelector('[role="dialog"]').querySelectorAll('button')].find(b=>b.textContent.trim()==='取消').click()`);
  await waitFor(`!document.querySelector('.sidebar').inert`, 'Skills modal closes');
}
async function verifySubscriptions(width, height) {
  await select(1); await ready('.subscription-card');
  const state = `document.querySelector('.subscriptions-panel').__vueParentComponent.setupState`;
  check(await evaluate(`(() => {const cards=[...document.querySelectorAll('.subscription-card')].map(el=>el.getBoundingClientRect());return cards.length===2&&Math.abs(cards[0].y-cards[1].y)<1&&cards[1].x>cards[0].right&&document.querySelector('.subscription-card').textContent.includes('每周')&&document.querySelectorAll('.subscription-platform').length===2;})()`), `Website subscriptions use two cards per row ${width}`);
  await evaluate(`(async()=>{const loaded=performance.getEntriesByType('resource').find(e=>new URL(e.name).pathname==='/src/subscriptions.ts');window.__subApi=(await import(loaded.name)).subscriptionApi;window.__subOriginal={...window.__subApi};window.__subRows=JSON.parse(JSON.stringify(${state}.items));window.__subInitial=JSON.parse(JSON.stringify(window.__subRows));window.__subCalls=[];window.__renewCalls=[];localStorage.removeItem('mollycloud:preview:subscription-auto-renew-no-reminder:1');window.__subApi.list=async()=>JSON.parse(JSON.stringify(window.__subRows));window.__subApi.autoRenew=async(id,input)=>{window.__renewCalls.push({id,input});const row=window.__subRows.find(item=>item.id===id);if(input.enabled!=null)row.auto_renew_enabled=input.enabled;row.auto_renew_error='';return JSON.parse(JSON.stringify(row));};window.__subApi.reset=id=>{window.__subCalls.push(id);return new Promise(resolve=>{window.__finishReset=()=>{const i=window.__subRows.findIndex(row=>row.id===id);const updated={...window.__subRows[i],id:Math.max(...window.__subRows.map(row=>row.id))+10,weekly_usage_usd:0,expires_at:new Date(Date.now()+7*86400000).toISOString()};window.__subRows[i]=updated;resolve(JSON.parse(JSON.stringify(updated)));};});};})()`);
  await evaluate(`document.querySelector('.subscription-reset').click()`); await ready('.subscription-dialog');
  check(await evaluate(`document.querySelector('.app-shell').inert&&document.querySelector('.subscription-dialog').contains(document.activeElement)&&document.querySelector('.subscription-dialog').textContent.includes('费用将从余额扣除')`), `Paid reset confirms charge and isolates background ${width}`);
  await screenshot(`subscription-reset-confirm-${width}`);
  await evaluate(`document.querySelector('.cancel-subscription').click()`); await waitFor(`!document.querySelector('.subscription-dialog')`, 'Reset cancelled');
  check(await evaluate(`window.__subCalls.length===0&&document.activeElement.classList.contains('subscription-reset')`), `Reset cancellation sends no request and restores focus ${width}`);
  await evaluate(`document.querySelector('.subscription-reset').click()`); await ready('.subscription-dialog');
  await evaluate(`document.querySelector('.confirm-subscription').click();document.querySelector('.confirm-subscription').click()`);
  await waitFor(`window.__subCalls.length===1&&document.querySelector('.cancel-subscription').disabled`, 'Single pending paid reset');
  await key('Escape','Escape',27);
  check(await evaluate(`window.__subCalls.length===1&&Boolean(document.querySelector('.subscription-dialog'))&&[...document.querySelectorAll('.subscription-reset')].every(button=>button.disabled)`), `Pending reset blocks duplicates and dialog dismissal ${width}`);
  await evaluate(`window.__finishReset()`); await waitFor(`!document.querySelector('.subscription-dialog')&&${state}.items[0].id===window.__subRows[0].id`, 'Replacement subscription applied');
  check(await evaluate(`${state}.items[0].weekly_usage_usd===0&&window.__subRows[0].id!==window.__subInitial[0].id`), `Reset replaces old ID and quota ${width}`);
  await evaluate(`document.querySelector('.subscription-auto-renew [role="switch"]').click()`); await ready('.subscription-dialog');
  check(await evaluate(`document.querySelector('.subscription-dialog').textContent.includes('额度耗尽或者订阅时间结束')`), `Auto renew explains website trigger semantics ${width}`);
  await screenshot(`subscription-auto-renew-confirm-${width}`);
  await evaluate(`document.querySelector('.cancel-subscription').click()`); await waitFor(`!document.querySelector('.subscription-dialog')`, 'Auto renew cancelled');
  check(await evaluate(`window.__renewCalls.length===0&&document.querySelector('.subscription-auto-renew [role="switch"]').getAttribute('aria-checked')==='false'`), `Auto renew cancellation leaves persisted state ${width}`);
  await evaluate(`document.querySelector('.subscription-auto-renew [role="switch"]').click()`); await ready('.subscription-dialog');
  await evaluate(`document.querySelector('.subscription-dialog .n-checkbox').click();document.querySelector('.confirm-subscription').click()`);
  await waitFor(`!document.querySelector('.subscription-dialog')&&document.querySelector('.subscription-auto-renew [role="switch"]').getAttribute('aria-checked')==='true'`, 'Renew enabled');
  check(await evaluate(`localStorage.getItem('mollycloud:preview:subscription-auto-renew-no-reminder:1')==='true'&&window.__renewCalls.length===1`), `No-reminder persists only after successful enable ${width}`);
  await evaluate(`document.querySelector('.subscription-auto-renew [role="switch"]').click()`); await waitFor(`document.querySelector('.subscription-auto-renew [role="switch"]').getAttribute('aria-checked')==='false'`, 'Renew disabled directly');
  await evaluate(`document.querySelector('.subscription-auto-renew [role="switch"]').click()`); await waitFor(`document.querySelector('.subscription-auto-renew [role="switch"]').getAttribute('aria-checked')==='true'`, 'Renew reminder skipped');
  check(await evaluate(`!document.querySelector('.subscription-dialog')&&window.__renewCalls.length===3`), `Account-scoped no-reminder skips only confirmation ${width}`);
  await evaluate(`window.__subApi.autoRenew=async()=>{throw new Error('模拟保存失败');};document.querySelector('.subscription-auto-renew [role="switch"]').click()`);
  await waitFor(`document.querySelector('.subscriptions-panel [role="alert"]')?.textContent.includes('模拟保存失败')`, 'Renew failure visible');
  check(await evaluate(`document.querySelector('.subscription-auto-renew [role="switch"]').getAttribute('aria-checked')==='true'`), `Failed auto renew save preserves enabled state ${width}`);
  await evaluate(`window.__subApi.autoRenew=async(id,input)=>{window.__renewCalls.push({id,input});const row=window.__subRows.find(item=>item.id===id);if(input.enabled!=null)row.auto_renew_enabled=input.enabled;row.auto_renew_error='';return JSON.parse(JSON.stringify(row));};window.__subRows[0].auto_renew_error='insufficient_balance';${state}.items=JSON.parse(JSON.stringify(window.__subRows));`);
  await waitFor(`Boolean(document.querySelector('.subscription-renew-error'))`, 'Balance-low notice visible');
  await evaluate(`document.querySelector('.subscription-reset').click()`); await ready('.subscription-dialog');
  check(await evaluate(`window.__renewCalls.at(-1).input.clear_error===true&&${state}.items[0].auto_renew_enabled===true&&!document.querySelector('.subscription-renew-error')`), `Manual reset acknowledges previous balance failure without disabling renew ${width}`);
  await evaluate(`window.__subApi.reset=async()=>{throw new Error('模拟余额不足');};document.querySelector('.confirm-subscription').click()`);
  await waitFor(`document.querySelector('.subscription-dialog [role="alert"]')?.textContent.includes('模拟余额不足')`, 'Reset failure shown');
  check(await evaluate(`Boolean(document.querySelector('.subscription-dialog'))&&${state}.items[0].id===window.__subRows[0].id`), `Failed reset retains original term ${width}`);
  await screenshot(`subscription-reset-error-${width}`);
  await evaluate(`window.__subApi.reset=async()=>{throw new Error('连接超时');};document.querySelector('.confirm-subscription').click()`);
  await waitFor(`document.querySelector('.confirm-subscription').disabled&&document.querySelector('.subscription-dialog').textContent.includes('刷新并核对')`, 'Ambiguous reset blocks another write');
  await evaluate(`window.__subRows[0].id+=1;[...document.querySelectorAll('.subscription-dialog button')].find(button=>button.textContent.includes('刷新并核对')).click()`);
  await waitFor(`!document.querySelector('.subscription-dialog')`, 'Committed replacement discovered by refresh');
  check(await evaluate(`${state}.items[0].id===window.__subRows[0].id`), `Timeout reconciliation adopts server replacement ${width}`);
  await evaluate(`window.__subRows=JSON.parse(JSON.stringify(window.__subInitial));Object.assign(window.__subApi,window.__subOriginal);${state}.items=JSON.parse(JSON.stringify(window.__subInitial));${state}.error='';${state}.notice='';localStorage.removeItem('mollycloud:preview:subscription-auto-renew-no-reminder:1');`);
  report.push({page:'subscription-management',width,height,reset:'isolated mock',autoRenew:'isolated mock'});
}

async function verifyKeyGroupsAndRecharge(width, height) {
  await select(2);
  const before=await evaluate(`document.querySelector('.key-group-trigger strong').textContent`);
  await evaluate(`document.querySelector('.key-group-trigger').click()`);
  await ready('.key-group-menu');
  await waitFor(`document.querySelectorAll('.key-group-option').length>=3`, 'Group options loaded');
  check(await evaluate(`getComputedStyle(document.querySelector('.key-group-search input')).caretColor==='rgb(0, 0, 0)'`), `Console input caret remains black ${width}`);
  check(await evaluate(`!document.querySelector('.app-shell').inert && document.activeElement.closest('.key-group-search')`), `Group dropdown focuses search ${width}`);
  const dropdown=await evaluate(`(()=>{const r=document.querySelector('.key-group-menu').getBoundingClientRect();return {x:r.x,y:r.y,right:r.right,bottom:r.bottom};})()`);
  check(dropdown.x>=0&&dropdown.right<=width&&dropdown.bottom<=height,`Group dropdown fits viewport ${width}`);
  await screenshot(`key-group-dropdown-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.key-group-menu')`, 'Dismiss group dropdown');
  check(await evaluate(`document.querySelector('.key-group-trigger strong').textContent`)===before,`Dismiss preserves group ${width}`);
  await evaluate(`document.querySelector('.key-group-trigger').click()`);
  await ready('.key-group-menu');
  await evaluate(`(()=>{const input=document.querySelector('.key-group-search input');input.value='Standard';input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await waitFor(`document.querySelectorAll('.key-group-option').length===1`, 'Search filters groups');
  await key('Enter','Enter',13);
  await waitFor(`!document.querySelector('.key-group-menu')&&document.querySelector('.key-group-trigger strong').textContent==='Molly Standard'`, 'Selection applies immediately');
  check(await evaluate(`document.querySelector('.key-cell code').textContent==='sk-••••942A'`),`Group change preserves key ${width}`);
  await mutate(`state.dashboard.keys.items[0].id='rejected-preview-key';`);
  await evaluate(`document.querySelector('.key-group-trigger').click()`);
  await ready('.key-group-menu');
  await evaluate(`[...document.querySelectorAll('.key-group-option')].find(el=>el.textContent.includes('Claude')).click()`);
  await waitFor(`Boolean(document.querySelector('.key-group-error'))`, 'Rejected group remains open');
  check(await evaluate(`document.querySelector('.key-group-trigger strong').textContent==='Molly Standard'`),`Failed group selection preserves row ${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.key-group-menu')`, 'Dismiss failed group');
  await mutate(`state.dashboard.keys.items[0].id='demo-key';state.dashboard.keys.items[0].group_id=1;state.dashboard.keys.items[0].group={name:'Molly Pro',platform:'openai',rate_multiplier:1};delete state.dashboard.keys.items[0].group_name;state.keyGroupSuccess='';`);
  const count=await evaluate(`document.querySelectorAll('.key-record').length`);
  await evaluate(`document.querySelector('.create-key-button').click()`);
  await ready('.create-key-dialog');
  check(await evaluate(`document.querySelector('.app-shell').inert`),`Create modal isolates background ${width}`);
  await screenshot(`create-key-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.create-key-dialog')`, 'Cancel key creation');
  check(await evaluate(`document.querySelectorAll('.key-record').length`)===count,`Cancel creates no key ${width}`);
  await evaluate(`document.querySelector('.create-key-button').click()`);
  await ready('.create-key-dialog');
  await evaluate(`(()=>{const input=document.querySelector('.create-key-dialog input');input.value='Design preview key';input.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('.create-key-dialog .key-group-trigger').click();})()`);
  await ready('.key-group-menu');
  await evaluate(`document.querySelector('.key-group-option').click()`);
  await waitFor(`!document.querySelector('.key-group-menu')`, 'Draft group selected');
  await evaluate(`document.querySelector('.create-key-dialog button[type="submit"]').click()`);
  await waitFor(`!document.querySelector('.create-key-dialog')&&document.querySelectorAll('.key-record').length===${count+1}`, 'Created key inserted');
  check(await evaluate(`document.querySelector('.key-cell code').textContent==='sk-••••DEMO'`),`Created key is masked ${width}`);
  // All deletion tests use the isolated browser preview and simulated requests.
  await evaluate(`(async()=>{const loaded=performance.getEntriesByType('resource').find(e=>new URL(e.name).pathname==='/src/ipc.ts');if(!loaded)throw new Error('IPC module resource missing');const api=(await import(loaded.name)).desktopApi;window.__keyTestApi=api;window.__originalKeyDelete=api.deleteApiKey;window.__keyDeleteCalls=0;api.deleteApiKey=async()=>{window.__keyDeleteCalls++;throw new Error('模拟删除失败');};})()`);
  await evaluate(`document.querySelector('.key-delete-button').click()`);
  await ready('.delete-key-dialog');
  await settleAnimations();
  check(await evaluate(`document.querySelector('.app-shell').inert&&document.querySelector('.delete-key-dialog').contains(document.activeElement)&&document.activeElement.textContent.includes('取消')`),`Delete modal isolates and focuses ${width}`);
  check(await evaluate(`document.querySelector('.delete-key-dialog').textContent.includes('Design preview key')&&document.querySelector('.delete-key-dialog').textContent.includes('无法撤销')`),`Delete identifies key and consequences ${width}`);
  const deletionBounds=await evaluate(`(()=>{const r=document.querySelector('.delete-key-dialog').getBoundingClientRect();return {x:r.x,y:r.y,right:r.right,bottom:r.bottom};})()`);
  check(deletionBounds.x>=0&&deletionBounds.y>=32&&deletionBounds.right<=width&&deletionBounds.bottom<=height,`Delete modal fits viewport ${width}`);
  await screenshot(`delete-key-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.delete-key-dialog')&&!document.querySelector('.app-shell').inert`,'Cancel deletion');
  check(await evaluate(`window.__keyDeleteCalls===0&&document.querySelectorAll('.key-record').length===${count+1}&&document.activeElement.classList.contains('key-delete-button')`),`Cancel preserves key and restores focus ${width}`);
  await evaluate(`document.querySelector('.key-delete-button').click()`);
  await ready('.delete-key-dialog');
  await evaluate(`document.querySelector('.confirm-delete-key').click()`);
  await waitFor(`document.querySelector('.delete-key-dialog [role="alert"]')?.textContent.includes('模拟删除失败')`,'Deletion failure shown');
  check(await evaluate(`window.__keyDeleteCalls===1&&document.querySelectorAll('.key-record').length===${count+1}`),`Delete failure preserves row ${width}`);
  await screenshot(`delete-key-error-${width}`);
  await evaluate(`window.__keyTestApi.deleteApiKey=()=>{window.__keyDeleteCalls++;return new Promise(resolve=>{window.__resolveKeyDelete=resolve;});};document.querySelector('.confirm-delete-key').click();document.querySelector('.confirm-delete-key').click();`);
  await waitFor(`document.querySelector('.delete-key-dialog').getAttribute('aria-busy')==='true'`,'Deletion pending');
  await key('Escape','Escape',27);
  check(await evaluate(`window.__keyDeleteCalls===2&&document.querySelector('.confirm-delete-key').disabled&&Boolean(document.querySelector('.delete-key-dialog'))`),`Delete blocks repeat and closing during request ${width}`);
  await evaluate(`window.__resolveKeyDelete()`);
  await waitFor(`!document.querySelector('.delete-key-dialog')&&document.querySelectorAll('.key-record').length===${count}&&!document.querySelector('.app-shell').inert`,'Deleted key removed');
  check(await evaluate(`document.activeElement.classList.contains('create-key-button')&&document.querySelector('.key-group-success').textContent.includes('已删除密钥')`),`Delete reports success and restores focus ${width}`);
  await mutate(`state.dashboard.keys.items.unshift({id:[...state.deletedKeyIds][0],name:'Stale deleted key',key:'sk-••••DEMO'});`);
  check(await evaluate(`document.querySelectorAll('.key-record').length===${count}&&!document.querySelector('.key-list').textContent.includes('Stale deleted key')`),`Stale refresh cannot restore deleted key ${width}`);
  await mutate(`state.dashboard.keys.items=state.dashboard.keys.items.filter(k=>k.id==='demo-key');state.keyGroupSuccess='';`);
  await evaluate(`window.__keyTestApi.deleteApiKey=window.__originalKeyDelete;window.__previewKeyRows=JSON.parse(JSON.stringify(document.querySelector('.app-shell').__vueParentComponent.setupState.dashboard.keys.items));document.querySelector('.key-delete-button').click()`);
  await ready('.delete-key-dialog');
  await evaluate(`document.querySelector('.confirm-delete-key').click()`);
  await waitFor(`!document.querySelector('.delete-key-dialog')&&Boolean(document.querySelector('.key-directory .empty-state'))`,'Deleting final key shows empty state');
  check(await evaluate(`document.querySelector('.key-directory-heading').textContent.includes('0')`),`Delete updates key count ${width}`);
  await mutate(`state.deletedKeyIds.clear();state.dashboard.keys.items=window.__previewKeyRows;state.keyGroupSuccess='';`);
  await evaluate(`document.querySelector('.balance-chip').click()`);
  await ready('.payment-compose');await settleAnimations();
  check(await evaluate(`document.querySelector('.overview-subnav [aria-current="page"]').dataset.page==='recharge'&&!document.querySelector('.recharge-webview')`),`Balance opens native recharge ${width}`);
  check(await evaluate(`document.querySelector('.recharge-panel').scrollWidth<=document.querySelector('.recharge-panel').clientWidth`),`Recharge fits width ${width}`);
  check(await evaluate(`!document.querySelector('.recharge-actions')&&!document.querySelector('.recharge-panel > button')`),`Recharge starts with tabs without a refresh row ${width}`);
  await evaluate(`[...document.querySelectorAll('.payment-tabs button')].find(b=>b.textContent==='我的订单').click()`);
  await ready('.payment-order-row');
  check(await evaluate(`[...document.querySelectorAll('.payment-order-row')].every(row=>row.tagName==='BUTTON'&&!row.querySelector('button')&&!row.textContent.includes('查看'))`),`Order rows replace separate view buttons ${width}`);
  await evaluate(`document.querySelector('.payment-order-row').focus()`);
  await key('Enter','Enter',13);
  await ready('.payment-detail');
  check(await evaluate(`Boolean(document.querySelector('.payment-order-row[aria-expanded="true"]'))&&document.querySelector('.payment-detail').textContent.includes('订单 #')`),`Order details remain keyboard accessible ${width}`);
  await screenshot(`recharge-orders-${width}`);
  await evaluate(`[...document.querySelectorAll('.payment-tabs button')].find(b=>b.textContent==='余额充值').click()`);
  await ready('.payment-amounts');
  await evaluate(`document.querySelector('.payment-amounts button').click()`);
  await settleAnimations();
  check(await evaluate(`getComputedStyle(document.querySelector('.payment-amounts .is-selected')).backgroundColor==='rgb(181, 255, 54)'`),`Selected amount uses solid lime ${width}`);
  await evaluate(`document.querySelector('.payment-methods [data-method="wxpay"]').click()`);
  await settleAnimations();
  check(await evaluate(`getComputedStyle(document.querySelector('.payment-methods [data-method="wxpay"]')).backgroundColor==='rgb(36, 160, 56)'`),`WeChat selected brand color ${width}`);
  await screenshot(`recharge-wechat-${width}`);
  await evaluate(`document.querySelector('.payment-methods [data-method="alipay"]').click()`);
  await settleAnimations();
  check(await evaluate(`getComputedStyle(document.querySelector('.payment-methods [data-method="alipay"]')).backgroundColor==='rgb(0, 174, 239)'`),`Alipay selected brand color ${width}`);
  check(await evaluate(`['auto','scroll'].includes(getComputedStyle(document.querySelector('.workspace')).overflowY)`),`Recharge can scroll ${width}`);
  await evaluate(`document.querySelector('.workspace').scrollTop=100000`);
  check(await evaluate(`document.querySelector('.recharge-panel').getBoundingClientRect().bottom<=innerHeight+1`),`Recharge bottom reachable ${width}`);
  await evaluate(`document.querySelector('.workspace').scrollTop=0`);
  await screenshot(`recharge-${width}`);
  await evaluate(`[...document.querySelectorAll('.payment-tabs button')].find(b=>b.textContent==='订阅套餐').click()`);
  await ready('.payment-plan');
  check(await evaluate(`document.querySelector('.payment-plan[data-purchasable="false"]').disabled&&document.querySelector('.payment-plan[data-purchasable="false"]').textContent.includes('不可购买')&&!document.querySelector('.payment-plans').textContent.includes('隐藏套餐')`),`Plans separate visibility from purchase ${width}`);
  await evaluate(`document.querySelector('.payment-plan[data-purchasable="false"]').click()`);
  check(await evaluate(`document.querySelector('.payment-summary .n-button').disabled&&!document.querySelector('.payment-plan.is-selected')`),`Display-only plan cannot be selected ${width}`);
  await screenshot(`recharge-plans-${width}`);
  await mutate(`state.info.payment_enabled=false;`, '.recharge-panel');
  check(await evaluate(`document.querySelectorAll('.payment-plan').length===3&&[...document.querySelectorAll('.payment-plan')].every(p=>p.disabled&&p.querySelector('.payment-plan-select').textContent==='不可购买')&&!document.querySelector('.payment-compose')`),`Payment disabled keeps plan details visible ${width}`);
  await screenshot(`recharge-plans-unavailable-${width}`);
  await mutate(`state.info.payment_enabled=true;`, '.recharge-panel');
  await evaluate(`document.querySelector('.payment-plan').click()`);
  await settleAnimations();
  check(await evaluate(`getComputedStyle(document.querySelector('.payment-plan.is-selected .payment-plan-select')).backgroundColor==='rgb(181, 255, 54)'`),`Selected subscription uses solid lime ${width}`);
  await screenshot(`recharge-plan-selected-${width}`);
  await evaluate(`document.querySelector('.payment-summary .n-button').click()`);
  await ready('.payment-detail');
  check(await evaluate(`document.querySelector('.payment-detail').textContent.includes('待支付')`),`Preview order stays pending ${width}`);
  await screenshot(`recharge-order-${width}`);
  await ready('.payment-window-status');
  check(await evaluate(`!document.querySelector('.app-shell').inert&&!document.querySelector('.window-titlebar').inert&&!document.querySelector('.payment-webview-panel')`),`Auto payment window leaves console interactive ${width}`);
  await screenshot(`payment-window-status-${width}`);
  await mutate(`state.activePage='overview';`);
  check(await evaluate(`!document.querySelector('.app-shell').inert&&Boolean(document.querySelector('.payment-window-status'))`),`Independent payment survives console navigation ${width}`);
  await mutate(`state.activePage='recharge';`);
  await evaluate(`[...document.querySelectorAll('.payment-detail-actions button')].find(b=>b.textContent.includes('打开支付窗口')).click()`);
  check(await evaluate(`document.querySelectorAll('.payment-window-status').length===1&&!document.querySelector('.app-shell').inert`),`Reopen payment reuses independent session ${width}`);
  await evaluate(`[...document.querySelectorAll('.payment-detail-actions button')].find(b=>b.textContent.includes('取消订单')).click()`);
  await waitFor(`document.querySelector('.payment-detail').textContent.includes('已取消')`,'Cancel pending preview order');
  await evaluate(`[...document.querySelectorAll('.payment-tabs button')].find(b=>b.textContent==='余额充值').click()`);
  await ready('#payment-panel-balance .payment-summary');
  await evaluate(`(async()=>{const loaded=performance.getEntriesByType('resource').find(e=>new URL(e.name).pathname==='/src/payments.ts');if(!loaded)throw new Error('Payment module resource missing');const api=(await import(loaded.name)).paymentApi;window.__paymentTestApi=api;window.__originalPaymentCreate=api.create;api.create=async()=>{throw new Error('模拟网络失败，请核对订单');};})()`);
  await evaluate(`document.querySelector('.payment-summary .n-button').click()`);
  await waitFor(`document.querySelector('.recharge-panel').textContent.includes('下单结果未确认')&&document.querySelector('.recharge-panel [role="alert"]')?.textContent.includes('模拟网络失败')`,'Unknown result preserves error and asks to inspect orders');
  await evaluate(`[...document.querySelectorAll('.payment-tabs button')].find(b=>b.textContent==='余额充值').click()`);
  await ready('#payment-panel-balance .payment-summary');
  check(await evaluate(`document.querySelector('.payment-summary .n-button').disabled`),`Unknown result blocks duplicate order ${width}`);
  await evaluate(`window.__paymentTestApi.create=window.__originalPaymentCreate;delete window.__originalPaymentCreate;delete window.__paymentTestApi;`);
  await evaluate(`[...document.querySelectorAll('.recharge-panel button')].find(b=>b.textContent.includes('已核对，允许重新下单')).click()`);
  report.push({page:'native-keys-and-payments',width,height,paymentWindow:'standalone'});
  await select(0);
}

async function verifyCompactSidebar(width, height) {
  const toggle = `.sidebar-toggle`;
  const expanded = await evaluate(`(() => {
    const rect = (el) => { const r = el.getBoundingClientRect(); return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height}; };
    const projectBar = document.querySelector('.sidebar-project-bar');
    const toggle = document.querySelector('.sidebar-toggle');
    const brand = document.querySelector('.sidebar-project-brand');
    const titlebar = document.querySelector('.window-titlebar');
    const logo = brand.querySelector('img');
    const brandText = brand.querySelector('strong');
    return {
      titlebar: rect(titlebar), titlebarText:titlebar.innerText.trim(), windowControls:titlebar.querySelectorAll('.window-titlebar__controls button').length,
      projectBar: rect(projectBar),
      projectBarText: projectBar.textContent.trim(),
      toggle: rect(toggle), toggleBackground: getComputedStyle(toggle).backgroundColor,
      brand: {rect:rect(brand),display:getComputedStyle(brand).display,text:brand.textContent.trim(),logo:rect(logo),textDisplay:getComputedStyle(brandText).display},
      nav: [...document.querySelectorAll('.nav-item')].map((el) => ({rect:rect(el),icon:rect(el.querySelector('.app-icon'))})),
      settings: rect(document.querySelector('.sidebar-settings')),
    };
  })()`);
  await evaluate(`document.querySelector(${JSON.stringify(toggle)}).focus(); document.querySelector(${JSON.stringify(toggle)}).click();`);
  await pause(240);
  const compact = await evaluate(`(() => {
    const sidebar = document.querySelector('.sidebar');
    const toggle = document.querySelector('.sidebar-toggle');
    const projectBar = document.querySelector('.sidebar-project-bar');
    const brand = document.querySelector('.sidebar-project-brand');
    const titlebar = document.querySelector('.window-titlebar');
    const logo = brand.querySelector('img');
    const brandText = brand.querySelector('strong');
    const nav = [...document.querySelectorAll('.nav-item')];
    const settings = document.querySelector('.sidebar-settings');
    const rect = el => { const r = el.getBoundingClientRect(); return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height}; };
    return {
      classApplied: document.querySelector('.app-shell').classList.contains('app-shell--sidebar-collapsed'),
      titlebar: rect(titlebar), titlebarText:titlebar.innerText.trim(), windowControls:titlebar.querySelectorAll('.window-titlebar__controls button').length,
      sidebar: rect(sidebar), toggle: rect(toggle), togglePressed: toggle.getAttribute('aria-pressed'), toggleLabel: toggle.getAttribute('aria-label'), toggleBackground:getComputedStyle(toggle).backgroundColor,
      projectBar: rect(projectBar), projectBarText: projectBar.textContent.trim(),
      brand: {display:getComputedStyle(brand).display,text:brand.textContent.trim(),logo:rect(logo),textDisplay:getComputedStyle(brandText).display},
      nav: nav.map(el => ({rect:rect(el), icon:rect(el.querySelector('.app-icon')), label:el.getAttribute('aria-label'), textHidden:getComputedStyle(el.querySelector('span')).display === 'none', iconVisible:el.querySelector('.app-icon').getBoundingClientRect().width > 0})),
      settings: {rect:rect(settings), label:settings.getAttribute('aria-label'), textHidden:getComputedStyle(settings.querySelector('span')).display === 'none'},
      horizontalOverflow: document.documentElement.scrollWidth > innerWidth || document.querySelector('.workspace').scrollWidth > document.querySelector('.workspace').clientWidth,
    };
  })()`);
  check(compact.classApplied && compact.sidebar.width === 68 && compact.togglePressed === 'true' && compact.toggleLabel === '展开菜单栏', `Compact sidebar state ${width}`);
  check(expanded.titlebar.height === 32 && expanded.titlebarText === '' && expanded.windowControls === 3 && compact.titlebarText === '' && compact.windowControls === 3 && expanded.toggle.y >= expanded.titlebar.y && expanded.toggle.bottom <= expanded.titlebar.bottom, `Custom titlebar controls and content ${width}`);
  check(expanded.projectBarText === 'MollyCloud' && expanded.brand.display === 'flex' && expanded.brand.text === 'MollyCloud' && expanded.brand.textDisplay !== 'none' && compact.brand.display === 'flex' && compact.brand.textDisplay === 'none' && expanded.toggleBackground === 'rgba(0, 0, 0, 0)' && compact.toggleBackground === 'rgba(0, 0, 0, 0)', `Sidebar project bar content ${width}`);
  check(compact.nav.length === navCount && compact.nav.every((item, index) => item.rect.height === 42 && item.label && item.textHidden && item.iconVisible && Math.abs(item.rect.y - expanded.nav[index].rect.y) < 0.1) && Math.abs(compact.settings.rect.y - expanded.settings.y) < 0.1 && compact.settings.label === '设置' && compact.settings.textHidden && compact.settings.rect.bottom <= height && !compact.horizontalOverflow, `Compact sidebar icons, fixed vertical positions, and bounds ${width}`);
  const expandedIconCenter = expanded.nav[0].icon.x + expanded.nav[0].icon.width / 2;
  const compactIconCenter = compact.nav[0].icon.x + compact.nav[0].icon.width / 2;
  check(Math.abs(expanded.brand.logo.x + expanded.brand.logo.width / 2 - expandedIconCenter) <= 1, `Expanded sidebar brand aligns with navigation icons ${width}`);
  check(Math.abs(compact.brand.logo.x + compact.brand.logo.width / 2 - compactIconCenter) <= 1, `Compact sidebar brand aligns with navigation icons ${width}`);
  check(Math.abs(compact.toggle.x - expanded.toggle.x) < 0.1, `Titlebar project control keeps its horizontal position while collapsing ${width}`);
  check(Math.abs(compact.toggle.y - expanded.toggle.y) < 0.1, `Project control keeps its vertical position while collapsing ${width}`);
  await page('Input.dispatchMouseEvent', { type:'mouseMoved', x:compact.toggle.x + compact.toggle.width / 2, y:compact.toggle.y + compact.toggle.height / 2 });
  await pause(180);
  check(await evaluate(`getComputedStyle(document.querySelector('.sidebar-toggle')).backgroundColor !== 'rgba(0, 0, 0, 0)'`), `Sidebar toggle background appears only on pointer hover ${width}`);
  await screenshot(`sidebar-compact-${width}`);
  await key(' ', 'Space', 32);
  check(await evaluate(`!document.querySelector('.app-shell').classList.contains('app-shell--sidebar-collapsed') && document.querySelector('.sidebar-toggle').getAttribute('aria-pressed') === 'false'`), `Compact sidebar keyboard restore ${width}`);
  report.push({ page:'sidebar-compact', width, expanded, ...compact });
}
async function verifySidebarScrollCues(width, height) {
  const navRect = await evaluate(`(() => {const r=document.querySelector('.sidebar nav').getBoundingClientRect();return {x:r.x+10,y:r.y+10};})()`);
  await page('Input.dispatchMouseEvent', {type:'mouseMoved', x:width-10, y:height-10});
  const hidden = await evaluate(`getComputedStyle(document.querySelector('.sidebar nav'),'::-webkit-scrollbar-thumb').backgroundColor`);
  await page('Input.dispatchMouseEvent', {type:'mouseMoved', ...navRect});
  const shown = await evaluate(`getComputedStyle(document.querySelector('.sidebar nav'),'::-webkit-scrollbar-thumb').backgroundColor`);
  check(hidden === 'rgba(0, 0, 0, 0)' && shown !== hidden, `Sidebar scrollbar appears only on hover ${width}`);
  check(await evaluate(`!document.querySelector('.sidebar-scroll-cue') && getComputedStyle(document.querySelector('.sidebar nav'),'::-webkit-scrollbar').width === '6px' && getComputedStyle(document.querySelector('.sidebar nav'),'::-webkit-scrollbar-button').display === 'none'`), `Thin scrollbar replaces arrows ${width}`);
  report.push({page:'sidebar-hover-scrollbar',width,height,hidden,shown});
}
const readShell = `(() => {
  const rect = el => { const r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(n => Math.round(n * 100) / 100); };
  const css = (el, names) => Object.fromEntries(names.map(name => [name, getComputedStyle(el)[name]]));
  const workspace = document.querySelector('.workspace');
  const heading = document.querySelector('.page-heading');
  const topbar = document.querySelector('.topbar');
  const active = document.querySelector('.nav-item.active');
  const content = [...document.querySelectorAll('.page-content')].find(el => el.getBoundingClientRect().width > 0);
  return {
    titlebar: rect(document.querySelector('.window-titlebar')),
    workspace: rect(workspace),
    shell: {
      sidebar: rect(document.querySelector('.sidebar')),
      nav: [...document.querySelectorAll('.nav-item')].map(el => rect(el)),
      settingsButton: rect(document.querySelector('.sidebar-settings')),
      themeButtonAbsent: !document.querySelector('.sidebar-theme'),
      settingsStyle: css(document.querySelector('.sidebar-settings'), ['fontFamily','fontSize','fontWeight','height','borderRadius','padding','backgroundColor']),
      navStyle: css(active, ['fontFamily','fontSize','fontWeight','borderRadius','padding','backgroundColor','boxShadow','borderLeftWidth']),
      titlebarStyle: css(document.querySelector('.window-titlebar'), ['backgroundColor','borderBottomWidth']),
      sidebarStyle: css(document.querySelector('.sidebar'), ['padding','backgroundColor','borderRightWidth']),
      workspaceStyle: css(workspace, ['backgroundColor','borderTopLeftRadius','borderTopRightRadius']),
    },
    header: topbar ? rect(topbar) : null,
    headerTitlesAbsent: !heading && !document.querySelector('.topbar h1,.topbar p'),
    actions: topbar ? rect(document.querySelector('.topbar-actions')) : null,
    accountLayout: topbar ? (() => { const balance=rect(document.querySelector('.balance-chip')); const user=rect(document.querySelector('.user-chip')); const refresh=rect(document.querySelector('.account-refresh-button')); return {balance,user,refresh}; })() : null,
    content: rect(content),
    contentPadding: getComputedStyle(content).paddingTop,
    horizontalOverflow: document.documentElement.scrollWidth > innerWidth || workspace.scrollWidth > workspace.clientWidth,
    headerClipped: heading ? heading.querySelector('h1').scrollWidth > heading.querySelector('h1').clientWidth : false,
    selectedCount: document.querySelectorAll('.nav-item[aria-current]').length,
    navCount: document.querySelectorAll('.nav-item').length,
    settingsVisible: (() => {
      const button = document.querySelector('.sidebar-settings').getBoundingClientRect();
      const sidebar = document.querySelector('.sidebar').getBoundingClientRect();
      const nav = document.querySelector('.sidebar nav').getBoundingClientRect();
      return button.width > 0 && button.height >= 40 && button.x >= sidebar.x && button.right <= sidebar.right && button.y >= nav.bottom && button.bottom <= innerHeight && button.bottom >= innerHeight - 60;
    })(),
    cardBorders: [...document.querySelectorAll('.stat-card, .panel, .table-panel, .usage-stat, .key-record')].map(el => css(el, ['borderTopColor','borderRightColor','borderBottomColor','borderLeftColor','borderWidth','borderRadius'])),
    contentFont: getComputedStyle(content).fontFamily,
    valueOverflows: [...document.querySelectorAll('.usage-stat__value, .token-category dd')].filter(el => el.scrollWidth > el.clientWidth).length,
    keyOverflow: [...document.querySelectorAll('.key-directory, .key-record, .ccs-import-actions')].some(el => el.scrollWidth > el.clientWidth + 1),
  };
})()`;

const visibleSettings = `(() => { const el = document.querySelector('.console-settings-dialog'); return Boolean(el && el.getBoundingClientRect().height > 0); })()`;
const readPreviewSettings = `localStorage.getItem('mollycloud:preview:console-settings')`;
async function openSettings() {
  await evaluate(`(() => { const button = document.querySelector('.sidebar-settings'); button.focus(); button.click(); })()`);
  await waitFor(visibleSettings, 'Console settings opened');
  await settleAnimations();
}
async function clickSettingsButton(text) {
  await evaluate(`(() => { const button = [...document.querySelector('.console-settings-dialog').querySelectorAll('button')].find(el => el.innerText.trim() === ${JSON.stringify(text)}); if (!button) throw new Error('Settings action not found'); button.click(); })()`);
  await waitFor(`!${visibleSettings}`, `Console settings ${text}`);
}
async function verifyConsoleSettings(width, height) {
  const initialStorage = await evaluate(readPreviewSettings);
  await openSettings();
  const layout = await evaluate(`(() => {
    const dialog = document.querySelector('.console-settings-dialog');
    const r = dialog.getBoundingClientRect();
    const mask = document.querySelector('.console-settings-backdrop') || document.querySelector('.n-modal-mask');
    const maskStyle = mask && getComputedStyle(mask);
    const maskRect = mask?.getBoundingClientRect();
    const buttons = [...dialog.querySelectorAll('button')].filter(el => ['取消','保存'].includes(el.innerText.trim())).map(el => { const b = el.getBoundingClientRect(); return {text:el.innerText.trim(),x:b.x,y:b.y,right:b.right,bottom:b.bottom,width:b.width,height:b.height}; });
    const role = dialog.matches('[role="dialog"]') ? dialog : dialog.closest('[role="dialog"]') || dialog.querySelector('[role="dialog"]');
    const release = dialog.querySelector('.console-settings-release');
    const download = release?.querySelector('.console-settings-download');
    const releaseRect = release?.getBoundingClientRect();
    const downloadRect = download?.getBoundingClientRect();
    return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom,buttons,
      release: release && { text:release.textContent.trim(), x:releaseRect.x, y:releaseRect.y, right:releaseRect.right, bottom:releaseRect.bottom, downloadLabel:download?.getAttribute('aria-label') || download?.textContent.trim(), downloadRight:downloadRect.right },
      role:Boolean(role), focusInside:dialog.contains(document.activeElement),
      focusedElement:document.activeElement?.outerHTML.slice(0,1500),
      ancestors:(() => {const result=[]; for(let el=dialog; el; el=el.parentElement) result.push({tag:el.tagName,id:el.id,class:el.className,inert:el.inert,ariaHidden:el.getAttribute('aria-hidden')}); return result;})(),
      radioLabels:[...dialog.querySelectorAll('input[name="console-close-action"]')].map(el => ({value:el.value,label:el.closest('label')?.innerText,checked:el.checked})),
      compactOptions:[...dialog.querySelectorAll('.console-close-option,.console-settings-toggle')].map(el => ({height:el.getBoundingClientRect().height,y:el.getBoundingClientRect().y})),
      autostart:dialog.querySelector('[aria-label="开机自启动"]')?.getAttribute('aria-checked'),
      autostartMinimized:dialog.querySelector('[aria-label="启动后最小化到${trayName}"]')?.getAttribute('aria-checked'),
      autostartMinimizedDisabled:(() => {const el=dialog.querySelector('[aria-label="启动后最小化到${trayName}"]'); return Boolean(el && (el.matches(':disabled') || el.getAttribute('aria-disabled') === 'true' || el.classList.contains('n-switch--disabled')));})(),
      maskFilter:maskStyle?.backdropFilter || maskStyle?.webkitBackdropFilter,
      shellFilter:getComputedStyle(document.querySelector('.app-shell')).filter,
      maskCoversViewport:Boolean(maskRect && maskRect.x <= 0 && maskRect.y <= 0 && maskRect.right >= innerWidth && maskRect.bottom >= innerHeight),
      outerOverflow:document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight};
  })()`);
  check(layout.x >= 0 && layout.y >= 0 && layout.right <= width && layout.bottom <= height && !layout.outerOverflow, `Settings dialog viewport bounds ${width}`);
  check(layout.maskCoversViewport && [layout.maskFilter,layout.shellFilter].some(value => /blur\((?!0(?:px)?\))/.test(value || '')), `Settings frosted glass backdrop ${width}`);
  check(layout.role && layout.focusInside, `Settings accessible modal focus ${width}`);
  check(layout.radioLabels.length === 2 && layout.radioLabels.some(radio => radio.value === 'quit' && radio.label?.includes('退出程序')) && layout.radioLabels.some(radio => radio.value === 'tray' && radio.label?.includes(`最小化到${trayName}`) && radio.checked), `Settings close action default/options ${width}`);
  check(layout.autostart === 'false', `Settings autostart default ${width}`);
  check(layout.autostartMinimized === 'false' && layout.autostartMinimizedDisabled, `Settings minimized autostart default dependency ${width}`);
  check(layout.compactOptions.length === 4 && layout.compactOptions.every(option => option.height >= 40 && option.height <= 52) && Math.abs(layout.compactOptions[2].y - layout.compactOptions[3].y) < 1, `Close and startup controls use compact rows ${width}`);
  check(layout.buttons.length === 2 && layout.buttons[0].text === '取消' && layout.buttons[1].text === '保存' && layout.buttons.every(button => button.height >= 40 && button.bottom <= layout.bottom && layout.bottom - button.bottom <= 48) && layout.buttons[1].x > layout.buttons[0].x && layout.right - layout.buttons[1].right <= 40, `Settings actions bottom right ${width}`);
  check(layout.release?.text === `v${appVersion}检查更新` && layout.release.downloadLabel === '检查更新' && layout.release.x >= layout.x + 32 && layout.release.downloadRight < layout.buttons[0].x && layout.release.bottom <= layout.bottom, `Settings version and update check ${width}`);
  check(await evaluate(`document.querySelector('.app-shell').inert`), `Settings makes background non-interactive ${width}`);
  for (let index = 0; index < 6; index++) {
    await key('Tab', 'Tab', 9);
    check(await evaluate(`document.querySelector('.console-settings-dialog').contains(document.activeElement)`), `Settings keyboard focus stays in dialog ${width}/${index}`);
  }
  await screenshot(`console-settings-${width}`);

  // Use native radio keyboard interaction, then discard the draft.
  const radioFocus = await evaluate(`(() => { const radio = document.querySelector('input[name="console-close-action"][value="quit"]'); radio.focus(); return {focused:document.activeElement?.outerHTML.slice(0,1500),disabled:radio.disabled,fieldsetDisabled:radio.closest('fieldset').disabled}; })()`);
  await key(' ', 'Space', 32);
  check(await evaluate(`document.querySelectorAll('input[name="console-close-action"]:checked').length === 1 && document.querySelector('input[name="console-close-action"]:checked').value === 'quit'`), `Settings radio keyboard exclusivity ${width}`);
  await clickSettingsButton('取消');
  const cancelled = await evaluate(readPreviewSettings);
  check(cancelled === initialStorage, `Settings cancel preserves saved preference ${width}`);
  check(await evaluate(`document.activeElement === document.querySelector('.sidebar-settings')`), `Settings cancel restores focus ${width}`);

  await openSettings();
  check(await evaluate(`document.querySelector('input[name="console-close-action"]:checked').value === 'tray'`), `Settings cancelled draft discarded ${width}`);
  await evaluate(`document.querySelector('input[name="console-close-action"][value="quit"]').click()`);
  await key('Escape', 'Escape', 27);
  await waitFor(`!${visibleSettings}`, 'Console settings Escape closed');
  check(await evaluate(readPreviewSettings) === initialStorage, `Settings Escape preserves saved preference ${width}`);
  check(await evaluate(`document.activeElement === document.querySelector('.sidebar-settings')`), `Settings Escape restores focus ${width}`);

  await openSettings();
  await evaluate(`document.querySelector('input[name="console-close-action"][value="quit"]').click()`);
  await evaluate(`document.querySelector('[aria-label="开机自启动"]').click()`);
  check(await evaluate(`{const el=document.querySelector('[aria-label="启动后最小化到${trayName}"]'); !el.matches(':disabled') && el.getAttribute('aria-disabled') !== 'true' && !el.classList.contains('n-switch--disabled')}`), `Settings minimized autostart enabled with autostart ${width}`);
  await evaluate(`document.querySelector('[aria-label="启动后最小化到${trayName}"]').click()`);
  await clickSettingsButton('保存');
  const saved = await evaluate(readPreviewSettings);
  check(saved !== initialStorage && Boolean(saved?.includes('quit')) && Boolean(saved?.includes('"autostart":true')) && Boolean(saved?.includes('"autostartMinimized":true')), `Settings preview saves isolated preference ${width}`);
  check(await evaluate(`document.activeElement === document.querySelector('.sidebar-settings')`), `Settings save restores focus ${width}`);
  await openSettings();
  check(await evaluate(`document.querySelector('input[name="console-close-action"]:checked').value === 'quit'`), `Settings saved preference reopened ${width}`);
  check(await evaluate(`document.querySelector('[aria-label="开机自启动"]').getAttribute('aria-checked') === 'true'`), `Settings saved autostart reopened ${width}`);
  check(await evaluate(`{const el=document.querySelector('[aria-label="启动后最小化到${trayName}"]'); el.getAttribute('aria-checked') === 'true' && !el.matches(':disabled') && el.getAttribute('aria-disabled') !== 'true' && !el.classList.contains('n-switch--disabled')}`), `Settings saved minimized autostart reopened ${width}`);
  await clickSettingsButton('取消');

  // Reload only this temporary browser profile. Never send a native close/quit.
  await page('Page.reload', { ignoreCache: true });
  await ready('.overview-page');
  await openSettings();
  const persisted = await evaluate(`document.querySelector('input[name="console-close-action"]:checked').value === 'quit'`);
  check(persisted, `Settings preference survives preview reload ${width}`);
  // Force an actual preview persistence failure to verify long error handling
  // and that the saved value is not replaced by an unsuccessful draft.
  await evaluate(`(() => {
    document.querySelector('input[name="console-close-action"][value="tray"]').click();
    window.__mollySettingsOriginalSetItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key, value) {
      if (key === 'mollycloud:preview:console-settings') throw new DOMException('无法保存设置：' + '存储暂时不可用，请保留当前选择并稍后重试。'.repeat(65), 'QuotaExceededError');
      return window.__mollySettingsOriginalSetItem.call(this, key, value);
    };
    document.querySelector('.console-settings-save').click();
  })()`);
  let errorState;
  try {
    await ready('.console-settings-error');
    errorState = await evaluate(`(() => {
      const dialog = document.querySelector('.console-settings-dialog').getBoundingClientRect();
      const body = document.querySelector('.console-settings-body');
      const footer = document.querySelector('.console-settings-actions').getBoundingClientRect();
      const buttons = [...document.querySelectorAll('.console-settings-actions button')].map(el => {const r = el.getBoundingClientRect(); return {disabled:el.disabled,x:r.x,y:r.y,right:r.right,bottom:r.bottom};});
      body.scrollTop = body.scrollHeight;
      return {x:dialog.x,y:dialog.y,right:dialog.right,bottom:dialog.bottom,footerTop:footer.top,footerBottom:footer.bottom,buttons,
        innerScroll:body.scrollHeight > body.clientHeight && body.scrollTop > 0,
        errorVisible:document.querySelector('.console-settings-error').getBoundingClientRect().bottom <= body.getBoundingClientRect().bottom + 1,
        saveValueUnchanged:localStorage.getItem('mollycloud:preview:console-settings') === ${JSON.stringify(saved)},
        draft:document.querySelector('input[name="console-close-action"]:checked').value,
        backgroundInert:document.querySelector('.app-shell').inert};
    })()`);
    check(errorState.saveValueUnchanged && errorState.draft === 'tray' && errorState.backgroundInert, `Settings save failure preserves persisted value and draft ${width}`);
    check(errorState.x >= 0 && errorState.y >= 0 && errorState.right <= width && errorState.bottom <= height && errorState.innerScroll && errorState.errorVisible && errorState.buttons.every(button => !button.disabled && button.y >= errorState.footerTop && button.bottom <= errorState.footerBottom), `Settings long error scroll keeps footer visible ${width}`);
    await screenshot(`console-settings-save-error-${width}`);
  } finally {
    await evaluate(`Storage.prototype.setItem = window.__mollySettingsOriginalSetItem; delete window.__mollySettingsOriginalSetItem;`);
  }
  await clickSettingsButton('取消');
  check(await evaluate(`!document.querySelector('.app-shell').inert`), `Settings restores background interaction ${width}`);
  report.push({page:'console-settings', viewportWidth:width, viewportHeight:height, ...layout, radioFocus, cancelledWithoutSave:cancelled === initialStorage, saved, persisted, errorState});
}

async function verifyPluginManager(width, height) {
  await select(5);
  await waitFor(`Boolean(document.querySelector('.ccswitch-frame'))`, 'CC plugin mounted');
  await openSettings();
  await evaluate(`document.querySelectorAll('.settings-tabs button')[1].click()`);
  await ready('.plugin-manager');
  await screenshot(`plugin-manager-${width}`);
  const choose = async (id, text) => {
    await evaluate(`(() => { const b=[...document.querySelector('[data-plugin="${id}"]').querySelectorAll('button')].find(b=>b.innerText.trim()===${JSON.stringify(text)});if(!b)throw new Error('Missing plugin action');b.click(); })()`);
    await pause(250);
  };
  await choose('ccswitch', '卸载');
  check(await evaluate(`Boolean(document.querySelector('.ccswitch-frame'))`), `Uninstall confirmation preserves mounted plugin ${width}`);
  await choose('ccswitch', '确认卸载');
  check(await evaluate(`!document.querySelector('.ccswitch-frame') && Boolean(document.querySelector('.plugin-empty'))`), `Uninstalled plugin unmounts iframe ${width}`);
  await choose('ccswitch', '安装随附版本');
  await waitFor(`Boolean(document.querySelector('.ccswitch-frame'))`, 'Plugin restored');
  await choose('images', '卸载'); await choose('images', '确认卸载');
  await clickSettingsButton('完成');
  await select(6);
  check(await evaluate(`Boolean(document.querySelector('.plugin-empty')) && !document.querySelector('.embedded-frame')`), `Image plugin is unavailable after uninstall ${width}`);
  await evaluate(`document.querySelector('.plugin-empty button').click()`);
  await ready('.plugin-manager');
  await choose('images', '安装随附版本');
  await clickSettingsButton('完成');
  await select(0);
  report.push({page:'plugin-lifecycle-preview',width,height});
}
async function verifyPetSettings(width, height) {
  await select(names.indexOf('assistant'));
  const open=async()=>{await evaluate(`document.querySelectorAll('.assistant-tabs button')[1].click()`);await ready('.pet-settings-fields');await settleAnimations();};
  const state=`(() => {let c=document.querySelector('.pet-settings-dialog').__vueParentComponent;while(c&&!('draft' in c.setupState))c=c.parent;return c.setupState;})()`;
  await open();
  check(await evaluate(`!document.querySelector('.app-shell').inert&&!document.querySelector('.n-modal-mask')&&!document.querySelector('.assistant-settings-button')&&getComputedStyle(document.querySelector('.assistant-chat__toolbar')).borderBottomWidth==='0px'`),`Assistant settings are inline with no title, gear or divider ${width}`);
  await evaluate(`document.querySelectorAll('.pet-settings-tabs button')[0].click()`);
  const layout=await evaluate(`(() => {const r=document.querySelector('.pet-settings-dialog').getBoundingClientRect(),body=document.querySelector('.pet-settings-body'),footer=document.querySelector('.pet-settings-dialog .console-settings-actions').getBoundingClientRect();return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,footerBottom:footer.bottom,innerScroll:body.scrollHeight>body.clientHeight,overflow:body.scrollWidth>body.clientWidth+1};})()`);
  check(layout.x>=0&&layout.y>=0&&layout.right<=width&&layout.bottom<=height&&layout.footerBottom<=height&&layout.innerScroll&&!layout.overflow,`Inline settings keep controls within workspace ${width}`);
  await screenshot(`pet-settings-model-${width}`);
  const setScale=async value=>evaluate(`(() => {const el=document.querySelector('input[aria-label="模型大小"]');el.value='${value}';el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await setScale(1.35);
  await evaluate(`document.querySelectorAll('.assistant-tabs button')[0].click()`);await ready('.assistant-chat');
  await open();check(await evaluate(`${state}.draft.modelScale===1.35`),`Assistant tab switch preserves settings draft ${width}`);
  await evaluate(`document.querySelector('.pet-settings-dialog .console-settings-actions button').click()`);await pause(100);
  check(await evaluate(`${state}.draft.modelScale===${state}.snapshot.draft.modelScale`),`Inline cancel restores settings snapshot ${width}`);
  await setScale(1.25);await evaluate(`document.querySelector('.pet-settings-dialog .console-settings-actions button:last-child').click()`);await pause(250);
  check(await evaluate(`JSON.parse(localStorage.getItem('mollycloud:preview:pet-settings')).modelScale===1.25&&document.querySelector('.pet-settings-notice').textContent.includes('保存')`),`Inline save persists settings without closing page ${width}`);
  await evaluate(`document.querySelectorAll('.pet-settings-tabs button')[1].click()`);await ready('#pet-api-key');
  check(await evaluate(`document.querySelector('#pet-api-key').value===''&&document.querySelector('#pet-api-key').type==='password'`),`Pet key remains blank and masked ${width}`);
  await screenshot(`pet-settings-assistant-${width}`);
  await evaluate(`(() => {const el=document.querySelector('#pet-persona');el.value='保存失败时保留草稿';el.dispatchEvent(new Event('input',{bubbles:true}));window.__petOriginalSetItem=Storage.prototype.setItem;Storage.prototype.setItem=function(key,value){if(key==='mollycloud:preview:pet-settings')throw new Error('测试：存储不可用');return window.__petOriginalSetItem.call(this,key,value);};})()`);
  await evaluate(`document.querySelector('.pet-settings-dialog .console-settings-actions button:last-child').click()`);await ready('.pet-settings-dialog .console-settings-error');
  check(await evaluate(`document.querySelector('#pet-persona').value==='保存失败时保留草稿'&&!document.querySelector('.app-shell').inert`),`Inline failure keeps editable draft ${width}`);
  await evaluate(`Storage.prototype.setItem=window.__petOriginalSetItem;delete window.__petOriginalSetItem;document.querySelector('.pet-settings-dialog .console-settings-actions button').click()`);
  await evaluate(`document.querySelectorAll('.assistant-tabs button')[0].click()`);await ready('.assistant-chat');
  report.push({page:'pet-settings',width,height,...layout});
}

async function verifySpeech(width, height) {
  await select(names.indexOf('assistant'));
  await evaluate(`document.querySelectorAll('.assistant-tabs button')[1].click()`); await ready('.pet-settings-fields');
  await evaluate(`document.querySelectorAll('.pet-settings-tabs button')[1].click()`); await ready('.speech-settings');
  const state = `(() => {let c=document.querySelector('.pet-settings-dialog').__vueParentComponent;while(c&&!('draft' in c.setupState))c=c.parent;return c.setupState;})()`;
  const setInput = async (id, value) => evaluate(`(() => {const el=document.getElementById('${id}');el.value=${JSON.stringify(value)};el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  const close = async () => { await evaluate(`document.querySelector('.pet-settings-dialog .console-settings-actions button').click();document.querySelectorAll('.assistant-tabs button')[0].click()`); await ready('.assistant-chat'); };
  const initial = await evaluate(`localStorage.getItem('mollycloud:preview:pet-settings')`);
  check(await evaluate(`${state}.draft.speech.provider==='mimo'&&${state}.draft.speech.voice==='冰糖'&&!${state}.draft.speech.enabled&&document.querySelector('#speech-key').type==='password'&&document.querySelector('#speech-key').value===''&&!document.querySelector('.speech-settings textarea')`), `Speech defaults to disabled MiMo Bing Tang and hides style prompt ${width}`);
  await evaluate(`document.querySelector('.speech-settings').scrollIntoView({block:'center'})`); await pause(160);
  await screenshot(`speech-mimo-settings-${width}`);
  await evaluate(`document.querySelector('.speech-settings .n-base-selection').click()`); await ready('.n-base-select-option');
  await evaluate(`[...document.querySelectorAll('.n-base-select-option')].find(o=>o.textContent.includes('自定义 OpenAI 协议')).click()`); await ready('#speech-base');
  await setInput('speech-base','https://tts.example.com/v1');await setInput('speech-model','tts-custom');await setInput('speech-voice','test-voice');await setInput('speech-key','synthetic-preview-only');
  await evaluate(`[...document.querySelectorAll('.speech-test-actions button')].find(b=>b.textContent.includes('试听')).click()`);await ready('.pet-settings-dialog .console-settings-error');
  check(await evaluate(`document.querySelector('.console-settings-error').textContent.includes('浏览器预览')`), `Speech preview refuses real synthesis ${width}`);
  await evaluate(`document.querySelector('.pet-settings-dialog .console-settings-actions button:last-child').click()`);await pause(250);
  check(await evaluate(`Boolean(document.querySelector('.pet-settings-dialog'))&&localStorage.getItem('mollycloud:preview:pet-settings')===${JSON.stringify(initial)}&&document.querySelector('#speech-key').value==='synthetic-preview-only'&&document.querySelector('.console-settings-error').textContent.includes('真实 API 密钥')`), `Speech key is never persisted by browser preview and failed save keeps draft ${width}`);
  await evaluate(`document.querySelector('.speech-settings').scrollIntoView({block:'end'})`);
  const bounds=await evaluate(`(() => {const d=document.querySelector('.pet-settings-dialog').getBoundingClientRect(),f=document.querySelector('.pet-settings-dialog .console-settings-actions').getBoundingClientRect(),body=document.querySelector('.pet-settings-dialog .console-settings-body');return {width:d.width,x:d.x,right:d.right,bottom:d.bottom,footerBottom:f.bottom,scroll:body.scrollHeight>body.clientHeight,overflow:body.scrollWidth>body.clientWidth+1};})()`);
  check(bounds.x>=0&&bounds.right<=width&&bounds.bottom<=height&&bounds.footerBottom<=height&&bounds.scroll&&!bounds.overflow,`Speech settings preserve fixed footer and internal scroll ${width}`);
  await screenshot(`speech-custom-settings-${width}`);await close('取消');
  check(await evaluate(`localStorage.getItem('mollycloud:preview:pet-settings')===${JSON.stringify(initial)}`),`Speech cancellation discards all draft fields ${width}`);
  await mutate(`state.assistantSpeechStatus={phase:'playing'};`);await screenshot(`assistant-speaking-${width}`);
  check(await evaluate(`document.querySelector('.assistant-speech-status').textContent.includes('停止朗读')&&document.querySelector('.assistant-composer').getBoundingClientRect().bottom<=${height}`),`Speech stop control stays inside composer ${width}`);
  await evaluate(`document.querySelector('.assistant-speech-status button').click()`);await pause(100);check(await evaluate(`!document.querySelector('.assistant-speech-status')`),`Stop speech clears status ${width}`);
  await mutate(`state.assistantSpeechStatus={phase:'error',error:'模拟语音连接失败'};`);await screenshot(`assistant-speech-error-${width}`);check(await evaluate(`document.querySelector('.assistant-speech-status').textContent.includes('模拟语音连接失败')`),`Speech errors remain visible and separate from chat errors ${width}`);
  await mutate(`state.assistantSpeechStatus={phase:'idle'};`);
  report.push({page:'online-speech',width,height,...bounds});
}
async function verifyMotion() {
  await page('Emulation.setDeviceMetricsOverride', {width:1180,height:760,deviceScaleFactor:1,mobile:false});
  await page('Page.navigate', {url:`${baseUrl}/?ui-preview=console`});
  await ready('.overview-page');
  for (const id of ['keys','assistant','keys','overview']) {
    await evaluate(`document.querySelector('.nav-item[data-page="${id}"]').click()`);
    await pause(60);
  }
  await settleAnimations();
  const aligned = await evaluate(`(() => {const a=document.querySelector('.nav-item.active').getBoundingClientRect();const b=document.querySelector('.sidebar .selection-indicator').getBoundingClientRect();return Math.abs(a.x-b.x)<1 && Math.abs(a.y-b.y)<1 && Math.abs(a.width-b.width)<1 && Math.abs(a.height-b.height)<1;})()`);
  check(aligned, 'Jelly indicator settles on latest selection after rapid clicks');
  await evaluate(`document.querySelector('.overview-subnav [data-page="usage"]').click()`);
  await pause(40);
  check(await evaluate(`document.querySelector('.overview-subnav .selection-indicator').getAnimations().length > 0`), 'Overview underline moves during selection');
  await settleAnimations();
  check(await evaluate(`Boolean(document.querySelector('.usage-page')) && !document.querySelector('.overview-page')`), 'Overview transition settles on selected content');
  const tabStyle = `(() => { const s=getComputedStyle(document.querySelector('.overview-subnav [aria-current="page"]'));return [s.fontSize,s.fontWeight,s.color,s.backgroundColor,s.paddingTop,s.paddingBottom]; })()`;
  const standard = await evaluate(tabStyle);
  if (isWindows) {
  await select(names.indexOf('netspeed'));
  await evaluate(`document.querySelectorAll('.island-tabs button')[1].click()`);
  await pause(40);
  check(await evaluate(`document.querySelector('.island-tabs .selection-indicator').getAnimations().some(a=>a.playState==='running')`), 'Island underline animates');
  for (const index of [2,0,1]) {
    await evaluate(`document.querySelectorAll('.island-tabs button')[${index}].click()`);
    await pause(40);
  }
  await settleAnimations();
  check(await evaluate(`[...document.querySelectorAll('.island-mode-grid button')].some(b=>b.textContent==='系统时间')&&getComputedStyle(document.querySelector('.island-section')).transform==='none'`), 'Island rapid switching settles and releases animation transform');
  const islandStyle=await evaluate(tabStyle.replace('.overview-subnav','.island-tabs'));
  check(JSON.stringify(islandStyle)===JSON.stringify(standard),'Island tabs match overview');
  }
  await select(names.indexOf('assistant'));
  await evaluate(`document.querySelectorAll('.assistant-tabs button')[1].click()`);await pause(40);
  check(await evaluate(`document.querySelector('.assistant-tabs .selection-indicator').getAnimations().some(a=>a.playState==='running')`),'Assistant tabs use shared sliding underline');
  for(const index of [0,1,0]){await evaluate(`document.querySelectorAll('.assistant-tabs button')[${index}].click()`);await pause(40);}
  await settleAnimations();check(await evaluate(`Boolean(document.querySelector('.assistant-chat'))&&!document.querySelector('.pet-settings-frame')&&getComputedStyle(document.querySelector('.assistant-chat')).transform==='none'`),'Assistant rapid switch shows final requested panel');
  await select(names.indexOf('skills'));
  await waitFor(`!!document.querySelector('.skills-frame')?.contentDocument.querySelector('.molly-skills-tabs')`,'Skill tabs ready for motion');
  const skillsEval=code=>evaluate(`(()=>{const document=window.document.querySelector('.skills-frame').contentDocument;const getComputedStyle=document.defaultView.getComputedStyle.bind(document.defaultView);return (${code});})()`);
  for (const route of ['/install','/backup','/my-skills']) {
    await skillsEval(`document.querySelector('a[href="#${route}"]').click()`);
    await pause(40);
  }
  await pause(170);
  check(await skillsEval(`document.querySelector('.selection-indicator').getAnimations().some(a=>a.playState==='running')`),'Skill underline animates');
  await pause(300);
  check(await skillsEval(`document.defaultView.location.hash==='#/my-skills'&&getComputedStyle(document.querySelector('.molly-skills-content')).transform==='none'`),'Skill rapid switching shows last request and releases transform');
  check(JSON.stringify(await skillsEval(tabStyle.replace('.overview-subnav','.molly-skills-tabs')))===JSON.stringify(standard),'Skill tabs match overview');
  await skillsEval(`document.querySelector('a[href="#/backup"]').click()`);await pause(30);
  await skillsEval(`document.querySelector('a[href="#/my-skills"]').click()`);await pause(350);
  check(await skillsEval(`document.defaultView.location.hash==='#/my-skills'&&getComputedStyle(document.querySelector('.molly-skills-content')).transform==='none'`),'Selecting current Skill tab cancels pending navigation cleanly');
  await select(names.indexOf('recharge')); await ready('#payment-panel-balance .payment-amounts');
  await evaluate(`document.querySelector('#payment-tab-subscription').click()`); await pause(30);
  check(await evaluate(`document.querySelector('.payment-tabs .selection-indicator').getAnimations().some(a=>a.playState==='running'&&a.effect.getTiming().duration===460)`), 'Recharge selection uses shared sliding pill motion');
  check(await evaluate(`document.querySelector('.payment-tab-content').getAnimations().some(a=>a.playState==='running'&&a.effect.getTiming().duration===100)`), 'Recharge content uses shared fade-out duration');
  await pause(95);
  check(await evaluate(`document.querySelector('#payment-panel-subscription')?.getAnimations().some(a=>a.playState==='running'&&a.effect.getTiming().duration===240)`), 'Recharge content uses shared fade-in duration');
  for (const tab of ['orders','balance','subscription','orders']) {
    await evaluate(`document.querySelector('#payment-tab-${tab}').click()`); await pause(30);
  }
  await settleAnimations();
  check(await evaluate(`document.querySelectorAll('.payment-tab-content').length===1&&Boolean(document.querySelector('#payment-panel-orders'))&&getComputedStyle(document.querySelector('.payment-tab-content')).transform==='none'`), 'Recharge rapid switching settles on the last tab and releases transform');
  check(await evaluate(`(() => {const a=document.querySelector('.payment-tabs [aria-selected="true"]').getBoundingClientRect(),b=document.querySelector('.payment-tabs .selection-indicator').getBoundingClientRect();return Math.abs(a.x-b.x)<1&&Math.abs(a.y-b.y)<1&&Math.abs(a.width-b.width)<1&&Math.abs(a.height-b.height)<1;})()`), 'Recharge indicator aligns after rapid switching');
  await screenshot('recharge-orders-motion');
  await page('Emulation.setEmulatedMedia', {features:[{name:'prefers-reduced-motion',value:'reduce'}]});
  for (const tab of ['subscription','balance','orders']) {
    await evaluate(`document.querySelector('#payment-tab-${tab}').click()`); await pause(40);
    check(await evaluate(`Boolean(document.querySelector('#payment-panel-${tab}'))&&document.querySelector('.payment-tab-content').getAnimations().every(a=>a.playState!=='running')&&document.querySelector('.payment-tabs .selection-indicator').getAnimations().every(a=>a.playState!=='running')`), `Recharge ${tab} tab respects reduced motion`);
  }
  await select(names.indexOf('assistant'));await evaluate(`document.querySelectorAll('.assistant-tabs button')[1].click()`);await pause(40);
  check(await evaluate(`Boolean(document.querySelector('.pet-settings-frame'))&&document.querySelector('.assistant-tabs .selection-indicator').getAnimations().every(a=>a.playState!=='running')`),'Assistant tabs respect reduced motion');
  await select(names.indexOf('skills'));
  await skillsEval(`document.querySelector('a[href="#/backup"]').click()`);await pause(40);
  check(await skillsEval(`document.defaultView.location.hash==='#/backup'&&document.getAnimations().every(a=>a.playState!=='running')`),'Skill tabs respect reduced motion');
  if (isWindows) {
  await select(names.indexOf('netspeed'));
  await evaluate(`document.querySelectorAll('.island-tabs button')[0].click()`);await pause(40);
  check(await evaluate(`!!document.querySelector('.island-metrics')&&document.querySelector('.island-tabs .selection-indicator').getAnimations().every(a=>a.playState!=='running')`),'Island tabs respect reduced motion');
  }
  await select(2);
  check(await evaluate(`document.querySelector('.sidebar .selection-indicator').getAnimations().filter(a=>a.playState==='running').length === 0`), 'Reduced motion disables jelly animation');
  await select(0);
  await openSettings();
  await evaluate(`document.querySelector('.console-settings-body').scrollTop=0`);
  await screenshot('settings-appearance-preview');
  report.push({page:'navigation-motion',rapidSelectionAligned:aligned,reducedMotion:true});
}
async function verifyAppearance() {
  await page('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  await page('Emulation.setDeviceMetricsOverride', { width: 1180, height: 760, deviceScaleFactor: 1, mobile: false });
  await page('Page.navigate', { url: `${baseUrl}/?ui-preview=console` });
  await ready('.overview-page');
  const rootTheme = `document.documentElement.dataset.theme`;
  const pref = `document.documentElement.dataset.themePreference`;
  const system = async (value) => {
    await page('Emulation.setEmulatedMedia', { features: value ? [{ name: 'prefers-color-scheme', value }] : [] });
    await pause(300);
  };
  check(await evaluate(`${pref} === 'system' && ${rootTheme} === 'light'`), 'Default appearance follows system');
  await system('dark');
  check(await evaluate(`${rootTheme} === 'dark'`), 'System change applies immediately');
  check(await evaluate(`!document.querySelector('.sidebar-theme') && document.querySelectorAll('.sidebar-footer button').length === 1`), 'Settings is the only sidebar footer button');
  await openSettings();
  await evaluate(`document.querySelector('input[name="console-theme"][value="light"]').focus()`);
  await key(' ', 'Space', 32);
  await clickSettingsButton('保存');
  check(await evaluate(`${rootTheme} === 'light' && ${pref} === 'light'`), 'Keyboard selection in settings saves explicit light');
  await system('light');
  await system('dark');
  check(await evaluate(`${rootTheme} === 'light'`), 'Explicit light overrides dark system');
  await page('Page.reload', { ignoreCache: true });
  await ready('.overview-page');
  check(await evaluate(`${rootTheme} === 'light' && ${pref} === 'light'`), 'Theme preference survives reload');
  await openSettings();
  await evaluate(`document.querySelector('input[name="console-theme"][value="dark"]').click()`);
  check(await evaluate(`${rootTheme} === 'light'`), 'Theme remains a draft before save');
  await clickSettingsButton('取消');
  await openSettings();
  check(await evaluate(`document.querySelector('input[name="console-theme"]:checked').value === 'light'`), 'Cancel discards theme draft');
  await evaluate(`document.querySelector('input[name="console-theme"][value="system"]').click()`);
  await key('Escape', 'Escape', 27);
  await waitFor(`!${visibleSettings}`, 'Theme settings Escape');
  check(await evaluate(`${pref} === 'light'`), 'Escape preserves theme');
  await openSettings();
  await evaluate(`document.querySelector('input[name="console-theme"][value="system"]').click()`);
  await clickSettingsButton('保存');
  check(await evaluate(`${rootTheme} === 'dark' && ${pref} === 'system'`), 'Saved system preference resolves current OS theme');
  await system('light');
  check(await evaluate(`${rootTheme} === 'light'`), 'System tracking resumes after settings save');

  // Stop emulating media before checking inherited iframe color-scheme: the
  // browser override otherwise forces iframe media queries as well.
  await system(null);
  await openSettings();
  await evaluate(`document.querySelector('input[name="console-theme"][value="dark"]').click()`);
  await clickSettingsButton('保存');
  await select(5);
  await waitFor(`!document.querySelector('.ccswitch-load-state') && Boolean(document.querySelector('.ccswitch-frame')?.contentDocument.querySelector('[data-provider-id="molly-preview-provider"]'))`, 'CC Switch appearance ready');
  await evaluate(`document.querySelector('.ccswitch-frame').contentWindow.__appearanceProbe = 42`);
  await select(7);
  await waitFor(`Boolean(document.querySelector('.skills-frame')?.contentDocument.querySelector('.molly-skills-tabs'))`, 'Skill Manager appearance ready');
  await evaluate(`document.querySelector('.skills-frame').contentWindow.__appearanceProbe = 42`);
  await select(6);
  await waitFor(`!document.querySelector('.embedded-load-state')`, 'Image appearance ready');
  let imageContext;
  for (const context of contexts.values()) {
    if (context.auxData?.isDefault && await evaluate(`location.href.includes('/image-workbench/')`, context).catch(() => false)) imageContext = context;
  }
  if (!imageContext) throw new Error('Image sandbox context not found');
  await evaluate(`window.__appearanceProbe = 42`, imageContext);
  for (const theme of ['dark', 'light', 'dark']) {
    if (await evaluate(rootTheme) !== theme) {
      await openSettings();
      await evaluate(`document.querySelector('input[name="console-theme"][value="${theme}"]').click()`);
      await clickSettingsButton('保存');
    }
    await pause(350);
    const image = await evaluate(`({ dark: matchMedia('(prefers-color-scheme: dark)').matches, background: getComputedStyle(document.documentElement).getPropertyValue('--background').trim(), probe: window.__appearanceProbe, parentBlocked: (()=>{try{return !parent.document}catch{return true}})(), ipc: typeof window.__TAURI_INTERNALS__ })`, imageContext);
    const cc = await evaluate(`(() => { const f=document.querySelector('.ccswitch-frame'); return {dark:f.contentDocument.documentElement.classList.contains('dark'),probe:f.contentWindow.__appearanceProbe}; })()`);
    const skills = await evaluate(`(() => { const f=document.querySelector('.skills-frame'); return {theme:f.contentDocument.documentElement.dataset.theme,probe:f.contentWindow.__appearanceProbe}; })()`);
    check(image.dark === (theme === 'dark') && cc.dark === (theme === 'dark') && skills.theme === theme, `All embedded themes follow ${theme}`);
    check(image.probe === 42 && cc.probe === 42 && skills.probe === 42, `Theme ${theme} preserves embedded state`);
    check(image.parentBlocked && image.ipc === 'undefined', 'Image sandbox remains isolated');
    report.push({page:'embedded-appearance',theme,image,cc,skills});
    await screenshot(`images-${theme}`);
    await select(5);
    await screenshot(`ccswitch-${theme}`);
    const ccChild = expression => evaluate(`(()=>{const document=window.document.querySelector('.ccswitch-frame').contentDocument;const getComputedStyle=document.defaultView.getComputedStyle.bind(document.defaultView);return (${expression});})()`);
    await ccChild(`document.querySelector('[data-provider-id="molly-preview-provider"] button[aria-label="编辑"]').click()`);
    await waitFor(`!!document.querySelector('.ccswitch-frame').contentDocument.querySelector('#provider-form')`, 'CC caret editor ready');
    await ccChild(`(()=>{const notice=document.querySelector('[role="dialog"]');if(notice)[...notice.querySelectorAll('button')].find(b=>b.textContent==='我知道了')?.click();})()`);
    await ccChild(`document.querySelector('#provider-form .cm-content[contenteditable="true"]').focus()`);
    await pause(100);
    check(await ccChild(`getComputedStyle(document.querySelector('#provider-form input')).caretColor==='rgb(0, 0, 0)'&&getComputedStyle(document.querySelector('#provider-form .cm-cursor')).borderLeftColor==='rgb(0, 0, 0)'&&getComputedStyle(document.querySelector('#provider-form .cm-content')).caretColor==='rgba(0, 0, 0, 0)'`), `CC inputs and custom code caret stay black in ${theme}`);
    await ccChild(`document.querySelector('#provider-form').closest('.fixed').querySelector('button[aria-label="返回"]').click()`);
    await waitFor(`!document.querySelector('.ccswitch-frame').contentDocument.querySelector('#provider-form')`, 'CC caret editor dismissed');
    await select(6);
  }
  await select(0);
  const colors = await evaluate(`({ background:getComputedStyle(document.body).backgroundColor, text:getComputedStyle(document.body).color })`);
  check(colors.background === 'rgb(34, 37, 31)' && colors.text === 'rgb(238, 241, 233)', 'Dark shared tokens applied');
  await screenshot('overview-dark');
  await openSettings();
  check(await evaluate(`document.querySelector('input[name="console-theme"]:checked').value === 'dark'`), 'Settings reflects the saved appearance');
  await screenshot('settings-dark');
  await clickSettingsButton('取消');
  // A failed write keeps the theme and the editable draft in the open dialog.
  await openSettings();
  await evaluate(`document.querySelector('input[name="console-theme"][value="light"]').click(); window.__appearanceSetItem = Storage.prototype.setItem; Storage.prototype.setItem = function(k,v) { if(k === 'mollycloud:preview:appearance') throw new DOMException('存储不可用','QuotaExceededError'); return window.__appearanceSetItem.call(this,k,v); }; [...document.querySelector('.console-settings-dialog').querySelectorAll('button')].find(el => el.innerText.trim() === '保存').click();`);
  await waitFor(`Boolean(document.querySelector('.console-settings-error'))`, 'Theme write error');
  check(await evaluate(`${rootTheme} === 'dark' && document.querySelector('input[name="console-theme"]:checked').value === 'light'`), 'Failed theme write preserves current mode and draft');
  await evaluate(`Storage.prototype.setItem = window.__appearanceSetItem; delete window.__appearanceSetItem;`);
  await clickSettingsButton('保存');
  check(await evaluate(`${rootTheme} === 'light' && !document.querySelector('.console-settings-error')`), 'Theme write retry succeeds');
  report.push({page:'appearance-behavior',colors});
}

async function verifyIslandClock() {
  await page('Emulation.setDeviceMetricsOverride',{width:720,height:240,deviceScaleFactor:1,mobile:false});
  await page('Page.navigate',{url:`${baseUrl}/scripts/fixtures/island-clock.html`});
  await ready('.island-container');
  await waitFor(`window.islandClockFixture.width===150&&getComputedStyle(document.querySelector('.island-container')).display!=='none'`,'Island ready without clock');
  await pause(700);
  const event=(name,payload)=>evaluate(`window.islandClockFixture.emit('molly-netspeed:${name}',${JSON.stringify(payload)})`);
  const mode=(mode,slots=['speed','resource',null])=>event('control-mode',{mode,slots});
  await mode('time');
  await ready('.time-box .system-clock');
  check(await evaluate(`window.islandClockFixture.width===150&&/^[0-9]{2}:[0-9]{2}:[0-9]{2}$/.test(document.querySelector('.system-clock').textContent.trim())`),'Standalone clock uses base width and local time');
  check(await evaluate(`!document.querySelector('.speed-box')&&document.querySelector('.inner-wrapper').contains(document.querySelector('.system-clock'))`),'Time is standalone content');
  const before=await evaluate(`document.querySelector('.system-clock').textContent`);
  await waitFor(`document.querySelector('.system-clock').textContent!==${JSON.stringify(before)}`,'Clock ticks');
  await screenshot('clock-only');
  for(const name of ['speed','resource','fps']) {
    await mode(name);await pause(400);
    check(await evaluate(`!document.querySelector('.system-clock')&&window.islandClockFixture.width===${name==='resource'?260:150}`),`Switching to ${name} replaces time`);
  }
  await mode('custom',['time','speed','resource']);
  await ready('.custom-slot-item.is-time .system-clock');await pause(400);
  check(await evaluate(`(()=>{const box=document.querySelector('.island-container').getBoundingClientRect(),items=[...document.querySelectorAll('.custom-slot-item')].map(el=>el.getBoundingClientRect());return window.islandClockFixture.width===260&&items.length===3&&items.every(r=>r.left>=box.left&&r.right<=box.right)&&items.every((r,i)=>!i||r.left>=items[i-1].right);})()`),'Time custom slot shares width without overlap');
  await screenshot('clock-custom');
  await mode('time');await ready('.time-box');
  await event('activity-pool',{ts:Date.now(),activities:[{id:'clock-test',title:'测试任务',subtitle:'本地模拟活动',kind:'',icon:'',color:'',progress:50,show_progress:true,priority:0,remaining_ms:null,extra:null}]});
  await ready('.activity-box');await pause(400);
  check(await evaluate(`window.islandClockFixture.width===360&&!document.querySelector('.system-clock')`),'Priority activity temporarily replaces time');
  await screenshot('clock-activity');
  await event('activity-pool',{ts:Date.now(),activities:[]});await ready('.time-box .system-clock');
  await waitFor(`window.islandClockFixture.width===150`,'Clock restores after activity');
  await event('control-island-theme',{theme:'white'});
  check(await evaluate(`getComputedStyle(document.querySelector('.system-clock')).color==='rgb(0, 0, 0)'`),'Clock follows light island theme');
  await screenshot('clock-light');
  await event('control-msg-mode',{enabled:true});await pause(700);
  check(await evaluate(`getComputedStyle(document.querySelector('.island-container')).display!=='none'`),'Time stays visible in quiet mode');
  await event('control-autohide-fs',{enabled:true});await event('fullscreen-changed',true);
  await waitFor(`getComputedStyle(document.querySelector('.island-container')).display==='none'`,'Fullscreen hides clock');
  await mode('speed');await mode('time');await pause(700);
  check(await evaluate(`getComputedStyle(document.querySelector('.island-container')).display==='none'`),'Selecting time does not override fullscreen hiding');
  await event('fullscreen-changed',false);
  await waitFor(`getComputedStyle(document.querySelector('.island-container')).display!=='none'&&window.islandClockFixture.width===150`,'Clock restored after fullscreen');
  await event('control-msg-mode',{enabled:false});await mode('speed');
  await waitFor(`!document.querySelector('.system-clock')&&window.islandClockFixture.width===150`,'Speed mode removes clock');
  await event('control-island-visibility',{show:false});await pause(750);
  await mode('time');await pause(500);
  check(await evaluate(`getComputedStyle(document.querySelector('.island-container')).display==='none'&&window.islandClockFixture.width===1`),'Time never wakes a disabled island');
  await evaluate(`localStorage.setItem('mollycloud:netspeed:nsd_system_time','true');localStorage.setItem('mollycloud:netspeed:nsd_msg_mode','true')`);
  await page('Page.reload',{ignoreCache:true});await ready('.time-box .system-clock');
  await waitFor(`window.islandClockFixture.width===150&&getComputedStyle(document.querySelector('.island-container')).display!=='none'`,'Time mode restores on quiet startup');
  // Upgrade from the previous added-on clock preserves both time and current content.
  await evaluate(`localStorage.removeItem('mollycloud:netspeed:nsd_system_time');localStorage.setItem('mollycloud:netspeed:nsd_show_system_time','true')`);
  await page('Page.reload',{ignoreCache:true});await ready('.custom-slot-item.is-time .system-clock');
  check(await evaluate(`window.islandClockFixture.width===260&&localStorage.getItem('mollycloud:netspeed:nsd_show_system_time')===null&&JSON.parse(localStorage.getItem('mollycloud:netspeed:nsd_custom_slots')).includes('time')`),'Old clock setting migrates into custom content');
  report.push({page:'island-clock',isolated:true,timeMode:true,customTime:true,quietAndFullscreen:true});
}

async function verifyNetSpeed(width, height) {
  await select(names.indexOf('netspeed'));
  check(await evaluate(`JSON.stringify([...document.querySelectorAll('.nav-item')].map(el=>el.dataset.page)) === JSON.stringify(['overview','keys','ccswitch','assistant','netspeed','images','skills'])`), `Requested navigation order ${width}`);
  for (const [index, name] of ['status','display','appearance'].entries()) {
    await evaluate(`document.querySelectorAll('.island-tabs button')[${index}].click()`);
    await settleAnimations();
    check(await evaluate(`document.documentElement.scrollWidth <= innerWidth && document.querySelector('.netspeed-panel').scrollWidth <= document.querySelector('.netspeed-panel').clientWidth`), `Island ${name} no overflow ${width}`);
    check(await evaluate(`getComputedStyle(document.querySelector('.island-card')).backgroundColor === '${darkMode ? 'rgb(34, 37, 31)' : 'rgb(255, 255, 255)'}'`), `Island ${name} shared theme ${width}`);
    await screenshot(`netspeed-${name}-${width}`);
    if (name==='display') {
      check(await evaluate(`!document.querySelector('[aria-label="常驻系统时间"]')&&document.querySelectorAll('.island-mode-grid button').length===6`),`Time is a primary content choice ${width}`);
      await evaluate(`[...document.querySelectorAll('.island-mode-grid button')].find(b=>b.textContent==='系统时间').click()`);
      await waitFor(`[...document.querySelectorAll('.island-mode-grid button')].find(b=>b.textContent==='系统时间').getAttribute('aria-pressed')==='true'`,'Time mode selected');
      check(await evaluate(`localStorage.getItem('mollycloud:preview:netspeed:nsd_system_time')==='true'`),`Time mode persisted ${width}`);
      await screenshot(`netspeed-time-${width}`);
      await evaluate(`[...document.querySelectorAll('.island-mode-grid button')].find(b=>b.textContent==='自定义组合').click()`);
      await ready('.island-slots');
      check(await evaluate(`document.querySelectorAll('.island-slots label').length===3`), `Island custom slots ${width}`);
      await evaluate(`document.querySelectorAll('.island-slots label')[2].querySelector('.n-base-selection').click()`);
      await ready('.n-base-select-option');
      await evaluate(`[...document.querySelectorAll('.n-base-select-option')].find(o=>o.textContent.trim()==='系统时间').click()`);
      await waitFor(`JSON.parse(localStorage.getItem('mollycloud:preview:netspeed:nsd_custom_slots')).includes('time')`,'Time custom slot saved');
      await screenshot(`netspeed-custom-${width}`);
      await evaluate(`document.querySelectorAll('.island-mode-grid button')[0].click()`);
    }
  }
  // A failed preference write must keep the old switch and show a visible error.
  await evaluate(`window.__islandSave=Storage.prototype.setItem; Storage.prototype.setItem=function(key,value){if(key.includes('netspeed:'))throw new Error('模拟存储失败');return window.__islandSave.call(this,key,value);};document.querySelector('[aria-label="启用灵动岛"]').click();`);
  await pause(100);
  check(await evaluate(`document.querySelector('[aria-label="启用灵动岛"]').getAttribute('aria-checked')==='true' && document.querySelector('.netspeed-panel .n-alert').innerText.includes('设置未保存')`), `Island storage failure preserves setting ${width}`);
  await evaluate(`Storage.prototype.setItem=window.__islandSave;delete window.__islandSave;`);
  report.push({page:'netspeed-settings',width,height});
}

try {
  await page('Page.enable');
  await page('Runtime.enable');
  await page('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: darkMode ? 'dark' : 'light' }] });
  if (clockOnly) {
    await verifyIslandClock();
  } else if (speechOnly) {
    for (const [width,height] of [[1440,900],[1180,760],[960,640]]) {
      await page('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
      await page('Page.navigate',{url:`${baseUrl}/?ui-preview=console`}); await ready('.nav-item');
      await verifySpeech(width,height);
    }
  } else if (subscriptionsOnly) {
    for (const [width,height] of [[1440,900],[1180,760],[960,640]]) {
      await page('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
      await page('Page.navigate',{url:`${baseUrl}/?ui-preview=console`});
      await ready('.nav-item');
      await verifySubscriptions(width,height);
    }
  } else if (paymentsOnly) {
    for (const [width,height] of [[1440,900],[1180,760],[960,640]]) {
      await page('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
      await page('Page.navigate',{url:`${baseUrl}/?ui-preview=console`});
      await ready('.nav-item');
      await verifyKeyGroupsAndRecharge(width,height);
    }
  } else if (netspeedOnly) {
    for (const [width,height] of [[1440,900],[1180,760],[960,640]]) {
      await page('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
      await page('Page.navigate',{url:`${baseUrl}/?ui-preview=console`});
      await ready('.nav-item');
      await verifyNetSpeed(width,height);
    }
  } else if (motionOnly) {
    await verifyMotion();
  } else if (appearanceOnly) {
    await verifyAppearance();
  } else if (productionSmoke) {
    // Exercise the real production entry with only public bootstrap responses stubbed.
    await page('Page.addScriptToEvaluateOnNewDocument', { source: `
      const originalFetch = globalThis.fetch;
      globalThis.fetch = (input, init) => {
        const url = typeof input === 'string' ? input : input.url;
        if (url === '/molly-api/health') return Promise.resolve(new Response(JSON.stringify({status:'ok'})));
        if (url === '/molly-api/api/v1/settings/public') return Promise.resolve(new Response(JSON.stringify({code:0,data:{site_name:'MollyCloud',login_agreement_enabled:true,login_agreement_documents:[]}})));
        return originalFetch(input, init);
      };
    ` });
    await page('Emulation.setDeviceMetricsOverride', { width: 1180, height: 760, deviceScaleFactor: 1, mobile: false });
    await page('Page.navigate', { url: `${baseUrl}/?ui-preview=console` });
    await ready('.check-row');
    await evaluate(`(() => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      for (const [selector, value] of [['#email','demo@example.com'],['#password','local-preview-only']]) {
        const input = document.querySelector(selector); setter.call(input, value); input.dispatchEvent(new Event('input', {bubbles:true}));
      }
      if (document.querySelectorAll('.check-row')[1].getAttribute('aria-checked') !== 'true') {
        throw new Error('Login agreement must be selected by default');
      }
    })()`);
    await pause(300);
    const smoke = await evaluate(`(() => {
      const style = getComputedStyle(document.querySelector('.primary-button'));
      return { theme: document.documentElement.dataset.theme, background: getComputedStyle(document.body).backgroundColor, font: getComputedStyle(document.querySelector('.login-input')).fontFamily, primary: style.backgroundColor, text: style.color, disabled: document.querySelector('.primary-button').disabled, previewExcluded: !document.querySelector('.app-shell') };
    })()`);
    report.push({ page: 'production-login', ...smoke });
    check(smoke.font.includes('Microsoft YaHei') && smoke.primary === 'rgb(181, 255, 54)' && smoke.text === 'rgb(35, 54, 0)' && !smoke.disabled && smoke.previewExcluded, 'Production theme/entry initialization');
    check(smoke.theme === (darkMode ? 'dark' : 'light') && smoke.background === (darkMode ? 'rgb(34, 37, 31)' : 'rgb(255, 255, 255)'), 'Production system appearance');
    await screenshot('production-login-1180');
  } else {

  for (const [width, height] of [[1440, 900], [1180, 760], [960, 640]]) {
    await page('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
    await page('Page.navigate', { url: `${baseUrl}/?ui-preview=login` });
    await ready('.check-row');
    const login = await evaluate(`(() => {
      const rows = [...document.querySelectorAll('.check-row')].map(el => {
        const box = el.querySelector('.n-checkbox-box').getBoundingClientRect();
        const label = el.querySelector('.n-checkbox__label').getBoundingClientRect();
        const lineHeight = parseFloat(getComputedStyle(el.querySelector('.n-checkbox__label')).lineHeight);
        return { difference: Math.abs(box.y + box.height / 2 - label.y - lineHeight / 2), boxSize: box.width, lineHeight, labelHeight: label.height };
      });
      const input = document.querySelector('.login-input');
      return { rows, horizontalOverflow: document.documentElement.scrollWidth > innerWidth, inputFont: getComputedStyle(input).fontFamily, tokenFont: getComputedStyle(document.documentElement).getPropertyValue('--font-body'), primaryColor: getComputedStyle(document.querySelector('.primary-button')).getPropertyValue('--n-color') };
    })()`);
    report.push({ page: 'login', width, height, ...login });
    check(login.rows.every(row => row.difference < 0.6 && row.boxSize === 16), `Checkbox alignment ${width}`);
    check(!login.horizontalOverflow, `Login overflow ${width}`);
    check(login.inputFont.includes('Microsoft YaHei'), `Naive UI shared font ${width}`);
    await screenshot(`login-${width}`);
    const wrapping = await evaluate(`(() => {
      const form = document.querySelector('.auth-form-wrap'); form.style.width = '260px';
      const row = document.querySelectorAll('.check-row')[1];
      const label = row.querySelector('.n-checkbox__label').getBoundingClientRect();
      const box = row.querySelector('.n-checkbox-box').getBoundingClientRect();
      const result = { wrapped: label.height > 24, difference: Math.abs(box.y + box.height / 2 - label.y - 12) };
      form.style.width = ''; return result;
    })()`);
    check(wrapping.wrapped && wrapping.difference < 0.6, `Wrapped checkbox alignment ${width}`);
    await evaluate(`document.querySelector('.check-row').focus()`);
    await key(' ', 'Space', 32);
    check(await evaluate(`document.querySelector('.check-row').getAttribute('aria-checked') === 'true'`), `Checkbox keyboard ${width}`);
    await evaluate(`document.querySelector('.agreement-link').focus()`);
    await key('Enter', 'Enter', 13);
    await ready('.agreement-modal');
    check(await evaluate(`document.querySelectorAll('.check-row')[1].getAttribute('aria-checked') === 'true'`), `Agreement defaults checked and link preserves it ${width}`);
    check(await evaluate(`getComputedStyle(document.querySelector('.agreement-modal')).borderRadius === '24px'`), `Agreement modal radius ${width}`);
    await screenshot(`agreement-${width}`);
    await key('Escape', 'Escape', 27);
    await mutate(`state.errorMessage = '邮箱或密码不正确，请重新输入';`, '.auth-shell');
    await screenshot(`login-error-${width}`);
    await mutate(`state.errorMessage = ''; state.phase = 'two-factor'; state.twoFactorHint = 'demo@mollycloud.cn';`, '.auth-shell');
    await ready('.totp-input');
    await screenshot(`two-factor-${width}`);

    await evaluate(`localStorage.removeItem('mollycloud:preview:console-settings')`);
    await page('Page.navigate', { url: `${baseUrl}/?ui-preview=console&preview-page=overview` });
    await ready('.overview-page');
    await verifyCompactSidebar(width, height);
    await verifySidebarScrollCues(width, height);
    await verifyConsoleSettings(width, height);
    await verifyPluginManager(width, height);
    let baseline;
    for (let index = 0; index < names.length; index++) {
      await select(index);
      if (names[index] === 'ccswitch') {
        await waitFor(`!document.querySelector('.ccswitch-load-state') && Boolean(document.querySelector('.ccswitch-frame')?.contentDocument.querySelector('[data-provider-id="molly-preview-provider"]'))`, `CC Switch ready ${width}`);
        check(await evaluate(`!document.querySelector('.ccswitch-frame').contentDocument.body.innerText.includes('欢迎使用 CC Switch')`), `CC Switch upstream onboarding removed ${width}`);
        const cardPoint = await evaluate(`(() => { const frame = document.querySelector('.ccswitch-frame'); const outer = frame.getBoundingClientRect(); const card = frame.contentDocument.querySelector('[data-provider-id="molly-preview-provider"]').getBoundingClientRect(); return {x:outer.x+card.x+card.width/2,y:outer.y+card.y+card.height/2}; })()`);
        await page('Input.dispatchMouseEvent', { type:'mouseMoved', ...cardPoint });
        await pause(200);
      }
      if (names[index] === 'images') {
        await waitFor(`!document.querySelector('.embedded-load-state')`, `Image workbench ready ${width}`);
      }
      await settleAnimations();
      const metrics = await evaluate(readShell);
      report.push({ page: names[index], width, height, ...metrics });
      baseline ??= metrics.shell;
      check(JSON.stringify(metrics.shell) === JSON.stringify(baseline), `Shared shell differs: ${names[index]} ${width}`);
      check(metrics.shell.titlebarStyle.backgroundColor === metrics.shell.sidebarStyle.backgroundColor && metrics.shell.titlebarStyle.borderBottomWidth === '0px' && metrics.shell.sidebarStyle.borderRightWidth === '0px', `Continuous titlebar and sidebar frame: ${names[index]} ${width}`);
      check(metrics.shell.workspaceStyle.borderTopLeftRadius === '16px' && metrics.shell.workspaceStyle.borderTopRightRadius === '16px' && metrics.workspace[0] + metrics.workspace[2] === width - 6, `Rounded workspace and shared outer inset: ${names[index]} ${width}`);
      check(metrics.headerTitlesAbsent, `Page title and greeting removed: ${names[index]} ${width}`);
      check(!metrics.horizontalOverflow && !metrics.valueOverflows && !metrics.headerClipped, `Content clipped: ${names[index]} ${width}`);
      if (['assistant', 'ccswitch', 'images'].includes(names[index])) {
        check(metrics.header === null && metrics.content.every((value, i) => value === metrics.workspace[i]) && metrics.contentPadding === '0px', `Embedded page fills workspace: ${names[index]} ${width}`);
      } else if (['netspeed', 'skills'].includes(names[index])) {
        check(metrics.header === null && metrics.content[1] >= metrics.workspace[1] && metrics.content[1] <= metrics.workspace[1] + 1 && metrics.contentPadding === '20px', `Tool page uses released header space: ${names[index]} ${width}`);
        check(await evaluate(`!document.querySelector('.account-refresh-button,.balance-chip,.user-chip,.account-reminder')`), `Tool page hides account controls: ${names[index]} ${width}`);
      } else {
        check(metrics.header !== null, `Shared topbar missing: ${names[index]} ${width}`);
        check(Math.abs(metrics.accountLayout.balance[1] - metrics.accountLayout.user[1]) < 0.1 && Math.abs(metrics.accountLayout.balance[3] - metrics.accountLayout.user[3]) < 0.1 && Math.abs(metrics.accountLayout.refresh[1] + metrics.accountLayout.refresh[3] / 2 - metrics.accountLayout.user[1] - metrics.accountLayout.user[3] / 2) < 0.1, `Account controls alignment ${names[index]} ${width}`);
      }
      check(metrics.selectedCount === 1, `Navigation state: ${names[index]} ${width}`);
      check(metrics.navCount === navCount && metrics.settingsVisible, `Platform navigation items and bottom settings visible: ${names[index]} ${width}`);
      check(metrics.shell.navStyle.boxShadow === 'none' && metrics.shell.navStyle.borderLeftWidth === '0px', `Navigation edge color: ${names[index]} ${width}`);
      check(metrics.cardBorders.every(card => parseFloat(card.borderWidth) === 0 || ['borderTopColor','borderRightColor','borderBottomColor','borderLeftColor'].every(side => card[side] === (darkMode ? 'rgb(56, 61, 50)' : 'rgb(222, 223, 219)'))), `Non-neutral card border: ${names[index]} ${width}`);
      check(await evaluate(`document.documentElement.dataset.theme === '${darkMode ? 'dark' : 'light'}'`), `Resolved theme: ${names[index]} ${width}`);
      check(metrics.shell.themeButtonAbsent, `Theme shortcut removed: ${names[index]} ${width}`);
      if (names[index] === 'assistant') {
        check(await evaluate(`!document.querySelector('.assistant-chat__toolbar h2,.assistant-settings-button')&&document.querySelectorAll('.assistant-tabs button').length===2&&getComputedStyle(document.querySelector('.assistant-chat__toolbar')).borderBottomWidth==='0px'`), `Molly chat heading ${width}`);
      }
      if (names[index] === 'keys') {
        check(!metrics.keyOverflow, `API key list has no horizontal scroll ${width}`);
        check(await evaluate(`document.querySelector('.key-record__quota').textContent.includes('US$12.6032')&&document.querySelector('.key-record__quota').textContent.includes('今日 US$0.0278')`),`Key actual and today spend match stats ${width}`);
        check(await evaluate(`(() => {const button=document.querySelector('.endpoint-copy-button');const create=document.querySelector('.create-key-button');const balance=document.querySelector('.balance-chip');const r=button.getBoundingClientRect(),a=create.getBoundingClientRect(),b=balance.getBoundingClientRect();return button.closest('.topbar')&&create.closest('.key-toolbar')&&!document.querySelector('.endpoint-card')&&r.width>=280&&r.width<=302&&r.height>=30&&r.height<=34&&r.x-a.right>=10&&r.x-a.right<=14&&Math.abs(r.y+r.height/2-a.y-a.height/2)<1&&Math.abs(r.y+r.height/2-b.y-b.height/2)<1&&button.getAttribute('aria-label').includes('API 端点');})()`), `API endpoint and adjacent creation align with account controls ${width}`);
      }
      if (['overview', 'subscriptions', 'usage', 'recharge'].includes(names[index])) {
        check(await evaluate(`document.querySelectorAll('.overview-subnav button').length === 4 && document.querySelector('.overview-subnav [aria-current="page"]').dataset.page === '${names[index]}' && document.querySelector('.nav-item.active').dataset.page === 'overview'`), `Overview secondary navigation ${names[index]} ${width}`);
        check(await evaluate(`(() => {const nav=document.querySelector('.overview-subnav');const header=document.querySelector('.topbar');const n=nav.getBoundingClientRect(),a=document.querySelector('.topbar-actions').getBoundingClientRect();return header.contains(nav)&&n.right<=a.x&&Math.abs(n.y+n.height/2-a.y-a.height/2)<1;})()`), `Overview tabs and account controls share one row ${names[index]} ${width}`);
      }
      check(await evaluate(`!document.querySelector('[aria-label="MCP 市场"]') && !document.querySelector('.mcp-market')`), `MCP market removed ${width}`);
      if (names[index] === 'ccswitch') {
        const embedded = await evaluate(`(() => {
          const frame = document.querySelector('.ccswitch-frame');
          const child = frame.contentDocument;
          const r = frame.getBoundingClientRect();
          frame.contentWindow.__mollyKeepAliveProbe = ${width};
          return { src: frame.getAttribute('src'), frame: {x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom},
            outerScroll: document.querySelector('.workspace').scrollHeight > document.querySelector('.workspace').clientHeight,
            childOverflow: child.documentElement.scrollWidth > child.documentElement.clientWidth,
            chinese: child.body.innerText.includes('供应商'), codex: Boolean(child.querySelector('button[aria-label="Codex"]')),
            preview: child.body.innerText.includes('界面预览'),
            card: child.querySelector('[data-provider-id="molly-preview-provider"]').innerText,
            controls: [...child.querySelectorAll('button')].map(el => ({text:el.innerText, title:el.getAttribute('title'), aria:el.getAttribute('aria-label')})),
          };
        })()`);
        report.push({ page:'ccswitch-embedded', width, height, ...embedded });
        check(embedded.frame.height > 240 && embedded.frame.bottom <= height && !embedded.outerScroll && !embedded.childOverflow, `CC Switch internal frame bounds ${width}`);
        check(embedded.chinese && embedded.codex && embedded.preview && embedded.card.includes('MollyCloud'), `CC Switch real UI/readonly preview ${width}`);
      }
      await screenshot(`${names[index]}-${width}`);
      if (names[index] === 'subscriptions') await verifySubscriptions(width, height);
      if (names[index] === 'netspeed') await verifyNetSpeed(width, height);
      if (names[index] === 'skills') await verifySkills(width, height);
      if (names[index] === 'ccswitch') {
        await evaluate(`document.querySelector('.ccswitch-frame').contentDocument.querySelector('[data-provider-id="molly-preview-provider"] button[aria-label="编辑"]').click()`);
        await waitFor(`Boolean(document.querySelector('.ccswitch-frame').contentDocument.querySelector('#provider-form'))`, `CC Switch edit panel ${width}`);
        await evaluate(`(() => { const child = document.querySelector('.ccswitch-frame').contentDocument; const notice = child.querySelector('[role="dialog"]'); if(notice) [...notice.querySelectorAll('button')].find(el => el.innerText === '我知道了')?.click(); })()`);
        await waitFor(`!document.querySelector('.ccswitch-frame').contentDocument.querySelector('[role="dialog"]')`, `CC Switch config notice dismissed ${width}`);
        const dialog = await evaluate(`(() => {
          const frame = document.querySelector('.ccswitch-frame');
          const child = frame.contentDocument;
          const dialog = child.querySelector('#provider-form').closest('.fixed');
          const r = dialog.getBoundingClientRect();
          const scroll = [...dialog.querySelectorAll('*')].find(el => el.scrollHeight > el.clientHeight && ['auto','scroll'].includes(frame.contentWindow.getComputedStyle(el).overflowY));
          if (scroll) scroll.scrollTop = scroll.scrollHeight;
          return { title: dialog.querySelector('h2')?.innerText, x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom, viewHeight:frame.clientHeight, viewWidth:frame.clientWidth,
            innerScroll: Boolean(scroll && scroll.scrollTop > 0), outerScroll: document.querySelector('.workspace').scrollHeight > document.querySelector('.workspace').clientHeight };
        })()`);
        report.push({page:'ccswitch-edit-panel', viewportWidth:width, viewportHeight:height, ...dialog});
        check(dialog.title?.includes('编辑') && dialog.y >= 0 && dialog.bottom <= dialog.viewHeight + 1 && dialog.width <= dialog.viewWidth && dialog.innerScroll && !dialog.outerScroll, `CC Switch edit dialog internal scrolling ${width}`);
        await screenshot(`ccswitch-edit-${width}`);
        await evaluate(`document.querySelector('.ccswitch-frame').contentDocument.querySelector('#provider-form').closest('.fixed').querySelector('button[aria-label="返回"]').click()`);
        await waitFor(`!document.querySelector('.ccswitch-frame').contentDocument.querySelector('#provider-form')`, `CC Switch edit dismissed ${width}`);
      }
    }
    await verifyKeyGroupsAndRecharge(width, height);
    await select(2);
    await mutate(`state.dashboard.keys.items[0].name = '用于多个开发环境的长名称密钥 ' + 'development-'.repeat(8); state.dashboard.keys.items[0].group.name = '团队共享分组'.repeat(8); state.dashboard.keys.items[0].quota_used = 1234567890.1234;`);
    const keyLayout = await evaluate(`(() => {
      const scope=document.querySelector('.keys-page');
      const elements=[scope,...scope.querySelectorAll('.key-record,.key-record > div,.ccs-import-actions,button')];
      return {overflow:elements.some(el=>el.scrollWidth>el.clientWidth+1),outside:elements.some(el=>el.getBoundingClientRect().right>innerWidth),copy:Boolean(scope.querySelector('.key-copy-button')),import:Boolean(scope.querySelector('.ccs-import-button'))};
    })()`);
    check(!keyLayout.overflow && !keyLayout.outside && keyLayout.copy && keyLayout.import, `Long key details wrap with visible actions ${width}`);
    await screenshot(`keys-long-content-${width}`);
    await mutate(`state.dashboard.keys.items[0].name='Molly Desktop';state.dashboard.keys.items[0].group.name='Molly Pro';state.dashboard.keys.items[0].quota_used=12.6;`);
    // Round trip catches scroll position/layout changes caused by leaving chat.
    await select(0);
    check(JSON.stringify((await evaluate(readShell)).shell) === JSON.stringify(baseline), `Shell round trip ${width}`);
    await mutate(`state.desktopUpdate = { version: '${previewUpdateVersion}', currentVersion: '${appVersion}', downloadUrl: 'https://desktop.veriolink.com/MollyCloud_${previewUpdateVersion}_x64-setup.exe', notes: '设计预览', sha256: 'a'.repeat(64), sizeBytes: 12345 };`);
    await waitFor(`Boolean(document.querySelector('.desktop-update-dialog'))`, `Desktop update dialog ${width}`);
    check(await evaluate(`document.querySelector('.desktop-update-dialog').innerText.includes('${previewUpdateVersion}') && document.querySelector('.desktop-update-dialog').innerText.includes('${appVersion}') && document.querySelector('.app-shell').inert`), `Desktop update confirmation and inert shell ${width}`);
    await screenshot(`desktop-update-prompt-${width}`);
    await evaluate(`document.querySelector('.desktop-update-actions button').click()`);
    const updateNotice = await evaluate(`(() => {
      const notice = document.querySelector('.update-available');
      const balance = document.querySelector('.balance-chip');
      const actions = document.querySelector('.topbar-actions');
      const rect = el => { const r = el.getBoundingClientRect(); return { x:r.x, y:r.y, right:r.right, bottom:r.bottom, width:r.width, height:r.height }; };
      return { notice: notice && rect(notice), balance: balance && rect(balance), actions: actions && rect(actions), text: notice?.innerText.trim(), title: notice?.getAttribute('title'), horizontalOverflow: document.documentElement.scrollWidth > innerWidth || document.querySelector('.workspace').scrollWidth > document.querySelector('.workspace').clientWidth };
    })()`);
    check(updateNotice.notice && updateNotice.text === '软件可更新' && updateNotice.title?.includes(previewUpdateVersion) && updateNotice.notice.right <= updateNotice.balance.x && !updateNotice.horizontalOverflow, `Desktop update indicator placement ${width}`);
    await screenshot(`desktop-update-${width}`);
    await mutate(`state.desktopUpdate = null;`);
    await select(5);
    check(await evaluate(`document.querySelector('.ccswitch-frame').contentWindow.__mollyKeepAliveProbe === ${width}`), `CC Switch iframe remains mounted ${width}`);
    // The import dialog is exercised without exposing or saving a real key. A preview
    // submission must fail truthfully; the backend result below only tests navigation.
    await select(2);
    await evaluate(`document.querySelector('.ccs-import-button').click()`);
    await ready('.ccs-import-dialog');
    const importDialog = await evaluate(`(() => {
      const dialog = document.querySelector('.ccs-import-dialog');
      const bounds = dialog.getBoundingClientRect();
      const fields = [...dialog.querySelectorAll('.ccs-import-field > span, .ccs-import-model legend')].map(el => el.textContent.trim());
      const actions = [...dialog.querySelectorAll('.console-settings-actions button')].map(el => el.innerText.trim());
      return {x:bounds.x,y:bounds.y,right:bounds.right,bottom:bounds.bottom,fields,actions,
        description:dialog.innerText, backgroundInert:document.querySelector('.app-shell').inert,
        agentCount:Number(dialog.dataset.agentCount),
        modelValue:dialog.querySelector('.ccs-import-model input')?.value,
        nameValue:dialog.querySelector('.ccs-import-field input')?.value,
        focusInside:dialog.contains(document.activeElement)};
    })()`);
    check(importDialog.x >= 0 && importDialog.y >= 0 && importDialog.right <= width && importDialog.bottom <= height && importDialog.backgroundInert && importDialog.focusInside, `CC Switch import dialog bounds/focus ${width}`);
    check(importDialog.fields.join('|') === '名称|导入到的 Agent|默认模型' && importDialog.actions.join('|') === '取消|导入' && importDialog.modelValue === 'gpt-5.5' && importDialog.nameValue.includes('MollyCloud'), `CC Switch import fields/defaults ${width}`);
    check(importDialog.agentCount === 9 && importDialog.description.includes('Claude Desktop'), `CC Switch import supported agents ${width}`);
    await evaluate(`document.querySelector('.ccs-import-submit').click()`);
    await ready('.ccs-import-dialog .console-settings-error');
    check(await evaluate(`document.querySelector('.ccs-import-dialog .console-settings-error')?.innerText.includes('浏览器预览不会保存真实密钥') && !document.querySelector('.ccs-import-actions').innerText.includes('再次导入')`), `CC Switch preview import rejected ${width}`);
    await screenshot(`ccswitch-import-preview-error-${width}`);
    await evaluate(`document.querySelector('.ccs-import-dialog .console-settings-actions button').click()`);
    await waitFor(`!document.querySelector('.ccs-import-dialog')`, `CC Switch import dialog dismissed ${width}`);
    await pause(260);
    check(await evaluate(`document.activeElement === document.querySelector('.ccs-import-button')`), `CC Switch import restores focus ${width}`);
    await mutate(`state.errorMessage = ''; state.importedProviders = { 'demo-key': {provider_id:'molly-preview-provider',app:'codex'} };`);
    check(await evaluate(`document.querySelector('.ccs-import-button').textContent.trim()==='导入到内置 CC Switch'&&!document.querySelector('.ccs-import-actions').textContent.includes('再次导入')&&!document.querySelector('.ccs-import-actions').textContent.includes('打开')`),`Key import label stays constant after success ${width}`);
    await select(0);
    await mutate(`state.dashboard.user.balance = 3.72;`);
    const reminder = await evaluate(`(() => { const el = document.querySelector('.account-reminder'); const s = getComputedStyle(el); return { innerBorder: Boolean(el.querySelector('.n-alert__border')), borders: [s.borderTopColor,s.borderRightColor,s.borderBottomColor,s.borderLeftColor], radius: s.borderRadius, overflow: s.overflow }; })()`);
    check(!reminder.innerBorder && new Set(reminder.borders).size === 1 && reminder.overflow === 'hidden', `Balance reminder edge ${width}`);
    await screenshot(`balance-reminder-${width}`);
    await select(1);
    await mutate(`state.dashboard.subscriptions.subscriptions[0].expires_at = new Date(Date.now() + 86400000).toISOString();`);
    check(await evaluate(`Boolean(document.querySelector('.account-reminder--subscription'))`), `Subscription reminder ${width}`);
    await screenshot(`subscription-reminder-${width}`);
    await mutate(`state.items = [];`, '.subscriptions-panel');
    await ready('.empty-state');
    await screenshot(`subscriptions-empty-${width}`);
    await select(2);
    await mutate(`state.dashboard.keys.items = [];`);
    await ready('.empty-state');
    await screenshot(`keys-empty-${width}`);
    await select(4);
    const chatBefore = await evaluate(`document.querySelector('.assistant-chat').getBoundingClientRect().bottom`);
    await evaluate(`document.querySelector('.assistant-toggle-cell [role="switch"]').click()`);
    await pause(100);
    check(await evaluate(`document.querySelector('.assistant-toggle-cell [role="switch"]').getAttribute('aria-checked') === 'false'`), `Pet toggle ${width}`);
    await evaluate(`document.querySelector('.assistant-toggle-cell [role="switch"]').click()`);
    await evaluate(`(async () => {
      const input = document.querySelector('.assistant-chat__input input');
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      for (let i = 0; i < 14; i++) {
        setter.call(input, '滚动检查 ' + (i + 1) + '：' + '长消息应在气泡内换行，输入框和会话外框保持固定。'.repeat(8));
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise(resolve => setTimeout(resolve, 20));
        document.querySelector('.assistant-send-button').click();
        await new Promise(resolve => setTimeout(resolve, 20));
      }
    })()`);
    const scroll = await evaluate(`(() => {
      const history = document.querySelector('.assistant-chat__history');
      const workspace = document.querySelector('.workspace');
      return { messages: history.querySelectorAll('article').length, historyHeight: history.scrollHeight, visibleHeight: history.clientHeight, scrollTop: history.scrollTop, outerScroll: workspace.scrollHeight > workspace.clientHeight, bottom: document.querySelector('.assistant-chat').getBoundingClientRect().bottom, inputCleared: document.querySelector('.assistant-chat__input input').value === '' };
    })()`);
    check(scroll.messages >= 28 && scroll.historyHeight > scroll.visibleHeight && scroll.scrollTop > 0 && !scroll.outerScroll && Math.abs(scroll.bottom - chatBefore) < 1 && scroll.inputCleared, `Chat long history ${width}`);
    await screenshot(`assistant-scroll-${width}`);
    await mutate(`state.assistantMessages = []; state.assistantChatError = '消息发送失败，请稍后重试';`);
    const chat = await evaluate(`(() => { const workspace = document.querySelector('.workspace'); const composer = document.querySelector('.assistant-composer').getBoundingClientRect(); return { outerScroll: workspace.scrollHeight > workspace.clientHeight, composerBottom: composer.bottom, empty: Boolean(document.querySelector('.assistant-chat__empty')) }; })()`);
    check(!chat.outerScroll && chat.composerBottom <= height && chat.empty, `Assistant empty/error state ${width}`);
    await screenshot(`assistant-empty-error-${width}`);
    await verifyPetSettings(width, height);
    await verifySpeech(width, height);
    report.push({ page: 'states', width, reminder, chat, scroll, wrapping });
  }
  }
  check(runtimeErrors.length === 0, 'Browser runtime exception');
  console.log(JSON.stringify({ screenshots: outputDir, checks: report.length, failures, runtimeErrors }, null, 2));
} finally {
  await writeFile(join(outputDir, productionSmoke ? 'report-production.json' : paymentsOnly ? 'report-keys-payments.json' : netspeedOnly ? 'report-netspeed.json' : 'report.json'), JSON.stringify({ report, failures, runtimeErrors }, null, 2));
  await call('Browser.close').catch(() => {});
  socket.close();
  browser.kill();
}
if (failures.length) process.exitCode = 1;
