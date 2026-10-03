(() => {
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  const wait = async (test, label) => {
    for (let i = 0; i < 200; i++) { if (await test()) return; await pause(100); }
    throw new Error(label);
  };
  if (window.parent !== window) {
    if (location.protocol !== 'molly-image:') return;
    const run = async () => {
      try {
        await wait(() => document.querySelector('header'), 'Image React UI did not load');
        await pause(300);
        let parentReadable = false;
        try { parentReadable = !!parent.document; } catch {}
        let ipc = 'UNAVAILABLE';
        if (window.__TAURI_INTERNALS__?.invoke) {
          ipc = await Promise.race([
            window.__TAURI_INTERNALS__.invoke('bootstrap_public').then(() => 'ALLOWED', () => 'DENIED'),
            pause(500).then(() => 'UNAVAILABLE'),
          ]);
        }
        const response = await fetch(window.__imageSmokeApi, {headers: {Authorization: 'Bearer mock-image-manual-only', Cookie: 'must-be-stripped'}});
        const text = await response.text();
        parent.postMessage({source: 'image-native-smoke', ok: true, parentReadable, ipc,
          url: location.href, status: response.status, bytes: text.length,
          tail: text.slice(-17), header: !!document.querySelector('header')}, '*');
      } catch (error) { parent.postMessage({source: 'image-native-smoke', ok: false, error: String(error)}, '*'); }
    };
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', run, {once:true});
    else void run();
    return;
  }
  const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
  const checks = [];
  const check = (condition, name) => {if (!condition) throw new Error(name); checks.push(name);};
  let finished = false;
  const report = result => {if (finished) return; finished=true; void invoke('smoke_report', {result: {...result, checks}});};
  let frame;
  let before;
  window.addEventListener('message', async event => {
    if (!frame || event.source !== frame.contentWindow || event.data?.source !== 'image-native-smoke') return;
    try {
      check(event.data.ok, event.data.error || 'Child probe completed');
      check(event.origin === 'null', 'Native image iframe has an opaque sandbox origin');
      check(!event.data.parentReadable, 'Sandbox cannot read the account console document');
      check(event.data.ipc !== 'ALLOWED', 'Sandbox cannot invoke general Tauri account commands');
      check(await invoke('bootstrap_calls') === before, 'Forbidden sandbox IPC never reaches the account handler');
      check(event.data.url.startsWith('molly-image://localhost/'), 'Production assets load through the macOS custom scheme');
      check(event.data.header, 'Embedded production React image UI loads');
      await wait(() => !document.querySelector('.embedded-load-state'), 'MessageChannel handshake did not settle');
      check(true, 'Restricted MessageChannel completes the real production handshake');
      check(event.data.status === 200, 'Manual image request reaches the loopback mock through native transport');
      check(event.data.bytes === 3 * 1024 * 1024 + 17 && event.data.tail === 'native-stream-end', 'Native 3 MB stream delivers every byte and final tail');
      const settings = JSON.parse(localStorage.getItem('molly-image-account-native-image-test')).state.settings;
      check(settings.profiles[0].apiKey === '', 'Account sentinel key is never automatically imported');
      check(settings.profiles[0].baseUrl === 'https://mollycloud.cn/v1', 'Original default image endpoint is retained');
      const savedFrame = frame;
      document.querySelector('.nav-item').click();
      document.querySelector('.nav-item[data-page="images"]').click();
      await pause(100);
      check(document.querySelector('.image-workbench-frame') === savedFrame, 'Navigation preserves the image iframe');
      report({ok:true, transport: {bytes:event.data.bytes, tail:event.data.tail}});
    } catch (error) {report({ok:false,error:String(error)});}
  });
  const start = async () => {
    try {
      await wait(() => document.querySelector('.nav-item[data-page="images"]'), 'Native console did not restore mock session');
      before = await invoke('bootstrap_calls');
      document.querySelector('.nav-item[data-page="images"]').click();
      await wait(() => document.querySelector('.image-workbench-frame'), 'Image navigation did not mount');
      frame = document.querySelector('.image-workbench-frame');
      check(!frame.sandbox.contains('allow-same-origin'), 'Production sandbox does not grant same-origin access');
      check(frame.src.startsWith('molly-image://localhost/'), 'macOS iframe URL uses the dedicated image scheme');
    } catch (error) {report({ok:false,error:String(error)});}
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, {once:true});
  else void start();
  setTimeout(() => report({ok:false,error:'Native image timeout', text:document.body?.innerText?.slice(-2000)}),45000);
})();
