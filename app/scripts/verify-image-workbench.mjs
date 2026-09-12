// Isolated browser/mock verification. No real account, API key or paid requests.
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import assert from 'node:assert/strict';
import { createServer as createTcpServer } from 'node:net';

const native = process.argv.includes('--native');
const output = resolve(import.meta.dirname, `../../artifacts/image-workbench${native ? '-native' : ''}`);
await mkdir(output, { recursive: true });
const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a5V8AAAAASUVORK5CYII=';
const calls = [];
let cancelledConnection = false;
const mock = createServer(async (request, response) => {
  response.setHeader('Access-Control-Allow-Origin', '*');
  response.setHeader('Access-Control-Allow-Headers', '*');
  if (request.method === 'OPTIONS') { response.writeHead(204).end(); return; }
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  calls.push({ url: request.url, method: request.method, auth: request.headers.authorization, type: request.headers['content-type'], body: Buffer.concat(chunks).toString() });
  response.setHeader('Content-Type', 'application/json');
  if (request.url === '/v1/slow') {
    response.flushHeaders();
    response.on('close', () => { if (!response.writableEnded) cancelledConnection = true; });
    setTimeout(() => response.end('{}'), 1000); return;
  }
  if (request.url === '/v1/stream') {
    response.setHeader('Content-Type', 'text/event-stream');
    response.write('data: first\n\n');
    setTimeout(() => response.end('data: second\n\n'), 500); return;
  }
  if (request.url === '/v1/error') { response.writeHead(401).end('{"error":{"message":"Mock unauthorized"}}'); return; }
  response.end(JSON.stringify({ created: Math.floor(Date.now() / 1000), data: [{ b64_json: png }] }));
});
await new Promise(resolve => mock.listen(0, '127.0.0.1', resolve));
const api = `http://127.0.0.1:${mock.address().port}/v1`;
const restoreOnly = process.argv.includes('--restore-check');
const profile = process.env.MOLLY_IMAGE_CHECK_PROFILE || await mkdtemp(join(tmpdir(), 'molly-image-check-'));
const reservation = createTcpServer();
await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
const debugPort = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const browser = native
  ? spawn(resolve(import.meta.dirname, '../src-tauri/target/debug/examples/image_workbench_smoke.exe'), [], { windowsHide: true, env: { ...process.env, MOLLY_IMAGE_SMOKE_PROFILE: profile, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}` }, stdio: ['ignore', 'pipe', 'pipe'] })
  : spawn('C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', ['--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
const endpoint = native ? await (async () => {
  for (let i=0;i<200;i++) {
    const version = await fetch(`http://127.0.0.1:${debugPort}/json/version`).then(r=>r.json()).catch(()=>null);
    if (version?.webSocketDebuggerUrl) return version.webSocketDebuggerUrl;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  throw new Error('Native WebView2 debug endpoint did not start');
})() : await new Promise((resolve, reject) => {
  let log = '';
  browser.on('error', reject);
  browser.stderr.on('data', chunk => { log += chunk; const match = log.match(/DevTools listening on (ws:\/\/[^\s]+)/); if (match) resolve(match[1]); });
});
const socket = new WebSocket(endpoint);
await new Promise(resolve => { socket.onopen = resolve; });
const pending = new Map();
const contexts = new Map();
const errors = [];
let counter = 0;
socket.onmessage = ({ data }) => {
  const message = JSON.parse(data);
  if (message.method === 'Runtime.executionContextCreated') contexts.set(`${message.sessionId}-${message.params.context.id}`, { ...message.params.context, session: message.sessionId });
  if (message.method === 'Runtime.executionContextDestroyed') contexts.delete(`${message.sessionId}-${message.params.executionContextId}`);
  if (message.method === 'Runtime.executionContextsCleared') for (const [key, value] of contexts) if (value.session === message.sessionId) contexts.delete(key);
  if (message.method === 'Runtime.exceptionThrown') errors.push(message.params.exceptionDetails);
  if (message.method === 'Target.attachedToTarget') void call('Runtime.enable', {}, message.params.sessionId);
  const handler = pending.get(message.id);
  if (handler) { pending.delete(message.id); message.error ? handler.reject(new Error(JSON.stringify(message.error))) : handler.resolve(message.result); }
};
function call(method, params = {}, sessionId) {
  return new Promise((resolve, reject) => { const id = ++counter; pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); });
}
const { targetId } = native
  ? (await call('Target.getTargets')).targetInfos.find(target=>target.type==='page')
  : await call('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
const page = (method, params = {}) => call(method, params, sessionId);
async function evaluate(expression, context) {
  const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture: true, ...(context ? { contextId: context.id } : {}) }, context?.session ?? sessionId);
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(expression, context) {
  for (let i = 0; i < 200; i++) { if (await evaluate(expression, context)) return; await pause(100); }
  throw new Error(`Timeout: ${expression}`);
}
async function childContext() {
  for (let i = 0; i < 150; i++) {
    for (const context of contexts.values()) {
      if (!context.auxData?.isDefault) continue;
      if (await evaluate(`location.href.includes('/image-workbench/') || location.hostname === 'molly-image.localhost'`, context).catch(() => false)) return context;
    }
    await pause(100);
  }
  throw new Error('Missing image iframe context');
}
async function screenshot(name) { const { data } = await page('Page.captureScreenshot', { format: 'png' }); await writeFile(join(output, `${name}.png`), Buffer.from(data, 'base64')); }
try {
  await page('Runtime.enable');
  await page('Page.enable');
  await page('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  await page('Emulation.setDeviceMetricsOverride', { width: 1180, height: 760, deviceScaleFactor: 1, mobile: false });
  const base = process.env.MOLLY_UI_URL || 'http://localhost:24320';
  await page('Page.navigate', { url: native ? 'http://tauri.localhost/' : `${base}/?ui-preview=console&preview-page=images` });
  const openPage = async () => {
    await waitFor(`document.querySelectorAll('.nav-item').length===7`);
    await evaluate(`document.querySelectorAll('.nav-item')[6].click()`);
    await waitFor(`!!document.querySelector('.image-workbench-frame')`);
  };
  await openPage();
  await waitFor(`!!document.querySelector('.image-workbench-frame')`);
  let child = await childContext();
  await waitFor(`!!document.querySelector('header')`, child);
  await screenshot('initial');
  assert.equal(await evaluate(`(()=>{try{return !!parent.document}catch{return false}})()`, child), false, 'iframe must not read the console');
  if (native) {
    const before = await evaluate(`window.__TAURI_INTERNALS__.invoke('bootstrap_calls')`);
    const probe = await evaluate(`Promise.race([window.__TAURI_INTERNALS__.invoke('bootstrap_public').then(()=> 'ALLOWED',()=> 'DENIED'),new Promise(r=>setTimeout(()=>r('UNAVAILABLE'),500))])`, child);
    assert.notEqual(probe, 'ALLOWED', 'sandbox must not invoke host account commands');
    assert.equal(await evaluate(`window.__TAURI_INTERNALS__.invoke('bootstrap_calls')`), before, 'sandbox request never reaches native account handler');
  }
  else assert.equal(await evaluate(`typeof window.__TAURI_INTERNALS__`, child), 'undefined');
  await waitFor(`!document.querySelector('.embedded-load-state')`);
  const readSettings = `JSON.parse(localStorage.getItem('molly-image-${native ? 'account-native-smoke-user' : 'preview-demo%40mollycloud.cn'}')).state.settings`;
  let settings = await evaluate(readSettings);
  if (restoreOnly) {
    assert.equal(settings.profiles[0].apiKey, 'sk-molly-image-test-only');
    assert.ok(settings.profiles[0].baseUrl.startsWith('http://127.0.0.1:'));
    await waitFor(`document.querySelectorAll('img').length>0`, child);
    assert.deepEqual(errors, []);
    console.log('PASS: manual credentials, custom endpoint and generated history survive a full native process restart');
    await writeFile(join(output, 'restart-report.json'), JSON.stringify({ passed: true, errors }, null, 2));
  } else {
  assert.equal(settings.profiles[0].baseUrl, 'https://mollycloud.cn/v1');
  assert.equal(settings.profiles[0].apiKey, '');
  assert.equal(settings.profiles[0].model, 'gpt-image-2');
  await evaluate(`document.querySelector('button[aria-label="设置"]').click()`, child);
  await waitFor(`!!document.querySelector('input[placeholder="sk-..."]')`, child);
  const input = async (selector, value) => {
    await evaluate(`(()=>{const el=document.querySelector(${JSON.stringify(selector)});el.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,${JSON.stringify(value)});el.dispatchEvent(new Event('input',{bubbles:true}));})()`, child);
    await pause(100);
    await evaluate(`document.querySelector(${JSON.stringify(selector)}).blur()`, child);
    await pause(100);
  };
  await input('input[placeholder="https://mollycloud.cn/v1"]', api);
  await input('input[placeholder="sk-..."]', 'sk-molly-image-test-only');
  await evaluate(`[...document.querySelectorAll('button')].find(x=>x.textContent==='保存密钥').click()`, child);
  await waitFor(`document.body.innerText.includes('密钥已保存')`, child);
  settings = await evaluate(readSettings);
  assert.equal(settings.profiles[0].apiKey, 'sk-molly-image-test-only');
  assert.equal(settings.profiles[0].baseUrl, api);
  await screenshot('manual-settings');
  await page('Page.reload');
  await openPage();
  await waitFor(`!!document.querySelector('.image-workbench-frame')`);
  child = await childContext();
  await waitFor(`!!document.querySelector('header')`, child);
  await waitFor(`!document.querySelector('.embedded-load-state')`);
  settings = await evaluate(readSettings);
  assert.equal(settings.profiles[0].apiKey, 'sk-molly-image-test-only', 'manual key survives reload');
  assert.equal(settings.profiles[0].baseUrl, api, 'custom endpoint is not replaced by preset');
  await evaluate(`window.testKeepAlive='same-frame';document.querySelector('[contenteditable="true"]').textContent='A small test image';document.querySelector('[contenteditable="true"]').dispatchEvent(new Event('input',{bubbles:true}))`, child);
  await pause(200);
  await evaluate(`[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')==='生成图像'&&!x.disabled).click()`, child);
  await waitFor(`document.querySelectorAll('img').length>0`, child);
  await pause(300);
  await screenshot('generated');
  const downloadDir = await mkdtemp(join(output, 'download-'));
  await call('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: downloadDir });
  await evaluate(`document.querySelector('img').click()`, child);
  await waitFor(`!!document.querySelector('button[aria-label="下载图片"]')`, child);
  await evaluate(`document.querySelector('button[aria-label="下载图片"]').click()`, child);
  let downloads = [];
  for (let i = 0; i < 100; i++) { downloads = await readdir(downloadDir); if (downloads.some(name=>name.endsWith('.png'))) break; await pause(100); }
  assert.ok(downloads.some(name=>name.endsWith('.png')), 'original download action saves an image');
  await evaluate(`document.querySelector('button[aria-label="关闭"]').click()`, child);
  assert.equal(calls.at(-1).url, '/v1/images/generations');
  assert.equal(calls.at(-1).auth, 'Bearer sk-molly-image-test-only');
  assert.equal(JSON.parse(calls.at(-1).body).prompt, 'A small test image');
  const fileInput = await call('Runtime.evaluate', { expression: `document.querySelector('input[type="file"]')`, contextId: child.id }, child.session);
  await call('DOM.setFileInputFiles', { objectId: fileInput.result.objectId, files: [join(downloadDir, downloads.find(name=>name.endsWith('.png')))] }, child.session);
  await waitFor(`document.querySelectorAll('img').length>=2`, child);
  await screenshot('reference-upload');
  await evaluate(`[...document.querySelectorAll('button')].find(x=>x.getAttribute('aria-label')==='生成图像'&&!x.disabled).click()`, child);
  for (let i=0;i<100 && calls.at(-1).url !== '/v1/images/edits';i++) await pause(100);
  assert.equal(calls.at(-1).url, '/v1/images/edits', 'uploaded reference uses original image editing flow');
  assert.ok(calls.at(-1).type.includes('multipart/form-data'));
  await evaluate(`document.querySelectorAll('.nav-item')[0].click()`);
  await evaluate(`document.querySelectorAll('.nav-item')[6].click()`);
  assert.equal(await evaluate(`window.testKeepAlive`, child), 'same-frame');
  const transport = await evaluate(`(async()=>{
    const response=await fetch('${api}/stream'); const reader=response.body.getReader();
    const started=performance.now(); const first=new TextDecoder().decode((await reader.read()).value);
    const early=performance.now()-started<400; const second=new TextDecoder().decode((await reader.read()).value);
    const controller=new AbortController();const slow=await fetch('${api}/slow',{signal:controller.signal});const cancelled=slow.text().then(()=> 'completed',error=>error.name+':'+error.message);controller.abort();
    const error=await fetch('${api}/error');
    const form=new FormData();form.append('image[]',new Blob([new Uint8Array([1,2,3])],{type:'image/png'}),'reference.png');form.append('mask',new Blob([new Uint8Array([4,5])],{type:'image/png'}),'mask.png');
    await fetch('${api}/images/edits',{method:'POST',headers:{Authorization:'Bearer manual-edit-key'},body:form});
    return {first,second,early,cancelled:await cancelled,errorStatus:error.status,errorBody:await error.json()};
  })()`, child);
  // WebView2 may normalize a cancelled Response body to TypeError; both the
  // rejected body and the closed upstream connection are required here.
  assert.ok(transport.first.includes('first') && transport.second.includes('second') && transport.early && transport.cancelled !== 'completed');
  assert.ok(cancelledConnection, 'cancel closes the in-flight request at the mock server');
  assert.equal(transport.errorStatus, 401);
  const edit = calls.at(-1);
  assert.equal(edit.url, '/v1/images/edits');
  assert.ok(edit.type.includes('multipart/form-data') && edit.body.includes('reference.png') && edit.body.includes('mask.png'));
  await page('Page.reload');
  await openPage();
  await waitFor(`!!document.querySelector('.image-workbench-frame')`);
  child = await childContext();
  await waitFor(`document.querySelectorAll('img').length>0`, child);
  await screenshot('restored-history');
  if (native) {
    await evaluate(`window.__TAURI_INTERNALS__.invoke('set_mock_account')`);
    await page('Page.reload');
    await openPage();
  } else await evaluate(`(()=>{const state=document.querySelector('.app-shell').__vueParentComponent.setupState;state.dashboard={...state.dashboard,user:{id:'another-image-test-user',email:'other@example.com',balance:12}}})()`);
  await pause(400);
  child = await childContext();
  await waitFor(`!!document.querySelector('header')`, child);
  await waitFor(`!document.querySelector('.embedded-load-state')`);
  assert.equal(await evaluate(`document.querySelectorAll('img').length`, child), 0, 'other account has no previous images');
  const other = await evaluate(`JSON.parse(localStorage.getItem('molly-image-${native ? 'account' : 'preview'}-another-image-test-user')).state.settings`);
  assert.equal(other.profiles[0].apiKey, '', 'other account has no previous key');
  assert.equal(other.profiles[0].baseUrl, 'https://mollycloud.cn/v1');
  await evaluate(`window.imageOriginalSetItem=Storage.prototype.setItem;window.imageSaveFailures=0;Storage.prototype.setItem=function(key,value){if(key.startsWith('molly-image-')){window.imageSaveFailures++;throw new DOMException('Test full storage','QuotaExceededError')}return window.imageOriginalSetItem.call(this,key,value)}`);
  await evaluate(`document.querySelector('button[aria-label="设置"]').click()`, child);
  await waitFor(`!!document.querySelector('input[placeholder="sk-..."]')`, child);
  await input('input[placeholder="sk-..."]', 'sk-failed-save-test');
  await evaluate(`[...document.querySelectorAll('button')].find(x=>x.textContent==='保存密钥').click()`, child);
  await waitFor(`document.body.innerText.includes('保存失败')`, child);
  await pause(150);
  assert.ok(await evaluate(`window.imageSaveFailures<12`), 'save failure feedback must not create a persistence loop');
  assert.equal(await evaluate(`JSON.parse(localStorage.getItem('molly-image-${native ? 'account' : 'preview'}-another-image-test-user')).state.settings.profiles[0].apiKey`), '', 'failed save must not replace persisted key');
  await screenshot('save-error');
  await evaluate(`Storage.prototype.setItem=window.imageOriginalSetItem`);
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'report.json'), JSON.stringify({ profile, passed: ['opaque sandbox isolation', 'empty manual key and default endpoint', 'manual save', 'persistent custom endpoint and key', 'real UI generation against mock', 'original image download', 'original reference upload and edit generation', 'keepalive', 'stream chunks', 'cancel', 'HTTP errors', 'multipart reference and mask', 'history restored', 'account isolation', 'save failure preserves stored key without loops'], errors, transport, requests: calls.map(({url,method,type})=>({url,method,type})) }, null, 2));
  console.log('PASS: image workbench integration, persistence, mock generation, streaming, cancellation and account isolation');
  }
} finally {
  await call('Browser.close').catch(() => undefined);
  socket.close();
  if (native) browser.kill();
  mock.closeAllConnections(); mock.close();
}
