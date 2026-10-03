import { test } from 'vitest';
import assert from 'node:assert/strict';
import { checkResourceConfig, containsCredential, isCredentialFile } from './check-package-inputs.mjs';

test('bundle allows shipped assets and rejects user data', () => {
  const config = { build: { frontendDist: '../dist' }, bundle: { resources: ['../THIRD_PARTY_NOTICES.md', '../public/models/', '../../vendor/netspeed-dynamic/helpers/NSD_Fps_Plugin.exe', '../../vendor/netspeed-dynamic/helpers/NSD_Taskbar_Plugin.exe'] } };
  assert.doesNotThrow(() => checkResourceConfig(config, 'windows'));
  assert.throws(() => checkResourceConfig({ ...config, bundle: { resources: [...config.bundle.resources, 'C:/Users/example/AppData/Roaming/cn.mollycloud.client'] } }, 'windows'));
  for (const path of ['api_key.bin', 'public/models/api_key.tmp', 'auth.json', 'public/.env.local', 'user-data/Default/Preferences', 'public/test.db']) assert.equal(isCredentialFile(path), true);
  assert.equal(isCredentialFile('public/models/Molly.psd'), false);
});
test('macOS accepts app/dmg resources and excludes Windows helpers and island windows', () => {
  const config = { build: { frontendDist: '../dist' }, app: { macOSPrivateApi: true, windows: [{ label: 'console' }, { label: 'main' }] }, bundle: { targets: ['app', 'dmg'], resources: ['../THIRD_PARTY_NOTICES.md', '../public/models/'] } };
  assert.doesNotThrow(() => checkResourceConfig(config, 'macos'));
  assert.throws(() => checkResourceConfig({ ...config, bundle: { ...config.bundle, resources: [...config.bundle.resources, '../../vendor/netspeed-dynamic/helpers/NSD_Fps_Plugin.exe'] } }, 'macos'));
  assert.throws(() => checkResourceConfig({ ...config, app: { ...config.app, windows: [{ label: 'netspeed-widget' }] } }, 'macos'));
});
test('rejects complete keys while allowing UI placeholders', () => {
  assert.equal(containsCredential('sk-' + 'a'.repeat(40)), true);
  assert.equal(containsCredential('gsk_' + 'a'.repeat(32)), true);
  assert.equal(containsCredential('sk-... sk-••••942A'), false);
});
