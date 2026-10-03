import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

if (process.platform !== 'darwin') throw new Error('macOS app/dmg 必须在 macOS 上构建。');
const app = resolve(import.meta.dirname, '..');
const result = spawnSync(process.execPath, [resolve(app, 'node_modules/@tauri-apps/cli/tauri.js'), 'build', '--bundles', 'app,dmg', ...process.argv.slice(2)], {
  cwd: app, stdio: 'inherit', env: { ...process.env, MOLLY_TARGET_PLATFORM: 'macos' },
});
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
