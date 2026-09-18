// Reuse this project's running Vite server instead of failing Tauri's beforeDevCommand.
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const appRoot = resolve(import.meta.dirname, '..');
const config = JSON.parse(await readFile(resolve(appRoot, 'src-tauri/tauri.conf.json'), 'utf8'));
const devUrl = new URL(config.build.devUrl);
const args = ['dev'];
let running = false;

try {
  const response = await fetch(new URL('/src/App.vue?raw', devUrl), {
    signal: AbortSignal.timeout(3000),
    redirect: 'error',
  });
  running = true;
  const text = await response.text();
  const source = await readFile(resolve(appRoot, 'src/App.vue'), 'utf8');
  if (!response.ok || text.trimEnd().replace(/;$/, '') !== `export default ${JSON.stringify(source)}`) {
    throw new Error(`${devUrl.port} 端口已被其他服务占用，未启动或停止任何服务。请先检查该端口。`);
  }
  args.push('--config', JSON.stringify({ build: { beforeDevCommand: '' } }));
  console.log(`复用当前项目的前端开发服务 ${devUrl.origin}，正在启动桌面开发版。`);
} catch (error) {
  if (running || !error?.cause || !['ECONNREFUSED', 'ENOTFOUND'].includes(error.cause.code)) {
    console.error(error instanceof Error ? error.message : error);
    process.exit(1);
  }
  // No running server: Tauri executes the usual npm run dev build/start command.
}

const child = spawn(process.execPath, [
  resolve(appRoot, 'node_modules/@tauri-apps/cli/tauri.js'),
  ...args,
  ...process.argv.slice(2),
], { cwd: appRoot, stdio: 'inherit', windowsHide: true });

child.on('error', error => { console.error(`桌面开发版启动失败：${error.message}`); process.exitCode = 1; });
child.on('exit', (code, signal) => { process.exitCode = code ?? (signal ? 1 : 0); });
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => child.kill(signal));
}
