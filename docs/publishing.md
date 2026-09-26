# npm 发布

主包 `qa-intent` 的命令入口是 `npm/bin/qai.js`，通过 optionalDependencies 分发
`qa-intent-darwin-arm64`、`qa-intent-darwin-x64`、`qa-intent-linux-arm64`、
`qa-intent-linux-x64`、`@zjarlin/qa-intent-win32-x64`。Windows 包使用发布者的个人 scope，
无需创建 npm 组织；主包仍为 `qa-intent`。

## 首次发布

1. 用 `npm login --auth-type=web` 登录 npm，并在浏览器完成安全密钥验证。
2. 等待 GitHub CI 五个平台构建和安装测试成功，下载该提交的五个平台 artifact。
3. 用 `BIN_DIR=<artifact目录> node npm/scripts/gen-platform-packages.mjs` 生成包。
   artifact 子目录名必须为 Rust target；每个目录含 qai 或 qai.exe。
4. `node npm/scripts/stage-main.mjs` 注入源码身份，再用
   `npm run test:package -- --staged` 验证主包与当前平台包的实际 tarball。
5. `node npm/scripts/publish.mjs` 先发布五个平台包，最后发布主包。
   首次发布可能要求 npm 再次二次验证；不要提交任何令牌。

## 自动发布

在这六个包的 npm 设置中配置 Trusted Publisher：
GitHub owner = `zjarlin`、repository = `qa-intent`、workflow = `ci.yml`，environment 留空。
包需先存在；仅在 workflow 中声明 id-token 权限不会自动建立 npm 信任关系。

CI 使用 Node 24、npm 11 和 OIDC provenance。默认分支推送/手动 dispatch 都可触发；
没有可信发布者时会报告发布失败。首次引导也兼容仓库 NPM_TOKEN secret，
应使用当前 npm 支持的凭据类型，不能依赖已废弃的 classic/Automation token。
账号验证及可信发布者配置完成后无需在每次推送时人工验证。

发布器仅在 registry 已存在精确版本时跳过；404 代表需要发布，其他错误立即失败。
发布五个平台包后先等待它们全部公开，再发布主包；主包公开后校验 registry README
与根目录 README.md 一致。每轮最多等待约 5 分钟，npm 处理更久时应等版本公开后重跑。
不要修改已发布版本的内容后仍沿用原版本。更新 Cargo.toml、Cargo.lock、
package.json 及所有 optionalDependencies 的版本后提交。

## 验证

每个平台 job 都执行真实 npm pack + 离线 npm install，
校验命令链接、版本、随包 skill 和示例编译。发布后 CI 再从公共 npm registry 安装。
独立的 `aio-cli.yml` 用 AIO 的 `tool release sync` 等待并同步同一提交的 npm 版本，
不重复发布 npm。AIO 身份校验要求这个工作流名称；npm 与市场的成功状态分别报告。
市场工作流最多等待约 10 分钟，超时后可在 npm 发布成功时重新运行。

```sh
cargo build --release --locked --target aarch64-apple-darwin
node npm/scripts/gen-platform-packages.mjs aarch64-apple-darwin
npm run test:package
npm view qa-intent version
```

原始构建产物始终保留在 GitHub Actions 的 artifact 中，认证失败不会删除已生成的包。
