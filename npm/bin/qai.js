#!/usr/bin/env node
// qai 的 Node 启动器：按平台解析并执行对应的原生二进制。
//
// 二进制通过 optionalDependencies 分发，每个平台一个包，
// 只安装当前平台需要的那个，避免下载无关平台。

"use strict";

const { spawnSync } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs");

const PLATFORM_PACKAGES = {
  "darwin-arm64": "@qa-intent/cli-darwin-arm64",
  "darwin-x64": "@qa-intent/cli-darwin-x64",
  "linux-x64": "@qa-intent/cli-linux-x64",
  "linux-arm64": "@qa-intent/cli-linux-arm64",
  "win32-x64": "@qa-intent/cli-win32-x64",
};

function resolveBinary() {
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];
  if (!pkg) {
    throw new Error(
      `不支持的平台：${key}\n` +
        `支持的平台：${Object.keys(PLATFORM_PACKAGES).join(", ")}\n` +
        `可改用 cargo install --path . 从源码安装。`
    );
  }

  // 从本脚本所在位置向上查找 node_modules，确保 npm 全局安装也能解析。
  const exe = process.platform === "win32" ? "qai.exe" : "qai";
  const searchDirs = [];
  let dir = __dirname;
  while (true) {
    searchDirs.push(path.join(dir, "node_modules"));
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }

  for (const modulesDir of searchDirs) {
    const candidate = path.join(modulesDir, pkg, "bin", exe);
    if (fs.existsSync(candidate)) return candidate;
  }

  throw new Error(
    `缺少平台二进制包 ${pkg}。\n` +
      `这通常是因为安装时使用了 --no-optional。请重新安装：npm i -g qa-intent\n` +
      `或改用 cargo install --path . 从源码安装。`
  );
}

const binary = resolveBinary();
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
