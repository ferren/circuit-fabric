# 协作能力探测契约 v1

2026-10-29，TASK-5-36-1（584），父任务 583。状态：**协议候选，阶段门禁未通过**。本记录不能代替现场验收，也不授权推进阶段 2。

## 版本与证据规则

本次基线提交 `03579f6`，工作区另有用户未提交修改。运行时现场版本：Codex CLI / 随包 App Server `0.165.0`，Claude Code `2.1.302`；CircuitFabric runtime crate `0.1.0`，扩展清单 `0.2.28`（仅磁盘清单，不是已加载版本）。未使用 TypeScript/Python Agent SDK。当前 EDA 运行时与 SDK 版本均未知。

`capability_evidence.rs` 定义独立的 `schemaVersion: 1` 证据记录，不改变 Bridge v1 operations，也不扩大其支持列表。字段为 capability、level、runtimeVersion、sdkVersion、scope、evidence。level 仅允许 unverified / documented / simulated / host_verified / failed / unsupported。只有 host_verified 且版本、范围、原始证据均非空时 `is_verified()` 才为真。客户端仍须审核证据内容，类型检查本身不证明证据真实。此模块是供后继消费的契约，尚未接入运行中的 hello 或 UI。

方法存在、SDK 文档、mock 测试均不能标为真实支持。版本未知不得补猜；升级 SDK、运行时或权限边界后应重新探测。`implemented_capabilities()` 是已有 Bridge 实现枚举，不能解释成 C14 宿主验收结果。

## 运行时矩阵

| 能力 | Codex 0.165.0 / 当前适配器 | Claude Code 2.1.302 / 当前适配器 | 本次证据与限制 |
|---|---|---|---|
| 初始化、创建线程 | 本机 App Server RPC 成功 | 仅版本命令成功 | `result.json` 和 `codex-rpc.jsonl`；未调用模型 |
| 原生线程恢复 | 适配器已有持久绑定和 thread/resume；空线程恢复实测失败 | 当前每任务 CLI 无 resume 参数 | `no rollout found`；不是已进行对话的恢复失败，也不证明恢复支持 |
| 授权工具 | 已有 MCP 发现及授权收窄代码 | 已有 strict MCP 配置和工具白名单 | 本次未实测模型调用工具；不能把代码检查标为现场通过 |
| 图像输入 | 已构造 localImage | 当前输入为文本，未接入图像消息 | 未实测；Claude 图像不能静默忽略后宣称成功 |
| 提问回答 | server request 一律拒绝，无回答回传接口 | print 模式，未接用户答复控制通道 | 当前适配器不支持交互闭环；不能靠摘要续办声称原线程问答通过 |
| 取消 | 取消标记及客户端退出逻辑已有 | 子进程终止路径已有 | 本次没有运行模型取消测试；取消不等于 EDA 写入回滚 |
| 原生子智能体 | 显式 features.multi_agent=false | 白名单未开放 Agent 类工具 | 当前适配器未开放；本次未委派子智能体 |

Codex 能力名称以 [OpenAI App Server 文档](https://learn.chatgpt.com/docs/app-server) 为参考。Claude 后续探测应对照 [Claude Code sessions 文档](https://code.claude.com/docs/en/agent-sdk/sessions)，不能将 Managed Agents 云端会话接口套到本机 Code CLI。本矩阵基于当前适配器检查和有限 RPC 实测，不据官方文档推定本机端到端支持。

## EDA 矩阵与命令边界

本轮未发现 LCEDA / EasyEDA / circuitfabric 进程；49630、49640 连接未建立。旧 `scripts/jlc-acceptance/probe_log.jsonl` 来自 2026-10-24，版本字段为空，且主要是图元、DRC、消息总线探测，不能作为本轮 C14 证据。

| 命令/能力 | 影响 | 幂等条件 | 确认与恢复范围 | 本次结论 |
|---|---|---|---|---|
| 读工程树、读 UUID | 只读，但活动图可能切换 | 同实例和绑定代次下可重读；采集前后复核身份 | 无写确认；不能用名称作为身份 | 未验证；getCurrentProjectInfo 不等于完整工程树 |
| 创建工程、文档、图页 | 原生工程结构写入 | 持久命令键和目标 UUID；超时后查询已创建对象，禁止直接重建 | 明确目标和创建内容；删除新建对象未验证前不自动补偿 | 未验证 |
| 打开、定位 | 改变用户现场上下文，可能触发未保存提示 | 目标 UUID、实例、绑定代次一致且已打开时可收敛 | 未保存时保存/保留/取消；恢复仅可尝试回到先前活动页 | 未验证 |
| 改名 | 元数据写入，影响引用显示 | 目标 UUID 和预期旧名一致；相同新名为无变化 | 明确 UUID 和新名；逆改名需旧名及无并发修改证据 | 未验证 |
| 保存 | 把编辑态持久化 | 同一修订重复保存；保存完成后需原生回读 | 明确范围；不能承诺撤回已保存文件 | 未验证 |
| 同步 | 含义须区分编辑器同步、云端同步、CF 观测刷新 | 按目标修订和同步类型去重 | 不混淆三者；未知结果先回读，不自动重放 | 未验证，尚不能冻结具体 SDK 调用 |
| 删除、覆盖、跨工程移动 | 高影响写入 | 请求键不能代替目标存在性和引用复核 | 默认禁用；明确影响确认且有可验证恢复后才开放 | 未验证、禁止对外标支持 |
| 未保存状态 | 只读探测加用户选择 | 状态带观测时间/修订，未知按未知处理 | 不静默保存、丢弃或覆盖 | 未验证 |
| 原生检查点 | 可能涉及持久数据写入/恢复 | 必须以宿主检查点 ID 和绑定核对 | 未证实覆盖文件/云端/跨页范围；不能用 Bridge 补偿称原生检查点 | 未验证 |
| 多实例识别 | 注册及路由身份 | 实例随机 ID + 连接代次 + 原生 UUID；重连复核 | 相同名称或扩展 UUID 不证明同实例 | 未验证 |

[DMT_Project 官方 API](https://prodocs.easyeda.com/en/api/reference/pro-api.dmt_project.html) 可用于后续核对签名；文档中的 API 名称不能直接写进 Bridge 支持表。

## 重现

```powershell
python scripts/collaboration-probe/runtime_probe.py --out artifacts/collaboration-584
node --test scripts/collaboration-probe/eda_probe.test.cjs
cargo test -p circuitfabric-codex-runtime --lib
```

runtime_probe 仅执行版本命令、TCP 连通性、初始化、创建及恢复空线程。使用输出目录内隔离 CODEX_HOME，不复制凭据，不产生推理费用。输出 result.json、原始双向 codex-rpc.jsonl 和 stderr；所有操作限时，结束时等待自己的子进程退出。空线程不落 rollout 的错误是预期可观察结果，不代表原生恢复验收通过。该脚本退出码只表示探测完成，不表示阶段门禁通过。

`eda_probe.cjs` 供真实扩展入口临时加载后调用 `await probeEda(eda, {runtimeVersion, sdkVersion})`，序列化返回值保存为原始 JSON。它只读取当前工程和图页，并重复读页身份，记录候选方法的存在性；不创建、打开、保存、同步或删除。不得把 Node mock 的结果提交为宿主 JSON。未超时的读调用也不能证明重启后的身份稳定。当前没有把临时探测菜单接入生产扩展。

## 双图页现场工程与阶段补验

**本轮未交付真实双图页工程。** 需要在真实宿主创建独立测试工程 `CF-C00-C14-<nonce>`，包含同一原理图下 `Probe-A`、`Probe-B` 两页及各自可区分内容。保存工程原生导出文件及创建前后树、工程/原理图/页 UUID、SDK 和客户端实际版本。按创建→打开 A→编辑→切 B 的保存/保留/取消分支→改名→保存→同步→关闭重开→重启读取的顺序取证；另开同名工程验证双实例不串图。导出文件未经宿主重新导入验证不能标通过。

运行时还需在有授权凭据的独立测试会话完成：第一轮记住 nonce，退出 App Server 后以同 threadId 恢复并回答 nonce；触发一次原生用户问题并用原 request ID 回答；输入一张已知内容图片并判断；分别验证取消和授权工具；另一运行时逐项记录原生支持和适配器缺口。不得使用当前工作会话冒充测试会话，也不得复制用户凭据进入扩展。

## 返回父任务的调整建议（待决，未批准）

保留阶段 1 阻塞和后继 backlog。补齐真实 EDA 现场、运行时推理凭据和可保存的双图页工程后再验收。Codex 用户输入处理需在后续统一会话阶段加入 request/answer 控制接口；Claude 当前的一次性 print 路径需要升级为可交互协议才能承诺原线程恢复/问答/图像。C14 工程写入在 API 签名、保存/同步含义与恢复范围实测前继续禁用；父任务确认这些缺口和范围后才能冻结生产协作协议。本轮只冻结证据格式 v1，不将候选命令实现为后续阶段功能。
