import { regressionBrowserPath } from "./browser-path.mjs";
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
const browser = spawn(regressionBrowserPath(), [
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
  const border = document.querySelector('.balance-pill__border-light');
  const borderStyle = getComputedStyle(border.querySelector('.balance-pill__border-mask'));
  const washStyle = getComputedStyle(border.querySelector('.balance-pill__border-sweep'));
  return { pill:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},
    head:window.__testHead, text:document.querySelector('#balance-pill-value').textContent.trim(), hidden:root.classList.contains('hidden'), low:root.classList.contains('balance-pill--low'),
    expanded:pill.getAttribute('aria-expanded'), tokens:document.querySelector('#balance-pill-tokens').textContent,
    fullBalance:document.querySelector('#balance-pill-full-value').textContent,
    panel:{left:p.left,top:p.top,right:p.right,bottom:p.bottom,hidden:panel.hidden},
    regions:window.__testRegions, borderLayers:document.querySelectorAll('.balance-pill__border-light').length,
    borderMask:borderStyle.maskComposite, flowDuration:getComputedStyle(border).animationDuration, lightLength:parseFloat(washStyle.width), rim:parseFloat(borderStyle.paddingTop), washGradient:washStyle.backgroundImage };
})()`;
async function captureBorderPreview(mode) {
  if(!process.argv.includes('--glow-preview')) return;
  await evaluate(`window.__testPreviewHeadMethod=window.__testView.getHeadBounds;window.__testPreviewHead=window.__testView.getHeadBounds();window.__testView.getHeadBounds=()=>window.__testPreviewHead`);
  await pause(250);
  const clip=await evaluate(`(() => {const r=document.querySelector('#balance-pill-trigger').getBoundingClientRect();return {x:Math.floor(r.left)-12,y:Math.floor(r.top)-12,width:Math.ceil(r.width)+24,height:Math.ceil(r.height)+24,scale:1};})()`);
  const period=await evaluate(`parseFloat(getComputedStyle(document.querySelector('.balance-pill__border-light')).animationDuration)*1000`);
  const frameCount=Math.ceil(period/100);
  const step=period/frameCount;
  await writeFile(join(outputDir,`border-motion-${mode}.json`),JSON.stringify({period,step,frameCount,clip}));
  for(const [theme,background] of [['light','#ffffff'],['dark','#121014']]) {
    await evaluate(`document.body.style.backgroundColor=${JSON.stringify(background)};document.querySelector('.balance-pill__border-light').style.visibility='hidden'`);
    let shot=await page('Page.captureScreenshot',{format:'png',clip});
    await writeFile(join(outputDir,`border-motion-${mode}-${theme}-off.png`),Buffer.from(shot.data,'base64'));
    await evaluate(`document.querySelector('.balance-pill__border-light').style.visibility=''`);
    for(let frame=0;frame<frameCount;frame++) {
      const time=frame*step;
      await evaluate(`document.querySelector('.balance-pill__border-light').getAnimations({subtree:true}).forEach(a=>{a.pause();a.currentTime=${time};})`);
      shot=await page('Page.captureScreenshot',{format:'png',clip});
      await writeFile(join(outputDir,`border-motion-${mode}-${theme}-${frame}.png`),Buffer.from(shot.data,'base64'));
    }
  }
  await evaluate(`document.body.style.backgroundColor='';window.__testView.getHeadBounds=window.__testPreviewHeadMethod;document.querySelector('.balance-pill__border-light').getAnimations({subtree:true}).forEach(a=>a.play())`);
}

async function verifyBorderMotion(label) {
  const result=await evaluate(`(() => {
    const border=document.querySelector('.balance-pill__border-light');
    const highlights=[...border.querySelectorAll('.balance-pill__border-sweep')].filter(el=>getComputedStyle(el).display!=='none');
    const animations=border.getAnimations({subtree:true});
    const period=parseFloat(getComputedStyle(border).animationDuration)*1000;
    const pill=document.querySelector('#balance-pill-trigger').getBoundingClientRect();
    const radius=(pill.height-1)/2, middle={x:pill.x+pill.width/2,y:pill.y+pill.height/2};
    const livePoints=highlights.map(el=>{const r=el.getBoundingClientRect();return {x:(r.left+r.right)/2,y:(r.top+r.bottom)/2};});
    const liveOppositeError=livePoints.length===2 ? Math.max(Math.abs(livePoints[0].x+livePoints[1].x-2*middle.x),Math.abs(livePoints[0].y+livePoints[1].y-2*middle.y)) : 0;
    let maxEdgeError=0, maxOppositeError=0;
    const samples=[];
    for(let i=0;i<=32;i++) {
      animations.forEach(a=>{a.pause();a.currentTime=period*i/32;});
      const points=highlights.map(el=>{const r=el.getBoundingClientRect();return {x:(r.left+r.right)/2,y:(r.top+r.bottom)/2};});
      for(const point of points) {
        const capX=Math.max(pill.left+0.5+radius,Math.min(point.x,pill.right-0.5-radius));
        maxEdgeError=Math.max(maxEdgeError,Math.abs(Math.hypot(point.x-capX,point.y-middle.y)-radius));
      }
      if(points.length===2) maxOppositeError=Math.max(maxOppositeError,Math.abs(points[0].x+points[1].x-2*middle.x),Math.abs(points[0].y+points[1].y-2*middle.y));
      samples.push(points.map(p=>Math.atan2(p.y-middle.y,p.x-middle.x)));
    }
    const turns=highlights.map(()=>0); let allCounterclockwise=true;
    for(let i=1;i<samples.length;i++) for(let h=0;h<highlights.length;h++) {
      let delta=samples[i][h]-samples[i-1][h];
      while(delta>Math.PI)delta-=2*Math.PI;
      while(delta<-Math.PI)delta+=2*Math.PI;
      allCounterclockwise&&=delta < -0.001;
      turns[h]+=delta;
    }
    animations.forEach(a=>{a.currentTime=0;a.play();});
    const perimeter=2*(pill.width-pill.height)+Math.PI*(pill.height-1);
    return {highlightCount:highlights.length,period,linearSpeed:perimeter/(period/1000),liveOppositeError,maxEdgeError,maxOppositeError,turns,allCounterclockwise};
  })()`);
  check(result.highlightCount===(label==='idle orb'?1:2), 'One idle highlight, two capsule highlights: '+label);
  check(Math.abs(result.linearSpeed-(label==='idle orb'?24.5:136))<0.1, 'Fixed border travel speed: '+label);
  check(result.maxEdgeError<0.25, 'Highlights follow the rounded border: '+label);
  if(result.highlightCount===2) check(result.liveOppositeError<0.25 && result.maxOppositeError<0.25, 'Highlights remain opposite during live playback and sampled phases: '+label);
  check(result.allCounterclockwise && result.turns.every(turn=>Math.abs(turn+Math.PI*2)<0.01), 'Visible highlights complete one counterclockwise lap without reversing: '+label);
  report.push({activity:'border-motion',label,...result});
}

try {
  await page('Page.enable');
  await page('Runtime.enable');
  await page('Emulation.setDeviceMetricsOverride', {width:700,height:700,deviceScaleFactor:1,mobile:false});
  await page('Page.addScriptToEvaluateOnNewDocument', { source: `
    localStorage.removeItem('petra-announced-version');
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
        if(['drag_start','drag_end','set_menu_open','open_console','open_codex_activity','add_codex_activity_home'].includes(command)) window.__testCommandLog.push({command,args});
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
        if(command==='get_codex_activity') return {status:null,tasks:[]};
        if(command==='get_api_key') return window.__testMolly ? 'mock-only-key' : '';
        if(command==='petra_assistant_chat') {
          if (!window.__testMolly) throw new Error('Unexpected assistant call in UI check');
          window.__testAssistantRequests.push(JSON.parse(JSON.stringify(args.body)));
          const last = args.body.messages.at(-1);
          if(last.role === 'tool') {
            window.__testAssistantResult = JSON.parse(last.content);
            return {choices:[{message:{content:'已读取悬浮窗显示信息。'}}]};
          }
          return {choices:[{message:{tool_calls:[{id:'test-floating-info',type:'function',function:{name:'get_floating_window_info',arguments:'{}'}}]}}]};
        }
        if(command==='get_assistant_config') return {enabled:true};
        if(command==='assistant_local_query') return {handled:false,content:null};
        if(command==='get_speech_settings') return {config:{enabled:false,provider:'mimo',baseUrl:'https://api.xiaomimimo.com/v1',model:'mimo-v2.5-tts',voice:'冰糖',volume:0.8},apiKeyConfigured:false};
        if(command==='save_pet_credentials') {
          if(window.__testKeySaveFails)throw new Error('测试：密钥保存失败');
          if(args.assistantApiKey!=null)window.__testApiKey=args.assistantApiKey;
          return {config:args.update.config,apiKeyConfigured:false};
        }
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
  for (let attempt = 0; attempt < 40 && !(await evaluate(`Boolean(document.querySelector('#balance-pill:not(.hidden)'))`)); attempt++) {
    if (await evaluate(`typeof window.__testEmit === 'function'`)) {
      await evaluate(`window.__testEmit('account-balance-updated', window.__testAccount)`);
    }
    await pause(100);
  }
  await ready('#balance-pill:not(.hidden)');
  check(!(await evaluate(`window.__testCommands.has('fetch_account_balance')`)), 'Pet balance is supplied by the console event');
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
  check(await evaluate(`{const children=[...document.querySelector('#menu').children]; children[0]?.classList.contains('mi') && children[0]?.querySelector('span:first-child')?.textContent.trim() === '打开控制台' && children[1]?.classList.contains('sep')}`), 'Open console is the first pet menu action');
  check(await evaluate(`document.querySelectorAll('#menu > .mi').length > 0 && !['模型','小助手设置','开机自启','今日抽卡','对话记录'].some(label => [...document.querySelectorAll('#menu > .mi > span:first-child')].some(el => el.textContent.trim() === label))`), 'Removed settings entries stay out of the pet menu');
  report.push({interaction:'model-context-menu',...interaction});
  await screenshot('model-right-click-menu');
  const openConsolePoint = await evaluate(`{const rect=document.querySelector('#menu > .mi').getBoundingClientRect(); ({x:rect.x+rect.width/2,y:rect.y+rect.height/2})}`);
  await mouseClick(openConsolePoint.x,openConsolePoint.y);
  interaction = await evaluate(readInteraction);
  check(!interaction.menuVisible && interaction.commandLog.some(entry => entry.command === 'open_console'), 'Open console menu action invokes the native console command');
  await mouseClick(menuPoint.x,menuPoint.y,'right');
  await pause(200);
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
    check(state.pill.width===40 && state.pill.height===40 && state.text==='3$' && state.borderLayers===1, 'Compact balance orb at '+size);
    check(state.borderMask.split(',').every(value=>value.trim()==='exclude') && Math.abs(parseFloat(state.flowDuration)-Math.PI*39/24.5)<0.001 && state.lightLength===5 && state.rim===5 && state.washGradient.includes('circle'), 'Round, slower white highlight on the idle orb at '+size);
    check(!state.hidden && !state.low && state.expanded==='false', 'Initial state at '+size);
    if(size===300) { await verifyBorderMotion('idle orb'); await captureBorderPreview('orb'); }
    await screenshot('balance-'+size);
    await pointer((state.pill.left+state.pill.right)/2,(state.pill.top+state.pill.bottom)/2);
    state=await evaluate(read);
    check(state.expanded==='true' && !state.panel.hidden && state.fullBalance==='3.72$' && state.tokens==='128,400 token', 'Hover usage at '+size);
    check(Math.abs((state.panel.right-state.panel.left)-(state.panel.bottom-state.panel.top))<0.1 && state.panel.left<state.pill.left && state.panel.bottom>state.pill.bottom, 'Usage expands as a larger circle toward bottom-left at '+size);
    for (let frame=0; frame<15 && !(await evaluate(`getComputedStyle(document.querySelector('#balance-pill-usage')).opacity==='1'`)); frame++) await pause(50);
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
  await evaluate(`window.__testAccount={balance:0.68,today_tokens:0};window.__testEmit('account-balance-updated', window.__testAccount)`);
  await pause(200);
  let low=await evaluate(read);
  check(!low.hidden && low.low && low.pill.width>low.pill.height && low.expanded==='false' && low.panel.hidden, 'Low balance stays in the warning capsule state');
  check(await evaluate(`document.querySelector('.balance-pill__low-label').textContent.trim()==='余额不足'`), 'Low balance capsule label');
  await screenshot('balance-low');
  await pointer((low.pill.left+low.pill.right)/2,(low.pill.top+low.pill.bottom)/2);
  check((await evaluate(read)).expanded==='false', 'Low balance capsule does not open usage details');
  for(const [balance,today_tokens,expected] of [[12.35,null,'暂无法获取今日用量'],[12.35,987654321,'987,654,321 token']]) {
    await evaluate(`window.__testAccount=${JSON.stringify({balance,today_tokens})};window.__testEmit('account-balance-updated', window.__testAccount)`);
    await pause(200);
    const state=await evaluate(read);
    check(!state.hidden && state.tokens===expected,'Usage data state '+expected);
  }
  await evaluate(`window.__testAccount=null;window.__testEmit('account-balance-updated', window.__testAccount)`);
  await pause(200);
  check((await evaluate(read)).hidden,'Hide account data after logout');
  await evaluate(`window.__testAccount={balance:3.72,today_tokens:128400};window.__testEmit('account-balance-updated', window.__testAccount);window.__testView.setScale(300)`);
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
  await evaluate(`window.__testNormalHead=window.__testView.getHeadBounds; window.__testView.getHeadBounds=()=>({left:0,top:0,right:80,bottom:100})`);
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
  check(!(await evaluate(`Boolean(document.querySelector('#announcement-panel'))`)), 'Fresh installation never shows upstream version announcement');
  await pointer(650,50);
  for (const [balance, expected] of [[9.9,'9$'],[99.99,'99$'],[100,'100$'],[128.9,'128$'],[1000,'1000$']]) {
    await evaluate(`window.__testEmit('account-balance-updated', {balance:${balance},today_tokens:128400})`);
    await pause(100);
    check(await evaluate(`document.querySelector('#balance-pill-value').textContent === ${JSON.stringify(expected)} && document.querySelector('#balance-pill-value').scrollWidth <= document.querySelector('#balance-pill-value').clientWidth`), 'Uncapped compact balance '+balance);
  }
  await screenshot('balance-three-and-four-digits');
  await evaluate(`window.__testView.getHeadBounds=window.__testNormalHead;
    window.__testEmit('account-balance-updated', {balance:128.9,today_tokens:128400});
    window.__activityTasks=[
      {key:'test:1',threadId:'01a09809-9dd4-7203-ae17-00575d41934a',turnId:'1',model:'test-model',provider:'test-provider',status:'running',progress:'thinking',steps:[{progress:'starting',at:'2026-09-16T10:00:00Z'},{progress:'thinking',at:'2026-09-16T10:00:01Z'}],unread:false,updatedAt:'2026-09-16T10:00:01Z'},
      {key:'test:2',threadId:'01a09810-9dd4-7203-ae17-00575d41934a',turnId:'2',model:'',provider:'',status:'blocked',progress:'blocked',unread:true,updatedAt:'2026-09-16T09:00:00Z'}
    ]; window.__testEmit('codex-activity-updated', {status:'blocked',tasks:window.__activityTasks})`);
  // Wait for the actual head-position transition, including slow render frames.
  await evaluate(`(async()=>{
    await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    await Promise.all(document.querySelector('#balance-pill').getAnimations().map(animation=>animation.finished.catch(()=>{})));
    await new Promise(resolve=>requestAnimationFrame(resolve));
  })()`);
  let activityState = await evaluate(read);
  report.push({activity:'initial-head-position',...activityState});
  check(activityState.pill.bottom<=activityState.head.top-12 && Math.abs((activityState.pill.left+activityState.pill.right-activityState.head.left-activityState.head.right)/2)<=1, 'Running capsule is centered above model head');
  check(activityState.pill.width>40 && activityState.pill.width<=280 && activityState.text==='128$', 'Capsule retains balance and fits content');
  check(await evaluate(`!document.querySelector('.codex-activity-badge') && document.querySelector('.balance-pill__progress').textContent==='正在思考' && document.querySelector('#balance-pill').dataset.activity==='running'`), 'No numeric badge; active task wins over unread terminal');
  await screenshot('codex-capsule-thinking');
  const shortWidth=activityState.pill.width;
  await evaluate(`document.querySelector('.balance-pill__progress').textContent='正在运行一个需要较长说明的命令并检查完整的任务执行结果和后续步骤';`);
  await pause(220);
  check(await evaluate(`document.querySelector('#balance-pill-trigger').getBoundingClientRect().width<=280 && document.querySelector('.balance-pill__progress').scrollWidth>document.querySelector('.balance-pill__progress').clientWidth && getComputedStyle(document.querySelector('.balance-pill__progress')).textOverflow==='ellipsis'`), 'Long progress is capped and truncated');
  await screenshot('codex-capsule-long-text');
  for (const [phase,label] of [['retrying','正在重试'],['command_running','正在运行命令'],['command_complete','运行了命令']]) {
    await evaluate(`window.__activityTasks[0].status='needs_input'; window.__activityTasks[0].progress='waiting_input'; window.__testEmit('codex-activity-updated', {status:'needs_input',tasks:window.__activityTasks})`);
    check(await evaluate(`document.querySelector('.balance-pill__progress').textContent==='等待你的输入' && document.querySelector('#balance-pill').dataset.activity==='needs_input'`), 'Question waiting state before '+phase);
    await evaluate(`window.__activityTasks[0].status='running'; window.__activityTasks[0].progress=${JSON.stringify(phase)}; window.__activityTasks[0].steps.push({progress:${JSON.stringify(phase)},at:'2026-09-16T10:00:02Z'}); window.__testEmit('codex-activity-updated', {status:'running',tasks:window.__activityTasks})`);
    await pause(250);
    check(await evaluate(`document.querySelector('.balance-pill__progress').textContent===${JSON.stringify(label)} && document.querySelector('#balance-pill').dataset.activity==='running'`), 'New progress replaces waiting: '+phase);
  }
  activityState=await evaluate(read);
  check(activityState.pill.width>=shortWidth && activityState.pill.width<280, 'Capsule shrinks back to actual text width');
  await pointer(activityState.pill.left+20,activityState.pill.top+20);
  await pause(300);
  activityState = await evaluate(read);
  report.push({activity:'codex-capsule', ...activityState});
  check(activityState.panel.hidden && activityState.expanded==='false', 'Running hover does not open a dropdown');
  await evaluate(`document.querySelector('#balance-pill-trigger').focus(); document.querySelector('#balance-pill-trigger').click()`);
  check((await evaluate(read)).panel.hidden && (await evaluate(read)).expanded==='false', 'Running focus and click do not open a dropdown');
  check(!activityState.regions.some(r=>r.id==='ui:balance-usage' || r.id==='ui:balance-bridge'), 'No hidden panel or bridge intercepts the pointer');
  check(await evaluate(`getComputedStyle(document.querySelector('#balance-pill-trigger')).opacity==='1' && !document.querySelector('.balance-pill__bridge')`), 'Capsule remains visible without a hover bridge');
  await evaluate(`document.querySelector('#balance-pill-trigger').blur(); window.__activityTasks[0].progress='thinking'; window.__activityTasks[0].progressTitle='Appending theme token CSS'; window.__testEmit('codex-activity-updated', {status:'running',tasks:window.__activityTasks})`);
  await pause(250);
  check(await evaluate(`document.querySelector('.balance-pill__progress').textContent==='Appending theme token CSS'`), 'Original Codex summary heading is shown verbatim');
  for (const [index,title] of ['正在思考', 'Appending theme token CSS', 'A very long public progress title that must be truncated at the capsule width limit'].entries()) {
    await evaluate(`window.__activityTasks[0].progressTitle=${JSON.stringify(title)}; window.__testEmit('codex-activity-updated', {status:'running',tasks:window.__activityTasks})`);
    await pause(250);
    check(await evaluate(`getComputedStyle(document.querySelector('.balance-pill__border-light')).display==='none'`), 'Capsule has no stationary highlights at any width: '+title);
    check(await evaluate(`getComputedStyle(document.querySelector('#balance-pill-trigger')).backdropFilter==='none' && getComputedStyle(document.querySelector('.balance-pill__usage')).backdropFilter==='none'`), 'Rounded floating surfaces do not create a backdrop rectangle');
    await screenshot('codex-border-width-'+index);
  }
  await evaluate(`window.__activityTasks[0].progressTitle='Appending theme token CSS'; window.__testEmit('codex-activity-updated', {status:'running',tasks:window.__activityTasks});document.body.style.backgroundColor='#121014'`);
  await screenshot('codex-border-dark-background');
  await evaluate(`document.body.style.backgroundColor=''`);
  await page('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]});
  check(await evaluate(`getComputedStyle(document.querySelector('.balance-pill__border-light')).display==='none'`), 'Capsule also has no highlights with reduced motion');
  await page('Emulation.setEmulatedMedia',{features:[]});
  // Exercise the real console-to-pet assistant event and tool dispatcher with
  // a local model response stub. No account login or network requests are used.
  await evaluate(`(async()=>{window.__testMolly=true; window.__testAssistantRequests=[]; const settings=JSON.parse(localStorage.getItem('live2d-pet-settings')); settings.assistant={...settings.assistant,enabled:true,model:'mock-model'}; localStorage.setItem('live2d-pet-settings',JSON.stringify(settings)); (await import('/src/petra/assistant/AssistantPanel.ts')).clearApiKeyCache();})()`);
  const queryMolly = async () => {
    await evaluate(`window.__testAssistantResult=null; window.__testEmit('petra-assistant-send',{text:'悬浮窗现在显示什么，任务完成了吗？'})`);
    for(let i=0;i<60 && !(await evaluate('window.__testAssistantResult'));i++) await pause(100);
    const result=await evaluate('window.__testAssistantResult');
    check(result?.ok, 'Molly reads floating information without account login');
    return result?.data;
  };
  let molly=await queryMolly();
  check(molly?.activity.text==='Appending theme token CSS' && molly?.balance.compact==='128$', 'Molly tool receives exact current title and displayed balance');
  check(await evaluate(`window.__testAssistantRequests[0].tools.some(t=>t.function.name==='get_floating_window_info')`), 'Floating info tool is included in assistant request');
  await evaluate(`window.__activityTasks[0].status='ready'; window.__activityTasks[0].progress='ready'; window.__testEmit('codex-activity-updated', {status:'ready',tasks:window.__activityTasks})`);
  await pause(250);
  check(await evaluate(`document.querySelector('.balance-pill__progress').textContent==='任务已完成' && document.querySelector('#balance-pill').classList.contains('balance-pill--capsule')`), 'Completion remains a head capsule');
  await screenshot('codex-completed-capsule');
  molly=await queryMolly();
  check(molly?.activity.text==='任务已完成' && molly?.activity.completionExpiresAt, 'Molly sees completion hold and deadline');
  await evaluate(`window.__activityTasks[0].status='running'; window.__activityTasks[0].progress='retrying'; window.__testEmit('codex-activity-updated', {status:'running',tasks:window.__activityTasks})`);
  check(await evaluate(`document.querySelector('.balance-pill__progress').textContent==='正在重试'`), 'New task immediately replaces completion');
  await evaluate(`window.__activityTasks[0].status='ready'; window.__activityTasks[0].progress='ready'; window.__testEmit('codex-activity-updated', {status:'ready',tasks:window.__activityTasks})`);
  await pause(10_000);
  await evaluate(`window.__testEmit('codex-activity-updated', {status:'ready',tasks:window.__activityTasks})`);
  check(await evaluate(`document.querySelector('.balance-pill__progress').textContent==='任务已完成'`), 'Completion still present at 10 seconds');
  await pause(10_350);
  check(await evaluate(`!document.querySelector('#balance-pill').classList.contains('balance-pill--capsule') && document.querySelector('#balance-pill-trigger').getAttribute('aria-label').includes('展开查看详情')`), 'Completion expires after 20 seconds despite duplicate snapshot');
  molly=await queryMolly();
  check(molly?.mode==='orb' && molly?.activity.status==='idle' && molly?.activity.completionExpiresAt===null, 'Molly and capsule agree after expiry');
  await evaluate(`(async()=>{const assistant=await import('/src/petra/assistant/AssistantPanel.ts');assistant.clearBubbles();})()`);
  for (const status of ['blocked','stopped']) {
    await evaluate(`window.__activityTasks[0].status=${JSON.stringify(status)}; window.__activityTasks[0].progress=${JSON.stringify(status)}; window.__activityTasks[0].unread=true; window.__testEmit('codex-activity-updated', {status:${JSON.stringify(status)},tasks:window.__activityTasks})`);
    await pointer(650,50);
    await pause(300);
    let idle=await evaluate(read);
    check(idle.pill.width===40 && idle.pill.height===40 && !(await evaluate(`document.querySelector('#balance-pill').classList.contains('balance-pill--capsule')`)), 'Terminal '+status+' restores small orb despite unread tasks');
    await pointer(idle.pill.left+20,idle.pill.top+20);
    await pause(250);
    idle=await evaluate(read);
    check(Math.abs(idle.panel.right-idle.panel.left-148)<1 && Math.abs(idle.panel.bottom-idle.panel.top-148)<1, 'Terminal '+status+' restores original round hover');
    check(await evaluate(`!document.querySelector('#balance-pill-usage').textContent.includes('Codex 任务') && !document.querySelector('.codex-activity-entry') && document.querySelector('#balance-pill-full-value').textContent==='128.90$' && document.querySelector('#balance-pill-tokens').textContent==='128,400 token'`), 'Idle round hover only shows balance and token usage');
  }
  await screenshot('codex-stopped-round-hover');
  check((await evaluate(read)).expanded==='true', 'Idle hover is open before a new task starts');
  await evaluate(`window.__activityTasks[0].status='running'; window.__activityTasks[0].progress='thinking'; window.__testEmit('codex-activity-updated', {status:'running',tasks:window.__activityTasks}); window.__testEmit('account-balance-updated',{balance:0.5})`);
  await pause(250);
  check((await evaluate(read)).panel.hidden, 'Starting a task immediately closes the idle hover');
  const lowCapsule = await evaluate(`({text:document.querySelector('#balance-pill-value').textContent,display:getComputedStyle(document.querySelector('#balance-pill-value')).display,width:document.querySelector('.balance-pill__orb').getBoundingClientRect().width})`);
  report.push({activity:'low-balance-capsule',...lowCapsule});
  check(lowCapsule.text==='0$' && lowCapsule.display!=='none' && Math.abs(lowCapsule.width-40)<0.5, 'Low balance stays visible in active capsule orb');
  await evaluate(`window.__testView.getHeadBounds=()=>({left:320,top:660,right:420,bottom:700})`);
  await pause(300);
  let bottomCapsule=await evaluate(read);
  await pointer(bottomCapsule.pill.left+20,bottomCapsule.pill.top+20);
  await pause(300);
  bottomCapsule=await evaluate(read);
  check(bottomCapsule.panel.hidden && bottomCapsule.expanded==='false' && bottomCapsule.pill.bottom<=692, 'Bottom-edge capsule remains visible without a dropdown');
  await pointer(650,50);
  await evaluate(`window.__testView.getHeadBounds=()=>({left:0,top:0,right:80,bottom:100})`);
  await pause(250);
  const topCapsule=await evaluate(read);
  check(topCapsule.pill.left>=8 && topCapsule.pill.top>=8, 'Capsule stays visible at top and left edge');
  await evaluate(`window.__testView.getHeadBounds=window.__testNormalHead`);
  await evaluate(`window.__testEmit('account-session-changed', {})`);
  await pause(150);
  check(!(await evaluate(`document.querySelector('#balance-pill').classList.contains('hidden')`)), 'Active progress remains visible without account balance');
  await evaluate(`window.__testEmit('codex-activity-updated', {status:null,tasks:[]})`);
  await pause(250);
  check(await evaluate(`document.querySelector('#balance-pill').classList.contains('hidden')`), 'No account or activity leaves orb hidden');
  check(runtimeErrors.length===0,'Browser runtime errors');
  console.log(JSON.stringify({report,failures,runtimeErrors},null,2));
} finally {
  await writeFile(join(outputDir,'report.json'),JSON.stringify({report,failures,runtimeErrors},null,2));
  await call('Browser.close').catch(()=>{});
  socket.close();
  browser.kill();
}
if(failures.length)process.exitCode=1;
