// Stages local release files only. Uploading/publishing is a separate user action.
import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { basename, resolve } from 'node:path';

const option = name => { const index = process.argv.indexOf(name); return index < 0 ? undefined : process.argv[index + 1]; };
const installer = option('--installer');
const output = option('--out');
const notes = option('--notes')?.trim();
if (!installer || !output) throw new Error('用法：npm run release:macos -- --installer <MollyCloud_版本_aarch64|x64|universal.dmg> --out <本地归档目录> [--notes 说明]');
const file = resolve(installer);
const name = basename(file);
const match = /^MollyCloud_(\d+\.\d+\.\d+)_(aarch64|x64|universal)\.dmg$/.exec(name);
if (!match) throw new Error('macOS 安装包文件名或架构无效。');
const [, version, architecture] = match;
const appDir = resolve(import.meta.dirname, '..');
for (const config of ['package.json', 'src-tauri/tauri.conf.json']) {
  if (JSON.parse(await readFile(resolve(appDir, config), 'utf8')).version !== version) throw new Error('安装包版本必须与前端和 Tauri 版本一致。');
}
if (!(await stat(file)).isFile()) throw new Error('安装包不是普通文件。');
const bytes = await readFile(file);
if (!bytes.length) throw new Error('安装包为空。');
const sha256 = createHash('sha256').update(bytes).digest('hex');
const manifest = { version, downloadUrl: `https://desktop.veriolink.com/${name}`, publishedAt: new Date().toISOString(), sha256, sizeBytes: bytes.length, ...(notes ? { notes } : {}) };
const directory = resolve(output);
await mkdir(directory, { recursive: true });
if (file !== resolve(directory, name)) await copyFile(file, resolve(directory, name));
await writeFile(resolve(directory, 'SHA256SUMS.txt'), `${sha256}  ${name}\n`);
const targets = architecture === 'universal' ? ['aarch64', 'x86_64'] : [architecture === 'x64' ? 'x86_64' : 'aarch64'];
for (const target of targets) await writeFile(resolve(directory, `latest-macos-${target}.json`), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`macOS 本地发行归档已生成：${directory}（未上传或发布）`);
