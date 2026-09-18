import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const frontend = resolve(import.meta.dirname, '../../vendor/skills-manager/frontend');
const npmCli = process.env.npm_execpath;
if (!npmCli) throw new Error('请通过 npm run build:skills-manager 构建内置 Skill 管理器。');

function npm(args) {
  const result = spawnSync(process.execPath, [npmCli, ...args], {
    cwd: frontend, stdio: 'inherit', windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

if (!existsSync(resolve(frontend, 'node_modules/vite/package.json'))) {
  npm(['ci', '--no-audit', '--no-fund']);
}
npm(['run', 'build']);
