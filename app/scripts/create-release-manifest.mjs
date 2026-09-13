import { createHash } from "node:crypto";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const releaseOrigin = "https://desktop.veriolink.com";

function readOption(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function fail(message) {
  console.error(`发布清单未生成：${message}`);
  process.exit(1);
}

const version = readOption("--version")?.trim().replace(/^v/i, "");
const installer = readOption("--installer");
const downloadUrl = readOption("--download-url");
const output = readOption("--out");
const notes = readOption("--notes")?.trim();

if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  fail("请提供 SemVer 版本号，例如 --version 0.1.2");
}
if (!installer || !output) {
  fail("需要 --installer <安装包路径> 和 --out <清单路径>");
}
if (!downloadUrl) {
  fail("需要 --download-url，例如 https://desktop.veriolink.com/MollyCloud_0.1.2_x64-setup.exe");
}

let parsedUrl;
try {
  parsedUrl = new URL(downloadUrl);
} catch {
  fail("--download-url 不是有效 URL");
}
if (parsedUrl.origin !== releaseOrigin || !parsedUrl.pathname.toLowerCase().endsWith(".exe")) {
  fail(`--download-url 必须是 ${releaseOrigin} 域名下的 .exe 文件`);
}

const installerPath = resolve(installer);
const outPath = resolve(output);
const [installerBytes, installerStat] = await Promise.all([readFile(installerPath), stat(installerPath)]);
if (!installerStat.isFile() || installerStat.size === 0) fail("安装包不存在或为空");

const manifest = {
  version,
  downloadUrl: parsedUrl.toString(),
  publishedAt: new Date().toISOString(),
  sha256: createHash("sha256").update(installerBytes).digest("hex"),
  ...(notes ? { notes } : {}),
};

await mkdir(dirname(outPath), { recursive: true });
await writeFile(outPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
console.log(`已生成更新清单：${outPath}`);
console.log(`版本：${manifest.version}`);
console.log(`SHA-256：${manifest.sha256}`);
