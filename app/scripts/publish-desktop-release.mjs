// Prepare locally by default. --publish requires a Cloudflare API token and
// writes the immutable installer before replacing latest.json in R2.
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { basename, resolve } from 'node:path';

const bucket = 'mollycloud-desktop';
const accountId = '50ea7ef5ffa2f5a9553caab2a4d3f1eb';
const publicOrigin = 'https://desktop.veriolink.com';
const appDir = resolve(import.meta.dirname, '..');
const option = name => { const index = process.argv.indexOf(name); return index < 0 ? undefined : process.argv[index + 1]; };
const version = option('--version');
const installer = option('--installer');
const notes = option('--notes') ?? '';
const publish = process.argv.includes('--publish');
const dryRunOutput = option('--out');
if (!version || !/^\d+\.\d+\.\d+$/.test(version) || !installer) {
  throw new Error('用法：npm run release:r2 -- --version 0.1.6 --installer <NSIS 安装包> [--notes 更新说明] [--publish]');
}
const file = resolve(installer);
const name = `MollyCloud_${version}_x64-setup.exe`;
const installerStat = await stat(file);
if (basename(file) !== name || !installerStat.isFile()) throw new Error('安装包文件名与版本不匹配');
if (publish) {
  if (dryRunOutput) throw new Error('--out 仅用于本地演练');
  const packageVersion = JSON.parse(await readFile(resolve(appDir, 'package.json'), 'utf8')).version;
  const tauriVersion = JSON.parse(await readFile(resolve(appDir, 'src-tauri/tauri.conf.json'), 'utf8')).version;
  if (packageVersion !== version || tauriVersion !== version) {
    throw new Error('发布版本必须与 MollyCloud 前端和 Tauri 包版本一致');
  }
  for (const path of ['src/App.vue', 'src/styles.css', 'src-tauri/src/update.rs', 'src-tauri/tauri.conf.json', 'dist/index.html']) {
    if ((await stat(resolve(appDir, path))).mtimeMs > installerStat.mtimeMs) {
      throw new Error(`安装包早于 ${path} 的修改，请重新打包后再发布`);
    }
  }
  if (!process.env.CLOUDFLARE_API_TOKEN) {
    throw new Error('发布需要 CLOUDFLARE_API_TOKEN；R2 Access Key ID 不能单独作为 Wrangler 凭据。');
  }
}
const url = `${publicOrigin}/${name}`;
const manifest = publish ? resolve(appDir, '../output/release', `latest-${version}.json`)
  : dryRunOutput ? resolve(dryRunOutput) : resolve(appDir, '../artifacts/update-dry-run', `latest-${version}.json`);

function run(program, args) {
  const result = spawnSync(program, args, { cwd: appDir, windowsHide: true, stdio: 'inherit', env: { ...process.env, CLOUDFLARE_ACCOUNT_ID: accountId } });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} 失败，退出码 ${result.status}`);
}
const manifestArgs = [resolve(appDir, 'scripts/create-release-manifest.mjs'), '--version', version,
  '--installer', file, '--download-url', url, '--out', manifest, '--notes', notes];
run(process.execPath, manifestArgs);
const release = JSON.parse(await readFile(manifest, 'utf8'));
console.log(`R2 目标：${bucket}/${name}，随后 ${bucket}/latest.json`);
if (!publish) {
  console.log('当前仅生成本地清单；核对安装包与版本后添加 --publish 才会上传。');
  process.exit(0);
}
const response = await fetch(`${publicOrigin}/latest.json?check=${Date.now()}`, { cache: 'no-store', signal: AbortSignal.timeout(10_000) });
if (response.ok) {
  const old = await response.json();
  const parts = value => /^\d+\.\d+\.\d+$/.test(value ?? '') ? value.split('.').map(Number) : null;
  const previous = parts(old.version);
  const next = parts(version);
  if (!previous) throw new Error('线上更新清单版本无效，已停止发布');
  if (previous[0] > next[0] || previous[0] === next[0] && (previous[1] > next[1] || previous[1] === next[1] && previous[2] >= next[2])) {
    throw new Error('新版本必须高于线上版本，已停止发布');
  }
} else if (response.status !== 404) {
  throw new Error(`线上更新清单返回 HTTP ${response.status}，已停止发布`);
}
const wranglerCli = [
  resolve(appDir, 'node_modules/wrangler/bin/wrangler.js'),
  ...(process.platform === 'win32' ? [resolve(process.env.APPDATA ?? '', 'npm/node_modules/wrangler/bin/wrangler.js')] : []),
].find(existsSync);
if (!wranglerCli) throw new Error('请先安装 Wrangler 4.x，再运行发布命令。');
run(process.execPath, [wranglerCli, 'r2', 'object', 'put', `${bucket}/${name}`, '--remote', '--file', file,
  '--content-type', 'application/vnd.microsoft.portable-executable',
  '--content-disposition', `attachment; filename=${name}`,
  '--cache-control', 'public, max-age=31536000, immutable']);
const head = await fetch(url, { method: 'HEAD', cache: 'no-store', signal: AbortSignal.timeout(15_000) });
if (!head.ok || Number(head.headers.get('content-length')) !== release.sizeBytes) {
  throw new Error('公开下载域名尚未提供预期大小的安装包，未发布 latest.json');
}
run(process.execPath, [wranglerCli, 'r2', 'object', 'put', `${bucket}/latest.json`, '--remote', '--file', manifest,
  '--content-type', 'application/json; charset=utf-8', '--cache-control', 'no-store, max-age=0']);
const verified = await fetch(`${publicOrigin}/latest.json?verify=${Date.now()}`, { cache: 'no-store', signal: AbortSignal.timeout(15_000) });
if (!verified.ok || (await verified.json()).sha256 !== release.sha256) {
  throw new Error('latest.json 已上传但公开域名校验失败，请检查 R2 域名/缓存配置');
}
console.log(`版本 ${version} 已发布，下载清单：${publicOrigin}/latest.json`);
