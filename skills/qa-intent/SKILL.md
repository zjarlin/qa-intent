---
name: qa-intent
description: 将垂直领域 QA、意图分类或答案判分需求转换为 qai CLI 的 taxonomy 和题库，编译 Laya/JEV 请求、导出监督样本并评测反馈。用于已有 qai CLI 的应用决策层集成。
---

# QA 意图编译器

将业务需求编译为两个可校验文件：taxonomy.json 定义判断契约，items.json/JSONL 定义输入材料。
先查看 `qai --help` 与随包 examples。qai 是 qa-intent npm 包提供的命令。

## 建模

- 明确是意图分类、选项归属还是参考答案语义判分。不要把三者的标签混用。
- qid 是稳定请求标识，不是训练头。默认只用本工具支持的 noul/choice 子集。
- choice 候选使用稳定 key 与清晰判定说明，至少两项；增加 other 等未覆盖类别时先定义边界。
- noul 是可判断的单命题。门槛由领域验证集与错误代价决定，不宣称统一的最佳数值。
- 题目歧义和缺失事实需要显式暴露。封闭候选题可以尝试模型推理，但能否可靠解答需评测。
- expected 是人工真值：noul 为布尔，choice 为候选 key。qid、type、候选变更须更新 taxonomy.version。
- 不把 expected/tags 写进推理材料。reference_answer 仅在有意提供参考答案的任务中设置。

## 输出与验证

生成 taxonomy.json 与 items.json 后运行：
```sh
qai -t taxonomy.json validate
qai -t taxonomy.json compile -i items.json --raw
qai -t taxonomy.json export -i items.json
```

不要发明字段；taxonomy 问题包含 type、instructions、criteria（choice）、
threshold、min_confidence、可选 action。题目包含 id、qid、question、
可选 reference_answer、user_answer、options、tags、expected。

调用经授权的业务端点时使用 SYSTEMONE_ENDPOINT、CODEX_GROUP_KEY 环境变量：
`qai -t taxonomy.json ask -i items.json`。
仅 decision.accepted 为 true 时使用 prediction，action 是输出建议，由应用执行。
保留实际 model 与 taxonomy_version，不把 choice.confidence 熵指标当作答案概率。

`qai review` 输出待复核记录，人工补 expected 后运行 `qai evaluate`；
`qai export` 生成通用监督 JSONL，不代表已经训练或注册模型。
编译/测试、真实模型评测、npm 发布和市场同步的完成状态分别说明。
