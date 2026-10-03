import { existsSync } from 'node:fs';

// All callers create their own temporary profiles; this never attaches to a user's browser.
export function regressionBrowserPath() {
  const override = process.env.MOLLY_BROWSER_PATH || process.env.MOLLY_EDGE_PATH;
  if (override) return override;
  const candidates = process.platform === 'darwin' ? [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
  ] : process.platform === 'win32' ? [
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  ] : ['/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome'];
  const browser = candidates.find(existsSync);
  if (!browser) throw new Error('UI 回归需要 Chrome 或 Edge；可通过 MOLLY_BROWSER_PATH 指定浏览器可执行文件。');
  return browser;
}
