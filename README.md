# qa-intent

输入自然语言业务场景，批量生成客服问答和意图标签，导出 JSON、JSONL 或 CSV，也可编译为 Laya/JEV 决策请求。Rust + clap 实现，命令名 `qai`。

**GitHub 和 npm 使用仓库根目录这一份 README。** 发布时原样打包，安装测试检查内容一致，发布后再次核对 npm registry 的 README。

## 1. 安装

```sh
npm install -g qa-intent@0.2.0
qai --version
qai generate --help
```

需要 Node.js 18+。提供 macOS ARM64/x64、Linux glibc ARM64/x64、Windows x64 原生二进制。Linux 预编译包要求 glibc 2.39+，不支持 Alpine/musl。安装时请保留 optionalDependencies。其他环境可在源码目录运行 `cargo install --path . --locked`。

## 2. 输入场景，直接生成题库

生成题库需要**能输出文本的通用模型**；Laya/JEV 用于后续判断，不负责写题。先配置你的服务，下面三个占位值必须替换：

```sh
export QAI_GENERATOR_BASE_URL="https://你的模型服务地址/v1"
export QAI_GENERATOR_API_KEY="你的API密钥"
export QAI_GENERATOR_MODEL="你的文本生成模型ID"

qai generate "电商客服。未发货订单可申请退款；已发货订单转人工核实。围绕退款、取消订单和信息不足生成问答，不编造退款到账时限。" \
  --api chat --count 50 --batch-size 10 --concurrency 2 \
  --output bank.json

qai validate --bank bank.json
```

上例使用兼容 `POST /v1/chat/completions` 的服务。若服务仅支持 JSON Object 模式，增加 `--json-mode`；本地结构校验仍然执行。默认使用 JSON Schema 严格结构化输出，模型不支持时会报错，不会静默换模型。

如果使用 OpenAI Responses 接口，把基地址设为 `https://api.openai.com/v1`，选择该服务可用且支持结构化输出的模型，并省略 `--api chat`，或显式指定 `--api responses`。基地址只到 `/v1`，CLI 自动追加接口路径。

上面的 `export` 是 macOS/Linux shell 写法。PowerShell 用 `$env:QAI_GENERATOR_MODEL="你的模型ID"` 等同样设置这三个变量；命令参数保持一致。

### 输入较长的场景或业务资料

```sh
qai generate --scenario-file scenario.txt --context policies.txt \
  --api chat --count 200 --batch-size 20 --concurrency 4 --output bank.json
```

`scenario.txt` 写业务范围、目标用户、希望覆盖的意图；`policies.txt` 写已确认的业务规则和客服口径，均为 UTF-8 文本。`--scenario-file -` 从 stdin 读取场景。位置参数场景和 `--scenario-file` 二选一。

程序先生成一份固定标签体系，再并发生成问答批次，检查数量、标签和重复问题，最后输出完整题库。JSON 结构正确不代表业务答案正确：没有提供的金额、政策和时效，提示词要求模型说明需核实，生成结果仍应人工审核。

### 没有 API 时，先导出提示词

```sh
qai generate "电商售后客服，退款规则以我提供的资料为准" \
  --context policies.txt --count 30 --prompt-only --output prompt.txt
```

此命令完全离线，不需要模型或密钥。将提示词交给任意文本模型，得到完整题库 JSON 后，用 `qai validate --bank bank.json` 校验，再使用下面的导出命令。提示词模式输出文本；正常生成模式输出 JSON。

## 3. 生成的 JSON 长什么样

`bank.json` 是完整交换文档，顶层字段如下：

| 字段 | 含义 |
| --- | --- |
| `schema_version` | 固定为 `qa-intent.bank.v1` |
| `scenario` | 输入的自然语言场景 |
| `generation` | 请求的生成模型 ID、接口类型，便于追溯；不保存密钥 |
| `taxonomy` | 版本化意图体系，含候选标签和 Laya/JEV 决策模型 ID |
| `items` | 问答列表，每条都有稳定 ID、问题、客服答案和建议意图 |

每条 `items` 的结构：

```json
{
  "id": "qa-000001",
  "qid": "intent_type",
  "question": "还没发货，能退款吗？",
  "answer": "未发货订单可以申请退款，请提供订单信息以核实状态。",
  "intent": "refund",
  "tags": ["未发货"],
  "needs_review": true
}
```

`answer` 是客服答复草稿；`intent` 是模型建议的分类标签。所有新生成条目的 `needs_review` 都是 `true`。人工核实并修正内容后，可将该条改为 `false`。

题目 ID 由 CLI 顺序分配。重复检测忽略空白和大小写，不做语义去重；近义改写是否冗余需复核。同一题库始终共用一套标签，`qid` 是问题标识，不是独立模型训练头。

完整示例：[customer-service-bank.json](https://github.com/zjarlin/qa-intent/blob/main/examples/customer-service-bank.json)。

## 4. 导出给客服使用

```sh
# JSON 数组，每条保留 question、answer、intent 等字段
qai export -i bank.json --format faq-json --output faq.json

# 带 UTF-8 BOM 的 CSV，可用 Excel 打开
qai export -i bank.json --format faq-csv --output faq.csv
```

CSV 列为 `id,question,answer,intent,tags,needs_review`；`tags` 单元格是 JSON 数组字符串。逗号、引号和换行按 CSV 标准转义。以公式字符开头的单元格添加单引号，避免被表格软件执行；需要原始文本时使用 JSON。

这是**通用客服数据导出**，没有调用任何客服平台上传接口。导入具体平台时，将 `question` 映射到问题列、`answer` 映射到答案列；平台有额外字段要求时在应用层转换。请先复核 `needs_review=true` 的内容再用于真实客服答复。

## 5. JSON 直接接入程序

不传 `--output` 时，stdout 只输出完整 JSON，进度写入 stderr，可以直接使用管道：

```sh
qai generate "售后客服：未发货可退款，已发货需人工核实" --api chat --count 20 > bank.json

# 不经过临时文件，直接把生成题库转换成客服 JSON 数组
qai generate "售后客服：未发货可退款" --api chat --count 20 \
  | qai export -i - --format faq-json
```

Python、JavaScript、Java、Go 等语言只需解析标准 JSON，不依赖特定 SDK。Python 读取题库的例子：

```python
import json

with open("bank.json", encoding="utf-8") as file:
    bank = json.load(file)

for item in bank["items"]:
    if not item["needs_review"]:
        print(item["question"], item["answer"], item["intent"])
```

启动 CLI 子进程时，请使用参数数组并检查退出码，解析 stdout，将 stderr 作为日志。生成任务在全部批次校验成功后才输出 JSON，不是逐 token 流式输出。`--output` 使用临时文件完成后落盘，不覆盖已有文件；shell 的 `>` 遵循 shell 自身的覆盖规则。

## 6. 接入 Laya/JEV 应用决策层

客服问答和模型决策请求用途不同。导出 System One 请求时，`answer`、`intent` 和标签不会进入推理 `state`，避免把答案泄露给分类模型：

```sh
# 每行一个可 POST 到 /v1/systemone 的请求
qai export -i bank.json --format systemone --output requests.jsonl

# 分别导出既有 CLI 使用的标签体系和题目
qai export -i bank.json --format taxonomy --output taxonomy.json
qai export -i bank.json --format items --output items.jsonl
qai -t taxonomy.json compile -i items.jsonl --raw
```

生成时默认决策模型是 `laya`，可用 `generate --decision-model typesafe/jev` 改为 JEV。这与 `generate --model` 指定的文本生成模型是两个独立设置。

运行时新增一条真实用户问题，无需重新生成题库或训练权重：

```sh
export SYSTEMONE_ENDPOINT="https://company-ai.addzero.site/v1/systemone"
export CODEX_GROUP_KEY="你的决策端点密钥"

printf '%s\n' '{"id":"live-001","qid":"intent_type","question":"我想取消还没发货的订单"}' \
  | qai -t taxonomy.json ask -i - --no-feedback
```

应用读取 `decision.prediction` 和 `decision.accepted`；只有 `accepted=true` 才进入业务分支，否则转人工或兜底。CLI 不执行业务动作，也不会自动向客服客户发送答案。修改 `taxonomy.json` 后，下次 CLI 调用即可使用新规则；常驻应用需自行重新加载。

`choice` 使用候选概率或 `answer_confidence` 判断门槛，不把熵指标 `confidence` 当作答案概率。默认 0.7 只是起点，需用领域验证集调整。`ask --model typesafe/jev` 可临时覆盖决策模型。

## 7. 训练导出、人工标签和评测

```sh
# 从完整生成题库导出训练草稿，保留标签来源和审核状态
qai export -i bank.json --format training --output training.jsonl

# 调用决策端点，默认在本地 feedback.jsonl 追加反馈
qai -t taxonomy.json ask -i items.jsonl
qai review
qai evaluate
```

`training` 每行包含 `request`、`target`、`label_source` 和 `needs_review`。未审核标签标为 `model_generated`，人工确认后标为 `human_reviewed`。格式为通用 `qa-intent.supervised.v1`，需要自行对接训练器；CLI 不执行模型权重微调。

`--format items` 对未审核条目不填写 `expected`；已审核条目才将 `intent` 写入 `expected`。修改审核状态后，应重新导出 `items.jsonl`。`evaluate` 根据反馈里的人工 `expected` 计算准确率、覆盖率和接受样本准确率，没有人工标签时准确率为 `null`。

原有手写 QaItem 的导出仍保留：`qai -t taxonomy.json export -i labeled.jsonl`，默认格式为 `supervised`，要求人工 `expected`。它不读取完整 `bank.json`，完整题库请显式选择 `training`。

原有题型支持 `noul` 和 `choice`。语义判分任务可使用 `reference_answer`、`user_answer`；示例见 [原有题库](https://github.com/zjarlin/qa-intent/blob/main/examples/items.json)。`expected` 和 `tags` 不进入推理材料。人工评测集应与生成训练集按来源、改写组隔离。

## 参数速查与错误处理

| 参数 | 默认值与用途 |
| --- | --- |
| `generate --count` | 20，总问答数，范围 1..10000 |
| `--batch-size` | 20，每次模型请求的题数，范围 1..100 |
| `--concurrency` | 2，并发批次数，范围 1..16 |
| `--retries` | 2，结构、数量、标签或重复校验失败后重新生成，范围 0..3 |
| `--timeout` | 120 秒，每次生成 HTTP 请求的超时 |
| `--api` | `responses`；兼容 Chat Completions 服务选 `chat` |
| `--base-url` | `QAI_GENERATOR_BASE_URL`；未设时为 `https://api.openai.com/v1` |
| `--model` | `QAI_GENERATOR_MODEL`，无默认值，必须是可用文本模型 |
| `--api-key` | `QAI_GENERATOR_API_KEY`；命令行优先，建议用环境变量避免 shell 历史留存 |
| `--json-mode` | 改用 JSON Object 模式；不会关闭本地校验 |
| `--output` / `-o` | 写入新文件；省略时输出到 stdout，目录必须已存在 |

正常情况下 API 请求数为 `1 + ceil(count / batch-size)`，第一步生成标签体系。HTTP 429/5xx 最多额外重试两次；`Retry-After` 秒数优先，等待上限 15 秒。拒绝生成、截断响应、401/403 等错误直接失败。重新生成可能产生额外模型费用。

错误返回非零退出码，生成失败不输出半份题库、不留下半写文件；当前没有断点续生成。提高并发可降低等待时间，但受服务限流约束，减少 batch-size 可缓解输出截断。Rust 编译/导出在本地执行，生成和判断速度主要由模型服务决定。

题库、业务资料、反馈可能包含业务数据，请按项目规则保管。生成会把场景和 `--context` 文本发送到你配置的生成服务；`ask` 则发送决策输入到决策服务。

## 开发与发布

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Rust 库入口 `src/lib.rs`：`generator` 负责批量生成，`bank` 负责题库校验/导出，`compiler` 负责 System One 编译。模型提示词维护在 [prompts/generate.md](https://github.com/zjarlin/qa-intent/blob/main/prompts/generate.md)。随包提供 [Skill](https://github.com/zjarlin/qa-intent/blob/main/skills/qa-intent/SKILL.md)。

CI 对五个平台构建并安装真实 npm tarball，检查 README 与源码一致。npm 通过 Trusted Publisher 发布；AIO 市场使用独立工作流，市场失败不会被当作 npm 发布成功的证明。详见 [发布说明](https://github.com/zjarlin/qa-intent/blob/main/docs/publishing.md) 和 [AIO 接入](https://github.com/zjarlin/qa-intent/blob/main/AIO.md)。

协议参考：[OpenAI 结构化输出](https://developers.openai.com/api/docs/guides/structured-outputs)、[固定版本 Laya 源码](https://github.com/NandhaKishorM/laya/blob/d211a50dd816280e01f7782668f23000664c8a32/laya/agent.py)。HTTP mock 测试验证协议和数据流，不代表真实模型生成质量或领域准确率。

MIT
