import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import process from "node:process";

function parseArgs(values) {
  const result = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value.startsWith("--")) result[value.slice(2)] = values[index + 1], index += 1;
  }
  return result;
}

async function sha256(file) {
  const data = await readFile(file);
  return createHash("sha256").update(data).digest("hex");
}

const args = parseArgs(process.argv.slice(2));
if (!args.source) {
  throw new Error("请使用 --source 指定包含 model3.json 的模型目录");
}

const source = resolve(args.source);
const destination = resolve("public/live2d/molly");
const manifestName = args.manifest || "seethrough_output.model3.json";
const manifestPath = join(source, manifestName);
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const references = manifest.FileReferences ?? {};
const relativeFiles = [references.Moc, references.Physics, references.DisplayInfo, ...(references.Textures ?? [])];
for (const motions of Object.values(references.Motions ?? {})) {
  for (const motion of motions) relativeFiles.push(motion.File);
}
const files = [manifestName, ...relativeFiles.filter(Boolean)];

await mkdir(destination, { recursive: true });
const hashes = {};
for (const relativeFile of files) {
  const from = join(source, relativeFile);
  const to = join(destination, relativeFile);
  await mkdir(dirname(to), { recursive: true });
  await copyFile(from, to);
  hashes[relativeFile.replaceAll("\\", "/")] = await sha256(to);
}

if (args.core) {
  const coreDestination = resolve("public/vendor/live2dcubismcore.min.js");
  await mkdir(dirname(coreDestination), { recursive: true });
  await copyFile(resolve(args.core), coreDestination);
  hashes["vendor/live2dcubismcore.min.js"] = await sha256(coreDestination);
}

if (args.preview) {
  const previewDestination = resolve("public/brand/molly.png");
  await mkdir(dirname(previewDestination), { recursive: true });
  await copyFile(resolve(args.preview), previewDestination);
  hashes["brand/molly.png"] = await sha256(previewDestination);
}

await writeFile(
  join(destination, "model-files.sha256.json"),
  `${JSON.stringify({ generated_at: new Date().toISOString(), files: hashes }, null, 2)}\n`,
  "utf8",
);
console.log(`已导入 ${files.length} 个 Live2D 运行时文件。`);
