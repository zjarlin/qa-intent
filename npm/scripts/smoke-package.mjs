// 对真实 tarball 执行离线安装，验证入口、平台解析、版本与编译结果。
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import assert from "node:assert/strict";
import targets from "../bin/targets.cjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const target = targets.find(t => t.os === process.platform && t.cpu === process.arch);
const temp = mkdtempSync(join(tmpdir(), "qai-package-"));
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const run = (args, cwd = root) => execFileSync(npm, args, { cwd, encoding: "utf8", shell: process.platform === "win32" });
const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const pack = dir => {
  const [result] = JSON.parse(run(["pack", "--json", "--pack-destination", temp], dir));
  return { ...result, archive: join(temp, result.filename) };
};
const platform = pack(join(root, "dist/npm", target.pkg));
const main = pack(root);
assert(main.files.some(f => f.path === manifest.bin.qai), "npm 包缺少命令入口");
assert(main.files.some(f => f.path === "skills/qa-intent/SKILL.md"), "npm 包缺少技能");
run(["install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--prefix", temp, platform.archive, main.archive]);
const installed = join(temp, "node_modules/qa-intent");
const entry = join(installed, manifest.bin.qai);
const invoke = args => execFileSync(process.execPath, [entry, ...args], { cwd: temp, encoding: "utf8" });
assert.equal(invoke(["--version"]).trim(), `qai ${manifest.version}`);
const output = invoke(["-t", join(installed, "examples/taxonomy.json"), "compile", "-i", join(installed, "examples/items.json"), "--raw"]);
assert.equal(output.trim().split("\n").length, 2);
for (const line of output.trim().split("\n")) assert.equal(JSON.parse(line).model, "laya");
assert(existsSync(join(temp, "node_modules/.bin", process.platform === "win32" ? "qai.cmd" : "qai")), "安装未生成命令链接");
console.log(`npm tarball 安装验证通过：${target.pkg}，${temp}`);
