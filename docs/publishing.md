# 发布到 npm

`qai` 以「主包 + 平台二进制包」的形式发布：

- 主包 `qa-intent`：只含 Node 启动器 `bin/qai.js`
- 平台包 `@qa-intent/cli-<os>-<arch>`：含原生二进制，通过 `optionalDependencies` 分发

npm 会按当前平台只安装匹配的那个平台包，用户不会下载无关平台。

## 一次性配置（二选一）

### 方式一：Trusted Publisher（推荐，无需长期 token）

在 npmjs.com 为 **每个包** 配置 GitHub Actions 发布者：

| 字段 | 值 |
|---|---|
| Publisher | GitHub Actions |
| Organization / User | `zjarlin` |
| Repository | `qa-intent` |
| Workflow filename | `ci.yml` |
| Environment | 留空 |

需要配置的包：

- `qa-intent`
- `@qa-intent/cli-darwin-arm64`
- `@qa-intent/cli-darwin-x64`
- `@qa-intent/cli-linux-x64`
- `@qa-intent/cli-linux-arm64`
- `@qa-intent/cli-win32-x64`

> 注意：首次发布前如果包不存在，需要先手动发布一次，或在 npmjs.com 预创建包名后配置发布者。

### 方式二：NPM_TOKEN

```bash
# 在 npmjs.com 生成 Automation token，然后：
gh secret set NPM_TOKEN --repo zjarlin/qa-intent
```

CI 已经在 `publish` 步骤同时兼容两种方式。

## 触发发布

推送到 `main` 即触发：

```bash
git push origin main
```

流程：`test` → 五平台 `build` → `publish`（先发平台包，再发主包）。

## 版本号

主包与平台包版本必须一致。改版本：

```bash
# 同时更新 package.json 与平台的 optionalDependencies
npm version 0.2.0 --no-git-tag-version
git commit -am "chore: 发布 0.2.0"
git push origin main
```

## 本地验证打包内容

```bash
cargo build --release --target aarch64-apple-darwin
BIN_DIR=$PWD/target node npm/scripts/gen-platform-packages.mjs
npm pack --dry-run dist/npm/qa-intent-cli-darwin-arm64
npm pack --dry-run .
```
