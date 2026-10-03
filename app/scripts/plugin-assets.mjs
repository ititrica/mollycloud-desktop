// Development counterpart of console_plugins::intercept. Only package assets
// are served, at a fixed local path; no user config/credentials are exposed.
import { readFile, realpath } from 'node:fs/promises';
import { resolve, sep, extname } from 'node:path';
const definitions = JSON.parse((await readFile(new URL('../plugins/definitions.json', import.meta.url), 'utf8')).replace(/^\uFEFF/, ''));
export async function pluginAsset(registryRoot, publicRoot, pathname) {
  const match = /^\/molly-plugins\/(ccswitch|skills)\/(.+)$/.exec(pathname);
  if (!match || match[2].includes('\\') || match[2].includes('%') || match[2].split('/').some(p=>!p || p==='..' || p==='.' || p.includes(':'))) throw new Error('Invalid plugin path');
  const [,id,path] = match;
  const def = definitions.find(d=>d.id===id);
  let records;
  try {records = JSON.parse(await readFile(resolve(registryRoot, 'installed.json'), 'utf8'));}
  catch(error) {if(error.code !== 'ENOENT') throw error;}
  const current = records?.[id]?.current;
  if (records && !current) throw new Error('Plugin is uninstalled');
  if (current && (current.package?.id!==id || current.package?.adapterApi!==def.adapterApi)) throw new Error('Incompatible plugin');
  let root = resolve(publicRoot, def.assetDir);
  if (current?.directory) {
    if (!new RegExp(`^packages/${id}/[a-zA-Z0-9.-]+$`).test(current.directory)) throw new Error('Invalid plugin directory');
    root = resolve(registryRoot,current.directory);
  }
  const canonicalRoot = await realpath(root);
  const file = await realpath(resolve(root,path));
  if (!file.startsWith(canonicalRoot+sep)) throw new Error('Plugin asset outside package');
  return {bytes:await readFile(file), mime: ({'.html':'text/html; charset=utf-8','.js':'text/javascript','.css':'text/css','.json':'application/json','.svg':'image/svg+xml','.png':'image/png','.woff2':'font/woff2','.woff':'font/woff','.ttf':'font/ttf','.wasm':'application/wasm'})[extname(file)] || 'application/octet-stream'};
}
export function pluginAssetsMiddleware(registryRoot, publicRoot) {
  return async (request,response,next) => {
    if (!request.url?.startsWith('/molly-plugins/')) return next();
    try {
      if (request.method !== 'GET') throw new Error('Method not allowed');
      const asset = await pluginAsset(registryRoot,publicRoot,request.url.split('?')[0]);
      response.setHeader('Content-Type',asset.mime);response.setHeader('Cache-Control','no-store');response.end(asset.bytes);
    } catch {response.statusCode=404;response.end('Plugin asset unavailable');}
  };
}
