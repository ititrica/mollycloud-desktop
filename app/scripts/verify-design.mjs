// Isolated desktop UI regression check; requires the local Vite dev server.
// No real login, credentials, clipboard, or desktop pet state are used.
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const baseUrl = process.env.MOLLY_UI_URL || 'http://localhost:24320';
const productionSmoke = process.argv.includes('--production');
const darkMode = process.argv.includes('--dark');
const appearanceOnly = process.argv.includes('--appearance');
const outputDir = resolve(import.meta.dirname, '../../artifacts/design-review', appearanceOnly ? 'appearance' : darkMode ? 'dark' : 'light');
await mkdir(outputDir, { recursive: true });
const profile = await mkdtemp(join(tmpdir(), 'molly-design-check-'));
const browser = spawn(process.env.MOLLY_EDGE_PATH || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', [
  '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
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
  const { data } = await page('Page.captureScreenshot', { format: 'png' });
  await writeFile(join(outputDir, `${name}.png`), Buffer.from(data, 'base64'));
}
async function settleAnimations() {
  await evaluate(`(async () => {
    await new Promise(resolve => requestAnimationFrame(resolve));
    await Promise.all(document.getAnimations().filter(animation => Number.isFinite(animation.effect?.getComputedTiming().endTime)).map(animation => animation.finished.catch(() => {})));
    await new Promise(resolve => requestAnimationFrame(resolve));
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
async function select(index) {
  await evaluate(`document.querySelectorAll('.nav-item')[${index}].click()`);
  await pause(250);
}
async function verifySkills(width, height) {
  await waitFor(`!document.querySelector('.skills-load-state') && Boolean(document.querySelector('.skills-frame')?.contentDocument.querySelector('.molly-skills-tabs'))`, 'Skill Manager ready');
  const child = expression => evaluate(`(() => {const frame=document.querySelector('.skills-frame'); const d=frame.contentDocument; const w=frame.contentWindow; return (${expression});})()`);
  for (const route of ['/', '/my-skills', '/install', '/global-workspace', '/organizer', '/backup', '/settings']) {
    await child(`d.querySelector('a[href="#${route}"]').click()`);
    await pause(350);
    const metrics=await child(`(() => {const frameRect=frame.getBoundingClientRect(); const content=d.querySelector('.molly-skills-content'); return {route:w.location.hash, width:w.innerWidth, height:w.innerHeight, frameBottom:frameRect.bottom, outerScroll:document.querySelector('.workspace').scrollHeight > document.querySelector('.workspace').clientHeight+1, overflow:d.documentElement.scrollWidth>w.innerWidth || content.scrollWidth>content.clientWidth+1, theme:d.documentElement.dataset.theme, font:w.getComputedStyle(d.body).fontFamily, bg:w.getComputedStyle(d.body).backgroundColor, text:d.body.textContent.slice(0,180)};})()`);
    check(metrics.route === '#'+route && !metrics.overflow && !metrics.outerScroll && metrics.frameBottom <= height, `Skills content bounds ${route} ${width}`);
    check(metrics.theme === (darkMode ? 'dark' : 'light') && metrics.font.includes('Microsoft YaHei') && metrics.bg === (darkMode ? 'rgb(20, 26, 33)' : 'rgb(248, 251, 252)'), `Skills shared theme ${route} ${width}`);
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

async function verifyMcpGate(width) {
  await evaluate(`localStorage.removeItem('mollycloud:preview:mcp-experimental-ack:v1')`);
  await page('Page.navigate', {url: `${baseUrl}/?ui-preview=console`});
  await ready('.overview-page');
  await select(7);
  await ready('#mcp-experimental-title');
  check(await evaluate(`document.querySelector('.app-shell').inert && !document.querySelector('.mcp-market')`), `MCP gate blocks initial loading ${width}`);
  await screenshot(`mcp-first-use-${width}`);
  await evaluate(`[...document.querySelectorAll('.console-settings-dialog button')].find(b=>b.textContent.trim()==='暂不进入').click()`);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'MCP cancel');
  check(await evaluate(`!localStorage.getItem('mollycloud:preview:mcp-experimental-ack:v1') && document.activeElement?.getAttribute('aria-label') === 'MCP 市场'`), `MCP cancellation keeps gate and returns focus ${width}`);
  await select(7);
  await ready('#mcp-experimental-title');
  await evaluate(`[...document.querySelectorAll('.console-settings-dialog button')].find(b=>b.textContent.trim()==='我已知晓').click()`);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'MCP acknowledged');
  await page('Page.navigate', {url: `${baseUrl}/?ui-preview=console&preview-page=mcp`});
  await ready('.mcp-market');
  check(await evaluate(`!document.querySelector('#mcp-experimental-title') && localStorage.getItem('mollycloud:preview:mcp-experimental-ack:v1')==='acknowledged' && !localStorage.getItem('mollycloud:mcp-experimental-ack:v1')`), `MCP acknowledgement survives reload with preview isolation ${width}`);
  await select(0);
}

async function verifyMcpMarket(width, height) {
  await waitFor(`document.querySelectorAll('.mcp-card').length === 21`, 'MCP install templates');
  check(await evaluate(`document.querySelector('.mcp-market').textContent.includes('目录预览')`), `MCP browser boundary ${width}`);
  await evaluate(`(() => { const el=document.querySelector('[aria-label="搜索 MCP"]'); el.value='modelcontextprotocol filesystem'; el.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await waitFor(`document.querySelectorAll('.mcp-card').length === 1`, 'MCP multiword search');
  check(await evaluate(`document.querySelector('.mcp-card').dataset.mcpId === 'filesystem'`), `MCP package search ${width}`);
  await evaluate(`(() => { const button=[...document.querySelector('.mcp-card').querySelectorAll('button')].find(el=>el.textContent.includes('安装到 Agent')); button.focus(); button.click(); })()`);
  await ready('.mcp-install-dialog');
  const layout=await evaluate(`(() => { const dialog=document.querySelector('.mcp-install-dialog'); const r=dialog.getBoundingClientRect(); const body=dialog.querySelector('.mcp-install-body'); return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,overflow:dialog.scrollWidth>dialog.clientWidth,scroll:body.scrollHeight>body.clientHeight,inert:document.querySelector('.app-shell').inert,targets:dialog.querySelectorAll('.mcp-target').length}; })()`);
  check(layout.x>=0 && layout.y>=0 && layout.right<=width && layout.bottom<=height && !layout.overflow && layout.inert && layout.targets===4, `MCP install dialog bounds and targets ${width}`);
  await screenshot(`mcp-install-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'MCP close releases host');
  await mutate(`state.query=''; state.source='awesome';`, '.mcp-market');
  check(await evaluate(`document.querySelectorAll('.mcp-card').length === 24 && [...document.querySelectorAll('.mcp-card')].every(card=>card.textContent.includes('从来源安装') && !card.textContent.includes('安装到 Agent'))`), `Community entries resolve source before installation ${width}`);
  await evaluate(`[...document.querySelector('.mcp-card').querySelectorAll('button')].find(el=>el.textContent.includes('从来源安装')).click()`);
  await waitFor(`document.querySelector('.mcp-modal-error')?.textContent.includes('桌面端')`, 'Source resolution respects desktop boundary');
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'Source resolution close');
  await mutate(`state.origin={name:'Source example',homepage:'https://example.test'}; state.plans=[{planId:'test',label:'npm · @example/mcp @ 1.2.3',sourceUrl:'https://example.test/README.md',entry:{id:'source-test',name:'Source example',description:'从来源解析的安装选项',source:'awesome',fields:[{key:'token',label:'API Key',kind:'secret',required:true}],recipe:{kind:'npm',package:'@example/mcp',version:'1.2.3',args:['--token','{token}']}}}];state.choosePlan('test');`, '.mcp-market');
  await ready('.mcp-install-dialog');
  check(await evaluate(`document.querySelector('[aria-label="API Key"]').type==='password' && document.querySelectorAll('.mcp-target').length===4 && document.querySelector('.mcp-plan-preview').textContent.includes('@example/mcp')`), `Source plan preview and secret fields ${width}`);
  await screenshot(`mcp-source-plan-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'Source plan close');
  await mutate(`state.removeTarget={name:'Source example',agent:'codex',canUpdate:true};state.cleanPackage=true;`, '.mcp-market');
  await ready('.mcp-install-dialog');
  check(await evaluate(`document.querySelector('.mcp-install-dialog').textContent.includes('回收站') && document.querySelector('.mcp-install-dialog .n-checkbox').getAttribute('aria-checked')==='true'`), `Uninstall preserves data and defaults to package cleanup ${width}`);
  await screenshot(`mcp-uninstall-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'Uninstall close');
  await mutate(`state.source='curated';`, '.mcp-market');
  await evaluate(`[...document.querySelector('.mcp-toolbar').querySelectorAll('button')].find(el=>el.textContent.trim()==='手动添加').click()`);
  await ready('.mcp-install-dialog');
  await evaluate(`(() => { const name=document.querySelector('[aria-label="MCP 名称"]'); name.value='preview-test'; name.dispatchEvent(new Event('input',{bubbles:true})); document.querySelector('.mcp-target .n-checkbox').click(); })()`);
  await evaluate(`[...document.querySelector('.mcp-install-dialog').querySelectorAll('button')].find(el=>el.textContent.trim()==='安装到所选 Agent').click()`);
  await waitFor(`document.querySelector('.mcp-modal-error')?.textContent.includes('浏览器预览不能修改')`, 'MCP preview rejects config writes');
  await screenshot(`mcp-preview-error-${width}`);
  await key('Escape','Escape',27);
  await waitFor(`!document.querySelector('.app-shell').inert`, 'MCP preview close');
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
  check(compact.classApplied && compact.sidebar.width === 76 && compact.togglePressed === 'true' && compact.toggleLabel === '展开菜单栏', `Compact sidebar state ${width}`);
  check(expanded.titlebar.height === 32 && expanded.titlebarText === '' && expanded.windowControls === 3 && compact.titlebarText === '' && compact.windowControls === 3 && expanded.toggle.y >= expanded.titlebar.y && expanded.toggle.bottom <= expanded.titlebar.bottom, `Custom titlebar controls and content ${width}`);
  check(expanded.projectBarText === 'MollyCloud' && expanded.brand.display === 'flex' && expanded.brand.text === 'MollyCloud' && expanded.brand.textDisplay !== 'none' && compact.brand.display === 'flex' && compact.brand.textDisplay === 'none' && expanded.toggleBackground === 'rgba(0, 0, 0, 0)' && compact.toggleBackground === 'rgba(0, 0, 0, 0)', `Sidebar project bar content ${width}`);
  check(compact.nav.length === 9 && compact.nav.every((item, index) => item.rect.height === 56 && item.label && item.textHidden && item.iconVisible && Math.abs(item.rect.y - expanded.nav[index].rect.y) < 0.1) && Math.abs(compact.settings.rect.y - expanded.settings.y) < 0.1 && compact.settings.label === '设置' && compact.settings.textHidden && compact.settings.rect.bottom <= height && !compact.horizontalOverflow, `Compact sidebar icons, fixed vertical positions, and bounds ${width}`);
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
    shell: {
      sidebar: rect(document.querySelector('.sidebar')),
      nav: [...document.querySelectorAll('.nav-item')].map(el => rect(el)),
      settingsButton: rect(document.querySelector('.sidebar-settings')),
      themeButtonAbsent: !document.querySelector('.sidebar-theme'),
      settingsStyle: css(document.querySelector('.sidebar-settings'), ['fontFamily','fontSize','fontWeight','height','borderRadius','padding','backgroundColor']),
      navStyle: css(active, ['fontFamily','fontSize','fontWeight','borderRadius','padding','backgroundColor','boxShadow','borderLeftWidth']),
      sidebarStyle: css(document.querySelector('.sidebar'), ['padding','backgroundColor']),
      workspaceStyle: css(workspace, ['backgroundColor']),
    },
    header: topbar ? rect(topbar) : null,
    headerFont: heading ? css(heading.querySelector('h1'), ['fontFamily','fontSize','lineHeight']) : null,
    actions: topbar ? rect(document.querySelector('.topbar-actions')) : null,
    accountLayout: topbar ? (() => { const balance=rect(document.querySelector('.balance-chip')); const user=rect(document.querySelector('.user-chip')); const refresh=rect(document.querySelector('.account-refresh-button')); return {balance,user,refresh}; })() : null,
    content: rect(content),
    contentPadding: getComputedStyle(content).paddingTop,
    horizontalOverflow: document.documentElement.scrollWidth > innerWidth || workspace.scrollWidth > workspace.clientWidth,
    headerClipped: heading ? heading.querySelector('h1').scrollWidth > heading.querySelector('h1').clientWidth : false,
    selectedCount: document.querySelectorAll('.nav-item[aria-current="page"]').length,
    navCount: document.querySelectorAll('.nav-item').length,
    settingsVisible: (() => {
      const button = document.querySelector('.sidebar-settings').getBoundingClientRect();
      const sidebar = document.querySelector('.sidebar').getBoundingClientRect();
      const nav = document.querySelector('.sidebar nav').getBoundingClientRect();
      return button.width > 0 && button.height >= 40 && button.x >= sidebar.x && button.right <= sidebar.right && button.y >= nav.bottom && button.bottom <= innerHeight && button.bottom >= innerHeight - 60;
    })(),
    cardBorders: [...document.querySelectorAll('.stat-card, .panel, .table-panel, .usage-stat, .mcp-card, .mcp-installed-card')].map(el => css(el, ['borderTopColor','borderRightColor','borderBottomColor','borderLeftColor','borderWidth','borderRadius'])),
    contentFont: getComputedStyle(content).fontFamily,
    valueOverflows: [...document.querySelectorAll('.usage-stat__value, .token-category dd')].filter(el => el.scrollWidth > el.clientWidth).length,
    tableInternalScroll: [...document.querySelectorAll('.n-data-table *')].some(el => el.scrollWidth > el.clientWidth && ['auto','scroll'].includes(getComputedStyle(el).overflowX)),
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
      autostart:dialog.querySelector('[aria-label="开机自启动"]')?.getAttribute('aria-checked'),
      autostartMinimized:dialog.querySelector('[aria-label="启动后最小化到托盘"]')?.getAttribute('aria-checked'),
      autostartMinimizedDisabled:(() => {const el=dialog.querySelector('[aria-label="启动后最小化到托盘"]'); return Boolean(el && (el.matches(':disabled') || el.getAttribute('aria-disabled') === 'true' || el.classList.contains('n-switch--disabled')));})(),
      maskFilter:maskStyle?.backdropFilter || maskStyle?.webkitBackdropFilter,
      shellFilter:getComputedStyle(document.querySelector('.app-shell')).filter,
      maskCoversViewport:Boolean(maskRect && maskRect.x <= 0 && maskRect.y <= 0 && maskRect.right >= innerWidth && maskRect.bottom >= innerHeight),
      outerOverflow:document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight};
  })()`);
  check(layout.x >= 0 && layout.y >= 0 && layout.right <= width && layout.bottom <= height && !layout.outerOverflow, `Settings dialog viewport bounds ${width}`);
  check(layout.maskCoversViewport && [layout.maskFilter,layout.shellFilter].some(value => /blur\((?!0(?:px)?\))/.test(value || '')), `Settings frosted glass backdrop ${width}`);
  check(layout.role && layout.focusInside, `Settings accessible modal focus ${width}`);
  check(layout.radioLabels.length === 2 && layout.radioLabels.some(radio => radio.value === 'quit' && radio.label?.includes('退出程序')) && layout.radioLabels.some(radio => radio.value === 'tray' && radio.label?.includes('最小化到托盘') && radio.checked), `Settings close action default/options ${width}`);
  check(layout.autostart === 'false', `Settings autostart default ${width}`);
  check(layout.autostartMinimized === 'false' && layout.autostartMinimizedDisabled, `Settings minimized autostart default dependency ${width}`);
  check(layout.buttons.length === 2 && layout.buttons[0].text === '取消' && layout.buttons[1].text === '保存' && layout.buttons.every(button => button.height >= 40 && button.bottom <= layout.bottom && layout.bottom - button.bottom <= 48) && layout.buttons[1].x > layout.buttons[0].x && layout.right - layout.buttons[1].right <= 40, `Settings actions bottom right ${width}`);
  check(layout.release?.text === 'v0.1.3更多版本客户端密码 7s3y' && layout.release.downloadLabel === '下载更多版本客户端' && layout.release.x >= layout.x + 32 && layout.release.downloadRight < layout.buttons[0].x && layout.release.bottom <= layout.bottom, `Settings version and client download ${width}`);
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
  check(await evaluate(`{const el=document.querySelector('[aria-label="启动后最小化到托盘"]'); !el.matches(':disabled') && el.getAttribute('aria-disabled') !== 'true' && !el.classList.contains('n-switch--disabled')}`), `Settings minimized autostart enabled with autostart ${width}`);
  await evaluate(`document.querySelector('[aria-label="启动后最小化到托盘"]').click()`);
  await clickSettingsButton('保存');
  const saved = await evaluate(readPreviewSettings);
  check(saved !== initialStorage && Boolean(saved?.includes('quit')) && Boolean(saved?.includes('"autostart":true')) && Boolean(saved?.includes('"autostartMinimized":true')), `Settings preview saves isolated preference ${width}`);
  check(await evaluate(`document.activeElement === document.querySelector('.sidebar-settings')`), `Settings save restores focus ${width}`);
  await openSettings();
  check(await evaluate(`document.querySelector('input[name="console-close-action"]:checked').value === 'quit'`), `Settings saved preference reopened ${width}`);
  check(await evaluate(`document.querySelector('[aria-label="开机自启动"]').getAttribute('aria-checked') === 'true'`), `Settings saved autostart reopened ${width}`);
  check(await evaluate(`{const el=document.querySelector('[aria-label="启动后最小化到托盘"]'); el.getAttribute('aria-checked') === 'true' && !el.matches(':disabled') && el.getAttribute('aria-disabled') !== 'true' && !el.classList.contains('n-switch--disabled')}`), `Settings saved minimized autostart reopened ${width}`);
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

async function verifyPetSettings(width, height) {
  const open = async () => {
    await evaluate(`document.querySelector('.assistant-settings-button').focus(); document.querySelector('.assistant-settings-button').click()`);
    await ready('.pet-settings-fields');
  };
  const close = async text => {
    await evaluate(`([...document.querySelectorAll('.pet-settings-dialog .console-settings-actions button')].find(el => el.innerText.trim() === ${JSON.stringify(text)})).click()`);
    await waitFor(`!document.querySelector('.pet-settings-dialog')`, `Pet settings ${text}`);
  };
  await open();
  const layout = await evaluate(`(() => {
    const dialog = document.querySelector('.pet-settings-dialog');
    const r = dialog.getBoundingClientRect(); const body = dialog.querySelector('.console-settings-body');
    const footer = dialog.querySelector('.console-settings-actions').getBoundingClientRect();
    const mask = document.querySelector('.n-modal-mask');
    return { x:r.x,y:r.y,right:r.right,bottom:r.bottom,innerScroll:body.scrollHeight > body.clientHeight,footerBottom:footer.bottom,
      inert:document.querySelector('.app-shell').inert,focusInside:dialog.contains(document.activeElement),glass:getComputedStyle(mask).backdropFilter,
      modelActions:['模型设置','模型大小','调整模型边界','显示边框','显示模型边框','模型调节（测试）'].every(label => dialog.innerText.includes(label)),
      plainToggle:getComputedStyle(document.querySelector('.assistant-toggle-cell')).backgroundColor === 'rgba(0, 0, 0, 0)'};
  })()`);
  check(layout.x >= 0 && layout.y >= 0 && layout.right <= width && layout.bottom <= height && layout.footerBottom <= height && layout.innerScroll, `Pet settings fixed frame and internal scroll ${width}`);
  check(layout.inert && layout.focusInside && layout.glass.includes('blur(9px)') && layout.modelActions && layout.plainToggle, `Pet settings migrated controls and shared glass ${width}`);
  await screenshot(`pet-settings-model-${width}`);
  const initial = await evaluate(`localStorage.getItem('mollycloud:preview:pet-settings')`);
  const setScale = async value => evaluate(`(() => { const el = document.querySelector('input[aria-label="模型大小"]'); el.value = ${JSON.stringify(String(value))}; el.dispatchEvent(new Event('input', {bubbles:true})); })()`);
  await setScale(1.35);
  await close('取消');
  check(await evaluate(`localStorage.getItem('mollycloud:preview:pet-settings')`) === initial, `Pet settings cancel discards draft ${width}`);
  check(await evaluate(`document.activeElement === document.querySelector('.assistant-settings-button')`), `Pet settings restores gear focus ${width}`);
  await open();
  await setScale(1.25);
  await close('保存');
  check(await evaluate(`JSON.parse(localStorage.getItem('mollycloud:preview:pet-settings')).modelScale`) === 1.25, `Pet settings saves model scale ${width}`);
  await open();
  await evaluate(`document.querySelectorAll('.pet-settings-tabs button')[1].click()`);
  await ready('#pet-api-key');
  check(await evaluate(`document.querySelector('#pet-api-key').value === '' && document.querySelector('#pet-api-key').type === 'password'`), `Pet key field starts empty and masked ${width}`);
  await screenshot(`pet-settings-assistant-${width}`);
  await evaluate(`(() => {
    const input = document.querySelector('#pet-persona'); input.value = '保存失败时保留草稿'; input.dispatchEvent(new Event('input', {bubbles:true}));
    window.__petOriginalSetItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key, value) { if(key === 'mollycloud:preview:pet-settings') throw new Error('测试：存储不可用'); return window.__petOriginalSetItem.call(this,key,value); };
  })()`);
  await evaluate(`document.querySelector('.pet-settings-dialog .console-settings-actions button:last-child').click()`);
  await ready('.pet-settings-dialog .console-settings-error');
  check(await evaluate(`document.querySelector('#pet-persona').value === '保存失败时保留草稿' && document.querySelector('.app-shell').inert`), `Pet settings failure keeps draft and dialog ${width}`);
  await evaluate(`Storage.prototype.setItem = window.__petOriginalSetItem; delete window.__petOriginalSetItem`);
  await key('Escape', 'Escape', 27);
  await waitFor(`!document.querySelector('.pet-settings-dialog')`, 'Pet settings escape closes');
  check(await evaluate(`JSON.parse(localStorage.getItem('mollycloud:preview:pet-settings')).assistant.persona !== '保存失败时保留草稿'`), `Pet settings failure did not overwrite saved data ${width}`);
  report.push({page:'pet-settings',width,height,...layout});
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
  await select(8);
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
    await select(6);
  }
  await select(0);
  const colors = await evaluate(`({ background:getComputedStyle(document.body).backgroundColor, text:getComputedStyle(document.body).color })`);
  check(colors.background === 'rgb(20, 26, 33)' && colors.text === 'rgb(229, 237, 245)', 'Dark shared tokens applied');
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

try {
  await page('Page.enable');
  await page('Runtime.enable');
  await page('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: darkMode ? 'dark' : 'light' }] });
  if (appearanceOnly) {
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
    check(smoke.font.includes('Microsoft YaHei') && smoke.primary === 'rgb(180, 233, 118)' && smoke.text === 'rgb(39, 80, 22)' && !smoke.disabled && smoke.previewExcluded, 'Production theme/entry initialization');
    check(smoke.theme === (darkMode ? 'dark' : 'light') && smoke.background === (darkMode ? 'rgb(20, 26, 33)' : 'rgb(248, 251, 252)'), 'Production system appearance');
    await screenshot('production-login-1180');
  } else {
  const names = ['overview', 'subscriptions', 'keys', 'usage', 'assistant', 'ccswitch', 'images', 'mcp', 'skills'];
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
    await verifyConsoleSettings(width, height);
    await verifyMcpGate(width);
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
      check(!metrics.horizontalOverflow && !metrics.valueOverflows && !metrics.headerClipped, `Content clipped: ${names[index]} ${width}`);
      if (['assistant', 'ccswitch', 'images'].includes(names[index])) {
        check(metrics.header === null && metrics.content[1] === metrics.titlebar[1] + metrics.titlebar[3] && metrics.content[3] === height - metrics.titlebar[3] && metrics.contentPadding === '0px', `Embedded page fills workspace: ${names[index]} ${width}`);
      } else {
        check(metrics.header !== null, `Shared topbar missing: ${names[index]} ${width}`);
        check(Math.abs(metrics.accountLayout.balance[1] - metrics.accountLayout.user[1]) < 0.1 && Math.abs(metrics.accountLayout.balance[3] - metrics.accountLayout.user[3]) < 0.1 && Math.abs(metrics.accountLayout.refresh[0] + metrics.accountLayout.refresh[2] - metrics.accountLayout.user[0] - metrics.accountLayout.user[2]) < 0.1 && Math.abs(metrics.accountLayout.refresh[1] - metrics.accountLayout.user[1] - metrics.accountLayout.user[3] - 8) < 0.1, `Account controls alignment ${names[index]} ${width}`);
      }
      check(metrics.selectedCount === 1, `Navigation state: ${names[index]} ${width}`);
      check(metrics.navCount === 9 && metrics.settingsVisible, `Nine navigation items and bottom settings visible: ${names[index]} ${width}`);
      check(metrics.shell.navStyle.boxShadow === 'none' && metrics.shell.navStyle.borderLeftWidth === '0px', `Navigation edge color: ${names[index]} ${width}`);
      check(metrics.cardBorders.every(card => parseFloat(card.borderWidth) === 0 || ['borderTopColor','borderRightColor','borderBottomColor','borderLeftColor'].every(side => card[side] === (darkMode ? 'rgb(48, 62, 77)' : 'rgb(227, 234, 240)'))), `Non-neutral card border: ${names[index]} ${width}`);
      check(await evaluate(`document.documentElement.dataset.theme === '${darkMode ? 'dark' : 'light'}'`), `Resolved theme: ${names[index]} ${width}`);
      check(metrics.shell.themeButtonAbsent, `Theme shortcut removed: ${names[index]} ${width}`);
      if (names[index] === 'assistant') {
        check(await evaluate(`document.querySelector('.assistant-chat__toolbar h2').textContent === 'Molly' && !document.querySelector('.assistant-chat__toolbar .section-kicker')`), `Molly chat heading ${width}`);
      }
      if (names[index] === 'keys' && width === 960) check(metrics.tableInternalScroll, `API table internal scrolling ${width}`);
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
      if (names[index] === 'mcp') await verifyMcpMarket(width, height);
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
    // Round trip catches scroll position/layout changes caused by leaving chat.
    await select(0);
    check(JSON.stringify((await evaluate(readShell)).shell) === JSON.stringify(baseline), `Shell round trip ${width}`);
    await mutate(`state.desktopUpdate = { version: '0.1.2', downloadUrl: 'https://desktop.veriolink.com/MollyCloud_0.1.2_x64-setup.exe', notes: '设计预览' };`);
    const updateNotice = await evaluate(`(() => {
      const notice = document.querySelector('.update-available');
      const balance = document.querySelector('.balance-chip');
      const actions = document.querySelector('.topbar-actions');
      const rect = el => { const r = el.getBoundingClientRect(); return { x:r.x, y:r.y, right:r.right, bottom:r.bottom, width:r.width, height:r.height }; };
      return { notice: notice && rect(notice), balance: balance && rect(balance), actions: actions && rect(actions), text: notice?.innerText.trim(), title: notice?.getAttribute('title'), horizontalOverflow: document.documentElement.scrollWidth > innerWidth || document.querySelector('.workspace').scrollWidth > document.querySelector('.workspace').clientWidth };
    })()`);
    check(updateNotice.notice && updateNotice.text === '软件可更新' && updateNotice.title?.includes('0.1.2') && updateNotice.notice.right <= updateNotice.balance.x && !updateNotice.horizontalOverflow, `Desktop update indicator placement ${width}`);
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
    check(await evaluate(`document.querySelector('.ccs-import-actions').innerText.includes('再次导入')`), `CC Switch import response feedback ${width}`);
    await evaluate(`document.querySelector('.ccs-import-actions button[aria-label]').click()`);
    await waitFor(`Boolean(document.querySelector('.ccswitch-frame').contentDocument.querySelector('[data-provider-id="molly-preview-provider"][data-molly-selected="true"]'))`, `CC Switch imported provider navigation ${width}`);
    check(await evaluate(`document.querySelector('.nav-item[aria-current="page"]').innerText === 'CC Switch'`), `CC Switch open action ${width}`);
    await screenshot(`ccswitch-open-provider-${width}`);
    await select(0);
    await mutate(`state.dashboard.user.balance = 3.72;`);
    const reminder = await evaluate(`(() => { const el = document.querySelector('.account-reminder'); const s = getComputedStyle(el); return { innerBorder: Boolean(el.querySelector('.n-alert__border')), borders: [s.borderTopColor,s.borderRightColor,s.borderBottomColor,s.borderLeftColor], radius: s.borderRadius, overflow: s.overflow }; })()`);
    check(!reminder.innerBorder && new Set(reminder.borders).size === 1 && reminder.overflow === 'hidden', `Balance reminder edge ${width}`);
    await screenshot(`balance-reminder-${width}`);
    await select(1);
    await mutate(`state.dashboard.subscriptions.subscriptions[0].expires_at = new Date(Date.now() + 86400000).toISOString();`);
    check(await evaluate(`Boolean(document.querySelector('.account-reminder--subscription'))`), `Subscription reminder ${width}`);
    await screenshot(`subscription-reminder-${width}`);
    await mutate(`state.dashboard.subscriptions.subscriptions = [];`);
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
    report.push({ page: 'states', width, reminder, chat, scroll, wrapping });
  }
  }
  check(runtimeErrors.length === 0, 'Browser runtime exception');
  console.log(JSON.stringify({ screenshots: outputDir, checks: report.length, failures, runtimeErrors }, null, 2));
} finally {
  await writeFile(join(outputDir, productionSmoke ? 'report-production.json' : 'report.json'), JSON.stringify({ report, failures, runtimeErrors }, null, 2));
  await call('Browser.close').catch(() => {});
  socket.close();
  browser.kill();
}
if (failures.length) process.exitCode = 1;
