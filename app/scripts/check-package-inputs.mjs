import { readdir, readFile, lstat } from 'node:fs/promises';
import { resolve, relative, basename, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = resolve(fileURLToPath(new URL('..', import.meta.url)));
export function checkResourceConfig(config) {
  const expected = ['../THIRD_PARTY_NOTICES.md', '../public/models/'];
  if (config.build?.frontendDist !== '../dist' || !Array.isArray(config.bundle?.resources)
    || config.bundle.resources.length !== expected.length || expected.some(path => !config.bundle.resources.includes(path))) {
    throw new Error('安装资源超出允许范围。仅分发 dist、第三方声明和内置模型；禁止包含用户配置或应用数据目录。');
  }
}
export function isCredentialFile(path) {
  return /(^|[/\\])(?:\.env(?:\..*)?|api[_-]?key\.(?:bin|tmp)|credentials?(?:\.[^/\\]+)?|auth\.json|console-settings\.json|live2d-pet-settings|.*\.(?:sqlite3?|db|log))$/i.test(path)
    || /(^|[/\\])(?:node_modules|target|\.git|user-data|webview2|profiles?)([/\\]|$)/i.test(path);
}
export function containsCredential(text) {
  return /\b(?:sk-(?:proj-|or-v1-)?[A-Za-z0-9_-]{24,}|gsk_[A-Za-z0-9]{24,}|AIza[A-Za-z0-9_-]{30,})\b/.test(text);
}
async function inspect(directory) {
  let count = 0;
  for (const name of await readdir(directory)) {
    const path = resolve(directory, name);
    const stat = await lstat(path);
    const label = relative(appDir, path);
    if (stat.isSymbolicLink() || isCredentialFile(label)) throw new Error(`禁止打包用户数据或链接：${label}`);
    if (stat.isDirectory()) { count += await inspect(path); continue; }
    if (['.js', '.json', '.html', '.css', '.txt', '.md', '.map'].includes(extname(name))) {
      if (containsCredential(await readFile(path, 'utf8'))) throw new Error(`检测到疑似硬编码密钥：${label}（不输出密钥内容）`);
    }
    count++;
  }
  return count;
}
if (process.argv[1] && basename(process.argv[1]) === 'check-package-inputs.mjs') {
  checkResourceConfig(JSON.parse(await readFile(resolve(appDir, 'src-tauri/tauri.conf.json'), 'utf8')));
  const directories = process.argv.includes('--built') ? ['public', 'dist'] : ['public'];
  for (const directory of directories) console.log(`安装资源检查：${directory}，${await inspect(resolve(appDir, directory))} 个文件通过。`);
}
