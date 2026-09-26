# qa-intent

Rust + clap 实现的垂直领域 QA 决策 CLI，命令名 `qai`。将题库按版本化 taxonomy 编译为
Laya/JEV System One 请求，输出可被应用消费的 JSONL，记录人工标签并离线评测。

## 安装与使用

```sh
npm install -g qa-intent
qai --version
qai init
qai validate
qai compile -i item.json --raw
qai ask -i item.json
qai review
qai evaluate
qai export -i labeled.jsonl > supervised.jsonl
```

npm 包包含五平台原生二进制的可选依赖；支持 macOS arm64/x64、Linux glibc arm64/x64、
Windows x64。Linux 构建使用 Ubuntu 24.04，要求 glibc 2.39+；Alpine/musl 未提供预编译包。
也可克隆仓库后执行 `cargo install --path . --locked`。
npm 的 `qai` 名称是命令，不依赖同名 npm 包。

从 [examples](examples) 复制业务模板即可运行：
`qai -t examples/taxonomy.json compile -i examples/items.json --raw`。
JSON 支持单对象/数组；`.jsonl` 和 `-i -`（stdin）逐行处理，适合大题库。

## 数据契约

单题示例：

```json
{
  "id": "ticket-01",
  "qid": "intent_type",
  "question": "请帮我查上个月的账单",
  "expected": "lookup"
}
```

`taxonomy.json` 定义稳定问题标识、类型、判定说明、候选与门槛：
```json
{
  "version": "1.0.0",
  "model": "laya",
  "questions": {
    "intent_type": {
      "type": "choice",
      "instructions": "用户希望完成什么任务？",
      "criteria": {
        "lookup": "查询已有事实或记录",
        "calculation": "数值计算",
        "other": "不属于其他类别"
      },
      "threshold": 0.7,
      "min_confidence": 0.7,
      "action": "route_to_solver"
    }
  }
}
```

首版支持 `noul` 和 `choice`，这是 CLI 支持的子集，不是上游全部题型。
`qid` 是请求/响应关联标识，不代表独立训练头。
`choice` 的 `criteria` 至少有两项；题目可用 `options: [{key, description}]`
动态替换候选，重复 key 会报错，不能把 `noul` 隐式改成 `choice`。

`reference_answer` 与 `user_answer` 只在语义判分等任务确有需要时提供。
`expected` 是人工真值：noul 为布尔，choice 为候选 key；它和 `tags` 不进入推理 state，
避免把评测标签泄露给模型。参考答案的有效性与题目本身的歧义由题库维护者负责。

## 调用与应用嵌入

默认端点为 `https://company-ai.addzero.site/v1/systemone`。
用 `SYSTEMONE_ENDPOINT` 或 `--endpoint` 覆盖；认证读取 `CODEX_GROUP_KEY`。
`--model typesafe/jev` 切换 JEV。仅调用端点需要网络，其他命令离线运行。

`ask` stdout 每行一个 JSON，包含：
`item_id, qid, model, taxonomy_version, decision, needs_review, answer, usage`。

- `decision.prediction`：布尔值或候选 key。
- `decision.accepted`：通过门槛后才可进入业务分支。
- `decision.action`：仅命中且通过门槛才返回，CLI 不执行业务动作。
- noul 用 `threshold` 判正例，再按所判标签的概率与 `min_confidence` 比较。
- choice 用所选候选的 `probabilities` 或 `answer_confidence`，
  通过 `max(threshold, min_confidence)` 才接受；不把熵指标 `confidence` 混用。
- 未知标签、非法概率、类型不符、缺失 answers、HTTP 错误均以非零退出码报告。

0.7 只是示例起点，应基于领域验证集与错误代价调参。
JSONL 批处理遇错停止，之前输出/回流的成功记录保留；调用方按 item_id 去重后重试。
HTTP 客户端复用连接，当前推理请求串行执行，性能主要受上游影响。
Rust 应用可复用 `src/lib.rs` 暴露的 Compiler、Client、Decision，不必启动子进程。

## 数据回流与训练边界

每次 ask 默认追加 `feedback.jsonl`，包含原始答案、输入材料、实际模型、
taxonomy 版本、预测与人工 expected。业务材料留在本地，不要把反馈文件提交到公共 Git。
`--no-feedback` 关闭记录，`--feedback <path>` 覆盖位置。
`review --pending-only false` 输出全部；默认只输出低置信或与人工真值不符的记录。

`evaluate` 按实际模型、taxonomy 版本、qid 分组，输出准确率、覆盖率和接受样本准确率；
无人工标签时准确率为 null。指标按记录计数，重试日志应先按业务规则去重。

`export` 生成 `qa-intent.supervised.v1` JSONL，含 request 与独立 target。
这是通用训练交换格式，需接入训练器适配；本 CLI 不执行权重微调、注册或切换检查点。
训练/评测切分应按题目来源、同义改写组进行，避免数据泄漏。
固定格式分类题是否可解、准确率与校准情况，需要真实领域评测确认。

## 开发与发布

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked --target aarch64-apple-darwin
node npm/scripts/gen-platform-packages.mjs aarch64-apple-darwin
npm run test:package
```

CI 对五平台进行原生构建和真实 tarball 安装测试。
默认分支推送后发布当前版本，已发布版本跳过，缺少认证会明确失败。
见 [发布说明](docs/publishing.md) 和 [AIO 说明](AIO.md)。
随 npm 包提供 [qa-intent skill](skills/qa-intent/SKILL.md)。

协议解析参考 [固定版本 Laya 源码](https://github.com/NandhaKishorM/laya/blob/d211a50dd816280e01f7782668f23000664c8a32/laya/agent.py)。
本地 mock 测试不代表已验证真实模型准确率。

MIT
