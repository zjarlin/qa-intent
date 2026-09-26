#!/usr/bin/env node
"use strict";
// 使用 Node 标准模块解析，以兼容 npm 全局安装与 pnpm 链接布局。
const { spawnSync } = require("node:child_process");
const targets = require("./targets.cjs");
try {
  const target = targets.find(t => t.os === process.platform && t.cpu === process.arch);
  if (!target) throw new Error(`不支持的平台：${process.platform}-${process.arch}`);
  const binary = require.resolve(target.pkg);
  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.signal) process.kill(process.pid, result.signal);
  process.exit(result.status ?? 1);
} catch (error) {
  console.error(`qai: ${error.message}\n请保留 optionalDependencies 后重新安装 qa-intent。`);
  process.exit(1);
}
