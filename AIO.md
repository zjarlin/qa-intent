# AIO CLI 接入

本项目通过 `aio plugin init . --kind cli --adopt` 接入。
`aio-cli.json` 声明命令 qai，主实现仍是 Rust + clap，npm 负责二进制分发。

`.github/workflows/ci.yml` 负责 Rust 验证、五平台打包与安装测试和 npm 发布。
`.github/workflows/aio-cli.yml` 仅等待同一提交的 npm 版本，并通过
`@zjarlin/aio@2026.9.22 tool release sync` 同步市场，不再次构建或发布 npm。
AIO 的 OIDC 校验要求工作流名为 `aio-cli.yml`，不能把同步步骤直接移入 `ci.yml`。
同步最多等待约 10 分钟；构建耗时更长时，完成后重新运行市场工作流即可。
本项目使用精确语义版本，不使用 AIO 默认的自动 dev 版本策略。

随包 `skills/qa-intent/SKILL.md` 由 AIO 安装流程发现，普通 npm 安装不会修改用户技能目录。
首次 npm 认证和 Trusted Publisher 配置见 [发布说明](docs/publishing.md)。
