import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pluginAsset } from './plugin-assets.mjs';
async function fixture(run) {
  const root=await mkdtemp(join(tmpdir(),'molly-plugin-assets-test-'));
  try {
    const registry=join(root,'plugins');const assets=join(root,'public');
    await mkdir(join(assets,'ccswitch'),{recursive:true});await mkdir(registry);
    await writeFile(join(assets,'ccswitch/index.html'),'bundled');
    await run(registry,assets);
  } finally { await rm(root,{recursive:true,force:true}); }
}
test('development serves the bundled plugin before migration',()=>fixture(async (registry,assets)=>{
  assert.equal((await pluginAsset(registry,assets,'/molly-plugins/ccswitch/index.html')).bytes.toString(),'bundled');
}));
test('development loads the installed version, and uninstall fails closed',()=>fixture(async (registry,assets)=>{
  const dir='packages/ccswitch/abc';await mkdir(join(registry,dir),{recursive:true});await writeFile(join(registry,dir,'index.html'),'updated');
  await writeFile(join(registry,'installed.json'),JSON.stringify({ccswitch:{current:{directory:dir,package:{id:'ccswitch',adapterApi:'ccswitch-3.20.4-molly-2'}}}}));
  assert.equal((await pluginAsset(registry,assets,'/molly-plugins/ccswitch/index.html')).bytes.toString(),'updated');
  await writeFile(join(registry,'installed.json'),JSON.stringify({ccswitch:{current:null}}));
  await assert.rejects(pluginAsset(registry,assets,'/molly-plugins/ccswitch/index.html'));
}));
test('development cannot serve private data through traversal',()=>fixture(async (registry,assets)=>{
  for(const path of ['/molly-plugins/ccswitch/../installed.json','/molly-plugins/ccswitch/%2e%2e/installed.json','/molly-plugins/ccswitch/C:/private.txt','/molly-plugins/unknown/index.html']) await assert.rejects(pluginAsset(registry,assets,path));
}));
test('incompatible or malformed installed records never fall back silently',()=>fixture(async (registry,assets)=>{
  await writeFile(join(registry,'installed.json'),'broken');await assert.rejects(pluginAsset(registry,assets,'/molly-plugins/ccswitch/index.html'));
  await writeFile(join(registry,'installed.json'),JSON.stringify({ccswitch:{current:{package:{id:'ccswitch',adapterApi:'unknown'}}}}));
  await assert.rejects(pluginAsset(registry,assets,'/molly-plugins/ccswitch/index.html'));
}));
