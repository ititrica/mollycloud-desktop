// Isolated desktop UI regression check; requires the local Vite dev server.
// No real login, credentials, clipboard, or desktop pet state are used.
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const baseUrl = process.env.MOLLY_UI_URL || 'http://localhost:24320';
const productionSmoke = process.argv.includes('--production');
const outputDir = resolve(import.meta.dirname, '../../artifacts/design-review');
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
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Timed out: ${method}`)); }, 15000);
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
const readShell = `(() => {
  const rect = el => { const r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(n => Math.round(n * 100) / 100); };
  const css = (el, names) => Object.fromEntries(names.map(name => [name, getComputedStyle(el)[name]]));
  const workspace = document.querySelector('.workspace');
  const heading = document.querySelector('.page-heading');
  const active = document.querySelector('.nav-item.active');
  const content = [...document.querySelectorAll('.page-content')].find(el => el.getBoundingClientRect().width > 0);
  return {
    shell: {
      sidebar: rect(document.querySelector('.sidebar')),
      nav: [...document.querySelectorAll('.nav-item')].map(el => rect(el)),
      settingsButton: rect(document.querySelector('.sidebar-settings')),
      settingsStyle: css(document.querySelector('.sidebar-settings'), ['fontFamily','fontSize','fontWeight','height','borderRadius','padding','backgroundColor']),
      navStyle: css(active, ['fontFamily','fontSize','fontWeight','borderRadius','padding','backgroundColor','boxShadow','borderLeftWidth']),
      sidebarStyle: css(document.querySelector('.sidebar'), ['padding','backgroundColor']),
      workspaceStyle: css(workspace, ['backgroundColor','scrollbarGutter']),
      header: rect(document.querySelector('.topbar')),
      headerFont: css(heading.querySelector('h1'), ['fontFamily','fontSize','lineHeight']),
      actions: rect(document.querySelector('.topbar-actions')),
      contentX: rect(content)[0],
      contentPadding: getComputedStyle(content).paddingTop,
    },
    horizontalOverflow: document.documentElement.scrollWidth > innerWidth || workspace.scrollWidth > workspace.clientWidth,
    headerClipped: heading.querySelector('h1').scrollWidth > heading.querySelector('h1').clientWidth,
    selectedCount: document.querySelectorAll('.nav-item[aria-current="page"]').length,
    navCount: document.querySelectorAll('.nav-item').length,
    settingsVisible: (() => {
      const button = document.querySelector('.sidebar-settings').getBoundingClientRect();
      const sidebar = document.querySelector('.sidebar').getBoundingClientRect();
      const nav = document.querySelector('.sidebar nav').getBoundingClientRect();
      return button.width > 0 && button.height >= 40 && button.x >= sidebar.x && button.right <= sidebar.right && button.y >= nav.bottom && button.bottom <= innerHeight && button.bottom >= innerHeight - 60;
    })(),
    cardBorders: [...document.querySelectorAll('.stat-card, .panel, .table-panel, .usage-stat')].map(el => css(el, ['borderTopColor','borderRightColor','borderBottomColor','borderLeftColor','borderWidth','borderRadius'])),
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
    return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom,buttons,
      role:Boolean(role), focusInside:dialog.contains(document.activeElement),
      focusedElement:document.activeElement?.outerHTML.slice(0,1500),
      ancestors:(() => {const result=[]; for(let el=dialog; el; el=el.parentElement) result.push({tag:el.tagName,id:el.id,class:el.className,inert:el.inert,ariaHidden:el.getAttribute('aria-hidden')}); return result;})(),
      radioLabels:[...dialog.querySelectorAll('input[name="console-close-action"]')].map(el => ({value:el.value,label:el.closest('label')?.innerText,checked:el.checked})),
      maskFilter:maskStyle?.backdropFilter || maskStyle?.webkitBackdropFilter,
      shellFilter:getComputedStyle(document.querySelector('.app-shell')).filter,
      maskCoversViewport:Boolean(maskRect && maskRect.x <= 0 && maskRect.y <= 0 && maskRect.right >= innerWidth && maskRect.bottom >= innerHeight),
      outerOverflow:document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight};
  })()`);
  check(layout.x >= 0 && layout.y >= 0 && layout.right <= width && layout.bottom <= height && !layout.outerOverflow, `Settings dialog viewport bounds ${width}`);
  check(layout.maskCoversViewport && [layout.maskFilter,layout.shellFilter].some(value => /blur\((?!0(?:px)?\))/.test(value || '')), `Settings frosted glass backdrop ${width}`);
  check(layout.role && layout.focusInside, `Settings accessible modal focus ${width}`);
  check(layout.radioLabels.length === 2 && layout.radioLabels.some(radio => radio.value === 'quit' && radio.label?.includes('退出程序')) && layout.radioLabels.some(radio => radio.value === 'tray' && radio.label?.includes('最小化到托盘') && radio.checked), `Settings close action default/options ${width}`);
  check(layout.buttons.length === 2 && layout.buttons[0].text === '取消' && layout.buttons[1].text === '保存' && layout.buttons.every(button => button.height >= 40 && button.bottom <= layout.bottom && layout.bottom - button.bottom <= 48) && layout.buttons[1].x > layout.buttons[0].x && layout.right - layout.buttons[1].right <= 40, `Settings actions bottom right ${width}`);
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
  await clickSettingsButton('保存');
  const saved = await evaluate(readPreviewSettings);
  check(saved !== initialStorage && Boolean(saved?.includes('quit')), `Settings preview saves isolated preference ${width}`);
  check(await evaluate(`document.activeElement === document.querySelector('.sidebar-settings')`), `Settings save restores focus ${width}`);
  await openSettings();
  check(await evaluate(`document.querySelector('input[name="console-close-action"]:checked').value === 'quit'`), `Settings saved preference reopened ${width}`);
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

try {
  await page('Page.enable');
  await page('Runtime.enable');
  if (productionSmoke) {
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
      return { font: getComputedStyle(document.querySelector('.login-input')).fontFamily, primary: style.backgroundColor, text: style.color, disabled: document.querySelector('.primary-button').disabled, previewExcluded: !document.querySelector('.app-shell') };
    })()`);
    report.push({ page: 'production-login', ...smoke });
    check(smoke.font.includes('Microsoft YaHei') && smoke.primary === 'rgb(180, 233, 118)' && smoke.text === 'rgb(39, 80, 22)' && !smoke.disabled && smoke.previewExcluded, 'Production theme/entry initialization');
    await screenshot('production-login-1180');
  } else {
  const names = ['overview', 'subscriptions', 'keys', 'usage', 'assistant', 'ccswitch', 'images'];
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
    await verifyConsoleSettings(width, height);
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
      const metrics = await evaluate(readShell);
      report.push({ page: names[index], width, height, ...metrics });
      baseline ??= metrics.shell;
      check(JSON.stringify(metrics.shell) === JSON.stringify(baseline), `Shared shell differs: ${names[index]} ${width}`);
      check(!metrics.horizontalOverflow && !metrics.valueOverflows && !metrics.headerClipped, `Content clipped: ${names[index]} ${width}`);
      check(metrics.selectedCount === 1, `Navigation state: ${names[index]} ${width}`);
      check(metrics.navCount === 7 && metrics.settingsVisible, `Seven navigation items and bottom settings visible: ${names[index]} ${width}`);
      check(metrics.shell.navStyle.boxShadow === 'none' && metrics.shell.navStyle.borderLeftWidth === '0px', `Navigation edge color: ${names[index]} ${width}`);
      check(metrics.cardBorders.every(card => ['borderTopColor','borderRightColor','borderBottomColor','borderLeftColor'].every(side => card[side] === 'rgb(227, 234, 240)')), `Non-neutral card border: ${names[index]} ${width}`);
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
    await select(5);
    check(await evaluate(`document.querySelector('.ccswitch-frame').contentWindow.__mollyKeepAliveProbe === ${width}`), `CC Switch iframe remains mounted ${width}`);
    // A preview import must fail truthfully. Model a backend response only to test
    // success feedback and ID-based navigation; this is not a native import test.
    await select(2);
    await evaluate(`document.querySelector('.ccs-import-button').click()`);
    await pause(220);
    check(await evaluate(`document.querySelector('.page-alert')?.innerText.includes('预览模式不会导入真实密钥') && !document.querySelector('.ccs-import-actions').innerText.includes('已导入')`), `CC Switch preview import rejected ${width}`);
    await screenshot(`ccswitch-import-preview-error-${width}`);
    await mutate(`state.errorMessage = ''; state.importedProviders = { 'demo-key': {provider_id:'molly-preview-provider',app:'codex'} };`);
    check(await evaluate(`document.querySelector('.ccs-import-actions').innerText.includes('已导入')`), `CC Switch import response feedback ${width}`);
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
