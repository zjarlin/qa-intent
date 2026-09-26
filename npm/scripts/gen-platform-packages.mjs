#!/usr/bin/env node
// 生成各平台的 npm 包目录结构，供 CI 打包发布。
//
// 用法：node npm/scripts/gen-platform-packages.mjs <version>
// 产物：dist/npm/<platform-package>/ 每个目录一个可发布包。

import { mkdirSync, writeFileSync, copyFileSync, chmodSync, existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..", "..");
const dist = join(root, "dist", "npm");
const version = process.argv[2] || JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;

const TARGETS = [
  { pkg: "@qa-intent/cli-darwin-arm64", os: "darwin", cpu: "arm64", rust: "aarch64-apple-darwin", exe: "qai" },
  { pkg: "@qa-intent/cli-darwin-x64", os: "darwin", cpu: "x64", rust: "x86_64-apple-darwin", exe: "qai" },
  { pkg: "@qa-intent/cli-linux-x64", os: "linux", cpu: "x64", rust: "x86_64-unknown-linux-gnu", exe: "qai" },
  { pkg: "@qa-intent/cli-linux-arm64", os: "linux", cpu: "arm64", rust: "aarch64-unknown-linux-gnu", exe: "qai" },
  { pkg: "@qa-intent/cli-win32-x64", os: "win32", cpu: "x64", rust: "x86_64-pc-windows-msvc", exe: "qai.exe" },
];

for (const t of TARGETS) {
  const dir = join(dist, t.pkg.replace("@", "").replace("/", "-"));
  const binDir = join(dir, "bin");
  mkdirSync(binDir, { recursive: true });

  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify(
      {
        name: t.pkg,
        version,
        description: `qai native binary for ${t.os}-${t.cpu}`,
        license: "MIT",
        os: [t.os],
        cpu: [t.cpu],
        files: ["bin"],
      },
      null,
      2
    ) + "\n"
  );

  const src = process.env.BIN_DIR
    ? join(process.env.BIN_DIR, t.rust, t.exe)
    : join(root, "target", t.rust, "release", t.exe);
  if (!existsSync(src)) {
    console.warn(`跳过 ${t.pkg}：未找到 ${src}`);
    continue;
  }
  const dst = join(binDir, t.exe);
  copyFileSync(src, dst);
  if (t.os !== "win32") chmodSync(dst, 0o755);
  console.log(`已生成 ${t.pkg} <- ${src}`);
}
