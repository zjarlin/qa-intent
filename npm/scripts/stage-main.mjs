// 在发布目录注入源码身份，不改写工作区 package.json。
import { cpSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = file => JSON.parse(readFileSync(join(root, file), "utf8"));
const manifest = read("package.json");
const git = args => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const source = {
  repository: process.env.GITHUB_REPOSITORY || "zjarlin/qa-intent",
  revision: process.env.GITHUB_SHA || git(["rev-parse", "HEAD"]),
  reference: process.env.GITHUB_REF || git(["symbolic-ref", "HEAD"])
};
if (!/^[a-f0-9]{40}$/.test(source.revision)) throw new Error("无效源码 revision");
const stage = join(root, "dist/npm/qa-intent");
mkdirSync(stage, { recursive: true });
for (const file of [...manifest.files, "LICENSE"]) cpSync(join(root, file), join(stage, file), { recursive: true });
manifest.aio = { cli: read("aio-cli.json"), source };
writeFileSync(join(stage, "package.json"), JSON.stringify(manifest, null, 2) + "\n");
mkdirSync(join(root, ".aio"), { recursive: true });
writeFileSync(join(root, ".aio/cli-release.json"), JSON.stringify({
  package: manifest.name, version: manifest.version, command: "qai", tag: "latest", source
}, null, 2) + "\n");
console.log(`已准备 ${manifest.name}@${manifest.version}，源码 ${source.revision}`);
