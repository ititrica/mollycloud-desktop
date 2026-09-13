// Isolated desktop UI regression check; requires the local Vite dev server.
// No real login, credentials, clipboard, or desktop pet state are used.
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const baseUrl = process.env.MOLLY_UI_URL || 'http://127.0.0.1:24324';
const balanceOnly = process.argv.includes('--balance-only');
const outputDir = resolve(import.meta.dirname, '../../artifacts/balance-pill');
await mkdir(outputDir, { recursive: true });
const profile = await mkdtemp(join(tmpdir(), 'molly-design-check-'));
const browser = spawn(process.env.MOLLY_EDGE_PATH || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', [
  '--headless=new', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-first-run', '--no-default-browser-check',
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
socket.onmessage = event => {
  const message = JSON.parse(event.data);
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
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Timed out: ${method}`)); }, 30000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
}
const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
const page = (method, params) => call(method, params, sessionId);
async function evaluate(expression) {
  const result = await page('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function ready(selector) {
  for (let i = 0; i < 200; i++) {
    if (await evaluate(`Boolean(document.querySelector(${JSON.stringify(selector)}))`)) {
      await evaluate('document.fonts.ready.then(() => true)');
      await pause(220);
      return;
    }
    await pause(100);
  }
  throw new Error(`Page did not render ${selector}`);
}
async function screenshot(name) {
  const { data } = await page('Page.captureScreenshot', { format: 'png' });
  await writeFile(join(outputDir, `${name}.png`), Buffer.from(data, 'base64'));
}

const report = [];
const failures = [];
const check = (condition, message) => { if (!condition) failures.push(message); };
async function pointer(x, y) {
  await page('Input.dispatchMouseEvent', { type:'mouseMoved', x, y });
  await pause(260);
}
async function mouseClick(x, y, button = 'left') {
  await pointer(x, y);
  await page('Input.dispatchMouseEvent', { type:'mousePressed', x, y, button, clickCount:1 });
  await page('Input.dispatchMouseEvent', { type:'mouseReleased', x, y, button, clickCount:1 });
}
async function petCenter() {
  const region = await evaluate(`window.__testRegions.find(region => region.id === 'pet')`);
  if (!region) throw new Error('Model interaction region is unavailable');
  const scale = await evaluate('window.__testScale');
  return { x:(region.x + region.width / 2) / scale, y:(region.y + region.height / 2) / scale };
}
const readInteraction = `(() => {
  const visible = selector => {
    const element = document.querySelector(selector);
    if (!element || element.hidden || element.classList.contains('hidden')) return false;
    const style = getComputedStyle(element);
    return style.display !== 'none' && style.visibility !== 'hidden' && element.getClientRects().length > 0;
  };
  return { infoVisible:visible('#info-panel'), assistantVisible:visible('#as-inputbar'),
    menuVisible:visible('#menu'), clickAnimations:window.__testClickAnimations,
    inputEvents:window.__testInputEvents, commandLog:window.__testCommandLog,
    assistantEnabled:JSON.parse(localStorage.getItem('live2d-pet-settings')).assistant.enabled };
})()`;
const read = `(() => {
  const pill = document.querySelector('#balance-pill-trigger');
  const r = pill.getBoundingClientRect();
  const root = document.querySelector('#balance-pill');
  const panel = document.querySelector('#balance-pill-usage');
  const p = panel.getBoundingClientRect();
  const orbit = document.querySelector('.balance-pill__orbit');
  const orbitStyle = getComputedStyle(orbit);
  const tailStyle = getComputedStyle(orbit, '::before');
  return { pill:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},
    head:window.__testHead, text:document.querySelector('#balance-pill-value').textContent.trim(), hidden:root.classList.contains('hidden'), low:root.classList.contains('balance-pill--low'),
    expanded:pill.getAttribute('aria-expanded'), tokens:document.querySelector('#balance-pill-tokens').textContent,
    fullBalance:document.querySelector('#balance-pill-full-value').textContent,
    panel:{left:p.left,top:p.top,right:p.right,bottom:p.bottom,hidden:panel.hidden},
    regions:window.__testRegions, orbits:document.querySelectorAll('.balance-pill__orbit').length,
    orbitDirection:orbitStyle.animationDirection, orbitDuration:orbitStyle.animationDuration, tailGradient:tailStyle.backgroundImage };
})()`;
try {
  await page('Page.enable');
  await page('Runtime.enable');
  await page('Emulation.setDeviceMetricsOverride', {width:700,height:700,deviceScaleFactor:1,mobile:false});
  await page('Page.addScriptToEvaluateOnNewDocument', { source: `
    localStorage.setItem('petra-announced-version','0.2.3');
    localStorage.setItem('live2d-pet-settings', JSON.stringify({audioEnabled:false,activity:'low',modelScale:1,assistant:{enabled:true}}));
    const callbacks = new Map();
    const listeners = new Map();
    let callbackId = 0;
    window.__testAccount = {balance:3.72,today_tokens:128400};
    window.__testScale = 1;
    window.__testCommands = new Set();
    window.__testCommandLog = [];
    window.__testInputEvents = [];
    window.__testClickAnimations = [];
    window.__testImportedModels = new Map();
    for (const type of ['pointerdown','pointerup','contextmenu']) {
      document.addEventListener(type, event => window.__testInputEvents.push({type,button:event.button,trusted:event.isTrusted}));
    }
    window.__testEmit = (event,payload=null) => { for(const id of listeners.get(event)||[]) callbacks.get(id)?.({event,payload}); };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener(){}};
    window.__TAURI_INTERNALS__ = {
      metadata: {currentWindow:{label:'main'},currentWebview:{label:'main'}},
      transformCallback(callback) { callbacks.set(++callbackId,callback); return callbackId; },
      unregisterCallback(id) {callbacks.delete(id);},
      async invoke(command,args={}) {
        window.__testCommands.add(command);
        if(['drag_start','drag_end','set_menu_open'].includes(command)) window.__testCommandLog.push({command,args});
        if(command==='plugin:event|listen') { const list=listeners.get(args.event)||[]; list.push(args.handler); listeners.set(args.event,list); return args.handler; }
        if(command==='plugin:event|emit'||command==='plugin:event|emit_to') { if(args.event==='petra-settings-response')window.__testSettingsResponse=args.payload; window.__testEmit(args.event,args.payload); return null; }
        if(command==='plugin:window|scale_factor') return window.__testScale;
        if(command==='plugin:window|outer_position') return {x:100*window.__testScale,y:100*window.__testScale};
        if(command==='plugin:app|version') return '0.1.0';
        if(command==='fetch_account_balance') { if(window.__testAccount===null)throw new Error('not signed in'); return window.__testAccount; }
        if(command==='read_model_manifest') return JSON.stringify({type:'psd',file:'Molly.psd',files:['Molly.psd','seethrough_output.psd']});
        if(command==='read_builtin_psd') return Array.from(new Uint8Array(await fetch('/models/'+args.name).then(r=>r.arrayBuffer())));
        if(command==='save_psd') { window.__testImportedModels.set(args.name,args.bytes); return args.name; }
        if(command==='read_psd') { if(!window.__testImportedModels.has(args.name))throw new Error('模型不存在'); return window.__testImportedModels.get(args.name); }
        if(command==='delete_imported_model') { if(window.__testDeleteFails)throw new Error('测试：模型删除失败'); window.__testImportedModels.delete(args.name); return null; }
        if(command==='get_assistant_config') return {enabled:true};
        if(command==='has_api_key') return Boolean(window.__testApiKey);
        if(command==='set_api_key') { if(window.__testKeySaveFails)throw new Error('测试：密钥保存失败'); window.__testApiKey=args.apiKey; return null; }
        if(command==='cursor_pos') {const s=window.__testScale;return {x:0,y:0,rx:-350*s,ry:-350*s,left:100*s,top:100*s};}
        if(command==='work_area_at') {const s=window.__testScale;return {left:0,top:0,width:1920*s,height:1080*s};}
        if(command==='sync_interaction_regions') {window.__testRegions=args.regions;return null;}
        if(command==='get_idle_seconds') return 0;
        if(command==='list_models') return [...window.__testImportedModels.keys()];
        return null;
      },
    };
  ` });
  await page('Page.navigate', {url:baseUrl+'/overlay.html'});
  await ready('#balance-pill:not(.hidden)');
  await evaluate(`(async () => {
    const {Rigged2DView}=await import('/src/petra/live2d/psd/Rigged2DView.ts');
    const original=Rigged2DView.prototype.getHeadBounds;
    Rigged2DView.prototype.getHeadBounds=function() {window.__testView=this;return window.__testHead=original.call(this);};
    const originalClick=Rigged2DView.prototype.playClick;
    Rigged2DView.prototype.playClick=function() {
      const result=originalClick.call(this);
      window.__testClickAnimations.push({clickPulse:this.clickPulse,scalePulse:this.scalePulse});
      return result;
    };
  })()`);
  await pause(300);
  if (!balanceOnly) {
  // Drive trusted browser mouse events against the same pet region sent to native hit testing.
  // Keep the assistant enabled to catch accidental reopening from the model click path.
  let interaction = await evaluate(readInteraction);
  check(interaction.assistantEnabled, 'Assistant is enabled for model interaction checks');
  check(!interaction.infoVisible && !interaction.assistantVisible, 'Panels are closed before model click');
  const clickPoint = await petCenter();
  await mouseClick(clickPoint.x,clickPoint.y);
  await pause(200);
  interaction = await evaluate(readInteraction);
  check(interaction.clickAnimations.length === 1 && interaction.clickAnimations[0].clickPulse > 0 && interaction.clickAnimations[0].scalePulse > 0,
    'Single model click triggers the original pet animation');
  check(!interaction.infoVisible && !interaction.assistantVisible, 'Single model click does not show weather, tasks, or assistant input');
  check(interaction.inputEvents.some(event => event.type === 'pointerup' && event.button === 0 && event.trusted), 'Model click uses trusted mouse events');
  check(!interaction.commandLog.some(entry => entry.command === 'drag_start'), 'Single click does not begin dragging');
  report.push({interaction:'model-click',point:clickPoint,...interaction});
  await screenshot('model-click-panels-closed');

  const dragPoint = await petCenter();
  await pointer(dragPoint.x,dragPoint.y);
  await page('Input.dispatchMouseEvent', {type:'mousePressed',x:dragPoint.x,y:dragPoint.y,button:'left',clickCount:1});
  await page('Input.dispatchMouseEvent', {type:'mouseMoved',x:dragPoint.x+30,y:dragPoint.y+18,buttons:1});
  await pause(80);
  await page('Input.dispatchMouseEvent', {type:'mouseReleased',x:dragPoint.x+30,y:dragPoint.y+18,button:'left',clickCount:1});
  await pause(100);
  interaction = await evaluate(readInteraction);
  const dragCommands = interaction.commandLog.filter(entry => entry.command.startsWith('drag_')).map(entry => entry.command);
  check(dragCommands.length === 2 && dragCommands[0] === 'drag_start' && dragCommands[1] === 'drag_end', 'Drag starts and ends through native commands');
  check(interaction.clickAnimations.length === 1, 'Drag release does not trigger click animation');
  check(!interaction.infoVisible && !interaction.assistantVisible, 'Drag does not open pet panels');
  report.push({interaction:'model-drag',...interaction});

  const menuPoint = await petCenter();
  await mouseClick(menuPoint.x,menuPoint.y,'right');
  await pause(200);
  interaction = await evaluate(readInteraction);
  check(interaction.menuVisible && interaction.commandLog.some(entry => entry.command === 'set_menu_open' && entry.args.open), 'Right click still opens the pet menu');
  check(interaction.inputEvents.some(event => event.type === 'contextmenu' && event.trusted), 'Context menu uses a trusted right click');
  check(await evaluate(`document.querySelectorAll('#menu > .mi').length > 0 && !['模型','小助手设置','开机自启','今日抽卡','对话记录'].some(label => [...document.querySelectorAll('#menu > .mi > span:first-child')].some(el => el.textContent.trim() === label))`), 'Removed settings entries stay out of the pet menu');
  report.push({interaction:'model-context-menu',...interaction});
  await screenshot('model-right-click-menu');
  await mouseClick(690,690);
  check(!(await evaluate(readInteraction)).menuVisible, 'Clicking outside closes the context menu');

  await evaluate(`window.__testEmit('petra-assistant-open')`);
  await pause(200);
  interaction = await evaluate(readInteraction);
  check(interaction.assistantVisible && !interaction.infoVisible, 'Explicit assistant entry still opens chat without the information panel');
  report.push({interaction:'explicit-assistant-entry',...interaction});
  await screenshot('explicit-assistant-entry');
  await evaluate(`import('/src/petra/assistant/AssistantPanel.ts').then(module => module.closeAssistant())`);
  await pointer(650,650);
  check(!(await evaluate(readInteraction)).assistantVisible, 'Assistant closes before balance regression checks');

  const petRequest = async request => {
    const id = crypto.randomUUID();
    await evaluate(`window.__testSettingsResponse = null; window.__testEmit('petra-settings-request', ${JSON.stringify({id, request})})`);
    for (let i = 0; i < 100; i++) {
      const response = await evaluate(`window.__testSettingsResponse`);
      if (response?.id === id) return response;
      await pause(50);
    }
    throw new Error('Pet settings response timed out');
  };
  const initialSettings = await petRequest({action:'read'});
  check(Boolean(initialSettings.value?.models.length) && !initialSettings.error, 'Pet settings bridge reads current model and assistant settings');
  const selectedModel = await petRequest({action:'select',model:{type:'manifest',name:'seethrough_output.psd'}});
  check(!selectedModel.error && selectedModel.value.draft.model.name === 'seethrough_output.psd', 'Pet settings switches bundled PSD models');
  const forbiddenDelete = await petRequest({action:'delete',name:'seethrough_output.psd'});
  check(Boolean(forbiddenDelete.error), 'Built-in models cannot be deleted');
  const importedModel = await evaluate(`(async () => { const {importPetModel} = await import('/src/petSettings.ts'); const bytes = await fetch('/models/Molly.psd').then(r=>r.arrayBuffer()); const result = await importPetModel(new File([bytes], 'settings-test-import.psd')); return {model:result.draft.model,models:result.models}; })()`);
  check(importedModel.model.type === 'import' && importedModel.models.some(model => model.name === 'settings-test-import.psd'), 'Console import saves and selects PSD via settings bridge');
  await evaluate(`window.__testDeleteFails = true`);
  const failedDelete = await petRequest({action:'delete',name:'settings-test-import.psd'});
  const afterDeleteFailure = await petRequest({action:'read'});
  check(Boolean(failedDelete.error) && afterDeleteFailure.value.draft.model.name === 'settings-test-import.psd', 'Delete failure restores active imported model');
  await evaluate(`window.__testDeleteFails = false`);
  const deletedModel = await petRequest({action:'delete',name:'settings-test-import.psd'});
  check(!deletedModel.error && deletedModel.value.draft.model.type === 'manifest' && !deletedModel.value.models.some(model => model.type === 'import'), 'Deleting active import selects built-in before removal');
  await petRequest({action:'select',model:initialSettings.value.draft.model});
  const draft = structuredClone(initialSettings.value.draft);
  draft.modelScale = 1.2; draft.boundsPadding.left = 12; draft.params.irisScale = 1.15; draft.auto.autoBlink = false;
  draft.debugBorder = true; draft.debugModelBounds = true; draft.assistant.greetInterval = 35;
  const savedSettings = await petRequest({action:'save',draft,apiKey:'synthetic-pet-settings-key'});
  check(!savedSettings.error && savedSettings.value?.draft.modelScale === 1.2 && savedSettings.value?.draft.boundsPadding.left === 12 && savedSettings.value?.draft.assistant.greetInterval === 35 && savedSettings.value?.apiKeyConfigured, 'Pet settings bridge applies and persists model and assistant settings');
  check(!JSON.stringify(savedSettings).includes('synthetic-pet-settings-key') && !await evaluate(`localStorage.getItem('live2d-pet-settings').includes('synthetic-pet-settings-key')`), 'Saved secret never appears in snapshot or local settings');
  const beforeFailure = await evaluate(`localStorage.getItem('live2d-pet-settings')`);
  await evaluate(`window.__testKeySaveFails = true`);
  const rejectedSettings = await petRequest({action:'save',draft:{...draft,modelScale:1.8},apiKey:'synthetic-failed-key'});
  check(Boolean(rejectedSettings.error) && await evaluate(`localStorage.getItem('live2d-pet-settings')`) === beforeFailure, 'Key save failure preserves saved settings');
  await evaluate(`window.__testKeySaveFails = false`);
  const invalidSettings = await petRequest({action:'save',draft:{...draft,modelScale:99}});
  check(Boolean(invalidSettings.error), 'Bridge rejects out-of-range settings');
  const restoredSettings = await petRequest({action:'save',draft:initialSettings.value.draft,clearApiKey:true});
  check(!restoredSettings.error && !restoredSettings.value.apiKeyConfigured, 'Settings restore and explicit key clearing work');
  report.push({interaction:'pet-settings-bridge',saved:true,rejectedInvalid:true,secretExcluded:true,failedSavePreserved:true});
  }

  for (const size of [180,300,450]) {
    await evaluate(`window.__testView.setScale(${size})`);
    await pause(250);
    let state=await evaluate(read);
    check(state.pill.top>=state.head.top-1 && state.pill.top<=state.head.top+7, 'Head vertical position at '+size);
    check(state.head.left-state.pill.right>=11 && state.head.left-state.pill.right<=21, 'Head horizontal position at '+size);
    check(state.pill.width===40 && state.pill.height===40 && state.text==='3$' && state.orbits===1, 'Compact balance orb at '+size);
    const brightTailStart = state.tailGradient.indexOf('rgba(255, 255, 255, 0.96) 0%');
    const transparentTailEnd = state.tailGradient.indexOf('rgba(0, 0, 0, 0) 36%');
    check(state.orbitDirection==='reverse' && state.orbitDuration==='4.8s' && brightTailStart>=0 && transparentTailEnd>brightTailStart, 'Counterclockwise light point keeps its trail behind it at '+size);
    check(!state.hidden && !state.low && state.expanded==='false', 'Initial state at '+size);
    await screenshot('balance-'+size);
    await pointer((state.pill.left+state.pill.right)/2,(state.pill.top+state.pill.bottom)/2);
    state=await evaluate(read);
    check(state.expanded==='true' && !state.panel.hidden && state.fullBalance==='3.72$' && state.tokens==='128,400 token', 'Hover usage at '+size);
    check(Math.abs((state.panel.right-state.panel.left)-(state.panel.bottom-state.panel.top))<0.1 && state.panel.left<state.pill.left && state.panel.bottom>state.pill.bottom, 'Usage expands as a larger circle toward bottom-left at '+size);
    check(await evaluate(`getComputedStyle(document.querySelector('#balance-pill-usage')).opacity==='1' && getComputedStyle(document.querySelector('#balance-pill-usage')).transitionDuration.includes('0.22s')`), 'Usage circle has a completed expansion transition at '+size);
    check(state.regions.some(region=>region.id==='ui:balance-pill') && state.regions.some(region=>region.id==='ui:balance-usage'), 'Native hover regions at '+size);
    await pointer((state.panel.left+state.panel.right)/2,state.pill.bottom+3);
    check((await evaluate(read)).expanded==='true','Hover bridge at '+size);
    await pointer((state.panel.left+state.panel.right)/2,(state.panel.top+state.panel.bottom)/2);
    check((await evaluate(read)).expanded==='true','Dropdown remains open at '+size);
    await screenshot('balance-hover-'+size);
    await pointer(650,650);
    check((await evaluate(read)).expanded==='false','Hover leave at '+size);
    report.push({size,...state});
  }
  await evaluate(`window.__testAccount={balance:0.68,today_tokens:0};window.__testEmit('account-session-changed')`);
  await pause(200);
  let low=await evaluate(read);
  check(!low.hidden && low.low && low.pill.width>low.pill.height && low.expanded==='false' && low.panel.hidden, 'Low balance stays in the warning capsule state');
  check(await evaluate(`document.querySelector('.balance-pill__low-label').textContent.trim()==='余额不足'`), 'Low balance capsule label');
  await screenshot('balance-low');
  await pointer((low.pill.left+low.pill.right)/2,(low.pill.top+low.pill.bottom)/2);
  check((await evaluate(read)).expanded==='false', 'Low balance capsule does not open usage details');
  for(const [balance,today_tokens,expected] of [[12.35,null,'暂无法获取今日用量'],[12.35,987654321,'987,654,321 token']]) {
    await evaluate(`window.__testAccount=${JSON.stringify({balance,today_tokens})};window.__testEmit('account-session-changed')`);
    await pause(200);
    const state=await evaluate(read);
    check(!state.hidden && state.tokens===expected,'Usage data state '+expected);
  }
  await evaluate(`window.__testAccount=null;window.__testEmit('account-session-changed')`);
  await pause(200);
  check((await evaluate(read)).hidden,'Hide account data after logout');
  await evaluate(`window.__testAccount={balance:3.72,today_tokens:128400};window.__testEmit('account-session-changed');window.__testView.setScale(300)`);
  await pause(200);
  await evaluate(`document.getElementById('balance-pill-trigger').focus()`);
  await pause(150);
  check((await evaluate(read)).expanded==='true','Keyboard focus opens usage');
  await page('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});
  await page('Input.dispatchKeyEvent',{type:'keyUp',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});
  check((await evaluate(read)).expanded==='false','Escape dismisses usage');
  await evaluate(`document.activeElement.blur();window.__testScale=1.5;window.__testEmit('tauri://scale-change',{scaleFactor:1.5})`);
  await pause(200);
  let scaled=await evaluate(read);
  const region=scaled.regions.find(r=>r.id==='ui:balance-pill');
  check(region && region.x===Math.floor((scaled.pill.left-2)*1.5) && region.y===Math.floor((scaled.pill.top-2)*1.5),'150% native hover region scale');
  report.push({scaleFactor:1.5,pill:scaled.pill,region});
  await evaluate(`window.__testView.getHeadBounds=()=>({left:0,top:0,right:80,bottom:100})`);
  await pause(180);
  let edge=await evaluate(read);
  check(edge.pill.left===8 && edge.pill.top===8,'Top/left screen edge clamp');
  await pointer(edge.pill.left+20,edge.pill.top+15);
  edge=await evaluate(read);
  check(edge.panel.left>=8 && edge.panel.right<=692,'Dropdown stays inside left edge');
  await pointer(650,50);
  await evaluate(`window.__testView.getHeadBounds=()=>({left:660,top:665,right:700,bottom:700})`);
  await pause(180);
  edge=await evaluate(read);
  await pointer(edge.pill.left+20,edge.pill.top+15);
  edge=await evaluate(read);
  check(edge.panel.bottom<=edge.pill.bottom && edge.panel.top>=8 && edge.panel.left>=8 && edge.panel.right<=692,'Expanded circle stays visible at bottom edge');
  check(runtimeErrors.length===0,'Browser runtime errors');
  console.log(JSON.stringify({report,failures,runtimeErrors},null,2));
} finally {
  await writeFile(join(outputDir,'report.json'),JSON.stringify({report,failures,runtimeErrors},null,2));
  await call('Browser.close').catch(()=>{});
  socket.close();
  browser.kill();
}
if(failures.length)process.exitCode=1;
