# qai — Laya/JEV 题库编译器

把垂直领域的 QA 题库编译成 [Laya/JEV System One](https://github.com/zjarlin/sub2api) 决策信封的工具。

Laya/JEV 是**判定器**，不是生成器：它做一次前向传播给出校准概率，只接受
`{state, questions}` 这一种请求形状。`qai` 负责把题库数据编译成这个形状，
并把结果回流成训练种子。

## 它解决什么

| 层 | 负责 | 本工具 |
|---|---|---|
| 题库（数据） | 题目、选项、参考答案、标签 | 输入 |
| 决策层（Laya/JEV） | 在给定材料下判定类别 / 是否 | 调用 |
| 应用层 | 阈值、动作映射、回流 | 本工具编排 |

不能交给 Laya 的事：生成文本、多步推理、开放式问答。`qai` 的标签体系把这层
边界写进契约，避免把题库塞进请求体。

## 安装

```bash
cargo install --path .
# 或
npm install -g qa-intent
```

## 快速开始

```bash
# 1. 生成一份示例标签体系
qai init

# 2. 校验契约
qai validate
# 标签体系合法：version=1.0.0 model=laya qid 数量=2
#   - equivalent [noul] 阈值=0.7
#   - intent_type [choice] 阈值=0.5，候选 3 个

# 3. 写一道题
cat > item.json <<'EOF'
{
  "id": "bird-01",
  "question": "树上7个鸟，开了一枪还剩几个鸟？",
  "qid": "equivalent",
  "reference_answer": "0个，枪声吓飞其他鸟",
  "user_answer": "都飞走了所以是0"
}
EOF

# 4. 编译成信封
qai compile -i item.json --raw | jq .
```

```json
{
  "model": "laya",
  "state": {
    "question": "树上7个鸟，开了一枪还剩几个鸟？",
    "reference_answer": "0个，枪声吓飞其他鸟",
    "user_answer": "都飞走了所以是0"
  },
  "questions": {
    "equivalent": {
      "type": "noul",
      "instructions": "用户的回答与参考答案在语义上是否等价？"
    }
  }
}
```

```bash
# 5. 调用决策端点并按阈值判定
export CODEX_GROUP_KEY='你的 codex 分组 Key'
qai ask -i item.json
# 题目: bird-01  qid: equivalent
#   概率: 0.9300  阈值: 0.7  命中: 是
#   置信度: 0.8800
#   动作: mark_correct
#   实际模型: laya-rl-agent
#   输入 token: 42  输出 token: 0

# 6. 查看需要复核的低置信/错判样本
qai review
```

## 标签体系（taxonomy）

标签体系是**训练与应用共用的契约**。qid 的定义集中在这里，不散落在业务代码里。

```json
{
  "version": "1.0.0",
  "model": "laya",
  "questions": {
    "intent_type": {
      "type": "choice",
      "instructions": "这道题属于哪一类意图？",
      "criteria": {
        "common_sense": "常识推理题",
        "calculation": "数值计算题",
        "lookup": "事实查询题"
      },
      "threshold": 0.5,
      "action": "route_to_solver"
    },
    "equivalent": {
      "type": "noul",
      "instructions": "用户的回答与参考答案在语义上是否等价？",
      "threshold": 0.7,
      "action": "mark_correct"
    }
  }
}
```

硬约束（由 `qai validate` 强制执行）：

- `type` 只允许 `noul` 和 `choice`
- `choice` 必须提供非空 `criteria`；`noul` 不允许有 `criteria`
- `qid` 必须是小写字母开头的语义化短名，拒绝 `q1` / `问题1` 这类编号
- `threshold` 必须在 `(0, 1)` 区间内

## 运行时植入题库

题库选项变化**不需要重新训练**：`compile` 会用题目自带的 `options`
动态生成 `criteria`，覆盖标签体系里的静态定义。模型只做匹配，选项由应用层提供。

```json
{
  "id": "bird-01",
  "question": "树上7个鸟，开了一枪还剩几个鸟？",
  "qid": "intent_type",
  "options": [
    {"key": "common_sense", "description": "需要常识推理"},
    {"key": "calculation", "description": "纯数值计算"}
  ]
}
```

## 回流机制

每次 `ask` 都会追加一条 JSONL 记录，低置信（`< 0.7`）或与业务真值不一致的样本
标记为 `needs_review`，作为下一轮训练的种子。

```json
{
  "item_id": "bird-01",
  "qid": "equivalent",
  "model": "laya",
  "answer": {"type": "noul", "noul": 0.93, "confidence": 0.9},
  "hit": true,
  "confidence": 0.88,
  "needs_review": false,
  "recorded_at": "2026-09-26T06:52:49Z"
}
```

## 命令

| 命令 | 说明 |
|---|---|
| `qai init [--force]` | 生成示例标签体系 |
| `qai validate` | 校验标签体系契约 |
| `qai compile -i <file> [--raw]` | 编译题库为信封，`--raw` 便于管道 |
| `qai ask -i <file>` | 编译 + 调用 + 阈值判定 + 回流 |
| `qai review` | 查看待复核样本 |

全局参数：`-t/--taxonomy <path>`、`--feedback <path>`。

`ask` 参数：`--model`、`--endpoint`（或 `SYSTEMONE_ENDPOINT`）、
`--api-key`（或 `CODEX_GROUP_KEY`）、`--timeout`、`--no-feedback`。

## 工程化形态

```
┌─ 应用层 ─────────────────────────────┐
│  题库 DB（题/选项/答案/标签）          │
│  ↓ qai compile                         │
│  {state, questions} 信封               │
│  ↓ POST /v1/systemone                  │
│  Laya/JEV（判定器）                     │
│  ↓ answers[qid]                        │
│  阈值 + 动作映射（判分/路由/复检）       │
└───────────────────────────────────────┘
        ↑ qai review 低置信样本回流
┌─ 训练层 ──────────────────────────────┐
│  QA 库 → 标签体系 → 编译 → 分桶         │
│  → 训练 → 评测 → 注册检查点             │
└───────────────────────────────────────┘
```

## 落地顺序

1. 建标签体系和编译模板，用现有 `laya` 跑，不训练
2. 接阈值与回流埋点，积累真实分布
3. 按 qid 统计准确率，找出需要提升的 qid
4. 只对样本充足、标签稳定的 qid 训新检查点
5. 评测通过后切流量，保留旧检查点回滚

## 开发

```bash
cargo test
cargo build --release
```

## 发布

见 [docs/publishing.md](docs/publishing.md)。推送到 `main` 会触发
五平台构建与 npm 发布（Trusted Publisher 或 `NPM_TOKEN` 二选一）。

## License

MIT
