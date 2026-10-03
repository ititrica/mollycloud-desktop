// Run with node --test; Vitest discovers its own frontend test suite.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve, sep } from 'node:path';
import { spawnSync } from 'node:child_process';

test('release preparation is local and requires publication credentials', async () => {
  const root = await mkdtemp(join(tmpdir(), 'molly-release-test-'));
  try {
    const installer = join(root, 'MollyCloud_9.9.9_x64-setup.exe');
    const output = join(root, 'latest.json');
    const bytes = Buffer.from('mock installer, never publish');
    await writeFile(installer, bytes);
    const script = resolve(import.meta.dirname, 'publish-desktop-release.mjs');
    const invoke = extra => spawnSync(process.execPath, [script, '--version', '9.9.9', '--installer', installer,
      '--out', output, ...extra], { cwd: resolve(import.meta.dirname, '..'), encoding: 'utf8',
      env: { ...process.env, CLOUDFLARE_API_TOKEN: '' } });
    const dryRun = invoke([]);
    assert.equal(dryRun.status, 0, dryRun.stderr);
    assert.match(dryRun.stdout, /仅生成本地清单/);
    const manifest = JSON.parse(await readFile(output, 'utf8'));
    assert.equal(manifest.version, '9.9.9');
    assert.equal(manifest.sizeBytes, bytes.length);
    assert.equal(manifest.sha256, createHash('sha256').update(bytes).digest('hex'));
    const publish = invoke(['--publish']);
    assert.notEqual(publish.status, 0);
    assert.match(publish.stderr, /--out 仅用于本地演练/);
    assert.equal(basename(installer), 'MollyCloud_9.9.9_x64-setup.exe');
  } finally {
    const checked = resolve(root);
    if (!checked.startsWith(resolve(tmpdir()) + sep) || !basename(checked).startsWith('molly-release-test-')) {
      throw new Error('Refusing to remove an unexpected test directory');
    }
    await rm(checked, { recursive: true });
  }
});
