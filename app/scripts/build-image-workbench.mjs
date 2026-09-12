import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const frontend = resolve(import.meta.dirname, '../../vendor/gpt-image-playground');
const npmCli = process.env.npm_execpath;
if (!npmCli) throw new Error('请通过 npm run build:image-workbench 构建生图工作台。');
function npm(args) {
  const result = spawnSync(process.execPath, [npmCli, ...args], {
    cwd: frontend, stdio: 'inherit', windowsHide: true,
    env: { ...process.env, VITE_MOLLY_EMBEDDED: 'true', VITE_DEFAULT_API_URL: 'https://mollycloud.cn/v1?model=gpt-image-2&profileName=MollyCloud' },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
if (!existsSync(resolve(frontend, 'node_modules/vite/package.json'))) npm(['ci', '--no-audit', '--no-fund']);
npm(['run', 'build', '--', '--outDir', '../../app/public/image-workbench', '--emptyOutDir']);
