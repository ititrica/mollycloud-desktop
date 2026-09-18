// Refresh discovery metadata only. Never derive install commands from Markdown.
// Offline: node scripts/sync-mcp-community.mjs ../../path/to/README-zh.md
// Online:  node scripts/sync-mcp-community.mjs --update
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const source='https://raw.githubusercontent.com/punkpeye/awesome-mcp-servers/main/README-zh.md';
const input=process.argv[2];
if (!input) throw new Error('Provide a README file or --update');
const markdown=input==='--update' ? await (await fetch(source, {signal:AbortSignal.timeout(20000)})).text() : await readFile(input,'utf8');
const section=markdown.split(/^## 服务器实现\s*$/m)[1]?.split(/^## /m)[0];
if (!section) throw new Error('Upstream server section changed; existing catalog was retained');
let category='其他';
const entries=new Map();
for (const line of section.split(/\r?\n/)) {
  if (line.startsWith('### ')) {
    category=line.replace(/^###\s+/, '').replace(/<[^>]*>/g,'').replace(/^[^\p{Letter}\p{Number}]+/u,'').trim();
    continue;
  }
  const match=line.match(/^[-*]\s+\[([^\]]+)\]\((https:\/\/github\.com\/[^\s)]+)\)(.*)$/);
  if (!match) continue;
  const [,name,homepage,tail]=match;
  const url=new URL(homepage);
  if(url.hostname!=='github.com'||url.username||url.password)continue;
  const canonical=homepage.replace(/\/$/,'').toLowerCase();
  if (entries.has(canonical)) continue;
  const description=tail.split(/\s[-–—]\s/).slice(1).join(' - ').replace(/\[([^\]]+)\]\([^)]+\)/g,'$1').replace(/<[^>]*>/g,'').trim();
  entries.set(canonical,{id:'community-'+createHash('sha256').update(canonical).digest('hex').slice(0,12),name,homepage,description:description||name,category,source:'awesome',fields:[],recipe:null});
}
if (entries.size<100)throw new Error('Unexpected upstream format; existing catalog was retained');
await writeFile(new URL('../src/mcp/community.json',import.meta.url),JSON.stringify([...entries.values()],null,2)+'\n');
await writeFile(new URL('../src/mcp/community-source.json',import.meta.url),JSON.stringify({source,license:'MIT',sha256:createHash('sha256').update(markdown).digest('hex'),count:entries.size},null,2)+'\n');
console.log(`Saved ${entries.size} browse-only server entries`);
