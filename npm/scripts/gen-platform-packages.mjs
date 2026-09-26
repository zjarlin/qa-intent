#!/usr/bin/env node
import { mkdirSync, writeFileSync, copyFileSync, chmodSync, statSync, readFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import targets from "../bin/targets.cjs";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const selected = process.argv[2] ? targets.filter(t => t.rust === process.argv[2]) : targets;
if (!selected.length) throw new Error("未知 Rust target");
const sources = selected.map(t => ({ ...t, src: process.env.BIN_DIR
  ? join(process.env.BIN_DIR, t.rust, t.exe)
  : join(root, "target", t.rust, "release", t.exe) }));
// 先校验所有输入，缺少任何二进制就失败，不能生成可误发布的空包。
for (const t of sources) {
  if (!statSync(t.src).isFile() || statSync(t.src).size === 0) throw new Error(`缺少二进制：${t.src}`);
  if (manifest.optionalDependencies[t.pkg] !== manifest.version) throw new Error(`版本不一致：${t.pkg}`);
}
for (const t of sources) {
  const dir = join(root, "dist/npm", t.pkg);
  mkdirSync(join(dir, "bin"), { recursive: true });
  copyFileSync(t.src, join(dir, "bin", t.exe));
  if (t.os !== "win32") chmodSync(join(dir, "bin", t.exe), 0o755);
  copyFileSync(join(root, "LICENSE"), join(dir, "LICENSE"));
  writeFileSync(join(dir, "package.json"), JSON.stringify({
    name: t.pkg, version: manifest.version, description: `qai native binary for ${t.os}-${t.cpu}`,
    license: "MIT", repository: manifest.repository, os: [t.os], cpu: [t.cpu],
    main: `bin/${t.exe}`, files: ["bin", "LICENSE"]
  }, null, 2) + "\n");
  console.log(`已生成 ${t.pkg}@${manifest.version}`);
}
