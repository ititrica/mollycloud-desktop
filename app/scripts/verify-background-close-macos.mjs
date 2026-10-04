// Supervise the native fixture: AppKit can exit with code 0 before Rust's final
// assertions run, so a successful process status alone must never pass this test.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, linkSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

if (process.platform !== 'darwin') throw new Error('Native close verification requires macOS.');
const app = resolve(import.meta.dirname, '..');
const binary = join(app, 'src-tauri/target/debug/examples/console_settings_smoke');
if (!existsSync(binary)) throw new Error('Build the console_settings_smoke example first.');
const declaration = spawnSync('/usr/bin/plutil', ['-extract', 'LSUIElement', 'raw', '-o', '-', join(app, 'src-tauri/Info.macos.plist')], { encoding: 'utf8' });
if (declaration.status !== 0 || declaration.stdout.trim() !== 'true') {
  throw new Error('The production macOS bundle must declare LSUIElement=true.');
}
// A fresh identity cannot inherit the installed application's utility exceptions.
const bundleId = `cn.mollycloud.close-check-${process.pid}`;
const output = resolve(app, '../artifacts/background-close-package-20261004');
mkdirSync(output, { recursive: true });
const temporary = mkdtempSync(join(tmpdir(), 'molly-close-check-'));
const bundle = join(temporary, 'MollyCloud Close Check.app');
const executable = join(bundle, 'Contents/MacOS/console_settings_smoke');
const marker = 'PASS: native console CloseRequested hides for tray, exits normally for quit, and persists selection in temporary storage';
const cases = [
  ['dashboard-no-pet', ['--no-pet', '--idle-seconds=10'], marker],
  ['login-no-pet', ['--login', '--no-pet', '--idle-seconds=10'], marker],
  ['dashboard-with-pet', ['--idle-seconds=10'], marker],
  ['dashboard-visible-pet', ['--visible-pet', '--idle-seconds=10'], marker],
  ['menu-quit', ['--no-pet', '--menu-quit', '--idle-seconds=3'], 'PASS: explicit menu bar quit exits from background mode and preserves tray selection'],
  ['native-quit', ['--no-pet', '--native-quit', '--idle-seconds=3'], 'PASS: ready for native explicit quit after both background checks'],
];
const caseOption = process.argv.indexOf('--case');
const selected = caseOption < 0 ? cases : cases.filter(([name]) => name === process.argv[caseOption + 1]);
if (!selected.length) throw new Error('Unknown or missing native verification case.');
try {
  mkdirSync(join(bundle, 'Contents/MacOS'), { recursive: true });
  try { linkSync(binary, executable); } catch { copyFileSync(binary, executable); }
  writeFileSync(join(bundle, 'Contents/Info.plist'), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>${bundleId}</string>
<key>CFBundleExecutable</key><string>console_settings_smoke</string>
<key>CFBundleName</key><string>MollyCloud Close Check</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.0.0</string>
<key>CFBundleVersion</key><string>0</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>`);
  for (const [name, args, completed] of selected) {
    const result = spawnSync(executable, args, {
      cwd: app, encoding: 'utf8', timeout: 50_000, maxBuffer: 4 * 1024 * 1024,
    });
    const log = `${result.stdout ?? ''}${result.stderr ?? ''}`;
    writeFileSync(join(output, `${name}.log`), log);
    if (result.error || result.status !== 0 || !log.includes(completed) || !log.includes(`[native-host] bundle_id=${bundleId}`)) {
      throw new Error(`${name} did not complete verification. Check ${join(output, `${name}.log`)}. ${result.error?.message ?? ''}`);
    }
    for (const message of ['first hidden window remains alive', 'second hidden window remains alive', '[native-role] foreground=true', '[native-role] foreground=false', '[close-event] Exit']) {
      if (!log.includes(message)) throw new Error(`${name}: missing lifecycle evidence: ${message}`);
    }
    if (name !== 'native-quit' && !log.includes('ExitRequested code=Some(0)')) throw new Error(`${name}: missing normal quit lifecycle`);
    console.log(`PASS: ${name}; native close, background role, retained pet and menu bar, foreground reopen and explicit quit.`);
  }
  console.log(`PASS: ${selected.length} native close cases completed. Evidence: ${output}`);
} finally {
  // The fixture has no URL schemes, app data or login session. Unregister only
  // its temporary bundle before removing it; preserve the installed app.
  spawnSync('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister', ['-u', bundle]);
  rmSync(temporary, { recursive: true, force: true });
}
