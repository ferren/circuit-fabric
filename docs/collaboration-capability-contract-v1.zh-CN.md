# 协作能力探测契约 v1.1

## 2026-10-30 连接后实测增补（评论 9086）

连接障碍已解除：真实 Bridge hello/status 均为 `executorConnected=true`。本轮通过用户插件中已选的 CircuitFabric 项目 `MojianC3` 发起只读回读，并通过 EDA「高级 → 运行脚本」执行原生 SDK 探测，结果为 `artifacts/collaboration-584-connected/native-readonly.json`。这两条路径均为真实宿主调用；Bridge 读取在该 CF 项目新增探测会话及观测审计，没有修改 EDA 图纸。

宿主 API 报告版本 `4.1.78`。类型查询与构建通过：`@jlceda/pro-api-types 0.4.27`（声明 SHA-256 `3e91ca9432bc6a640d5a7983c044ec63630ca94e5398744e17e0ac162674d5aa`），TypeScript `5.9.3`。安装资源 `pro-api/0.3.53.61b2011d` 与声明包版本不同，npm 没有同名 `0.3.53` 包；不据类型检查推定宿主支持全部新 API。

已实测成功：`getCurrentProjectInfo` 完整工程树、`getAllSchematicsInfo`、`getAllSchematicPagesInfo`、`getCurrentSchematicPageInfo` 和 `getSplitScreenTree`。工程内有两个板子、两个原理图、两个同名 P1 图页及两个 PCB；这是已有工程的两页现场，不是新建的专用双图页测试工程。

| 对象 | 实际 UUID |
|---|---|
| 原生工程“4寸墨水屏日历相框” | `135d2e3c9573727175c3c7596ab2f66b90484cf174de38bfb60d0c03aa72d9ea` |
| 原理图 Schematic1_2 | `6fd29b34354b42d3` |
| 其 P1 图页 | `5dfc602f34949c83` |
| 原理图 Schematic1_3 | `3294ada314a7bc43` |
| 其 P1 图页 | `929f75373a8c7455` |

当前 P1 连续两次 inspect 均为 40 器件、13 网络、inconclusive 空，快照哈希相同：`sha256:afb98667d474380f6156f874dfff31d1d29e77f5ecb5c5035e8ce7fca5462e32`。工程 UUID 和当前页 UUID 重复读取一致；**只验证同页即时身份，不代表切图、重开或重启后身份稳定**。原生树证明不能只用页名 P1 做绑定。

脚本首次 WebSocket 回传被真实宿主拒绝，提示“未启用扩展和脚本的外部交互权限”。没有修改宿主权限设置，改用 SDK 原生保存对话框成功导出 JSON。这是原生脚本入口权限，与已获外部交互权限的 CircuitFabric 扩展执行器不同。

新增关键约束：类型查询得到的 `DMT_Project.openProject` JSDoc 明确说明，原工程如有未保存变更会被**直接丢失**；当前 SDK 查询未找到可靠 dirty/unsaved 状态接口，也未找到独立文档 sync API。因此“打开工程”不能自动当作无害的可重试命令。继续创建/切换独立测试工程前，需要用户确认当前工程已保存；不替用户保存当前工程，也不静默切换。已准备 `eda-host/lifecycle.ts`，默认门闩在任何写入前阻止执行，目标为独立 `CF584-Lifecycle-20261030` 工程；尚未执行，不计入现场通过项。

已确认连接和真实工程树，但文档创建/打开/改名/保存/同步、专用双图页测试工程、未保存分支、原生检查点、重启与多实例身份，以及四运行时原生问答/图像/恢复仍未完成验收。阶段门禁继续关闭；父任务应按实际安全约束确认后续探测方式。

2026-10-30 增补（评论 9082）：验收覆盖当前桌面端的 **Codex、Claude Code、Kimi Code、DSH 全部四种运行时**。下方 2026-10-29 的结果保留为历史基线，以本增补的现场结果为准；证据数据结构仍为 schemaVersion 1，探测脚本版本为 1.1.0。

## 2026-10-30 重新确认

实际版本命令均成功：Codex `0.165.0`、Claude Code `2.1.302`、Kimi Code `2.3.1`、DSH `0.1.2-rc.1`。这证明本机 CLI 可以启动，不证明已完成 EDA 受控工具任务。探测必须使用与桌面相同的用户 PATH；受限环境可能看不到 kimi/dsh。Windows DSH 探测直接调用已安装 Node 入口，避免 cmd 包装器影响进程超时回收。

EDA **确实已打开**：真实进程的窗口标题为 `4寸墨水屏日历相框 | 嘉立创EDA(专业版) - V4.1.78`，运行包版本 `4.1.78.34f5b8d9`；磁盘资源目录包含 `pro-api/0.3.53.61b2011d`、`pro-ui/4.1.78.435b55cf`。资源目录版本不等同于扩展当前实际使用的 SDK；未得到入口回报前 SDK 运行版本仍未验证。

本轮临时启动磁盘已有 Bridge（构建时间 2026-10-28），真实 WebSocket hello/status/list_projects 均成功，但 hello 和 status 都返回 **executorConnected=false**。列出的 Grow-G1.2 / Microduck / MojianC3 / WirelessMic 是 CircuitFabric 项目注册表，**不是 EDA 原生工程树**。因此当前阻塞从“没有 EDA 进程”更新为“EDA 已开，但插件执行器尚未接入 Bridge，未取得原生 API 回读”。本任务启动的 Bridge 已在探测结束后停止，未承诺常驻服务。

| 能力 / 当前代码路径 | Codex | Claude Code | Kimi Code | DSH |
|---|---|---|---|---|
| 版本命令真实验证 | 0.165.0，通过 | 2.1.302，通过 | 2.3.1，通过 | 0.1.2-rc.1，通过 |
| 桌面任务分派 | 已有适配器 | 已有适配器 | 已有适配器，kimi_account | 已有适配器，Chat Completions |
| EDA 当前聊天路由 | 固定此运行时，仍待端到端验证 | 未接入 | 未接入 | 未接入 |
| 原生会话恢复 | 持久 thread/resume 代码；本轮空线程无 rollout，不计通过 | 当前每任务 CLI，无原生绑定 | 当前 one-shot stream-json，无原生绑定 | 当前 headless 每任务启动，无原生绑定 |
| 受控 MCP 工具 | 已有发现、授权收窄 | 已有 strict MCP 和白名单 | kimi_command 收到 servers 但未注入命令，不能宣称已有授权工具闭环 | patch 注入指定 MCP，关闭默认文件/执行工具 |
| 图像输入 | localImage 代码，实际模型识图未验证 | 当前显式拒绝 | 当前显式拒绝 | 当前显式拒绝 |
| 用户提问回答 | server request 当前拒绝 | 未接交互控制通道 | 未接交互控制通道 | 未接交互控制通道 |
| 取消 | 取消标记和子进程路径已有，四者本轮均未做真实生成取消验收 | 同左 | 同左 | 同左 |
| 子智能体 | 显式关闭 | 工具白名单未开放 | 当前未提供隔离委派契约，原生能力未验证 | patch 显式关闭 subagent 工具 |
| 当前可用降级边界 | 可继续现有 Codex 路径，不能据此宣称全部能力通过 | 将来只能明确标注摘要续办，不能伪装原线程 | 同左；工具隔离未核定前不得开放工程写入 | 同左；不凭 headless 输出伪造 token 用量 |

代码证据：`execution.rs` 中 AgentKind、图像前置拒绝、claude_command / kimi_command / dsh_command；`circuitfabric-jlc-bridge.rs` 的聊天 Provider 选择、backend_id 和 run_session_observed 均固定 Codex；iframe 也没有运行时选择器。此为阶段 1 发现的实际缺口，**不能把增加版本探测误报为已经实现四运行时 EDA 支持**。依既有阶段约束，统一运行时分派、会话绑定、用户输入与 UI 选择器仍由阶段 3 实现；父任务应按四运行时验收，不缩减成二选一。

新增只读重现命令：

```powershell
python scripts/collaboration-probe/bridge_probe.py --out artifacts/collaboration-584-recheck --start-bridge target/debug/circuitfabric-jlc-bridge.exe --settings "$env:APPDATA/CircuitFabric/runtime.json"
python scripts/collaboration-probe/runtime_probe.py --out artifacts/collaboration-584-recheck/runtime
```

bridge_probe 依赖已安装的 Python websockets，单次启动窗口 8 秒，RPC 分别限时，原始帧保存在 bridge-rpc.jsonl，结果保存在 bridge-result.json。仅由 --start-bridge 启动的子进程才由脚本结束；连接既有服务时不停止别人的服务。它不写 EDA、不切图、不创建工程、不触发模型。临时服务停止后，版本探测中 TCP 端口超时不与此前的 WebSocket 成功矛盾。

本轮仍需真实扩展执行器连接后再采集工程树、页 UUID、创建/打开/保存/同步、稳定身份和双图页工程。阶段门禁未通过，后继保持 backlog；本增补不改变原来的确认、幂等和恢复要求。

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
