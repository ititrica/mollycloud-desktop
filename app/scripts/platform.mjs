import { homedir } from 'node:os';
import { resolve } from 'node:path';

export function targetPlatform(env = process.env, host = process.platform) {
  const value = env.MOLLY_TARGET_PLATFORM || env.TAURI_ENV_PLATFORM || host;
  if (['darwin', 'macos'].includes(value)) return 'macos';
  if (['win32', 'windows'].includes(value)) return 'windows';
  if (value === 'linux') return 'linux';
  throw new Error(`不支持的桌面目标平台：${value}`);
}

export function pluginRegistryRoot(platform = targetPlatform()) {
  if (platform === 'macos') return resolve(homedir(), 'Library/Application Support/cn.mollycloud.client/plugins');
  if (platform === 'windows') return process.env.APPDATA ? resolve(process.env.APPDATA, 'cn.mollycloud.client/plugins') : undefined;
  return resolve(process.env.XDG_DATA_HOME || resolve(homedir(), '.local/share'), 'cn.mollycloud.client/plugins');
}
