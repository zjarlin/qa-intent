# AIO CLI 接入

本项目通过 `aio plugin init . --kind cli --adopt` 接入。
`aio-cli.json` 声明命令 qai，主实现仍是 Rust + clap，npm 负责二进制分发。

自动发布统一在 `.github/workflows/ci.yml`：Rust 验证、五平台打包与安装测试、
npm 发布、最后 `@zjarlin/aio@2026.9.22 tool release sync` 同步市场。
未保留生成器的第二份 TypeScript 发布工作流，以免重复发布和错误构建。
本项目使用精确语义版本，不使用 AIO 默认的自动 dev 版本策略。

随包 `skills/qa-intent/SKILL.md` 由 AIO 安装流程发现，普通 npm 安装不会修改用户技能目录。
首次 npm 认证和 Trusted Publisher 配置见 [发布说明](docs/publishing.md)。
