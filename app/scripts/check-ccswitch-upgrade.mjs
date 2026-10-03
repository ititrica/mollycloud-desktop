// Run after changing vendored CC Switch; all native fixtures use temporary
// manager and system-user directories with mock credentials.
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const app = resolve(import.meta.dirname, '..');
const root = resolve(app, '..');
const tauri = resolve(app, 'src-tauri/Cargo.toml');
const backend = resolve(root, 'vendor/cc-switch/backend/Cargo.toml');
const target = resolve(app, 'src-tauri/target');
const npmCli = process.env.npm_execpath;
if (!npmCli) throw new Error('请通过 npm run check:ccswitch-upgrade 运行检查。');

const commands = [
  [process.execPath, [npmCli, 'run', 'build']],
  ['cargo', ['test', '--manifest-path', backend, '--target-dir', target, '--locked', '--lib', 'embedded::tests', '--', '--test-threads=1']],
  ['cargo', ['test', '--manifest-path', backend, '--target-dir', target, '--locked', '--lib', 'database::schema::tests::migrate_v18_to_v19_preserves_existing_molly_data']],
  ['cargo', ['test', '--manifest-path', backend, '--target-dir', target, '--locked', '--lib', 'molly_probe_uses_website_without_changing_model_endpoint']],
  ['cargo', ['test', '--manifest-path', tauri, '--locked', '--lib', 'molly_ccswitch_usage_tests']],
  ['cargo', ['test', '--manifest-path', tauri, '--locked', '--lib', 'ccswitch_deeplink::tests']],
  ['cargo', ['test', '--manifest-path', tauri, '--locked', '--lib', 'console_plugins::tests']],
  ['cargo', ['run', '--manifest-path', tauri, '--locked', '--example', 'plugin_smoke', '--features', 'plugin-smoke']],
  ['cargo', ['run', '--manifest-path', tauri, '--locked', '--example', 'ccswitch_smoke', '--features', 'ccswitch-smoke']],
  ['cargo', ['run', '--manifest-path', tauri, '--locked', '--example', 'ccswitch_smoke', '--features', 'ccswitch-smoke', '--', '--init-failure']],
];

for (const [program, args] of commands) {
  console.log(`Checking: ${program === process.execPath ? 'npm' : program} ${args.slice(0, 7).join(' ')}`);
  const result = spawnSync(program, args, { cwd: app, stdio: 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
