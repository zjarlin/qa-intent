// 检查真实 registry 状态，只跳过已存在的版本；网络/认证失败不能伪装成未发布。
import { readFileSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import targets from "../bin/targets.cjs";
import { releaseVisible, waitForRegistry, verifyReadme } from "./registry.mjs";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const directories = [...targets.map(t => join(root, "dist/npm", t.pkg)), join(root, "dist/npm/qa-intent")];
const packages = directories.map(cwd => {
  const manifest = JSON.parse(readFileSync(join(cwd, "package.json"), "utf8"));
  const entry = manifest.main ?? manifest.bin.qai;
  if (statSync(join(cwd, entry)).size === 0) throw new Error("不能发布空入口");
  return { cwd, manifest };
});
async function publish({ cwd, manifest }) {
  if (await releaseVisible(manifest)) {
    console.log(`已发布 ${manifest.name}@${manifest.version}`);
    return;
  }
  const args = ["publish", "--access", "public"];
  if (process.env.GITHUB_ACTIONS === "true") args.push("--provenance");
  execFileSync("npm", args, { cwd, stdio: "inherit" });
}
const platformPackages = packages.slice(0, -1);
for (const pkg of platformPackages) await publish(pkg);
await waitForRegistry(platformPackages.map(p => p.manifest));
const main = packages.at(-1);
await publish(main);
await waitForRegistry([main.manifest]);
await verifyReadme(main.manifest.name, readFileSync(join(root, "README.md"), "utf8"));
