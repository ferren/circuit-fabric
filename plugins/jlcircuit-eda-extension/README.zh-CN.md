# CircuitFabric 嘉立创 EDA 扩展

v0.2.23 重做器件重建（删除回滚与放置）。实测图元 `getState_Component()` 返回的 16 位 `uuid` 是工程内引用，`lib_Device.get` 对图上全部元件都返回 `undefined`，旧的"先核验库身份"永远不通过。现在依次尝试：精确库查找 → 按观测记录的立创编号（`getState_SupplierId()`）调用 `lib_Device.getByLcscIds` 取回 EDA 核验过的器件（离线客户端改传系统库 UUID 重试）→ 仍失败时直接用原库身份调用 `create` 兜底（未经核验，可能失败或结果未知）。结果回报 `restoreMethod`（`exact`/`lcsc`/`original_unverified`）和逐步诊断。配套 Bridge 不再因删除不可回滚而阻止批准，改在计划中给出 `rollbackWarnings`，扩展审批面板醒目显示，批准按钮改为"已知晓回滚风险"。观测附带只读 `libraryProbe` 诊断，不进入快照哈希。需同时更新 Bridge。

v0.2.21 修正重建器件时把工程 UUID 当作工程库 UUID 的问题：工程库改由 `lib_LibrariesList.getProjectLibraryUuid()` 取得；原库与工程库都查不到时，再以不指定库的 `lib_Device.get(uuid)` 让 EDA 定位，只接受返回的同一器件及其自报所属库。EDA 以普通对象拒绝时，诊断输出其 JSON 而非 `[object Object]`。45 项扩展测试通过；真实 U1 恢复及回读验收尚未完成。

v0.2.20 修复器件重建时混用工程内器件 UUID 与来源库 UUID 的路径。创建前使用 `lib_Device.get` 精确核验保存的身份；原库查不到时，仅到当前已绑定工程的库中核验同一个器件 UUID。查不到、读取超时或子部件不明确时返回具体错误，不调用 `create`；查询后再次核对活动文档。每次只读查询最多 5 秒，写入已发出时仍保留在途保护。使用现有 v0.2.19 配套 Bridge，只需更新扩展并重启 EDA 结束旧调用，在原项目、原历史会话中对原计划重试回滚。40 项扩展测试与本机 SDK 离线复现通过；真实 U1 恢复及回读验收尚未完成。

v0.2.19 为未知回滚重试增加不调用 EDA API 的空闲探针，报告实际加载的执行器版本、在途请求及阶段。Bridge 同步更新操作所属会话的运行、完成或失败状态，保留原有用量及引用。探针缺少空闲标记、忙碌、5 秒无响应分别报告；检查失败不再追加整页回读，也不会释放未知写入锁。更新 Bridge 和扩展后，选择原历史会话，对原计划重试回滚；开始时会显示当前执行器版本。

v0.2.18 修复回滚超时后永久卡在未知状态的恢复入口。库件创建等待上限为 120 秒并显示创建/属性恢复阶段；未知补偿只能在执行器确认空闲、完整新鲜观测与原删除后现场完全相同时重试（当前限单元件删除）。在途写入、现场漂移、已有同位号元件仍会阻止重复创建。重建恢复已采集的 uniqueId 与自定义属性，但实际库件、引脚连接和属性仍需回读/人工核对，不能把 `rollbackable` 理解为完整恢复保证。

本次需同时更新扩展与 Bridge，并重启 EDA 客户端结束旧在途调用。重连后选择原项目、原历史会话，面板恢复最近的已执行计划及回滚入口，不恢复批准或重复 apply 权限。点击原计划「回滚」，不要重新生成删除计划。界面显示实际失败原因；`eda_inspect` 返回最近 ChangeSet 执行审计摘要，避免将 parent_snapshot_hash 为空误判为补偿失败原因。

v0.2.17 修复普通元件与辅助符号混在同一集合导致 `components inconclusive` 的问题。通过官方 `getState_ComponentType()` 区分 `part` 与图框、网络标识、端口等辅助对象；辅助对象保存在观测的 `symbols` 中，其名称/位置/引脚变化纳入物理基线。普通元件缺位号、未知类型、身份或必要字段缺失仍报告不完整。

本次需要同时更新扩展和 Bridge。新 Bridge 会在界面和 `eda_inspect` 返回 `diagnostics`、原始校验结果和采集时间，明确该工具读取最近已保存的观测。旧会话可以继续使用；更新后必须在扩展里点击「观测当前设计」进行新采集，并重新请求预览。辅助符号现在参与物理哈希，旧基线/旧计划不可复用；旧预览不会自动变成可批准状态。DRC 为 inconclusive 且问题数为 0 时，界面会明确提示报告不可完整解析，不能视为通过。

2026-10-25：修复 Bridge 对内置 EDA 工具的项目授权筛选。`eda_inspect`、`eda_propose_patch`、`eda_request_preview` 随 EDA 会话自动绑定，不需要在通用 MCP 设置中另行勾选；其他 MCP 仍受原有全局与项目授权约束。此修复只需更新并重启 Bridge，无需重装扩展。修复前创建的会话保存了空工具范围，需在扩展会话下拉框选择「新会话」重新发起；旧历史保留，不能直接复用旧工具授权绑定。

例如请求「移除 U1」应先产生候选补丁与可审阅的预览卡片。观测不完整或缺少删除补偿数据时，预览仍会生成，显示未满足的条件并禁用批准；条件满足后由操作员分别点击批准、执行。聊天文本中的 JSON 本身不会写入 EDA。

v0.2.16 接通 Bridge 本地 `preview` 和显式审批。会话携带已观测的语义快照，模型只能提交候选 IRPatch；助手展示 diff、风险、前置条件和回滚能力，操作员点击后才持久化审批。桌面端读取同一份 ChangeSet；预览校验与写入/回读/验证状态分开。修复观测缺失标志丢失、getter 接收者丢失、空/异常 DRC 被误判通过、同内容快照覆盖身份漂移历史等问题。

已接通 `apply/readback/rollback`：当前支持 `set_component_value` 和 `rename_net`（更新网络的全部导线）。批准后需单独点击执行；每一步核对目标文档、外部图元与原值，持久化补偿记录。写入确认与回读、验证分别显示；缺失或延迟回读保持 inconclusive，不自动撤销已确认写入。取消先收集在途结果再停止后续步骤；回滚按逆序核对当前值并补偿。

**实现边界：** Bridge WebSocket、扩展模拟宿主和桌面页面有自动化测试；结构变更的真实 EDA 手工验收与 PCB 观测尚未完成。代码已支持删除元件、克隆已有库件放置元件及简单引脚连线：删除需要稳定身份和可补偿的库件/位置数据，新增元件需要可用的 `clone_from`，网络成员必须引用目标快照中存在的引脚；任一步前置条件不满足均不能批准。任意库件选择、导线整理与命名悬空端口尚无完整操作支持。审批按钮只记录决定，单独点击执行后才发生 EDA 写入。协议与兼容边界见 `docs/bridge-operations-protocol.zh-CN.md`。

此扩展独立实现，不依赖或包含 `JLCircuit-Agent` 源码。安装后从 EDA 顶部菜单打开 **CircuitFabric**，自动连接本机 `ws://127.0.0.1:49630/bridge`。窗口右上角是项目、会话两个下拉框：左边选择 CircuitFabric 项目（显示名称和 ID），右边选择该项目的 EDA 会话，无需手输 ID；Bridge 地址输入框缩短后放在两个下拉框下方。

连接不依赖项目注册：列表为空也保持连接。在桌面端创建或打开项目后，点开项目下拉框会自动刷新列表（不会清空当前已选项目），选中项目并收到确认后即可发送请求。切换项目会创建新会话；失效项目会显示具体原因，重新打开下拉刷新后重选即可，无需断开连接。

协议顺序：`hello`（不带项目 ID）→ `hello_ack` → `list_projects` → `projects`（`id` / `name`）→ `select_project` → `project_selected` → `chat`。请同时更新 bridge 二进制和扩展包；旧版 bridge 不支持此流程。

先在桌面端保存 App Server 设置，再运行：

```powershell
cargo run -p jlcircuit-eda-bridge --bin circuitfabric-jlc-bridge
```

扩展包不保存 API Key；bridge 只接受 `127.x.x.x` 回环连接，Codex 认证仍由本机 `codex app-server` 处理。

v0.2.15 在扩展入口新增 EDA 适配层（只读观测）：`inspectSchematic`/`readContext`/`runDrc`/`captureCanvas` 逐个命名空间守卫，缺失或失败的观测面一律记入 `inconclusive`，绝不把「API 调用成功」冒充成「验证成功」。入口经 `eda.sys_WebSocket` 连接 `ws://127.0.0.1:49630/eda-operations` 作为执行器，注册 `executor_hello`、按 `eda_operation` 分派并回 `operation_result`/`operation_error`（协议见 `docs/bridge-operations-protocol.zh-CN.md`）；iframe 只通过 `sys_MessageBus` 把 Bridge 地址推给入口，不接触 `eda` 业务 API。助手底部新增「观测当前设计 / DRC 检查 / 画布截图」三个按钮，结果气泡显示哈希与不确定面。

v0.2.14 把 Bridge 地址输入框从底部输入区移到右上角项目、会话下拉框下方，并缩短为固定宽度，底部只保留消息输入与发送操作。

v0.2.13 把项目选择和会话选择移入右上角两个原生下拉框，底部不再保留项目只读输入和动态按钮列表。点开项目下拉框自动发送一次 `list_projects` 刷新；选中项目后自动创建本地新会话并列出该项目（且仅该项目）的 EDA 会话。会话下拉首项始终是「新会话」；已有会话优先显示标题——标题由 bridge 从会话记录里第一条用户消息推导（折叠空白、按 30 字符截断），没有标题时回退为「开始时间 · 状态」。选择已有会话通过同一条 WebSocket 发送 `load_session` 续聊；选择「新会话」只在本地新建，不请求 bridge。会话选项的 `title` 字段需要配套 0.2.13 起的新 bridge 二进制：旧 bridge 不下发 `title` 时选项仍可用，只是回退到时间加状态的文案。

v0.2.12 统一使用 ES5 `compat.js` 管理连接、项目选择和发送事件，移除旧内联脚本，修复按 Enter 时重复创建 WebSocket、停留在「正在连接 Agent」的问题。点击发送与 Enter 均复用已选择项目的连接；Shift + Enter 换行，中文输入法确认候选词时不会发送。重连后需重新选择项目。

选择项目后只列出该项目的 EDA 会话，选中历史后继续发送；「清空」新建会话并保留旧历史。需要配套支持 `list_sessions` / `load_session` 和持久 thread 的 bridge。打包器显式用 ZIP 规范的正斜杠路径（如 `iframe/index.html`），不依赖 Windows 的反斜杠路径。恢复设计见 `docs/project-session-recovery.zh-CN.md`。

入口源码位于 `src/index.js`；运行 `powershell -NoProfile -File scripts/package-jlc-extension.ps1` 会先生成 `dist/index.js` 再打包，避免依赖本机残留的构建文件。入口、兼容语法和连接逻辑可用 `node --test plugins/jlcircuit-eda-extension/tests/*.test.cjs` 验证；这些测试使用模拟宿主，不能替代 EDA 内的安装验收。
