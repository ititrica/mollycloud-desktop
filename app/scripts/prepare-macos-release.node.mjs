import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

test('macOS release preparation separates architectures and rejects Windows installers', async () => {
  const root = await mkdtemp(join(tmpdir(), 'molly-macos-release-'));
  try {
    const version = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8')).version;
    const invoke = name => spawnSync(process.execPath, [resolve(import.meta.dirname, 'prepare-macos-release.mjs'), '--installer', join(root, name), '--out', join(root, 'release')], { encoding: 'utf8' });
    const name = `MollyCloud_${version}_aarch64.dmg`;
    await writeFile(join(root, name), 'mock dmg; local-only test');
    assert.equal(invoke(name).status, 0);
    const manifest = JSON.parse(await readFile(join(root, 'release/latest-macos-aarch64.json'), 'utf8'));
    assert.equal(manifest.downloadUrl, `https://desktop.veriolink.com/${name}`);
    assert.equal(manifest.version, version);
    await assert.rejects(readFile(join(root, 'release/latest-macos-x86_64.json')), /ENOENT/);
    assert.notEqual(invoke(`MollyCloud_${version}_x64-setup.exe`).status, 0);
  } finally { await rm(root, { recursive: true }); }
});
