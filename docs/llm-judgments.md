# 第三方 LLM 判断后端

「智能体与工具 → Jev 判断工具」支持 TypeSafe Jev 和内嵌 Rust LLM adapter。两种后端共用 `typesafe-jev` server id 和 `evaluate` 工具，沿用现有全局/项目授权及数据手册复核。

## 配置和使用

页面由状态总览、配置列表和连接测试组成：列表值始终等于已保存状态，「判断后端」与「API 密钥」在弹窗中修改。

1. 在配置列表「判断后端」行点「编辑」，在弹窗中选择「第三方 LLM 模拟」分支，再选择 DeepSeek 或 z.ai 预设，或填写自定义兼容 Chat Completions 的 Base URL、模型 ID、密钥环境变量名。Base URL 自动追加 `/chat/completions`，不要填写完整接口路径。模型 ID 可按账户权限修改。
2. 默认使用概率回答和 JSON Object；支持离散回答、JSON Schema、Prompted JSON、归一化、单次请求超时（5～180 秒）及结构修正重试（0～3 次），均为带标签的分段选项。JSON Schema 需服务商支持；不支持 JSON 输出参数的端点可选 Prompted JSON。
3. 点击弹窗中的「保存」后立即生效并写回 `runtime.json`；点「取消」保持现状，重新打开弹窗会把表单恢复为已保存值。保存后状态总览、配置列表与左侧卡片摘要立即显示新后端；保存失败在弹窗内提示。
4. 在配置列表「API 密钥」行点「更换」，解锁「密钥保险库」后保存对应 API Key，或在启动环境设置同名变量。变量名随已保存后端切换，密钥值不进 `runtime.json`、启动参数或错误日志。
5. 启用与全局/项目授权是列表行内的即时开关（点击立即写盘并取消进行中的任务）。切换后端保留原来的启用状态和授权；此前停用的工具需要重新启用。
6. 在「连接与测试」区：「连接并发现工具（不计费）」只握手和发现工具，不请求模型。「测试真实判断（消耗额度）」使用已保存配置，发送一条 LED 判断，消耗模型额度。

| 预设 | Base URL | 默认模型 | 密钥变量 |
| --- | --- | --- | --- |
| DeepSeek | `https://api.deepseek.com/v1` | `deepseek-flash` | `DEEPSEEK_API_KEY` |
| z.ai 编程包 | `https://api.z.ai/api/coding/paas/v4` | `glm-5.3-flash` | `ZAI_API_KEY` |
| z.ai 按量付费 | `https://api.z.ai/api/paas/v4` | `glm-4.7` | `ZAI_API_KEY` |

**z.ai 端点按密钥的计费方式二选一**（官方文档：[Chat Completions](https://docs.z.ai/api-reference/llm/chat-completion)、[Coding Plan 配置](https://docs.z.ai/devpack/tool/others)）：

* GLM Coding Plan 订阅密钥 → `https://api.z.ai/api/coding/paas/v4`；用错端点典型报错为 1113「余额不足」或 401。
* 按量付费（账户余额）密钥 → `https://api.z.ai/api/paas/v4`。
* 不要使用 `https://api.z.ai/api/v1`：非官方文档端点，鉴权失败时以 HTTP 200 + 非 OpenAI 错误对象返回，会导致判断调用解析失败（适配器现已识别该形态并明确报错）。
* LLM Provider 页与 Jev 页的密钥变量名相互独立（如 `Z_API_KEY` 与 `ZAI_API_KEY`）；Jev 页保存密钥时以「② API 密钥」显示的变量名为准。

可在后端弹窗中选择「TypeSafe Jev（随附二进制）」分支后点「保存」切回。第三方参数保留在 `catalog.llm_judge`，下次启动可恢复。首次使用 LLM 模式不依赖 TypeSafe 二进制。

## 工具接口

`tools/list` 提供名称、用途和参数结构，`initialize` 提供使用指引，沿用现有 MCP 发现和注入通道。例如：

```json
{
  "state": "Pin 1 VIN Power input",
  "questions": {
    "faithful": {
      "type": "noul",
      "instructions": "Does the quoted evidence identify VIN as a power input?"
    },
    "category": {
      "type": "choice",
      "instructions": "Classify the source line, choosing other if evidence is insufficient.",
      "criteria": {"pin": "Pin table row", "other": "Other or uncertain"},
      "min_confidence": 0.5
    },
    "quality": {
      "type": "score",
      "instructions": "Assess how complete the pin description is.",
      "criteria": ["Missing", "Partial", "Complete"]
    }
  }
}
```

`noul` 输出 0～1 的是/否估计；`choice` 输出标签、分布和集中度；`score` 输出从 0 开始的量表期望值、legend 和分布。问题 ID、标签和分布完整性严格校验；遗漏、额外标签、越界值、零和分布不会被当成有效判断。归一化关闭时，分布总和必须为 1（容差 `1e-6`）。离散模式将布尔/标签/等级编码为 0/1 或 one-hot 分布。

`choice` 集中度为 `(N * p_top - 1) / (N - 1)`；`noul` 的弃权判断使用 `abs(2*p - 1)`。低于 `min_confidence` 时设置 `uncertain: true`，choice 返回 `__uncertain__`，保留分布。score 集中度也使用归一化最高概率公式，并在元数据中明确公式，未声称与 TypeSafe score 公式相同。

支持 `items` 映射及可选共享 `state`：每条记录分别请求模型，上游状态为 `{"item": ..., "context": ...}`。结果为 `results`、`errors` 和 `meta`；全部失败时返回 MCP 工具错误。单次 evaluate 总时限 180 秒（低于 MCP 客户端的 210 秒），条目按顺序执行；大批次或较慢模型请分批调用。HTTP 错误、拒绝和截断直接报错；仅不符合答案结构的响应执行配置次数的修正重试。

## Python adapter 与 Rust 生态

本实现参照官方 [system-one-adapter-python](https://github.com/typesafe-ai/system-one-adapter-python) 的类型化回答、严格校验、两种回答模式、概率归一化及结构修正机制，以 `reqwest` + `serde_json` 原生集成；未引入 Python 运行时，也未声称完整复刻该库的所有 provider、重试和调试功能。

Rust 项目 [jev-rs](https://github.com/yijunyu/jev-rs) 通过 next-token logprobs 获取分布，并提供 MCP `judge` / System One HTTP 服务。它与 Python adapter 的 JSON 输出机制不同，需要服务商支持对应 logprobs 参数。此次采用通用 JSON 适配，覆盖 z.ai、DeepSeek 和其它兼容端点。

LLM 概率未经校准。响应携带 `meta.backend: "llm_adapter"`、`meta.calibrated: false` 和 `probability_source`；离散 one-hot 只表示选择，不能解读为实际 100% 置信度。现有原文证据校验、授权检查和阈值仍执行。数据手册提取结果会记录 LLM 后端；中断后切换后端或模型再继续时，旧判断缓存失效，候选数据重新复核。

端点和参数参照 [z.ai API](https://docs.z.ai/api-reference/llm/chat-completion) 和 [DeepSeek JSON 文档](https://api-docs.deepseek.com/guides/json_mode/)。服务商实测需要有效密钥；本地测试通过真实 MCP 子进程和 HTTP mock 验证协议、密钥注入、解析及错误路径。

## 构建与分发

`cargo build -p circuitfabric-desktop --features native-ui` 将 adapter 编入应用。MCP 使用同一个可执行文件的 `--jev-mcp <非密钥配置 JSON>` 分支，在 GPUI 初始化前处理，不打开额外窗口。可选 `circuitfabric-judge` launcher 使用同一实现，供集成测试或独立 MCP 客户端使用。

```text
cargo test -p circuitfabric-codex-runtime
cargo check -p circuitfabric-desktop --features native-ui
```
