// 检查真实 registry 状态，只跳过已存在的版本；网络/认证失败不能伪装成未发布。
import { readFileSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import targets from "../bin/targets.cjs";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const directories = [...targets.map(t => join(root, "dist/npm", t.pkg)), join(root, "dist/npm/qa-intent")];
for (const cwd of directories) {
  const manifest = JSON.parse(readFileSync(join(cwd, "package.json"), "utf8"));
  const entry = manifest.main ?? manifest.bin.qai;
  if (statSync(join(cwd, entry)).size === 0) throw new Error("不能发布空入口");
  const response = await fetch(`https://registry.npmjs.org/${encodeURIComponent(manifest.name)}/${manifest.version}`);
  if (response.ok) {
    console.log(`已发布 ${manifest.name}@${manifest.version}`);
    continue;
  }
  if (response.status !== 404) throw new Error(`registry 返回 HTTP ${response.status}，停止发布`);
  const args = ["publish", "--access", "public"];
  if (process.env.GITHUB_ACTIONS === "true") args.push("--provenance");
  execFileSync("npm", args, { cwd, stdio: "inherit" });
}
