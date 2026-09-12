import { test } from 'vitest';
import assert from 'node:assert/strict';
import { checkResourceConfig, containsCredential, isCredentialFile } from './check-package-inputs.mjs';

test('bundle allows shipped assets and rejects user data', () => {
  const config = { build: { frontendDist: '../dist' }, bundle: { resources: ['../THIRD_PARTY_NOTICES.md', '../public/models/'] } };
  assert.doesNotThrow(() => checkResourceConfig(config));
  assert.throws(() => checkResourceConfig({ ...config, bundle: { resources: [...config.bundle.resources, 'C:/Users/example/AppData/Roaming/cn.mollycloud.client'] } }));
  for (const path of ['api_key.bin', 'public/models/api_key.tmp', 'auth.json', 'public/.env.local', 'user-data/Default/Preferences', 'public/test.db']) assert.equal(isCredentialFile(path), true);
  assert.equal(isCredentialFile('public/models/Molly.psd'), false);
});
test('rejects complete keys while allowing UI placeholders', () => {
  assert.equal(containsCredential('sk-' + 'a'.repeat(40)), true);
  assert.equal(containsCredential('gsk_' + 'a'.repeat(32)), true);
  assert.equal(containsCredential('sk-... sk-••••942A'), false);
});
