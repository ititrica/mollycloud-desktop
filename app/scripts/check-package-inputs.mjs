import { createHash } from 'node:crypto';
import { readdir, readFile, lstat } from 'node:fs/promises';
import { resolve, relative, basename, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { targetPlatform } from './platform.mjs';

const appDir = resolve(fileURLToPath(new URL('..', import.meta.url)));
export function checkResourceConfig(config, platform = targetPlatform()) {
  const expected = ['../THIRD_PARTY_NOTICES.md', '../public/models/', ...(platform === 'windows' ? ['../../vendor/netspeed-dynamic/helpers/NSD_Fps_Plugin.exe', '../../vendor/netspeed-dynamic/helpers/NSD_Taskbar_Plugin.exe'] : [])];
  if (config.build?.frontendDist !== '../dist' || !Array.isArray(config.bundle?.resources)
    || config.bundle.resources.length !== expected.length || expected.some(path => !config.bundle.resources.includes(path))
    || (config.bundle.externalBin?.length ?? 0) > 0) {
    throw new Error('安装资源超出允许范围。仅分发 dist、第三方声明、内置模型及 Windows 专用的经校验灵动岛辅助组件；禁止包含用户配置或应用数据目录。');
  }
  if (platform === 'macos' && (config.app?.windows?.some(window => window.label === 'netspeed-widget') || !config.app?.macOSPrivateApi
    || JSON.stringify(config.bundle.targets) !== JSON.stringify(['app', 'dmg']))) throw new Error('macOS 必须使用透明桌宠 app/dmg 配置，且不能包含灵动岛窗口。');
}
export function isCredentialFile(path) {
  return /(^|[/\\])(?:\.env(?:\..*)?|api[_-]?key\.(?:bin|tmp)|credentials?(?:\.[^/\\]+)?|auth\.json|console-settings\.json|live2d-pet-settings|.*\.(?:sqlite3?|db|log))$/i.test(path)
    || /(^|[/\\])(?:node_modules|target|\.git|user-data|webview2|profiles?)([/\\]|$)/i.test(path);
}
export function containsCredential(text) {
  return /\b(?:sk-(?:proj-|or-v1-)?[A-Za-z0-9_-]{24,}|gsk_[A-Za-z0-9]{24,}|AIza[A-Za-z0-9_-]{30,})\b/.test(text);
}
async function inspect(directory, platform) {
  let count = 0;
  for (const name of await readdir(directory)) {
    const path = resolve(directory, name);
    const stat = await lstat(path);
    const label = relative(appDir, path);
    if (stat.isSymbolicLink() || isCredentialFile(label)) throw new Error(`禁止打包用户数据或链接：${label}`);
    if (stat.isDirectory()) { count += await inspect(path, platform); continue; }
    if (platform !== 'windows' && (name === 'island.html' || /^NetSpeedPanel[-.]/.test(name))) throw new Error(`macOS 资源包含 Windows 灵动岛入口：${label}`);
    if (/\.(?:exe|dll|com|scr|cpl|msi|bat|cmd|ps1)$/i.test(name)) {
      throw new Error(`前端与模型资源禁止夹带可执行程序或启动脚本：${label}`);
    }
    if (['.js', '.json', '.html', '.css', '.txt', '.md', '.map'].includes(extname(name))) {
      const text = await readFile(path, 'utf8');
      if (containsCredential(text)) throw new Error(`检测到疑似硬编码密钥：${label}（不输出密钥内容）`);
      if (platform !== 'windows' && extname(name) === '.js' && /plugin:molly-netspeed\||molly-netspeed:island|NSD_Fps_Plugin|NSD_Taskbar_Plugin/.test(text)) throw new Error(`macOS 资源包含 Windows 灵动岛实现：${label}`);
    }
    count++;
  }
  return count;
}
if (process.argv[1] && basename(process.argv[1]) === 'check-package-inputs.mjs') {
  const platform = targetPlatform();
  const config = JSON.parse(await readFile(resolve(appDir, 'src-tauri/tauri.conf.json'), 'utf8'));
  if (['macos', 'windows'].includes(platform)) {
    const specific = JSON.parse(await readFile(resolve(appDir, `src-tauri/tauri.${platform}.conf.json`), 'utf8'));
    Object.assign(config.app, specific.app);
    Object.assign(config.bundle, specific.bundle);
  }
  checkResourceConfig(config, platform);
  const helpers = {"NSD_Fps_Plugin.exe":"60c86f785a63f86d1e614a33676e20bbe54969657662c758a69cfe8b862bb810","NSD_Taskbar_Plugin.exe":"94c56b721c807461f7d886ab4ce99e80a0d0058cd22de1c90763da7209b88a3b"};
  for (const [name, hash] of platform === 'windows' ? Object.entries(helpers) : []) {
    const bytes = await readFile(resolve(appDir, '../vendor/netspeed-dynamic/helpers', name));
    if (createHash('sha256').update(bytes).digest('hex') !== hash) throw new Error('灵动岛辅助组件校验失败：' + name);
  }
  const directories = process.argv.includes('--built') ? ['public', 'dist'] : ['public'];
  for (const directory of directories) console.log(`安装资源检查（${platform}）：${directory}，${await inspect(resolve(appDir, directory), platform)} 个文件通过。`);
}
