# 协作能力探测契约 v1.4（候选，门禁未通过）

## 2026-10-30 评论 9171：已确认的处理规则与其余验证边界

用户明确确认，以下规则取代历史章节中对应的“等待决定”：

1. dirty 保持 unknown 时禁止隐式关闭/工程切换；会话中说明目标与影响，等待用户显式说“已保存好”后才继续对应操作。确认只适用于该次目标和当前编辑状态；有新编辑、目标变化或重连后需重新确认，不能沿用旧保存确认。未找到 dirty API 不再单独视为必须实现原生检测的阻塞；后继需验证这条交互门禁。
2. 导入 API 将原生归档/外部工程装载到 EDA，用于导入和备份恢复，不是日常打开已有工程或本地保存的必经路径。需要使用时通知用户手工在 EDA 中授权，再执行并核对结果。现有“权限拒绝”证据保留，未补验时不标导入支持；不要求为日常编辑提前开启该权限。
3. 当前目标按“无原生检查点/回滚能力”记录，不再追索该能力作为本阶段阻塞。不承诺自动撤销已保存写入；手工备份/重新导入有独立边界，不能命名为原生检查点。

其余项目分为待验证和可选限制方案，不能把建议当用户已批准：

| 项目与用途 | 当前问题 | 可执行方案与剩余证据 |
|---|---|---|
| 多窗口工程写入路由 | Bridge 单 executor，不能保证请求进入正确窗口 | 建议先只允许一个明确绑定工程的执行器；第二连接、目标 UUID 不匹配或重连时拒绝写入。需验证拒绝和重新绑定，完整多实例支持留后继，不宣称已支持 |
| Claude/Z.ai 基础调用 | 测试进程无认证变量，apiKeySource=none | 仍需现有 Provider 正式注入认证后成功调用；仅配置地址/模型不能替代。无需原生 Anthropic 网络，不复制保险库秘密 |
| 受控工具 | 有配置代码，缺真实调用证据；Kimi servers 未注入，不能假定有工具闭环 | 阶段 1 用独立只读测试工具核对传入、授权、结果、拒绝；四运行时统一 EDA 接入的实现留阶段 3 |
| 各运行时原生附加能力 | Codex CLI 恢复/图片、App Server 问答/取消已过；其余按矩阵有缺项 | 不必把四运行时每格都做成同等支持。可将未验证能力保持禁用；Kimi 恢复固定原 cwd；摘要续办必须标新会话；子智能体保持关闭。需逐项验证实际开放的能力，不能将文本描述图片当图像输入 |
| 工程改名/删除/覆盖/移动 | 未实测，影响持久对象 | 建议用户手工完成后重新读取树和 UUID；自动命令禁用。是否作为首期限制需父任务记录，不算原生 API 已验证 |
| 编辑器刷新 | 无独立刷新现场证据 | 每次操作前后重新读取活动 UUID、工程树和目标内容；不能假设主动重开/切换安全，也不把回读命名为原生刷新支持 |
| 活动 SDK 版本 | 无可靠版本回报 | 保留 unknown，固定类型包版本/哈希、EDA 版本及逐 API 宿主结果；不猜测活动 SDK。可用可重复证据表管理兼容性 |

**创建位置记录纠正**：类型查询确认 createProject 第三参数是 teamUuid，第四参数是 folderUuid，不是本地路径。先前脚本传仓库目录属于参数使用错误；不能据此声称“宿主忽略了目录参数”。脚本已改为使用当前工程 teamUuid，默认创建门禁仍关闭，没有再次创建工程。按宿主当前工作区创建后回读身份可作为建议路径；任意目录创建仍未验证。

父任务应按本次明确规则及后续选择评估阶段门禁。自动导入、原生检查点和四运行时功能完全对等不应混为同一项“全部实现”要求；实际未验证能力继续如实记录，未自行宣布 584 通过。

## 2026-10-30 评论 9124：专用现场重启、内容与未保存关闭实测

用户已准备专用现场，并指定 Claude CLI 使用 Z.ai，不要求 Anthropic 原生网络。测试工程 UUID 与既有 fixture 一致，没有重新创建工程。类型包仍为 0.4.27，声明 SHA-256 `3e91ca9432bc6a640d5a7983c044ec63630ca94e5398744e17e0ac162674d5aa`，TS 5.9.3；活动 SDK 版本未知。以下更新覆盖历史章节中的相应未验证项。

| 项目 | 新实测结果与边界 |
|---|---|
| 双图页内容 | Probe-A / Probe-B 分别含 CF584-CONTENT-A-20261030 / CF584-CONTENT-B-20261030；save=true，关闭重开回读正确，无跨页混淆 |
| 进程重启 | 原主进程 42548 正常退出，新主进程 9232 从启动页打开测试工程。只读脚本回读工程 UUID、两页 UUID、不同内容均一致；回读未保存或创建 |
| 未保存关闭 | Probe-A 添加额外标记，getAll 确认存在；未 save，closeDocument=true，无保存确认。重开后额外标记消失，原已保存标记仍在。因此 API 关闭可能丢弃编辑，保存/保留/取消流程未通过 |
| 原生归档 | 导出 CF584-Content-20261030.epro2（12914 字节），ZIP CRC 正确，epru 含两个区别标记 |
| 导入 API | 单次 importProjectByProjectFile 调用被宿主拒绝：未启用扩展和独立脚本的外部交互权限。未修改权限、未重试，隔离目标目录为空；导入和恢复仍未通过 |
| 窗口与实例 | 命令行传工程路径产生空白窗口，已正常关闭；成功回读来自启动页已有工程条目。曾见同一 PID 9232 下两个 HWND，不能用 PID 当实例身份。不把空白窗口算第二工程实例成功，多实例路由未验证 |
| Claude / Z.ai | 继承环境有 Z.ai 的 ANTHROPIC_BASE_URL 和 glm-5.3 模型映射，无认证变量。CLI 2.1.302 返回 apiKeySource=none / authentication_failed，未发模型请求。这是当前进程凭据注入缺口，不代表 Z.ai 不支持，也不要求原生 Anthropic 登录 |

原始证据在 `artifacts/collaboration-584-ready/`：native-readonly.json、content-write-result.json、content-after-restart.json、dirty-result.json、archive-result.json、原生归档、claude/result.json。`verify_content.py` 交叉校验三份宿主结果，拒绝错误页 UUID、跨页替换、缺少未保存标记三项负向控制。校验器不替代宿主执行。最后保留专用测试工程，未修改用户原工程。

返回父任务 583：持久化与重启补齐；导入权限、Claude 凭据注入、dirty 三态/显式保存保留取消、多实例路由仍有缺口。unknown dirty 时必须拒绝隐式 close/open/switch；显式丢弃需绑定工程/页并确认，closeDocument=true 仅表示关闭成功。归档不冒充原生 checkpoint。需在独立测试配置启用宿主外部交互后补导入，通过现有 Provider 正式注入路径补 Claude，不索取或复制保险库密码。统一四运行时 EDA 接入和多实例路由实现留给后继阶段；阶段门禁继续关闭，不自动推进。

## 2026-10-30 续跑 1552：现场前提与父任务处理方案

平台 wakeup #3 已实际触发一次并自动停用，不存在后台探测。原生只读脚本再次执行成功，`artifacts/collaboration-584-host-final/native-readonly.json` 的所有 calls 与此前连接后结果完全一致：EDA `4.1.78`，原工程 UUID `135d2e3c9573727175c3c7596ab2f66b90484cf174de38bfb60d0c03aa72d9ea`、当前 P1 UUID `5dfc602f34949c83`；当前仍为用户原工程，不是专用测试工程。`host-verification.json` 为交叉校验摘要，不把结果相等扩大为重启或未保存状态已验证。

实际文件菜单中当前文档“保存”呈灰色；这不是所有图页/窗口 dirty 状态的可靠 API。SDK 查询 modified / unsaved / checkpoint 均无成员匹配，sys_Window 没有已验证的独立工程实例创建接口。未切换、保存或关闭原工程；临时脚本面板已关闭，原助手保留。没有执行重启、重新导入、删除或双页内容写入，这些仍未验证。

剩余现场前提已请求：将已有 `CF584-Lifecycle-20261030` 保存并作为唯一测试窗口，用于后续关闭重开/重启、未保存分支和归档导入；Claude 原生 CLI 需可用登录，不要求发送凭据。此前“原工程已保存”授权继续测试，但不能替代重启测试所需的独立测试现场。当前暂停的是这部分现场测试，不是再次等待同步范围决定。

返回父任务 583 的具体方案：

1. 在后续统一会话协议中注册 `instanceId + connectionGeneration + projectUuid + documentUuid`，将当前单 executor 槽改为按实例路由；缺少绑定时拒绝工程写入，不用 editor-window-main 或标题猜测。
2. 将 dirty 状态设为 `known_clean / known_dirty / unknown`。未得到宿主证据时保持 unknown；打开工程不能默认重试，也不能隐式保存或丢弃。需保存/保留/取消的流程在专用测试现场逐项取证后再开放。
3. 原生 checkpoint 未验证，不能承诺跨页、磁盘文件或重启恢复。原生导出仅作为已保存工程的备份候选，完成宿主重新导入与内容校验前不冒充检查点。
4. 本地持久化已有通过证据，但其余关键项继续关闭门禁；不将“矩阵记录了未验证项”解释为验收通过。待测试现场和认证路径就绪再补验，不循环定时唤醒同一前提。

## 2026-10-30 当前结果（评论 9113，执行 1550 / 1551）

执行 1550 因平台模型容量错误中断；已完成的文件保留，1551 从原会话续测，没有重复创建 EDA 工程。本节是当前矩阵；下方带日期的旧矩阵保留为历史，不得单独作为当前支持表。

| 真实探测 | Codex 0.165.0 | Claude Code 2.1.302 | Kimi Code 2.4.0 | DSH 0.1.2-rc.1 |
|---|---|---|---|---|
| 模型调用 | 已验证 | 失败：原生 CLI 返回 authentication_failed / Not logged in，退出 1 | 已验证，随机标记正确 | 已验证，随机标记正确 |
| 持久会话恢复 | 原生 CLI 新进程 exec resume 已验证（评论 9107） | 未验证，受此次认证失败影响 | 新进程恢复同一 session 已验证，标记正确；必须使用原 cwd | 未验证；此次 headless 只执行单任务 |
| 图像 | 原生 CLI 三色图识别已验证（评论 9107） | 未验证 | 未验证 | 未验证 |
| 原生提问回答 | App Server 已验证：收到 item/tool/requestUserInput 后生成新答案，用原 request ID 回传，模型精确复述 | 未验证 | 未验证 | 未验证 |
| 真实生成取消 | App Server 已验证：收到非空文本 delta 后 turn/interrupt 成功，同一 turn 最终 interrupted | 未验证 | 未验证 | 未验证 |
| EDA 统一接入 | 当前聊天固定 Codex，端到端工具任务仍未验收 | 阶段 3 缺口 | 阶段 3 缺口 | 阶段 3 缺口 |

Kimi 此次实际版本命令与流事件均报告 `2.4.0`，取代早前 `2.3.1` 的版本基线；不把旧版本帮助或成功结果套到新版本。首次恢复从不同输出目录执行，原生 CLI 明确拒绝“created under a different directory”；改用原目录后退出 0、标记匹配。失败与成功结果分别保留，不能解释成任意目录恢复通过。

Codex App Server 使用现有登录、临时测试线程、只读沙箱，关闭 shell 与 multi_agent，未调用工程工具。提问的回答由探测客户端自动生成，不是 EDA UI 或人机交互验收。取消只验证原生生成中止，不代表 EDA 写入撤销。实际模型为 `gpt-6.1-sol`；现有配置的 Codegraph.type 警告和模型列表刷新超时保存在 stderr，最终测试通过不豁免这些配置问题。原始双向记录通过 `verify_interaction.py` 校验，四项负向控制（错误 turn、缺少真实 delta、错误 request ID、提前泄露答案）均被拒绝；校验器结果只是原始证据的交叉检查。

原始文件在 `artifacts/collaboration-584-followup/`：`codex-interaction/result.json`、`rpc.jsonl`、`stderr.txt`；`kimi/result.json`（首轮）、`kimi-resume/result.json`（不同 cwd 拒绝）、`kimi-resume-cwd/result.json`（恢复成功）；`dsh/result.json`；`claude/result.json`。旧采集器在 Claude 失败、Kimi 尚未完成解析时写过 unverified，当前矩阵按实际退出码与原始消息判定；不修改历史原始输出。DSH 输出含其原生 reasoning，不能据此推断 token 用量或工具闭环。

**尚未通过的条件与处理**：Claude 原生认证需要可用的现有账号或 Provider 注入路径；不能因 CLI 未登录而宣布 Claude 本身不支持。继续补 EDA 重启、多窗口/实例路由、未保存编辑的保存/保留/取消、原生归档重新导入和双页可区分内容持久化。SDK `0.4.27` 查询未找到 instance 成员，sys_Environment 不提供实例 ID；已有 editorTree 的 `editor-window-main` 是容器 ID，不能当全局实例身份。Bridge 当前 registry 只有一个 executor stream（代码检查，不是双实例现场验证），后继必须增加实例 ID、连接代次及原生 UUID 的路由绑定，不能按进程、窗口标题或图页名称路由。dirty/原生检查点缺口仍按未知处理；云端同步已排除，不重新作为阻塞。关键项未通过时，584 保持未验收，后继阶段不自动推进。

复现及边界见 `scripts/collaboration-probe/README.zh-CN.md`。本节补齐原生协议证据，不修改生产适配器，不提前实现阶段 3。

## 2026-10-30 验收范围决定（评论 9105）

用户已选择评论 9103 的 A 方案，并明确“不需要云端同步”。父任务 583 与本任务描述已记录此决定：本期工程持久化按本地保存、图页关闭重开及工程切走切回后的身份/内容回读验收；云端同步排除本期范围，不再作为阻塞。编辑器刷新是独立能力，仍需自己的证据，不因本地保存通过而标支持。此决定取代下方历史章节中“同步范围待确认”的表述。

已有双页证据覆盖保存、页名与 UUID 回读；不扩大为重启、重新导入或任意设计内容持久化均已通过。其余原生运行时问答、图像、线程恢复、取消和现场重启/多实例等关键验证仍须继续；四运行时可用范围不缩减，阶段门禁不自动通过。

本轮新增真实推理证据：`model_probe.py` 使用现有运行时配置，发送确定性三色 PNG 和随机标记，不复制认证信息。Codex 原生 CLI `0.165.0` 正确返回 red/green/blue 顺序，并由另一个 CLI 进程 `exec resume <thread-id>` 恢复已持久化会话、返回上一轮标记；两次退出码均为 0。原始 JSON 见 `artifacts/collaboration-584-option-a/runtime/model-result.json`。这覆盖 CLI 原生图像输入与持久会话恢复，不覆盖 App Server 恢复、交互式 request/answer、EDA 工具任务或其他三种运行时；下方历史矩阵中 Codex 对应“未验证”由本段限定范围的结果补充。

该次运行还记录了模型列表刷新超时，以及现有配置 `mcp_servers.Codegraph.type` 被忽略的警告；最终推理与恢复成功，不将其解释成工具配置正常或所有网络请求通过。

## 2026-10-30 生命周期实测增补（评论 9090）

用户明确确认工程已保存并允许继续。以此授权运行 `lifecycle.ts` 的一次性副本，仓库默认 `ALLOW_SWITCH_AFTER_SAVE=false` 保留。真实宿主仍为 `4.1.78`；类型包 `0.4.27`、声明 SHA-256 与下节一致。下列结果取代历史章节中的“生命周期未执行”。原始结果为 `lifecycle-result.json`、`reopen-result.json`，附原生 `CF584-Lifecycle-20261030-sdk.epro2`；不是模拟工程或构建产物。

| 能力 | 本次真实结果及范围 |
|---|---|
| 创建、打开工程 | `createProject` 返回 UUID，`openProject=true`，当前工程 UUID 回读匹配 |
| 创建原理图、图页 | 新建原理图自带一页，再显式创建第二页，两页 UUID 不同 |
| 改名、打开、保存 | Probe-A / Probe-B 改名成功，各自打开、校验活动 UUID 后 `sch_Document.save=true` |
| 关闭、重开图页 | 两页分别 `closeDocument=true` 后重开，页名及 UUID 不变 |
| 切换工程后的身份 | 切回原工程、再打开测试工程，工程 UUID 与两页 UUID/页名保持一致；只覆盖同一宿主进程，不覆盖重启 |
| 恢复现场 | 结束前恢复原工程及原 P1 页；未修改或替用户保存原工程 |
| 原生导出 | ZIP CRC 通过，内部 SCH_PAGE UUID、页名及父原理图与 SDK 结果一致；未进行重新导入测试 |
| 同步、dirty、检查点、多实例 | 未验证；SDK 查询未找到独立文档 sync、dirty 或 checkpoint 接口，不能将本地 save/回读等同于云同步 |
| 删除、工程改名 | 未执行；不因图页改名通过而标记工程改名通过，删除继续禁用 |

测试工程 UUID：`acf0e4b3437356162b1742564a56cd8942fdca179333f9958e04a76cf63d83c0`；原理图 UUID：`fcd4be00325b112b`；Probe-A：`0b6e010013c4d426`；Probe-B：`112ce6c2685aa137`。宿主还自动创建了 Board1/P1/PCB/Panel，双测试页属于另一个独立原理图，不把默认 P1 算作测试页。

**位置约束发现**：传给 `createProject` 的第三参数是仓库目标目录，但宿主实际 `teamUuid` 回读仍为原 EDA 工作区 `C:\Users\Ferren\Projects\EDA`，生成 `CF584-Lifecycle-20261030.eprj3`。因此不宣称已验证创建位置控制；必须以实际工程回读为准，生产命令不能依据传参推断目标位置。导出的 `.epro2` 在工作区内；不自动删除原生测试工程。窗口标题在切换中仍显示旧工程，不能据标题或 PID 判定当前 UUID。

复现：构建 `eda-host` 后，先取得保存确认、审查初始 UUID 和同名工程，再运行授权副本；不要重复运行创建。`reopen.ts` 固定绑定上述已创建工程及两页，执行关闭重开、切走切回和原现场恢复。`verify_lifecycle.py <证据目录>` 对两个 JSON 和 `.epro2` 进行交叉检查；检查通过仅表示证据内部与原生归档一致，不扩大现场测试范围。

**返回父任务的待决定方案**：将“本地保存并重新打开”“编辑器观测刷新”“云端同步”分为独立能力，后两者在取得明确接口及现场证据前禁用；当前宿主先支持已验证的本地保存路径，需父任务明确调整验收后才能推进。此建议尚未获接受。四运行时原生问答/图像/线程恢复及 EDA 多运行时接入缺项仍在，阶段 1 门禁未通过，不推进后继。

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
