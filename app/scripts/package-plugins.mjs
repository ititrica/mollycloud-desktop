// Build distributable, adapted UI packages. Native adapters are version-gated
// by adapterApi; upstream desktop installers are never executed as plugins.
import { readFile, writeFile, mkdir, mkdtemp, cp, rm, lstat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
const app = resolve(import.meta.dirname, '..');
const definitions = JSON.parse((await readFile(join(app, 'plugins/definitions.json'), 'utf8')).replace(/^\uFEFF/, ''));
const output = resolve(app, '../artifacts/plugin-release');
await mkdir(output, { recursive: true });
const catalog = { schema: 1, plugins: [] };
const sources = {ccswitch:'vendor/cc-switch/frontend',images:'vendor/gpt-image-playground',skills:'vendor/skills-manager/backend'};
const python = process.env.MOLLY_PYTHON || 'python';
const archive = `import pathlib,sys,zipfile
source=pathlib.Path(sys.argv[1])
with zipfile.ZipFile(sys.argv[2],'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
 for f in sorted(source.rglob('*')):
  if f.is_symlink(): raise ValueError('symbolic links are not package inputs')
  if f.is_file():
   info=zipfile.ZipInfo(f.relative_to(source).as_posix(),(2026,1,1,0,0,0))
   info.compress_type=zipfile.ZIP_DEFLATED
   info.external_attr=0o100644 << 16
   z.writestr(info,f.read_bytes())
`;
const frontends = {ccswitch:'vendor/cc-switch/frontend',images:'vendor/gpt-image-playground',skills:'vendor/skills-manager/frontend'};
for (const d of definitions) {
  const upstream = JSON.parse(await readFile(join(app, '..', frontends[d.id], 'package.json'), 'utf8'));
  if (upstream.version !== d.upstreamVersion) throw new Error(`Plugin ${d.id} claims a different upstream version than its source`);
  const stage = await mkdtemp(join(tmpdir(), 'molly-plugin-package-'));
  try {
    const input = join(app, 'public', d.assetDir);
    await lstat(join(input, 'index.html'));
    await cp(input, stage, { recursive: true, dereference: false });
    const manifest = {id:d.id, version:d.version, upstreamVersion:d.upstreamVersion, adapterApi:d.adapterApi, hostVersion:d.hostVersion};
    await writeFile(join(stage, 'plugin.json'), JSON.stringify(manifest, null, 2));
    await cp(join(app, 'THIRD_PARTY_NOTICES.md'), join(stage, 'THIRD_PARTY_NOTICES.md'));
    const licenses = [join(app, '..', sources[d.id], 'LICENSE'), join(app, '..', sources[d.id], '..', 'LICENSE')];
    let copied = false;
    for (const license of licenses) { try { await cp(license, join(stage, 'LICENSE')); copied = true; break; } catch (e) { if(e.code !== 'ENOENT') throw e; } }
    if (!copied) throw new Error(`Missing upstream license: ${d.id}`);
    const filename = `${d.id}-${d.version}.zip`;
    const result = spawnSync(python, ['-c', archive, stage, join(output, filename)], { windowsHide:true, stdio:'inherit' });
    if (result.error || result.status !== 0) throw result.error || new Error('ZIP build failed');
    const bytes = await readFile(join(output, filename));
    catalog.plugins.push({...manifest, url:`https://desktop.veriolink.com/plugins/${filename}`, sha256:createHash('sha256').update(bytes).digest('hex'), size:bytes.length, notes:'MollyCloud 独立工作台插件；保留公共主题、隔离边界和本地数据。'});
    console.log(`${filename}: ${(bytes.length/1024/1024).toFixed(2)} MB`);
  } finally { await rm(stage, { recursive:true, force:true }); }
}
await writeFile(join(output, 'index.json'), JSON.stringify(catalog, null, 2)+'\n');
await writeFile(join(output, 'UPLOAD.md'), '# 插件发布清单\n\n上传到 mollycloud-desktop 存储桶，保持下列对象键。先上传 ZIP 并验证可下载，最后上传 index.json。\n\n'+catalog.plugins.map(p=>`- plugins/${p.url.split('/').at(-1)} (${p.size} bytes, SHA-256: ${p.sha256})`).join('\n')+'\n- plugins/index.json（Cache-Control: no-cache）\n\n公共目录：https://desktop.veriolink.com/plugins/index.json\n');
console.log(`Output: ${output}`);
