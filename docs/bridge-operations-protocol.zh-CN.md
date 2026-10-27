# Bridge 操作协议 v1

Bridge（`circuitfabric-jlc-bridge`，Rust 进程）不在 EDA 前端上下文内；真实 EDA API 调用由扩展入口（`plugins/jlcircuit-eda-extension/src/index.js`）作为「EDA 执行器」代理执行。本文档是两侧共享的版本化契约。协议版本 `operationsProtocolVersion = 1`（消息字段名 `protocolVersion`）。

## 1. 通道

同一 TCP 监听（默认 `127.0.0.1:49630`），按 WebSocket 握手路径分流：

- `/bridge`：会话通道（iframe ↔ Bridge）。保留既有消息（`hello/list_projects/select_project/chat/list_sessions/load_session/ping/status`），新增第 3 节的操作消息。
- `/eda-operations`：执行器通道（扩展入口 ↔ Bridge，经 `eda.sys_WebSocket`）。同时最多一个活跃执行器；新执行器接入即替换旧的。

Bridge 的 accept 循环按连接派发线程，两通道并存；会话通道之间互不影响。

## 2. 执行器通道消息

### 2.1 执行器注册

入口 → Bridge（连接后首条消息，5 秒超时，否则断开）：

```json
{
  "type": "executor_hello",
  "protocolVersion": 1,
  "extensionId": "circuitfabric-jlc-eda",
  "extensionVersion": "0.2.15",
  "eda": {
    "edition": "jlceda-pro | easyeda-pro | unknown",
    "client": true,
    "web": false,
    "apiTypesVersion": "0.4.26"
  }
}
```

Bridge → 入口：

```json
{
  "type": "executor_hello_ack",
  "protocolVersion": 1,
  "bridge": "CircuitFabric",
  "operations": ["inspect", "read_context", "drc", "visual_capture", "apply", "readback", "rollback"],
  "unsupportedOperations": []
}
```

### 2.2 操作请求（Bridge → 入口）

```json
{
  "type": "eda_operation",
  "requestId": "op-<单调递增整数>",
  "operation": "inspect | read_context | drc | visual_capture | apply | readback | rollback",
  "protocolVersion": 1,
  "projectId": "<CircuitFabric 项目 ID>",
  "sessionId": "<会话 ID>",
  "document": { "kind": "schematic" },
  "baselineSnapshotHash": null,
  "planHash": null,
  "scope": ["project.read"],
  "params": {}
}
```

绑定规则：`projectId`/`sessionId` 来自会话通道上发起操作的已选项目与当前会话；只读操作的 `baselineSnapshotHash/planHash` 为 `null`，物化操作绑定磁盘计划；只读 scope 为 `project.read`，写入额外要求 `project.write`。

### 2.3 操作事件（入口 → Bridge）

```json
{ "type": "operation_progress", "requestId": "op-3", "operation": "inspect", "phase": "枚举元件", "completed": 1, "total": 4 }
```

```json
{
  "type": "operation_result",
  "requestId": "op-3",
  "operation": "inspect",
  "ok": true,
  "capturedAt": "2026-10-22T08:00:00.000Z",
  "payload": { },
  "inconclusive": []
}
```

```json
{
  "type": "operation_error",
  "requestId": "op-3",
  "operation": "inspect",
  "code": "eda_api_unavailable | eda_error | unsupported_operation | cancelled",
  "message": "人类可读原因",
  "inconclusive": ["nets"]
}
```

`inconclusive` 是字符串数组，列出缺失/不可用的观测面（见第 5 节的取值表）。任何缺失字段都必须出现在 `inconclusive` 中，不得静默省略。

## 3. 会话通道新增消息（iframe ↔ Bridge）

### 3.1 iframe → Bridge

```json
{ "type": "inspect", "sessionId": "…", "document": { "kind": "schematic" } }
{ "type": "read_context", "sessionId": "…" }
{ "type": "drc", "sessionId": "…", "document": { "kind": "schematic" } }
{ "type": "visual_capture", "sessionId": "…", "document": { "kind": "schematic" } }
{ "type": "cancel_operation", "requestId": "op-3" }
```

前提：已 `select_project`；否则回 `error`（`message: 请先选择项目` 风格，与既有错误一致）。`document.kind` v1 仅支持 `"schematic"`。

`preview` 在 Bridge 本地实现，不调用执行器；协议见第 8 节。写入、回读和回滚均绑定持久化计划：

```json
{ "type": "apply", "sessionId": "s1", "planHash": "sha256:<计划摘要>" }
```

### 3.2 Bridge → iframe

```json
{ "type": "operation_started", "requestId": "op-3", "operation": "inspect" }
{ "type": "operation_progress", "requestId": "op-3", "operation": "inspect", "phase": "枚举元件", "completed": 1, "total": 4 }
```

inspect 结果（Bridge 已构建并持久化 observed snapshot）：

```json
{
  "type": "inspect_result",
  "requestId": "op-3",
  "ok": true,
  "snapshotHash": "sha256:…",
  "logicalHash": "sha256:…",
  "physicalHash": "sha256:…",
  "components": 12,
  "nets": 9,
  "inconclusive": [],
  "validation": [ { "constraintId": "schema.identity.topology", "status": "passed" } ],
  "capturedAt": "2026-10-22T08:00:00.000Z"
}
```

```json
{
  "type": "drc_result",
  "requestId": "op-4",
  "ok": true,
  "status": "passed | failed | inconclusive",
  "errors": [ { "type": "error | warn | fatalError", "rule": "IN-IN", "net": null, "primitives": [ { "designator": "U1", "primitiveId": "…" } ] } ],
  "inconclusive": []
}
```

```json
{
  "type": "visual_capture_result",
  "requestId": "op-5",
  "ok": true,
  "evidence": { "contentHash": "sha256:…", "mime": "image/png", "bytes": 12345 },
  "inconclusive": []
}
```

```json
{
  "type": "read_context_result",
  "requestId": "op-6",
  "ok": true,
  "context": { "projectName": "…", "pageName": "…", "documentKind": "schematic", "environment": { "client": true, "web": false } },
  "inconclusive": []
}
```

```json
{ "type": "operation_error", "requestId": "op-3", "operation": "inspect", "code": "eda_executor_unavailable | eda_api_unavailable | eda_error | operation_timeout | cancelled | bad_request", "message": "…", "inconclusive": [] }
```

`eda_executor_unavailable`：执行器未连接。`operation_timeout`：执行器 30 秒未回结果。

`status` / `hello_ack` / `project_selected` 的应答在既有字段上增加：

```json
{
  "capabilities": ["inspect", "preview", "drc", "visual-capture", "bridge-chat", "bridge-context"],
  "operations": ["inspect", "read_context", "drc", "visual_capture", "preview"],
  "executorConnected": true
}
```

能力声明包含 `apply/readback/rollback`。执行器握手列出四个只读操作及三个物化操作，Bridge 会话握手额外列出本地 `preview`。操作串行执行；物化期间操作锁覆盖新鲜观测、准入、逐步写入及回读。

## 4. inspect 观测载荷（入口 → Bridge 的 `payload`）

```json
{
  "document": { "kind": "schematic", "projectUuid": "…", "projectName": "…", "pageUuid": "…", "pageName": "…" },
  "components": [
    {
      "primitiveId": "…", "designator": "R1", "name": "10k", "footprint": "0603",
      "uniqueId": "…", "libraryUuid": "…", "componentUuid": "…",
      "x": 0, "y": 0, "rotation": 0, "mirror": false,
      "properties": { "Manufacturer": "…" },
      "pins": [ { "number": "1", "name": "A", "x": 0, "y": 0, "pinType": "…" } ]
    }
  ],
  "wires": [ { "primitiveId": "…", "net": "VCC", "points": [[0, 0], [10, 0]] } ],
  "buses": [ { "primitiveId": "…", "name": "DBUS" } ],
  "nets": [ { "name": "VCC", "wires": [ { "name": "…", "id": "…", "pageName": "…", "pageUuid": "…" } ] } ]
}
```

`inconclusive` 取值（观测面标识）：`document`、`components`、`pins`、`physicalPlacement`、`wires`、`nets`、`buses`、`netMembership`、`drc`、`canvasImage`、`environment`。命名空间缺失或调用失败时把对应观测面加入 `inconclusive`，其余照常返回。

## 5. observed snapshot 构建规则（Bridge 侧）

- 逻辑身份：元件逻辑 ID 为 `component:<designator>`；designator 缺失的元件不入快照，并把 `components` 记入 `inconclusive`。引脚 ID 用引脚号。网络逻辑 ID 为 `net:<name>`；`nets[]` 缺失时从 `wires[].net` 归集网络名，并把 `nets` 记入 `inconclusive`。
- 网络成员（pin ↔ net）：用坐标启发式——引脚坐标与某导线折线**端点**精确相等则该引脚属于该导线网络；一对多或无匹配均不列入成员，有歧义时把 `netMembership` 记入 `inconclusive`。
- 排序：元件按 designator、引脚按引脚号、网络按名称、成员按 `元件:引脚` 字典序排序后再构建快照——同一原理图两次 inspect 必须得到相同 `snapshotHash`/`logicalHash`/`physicalHash`。
- `logicalHash`/`snapshotHash` 由 `circuitfabric-core` 的 `stamp_snapshot` 计算；`physicalHash` 对位号、放置、封装、工程/图页 UUID、导线几何做摘要。导线只归一正反方向，保留折线顺序，不能把不同路径按同一点集归并。仅位置/旋转变化时 `logicalHash` 不变。
- 外部引用映射（逻辑 ID → EDA 图元 ID + 页 UUID）不进快照，随观测记录持久化（第 6 节）。
- 校验：`circuitfabric_core::validate_snapshot` 的结果写入观测记录；`inconclusive` 非空时追加一条 `layer: "observation"`、`status: "inconclusive"` 的 ConstraintResult 到记录（不进快照本体）。
- 快照 `authority = observed`，`parent_snapshot_hash` = 同项目上一次 inspect 的 snapshotHash（无则 null）。

## 6. 持久化与审计

- 观测记录：`observations/<snapshotHash 去前缀>.json` 为该内容最近一次记录，`observations/inspect-history/` 保留逐次历史；同内容下图元 ID 改变仍能检测身份漂移。记录同时保存 `observedPayload` 与扩展返回的缺失面。缺失面与 Bridge 解析缺失面取并集。`logic/snapshots/observed-<时间>-<哈希>.json` 供桌面端读取。所有路径均相对于项目根。
- 最新基线与融合服务都按 `receivedAt`、历史文件名排序；同毫秒按持久化次序决胜。无历史目录的旧项目回退到内容记录目录。融合结果保存到 `observations/fusion.json`：同次观测的全部重复逻辑 ID、同页图元双归属为 `conflicted`；图元集合或页变化、同页图元跨历史重新归属为 `drifted`。历史映射不表示当前存在；图元 ID 的作用域含页 UUID。
- DRC 报告：`<项目根>/observations/drc-<unix秒>.json`（`schemaVersion: 1`、`kind: "drc"`、请求绑定、`status`、`errors`、`inconclusive`）。视觉证据：`<项目根>/observations/capture-<unix秒>.png` 及同名 `.json` 元数据（contentHash 为 PNG 字节的 `sha256:` 摘要）。
- 会话审计：只读请求、完成、执行器错误、取消、超时和预览/审批摘要进入 EDA 类别会话。`read_context` 记录审计但不另存观测文件。取消停止 Bridge 等待并丢弃迟到结果，当前不保证中止正在执行的官方 API；仅允许只读操作使用此行为。

## 7. 安全不变量

- 任何消息都不携带 API Key 或凭据；执行器注册只声明扩展与 EDA 版本信息。
- iframe（会话通道）永远不接触 `eda` 原始 API；模型/会话侧只能发起操作请求，由执行器在 EDA 侧代理。
- 模型侧只能经 Bridge 注入的 MCP 工具服务器（`jlc-eda-tools`）接触设计：恰含 `eda_inspect`/`eda_propose_patch`/`eda_request_preview` 三个工具，没有 apply、审批、回滚或原始 EDA API 入口；工具进程不声明任何环境变量，拿不到 Provider 密钥与保险库内容。
- EDA 图元 ID 只作为外部引用出现在观测记录中，绝不成为 CircuitFabric 逻辑身份。

## 8. 本地预览与审批（0.2.16）

```json
{
  "type": "preview",
  "sessionId": "s1",
  "baselineSnapshotHash": "sha256:<64 位摘要>",
  "document": { "kind": "schematic", "projectUuid": "p1", "pageUuid": "page-1" },
  "patch": {
    "base_snapshot_hash": "sha256:<同一摘要>",
    "operations": [{ "annotate_semantic": { "key": "domain", "value": "power" } }],
    "rationale": "标注用途"
  }
}
```

Bridge 校验当前项目、最新 observed baseline、补丁与请求的基线一致、文档一致、候选 IR 身份/拓扑，构建融合映射与 MaterializationPlan。计划哈希覆盖完整补丁、项目、会话、文档、基线、步骤、外部引用、风险和补偿声明。未知操作或无效哈希拒绝；不把哈希解释为任意文件路径。

返回 `preview_result` 包含 `planHash/baselineSnapshotHash/changeSetId/plan/approvalAllowed`，以及独立的 `write: not_run`、`readback: not_run`、`verification: inconclusive`。计划保存到 `observations/plan-<摘要>.json`；同一份桌面 ChangeSet 保存到 `logic/changesets/jlc-<摘要>.json`。重复预览不清空已有审批或审计。

```json
{ "type": "approve_plan", "sessionId": "s1", "planHash": "sha256:<计划摘要>", "baselineSnapshotHash": "sha256:<基线摘要>", "decision": "approved", "reason": "确认计划" }
```

操作员可选 `approved/rejected`；批准要求当前完整观测仍匹配基线、预览校验通过、未开始写入、磁盘计划哈希验证通过、项目/会话绑定一致。审批保存到 ChangeSet 审计并另存 approval 记录。桌面端也能审批同一 ChangeSet。`previewValidation` 独立于 `execution.verification`，不得借批准修改 write/readback/verification。旧记录缺少预览校验时禁用批准，需重新预览。

### 8.1 子任务 D 的执行前准入契约

`review::authorize_apply(root, project_id, session_id, plan_hash, current_snapshot_hash)` 是只读校验入口，返回已校验计划或拒绝原因。D 必须持有操作锁，先由执行器完成一次新 inspect 并持久化，再把这次实际观测的哈希传入，随后才能进入写入；不能把用户请求中的基线哈希冒充当前观测。校验与写入之间的并发控制由 D 负责。

准入要求磁盘计划的内容摘要及自身 `planHash` 一致，版本/后端/项目/会话一致；实际当前观测与持久化最新观测均匹配计划基线，观测完整且文档一致；ChangeSet 绑定的计划、基线、目标快照一致，预览校验通过、写入未开始，最新显式决定为 approved 且审计绑定同一基线。对引用已有图元的步骤还独立复核融合映射，防止图元 ID 变化但语义快照哈希不变时误写。

ChangeSet 审计是桌面端和 Bridge 共用的审批事实源；`observations/approval-<摘要>.json` 只是 Bridge 决定的查询副本，旧的 approved 文件不得覆盖后续桌面拒绝。准入成功不修改执行状态，也不代表 EDA 写入或回读成功。

模型上下文只携带语义快照及不确定面。模型在会话中只能通过受控工具接触设计（子任务 E）：Bridge 把 MCP 工具服务器 `jlc-eda-tools`（Bridge 二进制的 `--agent-tools` 模式）注入 Codex 运行时，按 `ToolAuthorizationSettings` 授权登记，恰含 `eda_inspect`（读取最新观测快照引用与语义内容）、`eda_propose_patch`（校验候选 IRPatch 并持久化到 `observations/candidate-<摘要>.json`）、`eda_request_preview`（请求受控后端生成 MaterializationPlan 与 ChangeSet，可传 `candidateHash` 引用或内联补丁）。三个工具都不连接执行器、不写入 EDA、不产生审批；审批与执行仍只能由操作员经 `approve_plan`/`apply` 通道完成。模型纯文本（含 `circuitfabric-patch` 代码块）不再有任何副作用，Bridge 检测到误用时只回以纠正提示。工具请求与结果进入 EDA 会话审计；候选与计划按内容寻址持久化，运行时重启后凭 `circuitfabric-thread.json` 与这些已存储引用恢复任务。回合结束后 Bridge 把本回合为该会话新建的预览以 `preview_result` 转发给会话通道，扩展展示 diff/风险/前置条件后由人点击批准。

候选补丁操作为外标签单键对象，载荷为面向作者的扁平语义（不含系统派生字段）：`set_component_value {component_id,value}`、`rename_net {net_id,name}`、`add_component {reference,value,pins:[{id,name}],clone_from?}`、`remove_component {component_id}`、`add_net {name,pins:[{component_id,pin_id}]}`、`annotate_semantic {key,value}`。新增元件/网络不携带 `id`/`evidence`：逻辑 ID 由系统按观测约定派生（`component:<reference>`、`net:<name>`）；`add_net` 目标名称已存在时视为向该网络显式挂接成员（去重），不是隐式合并网络。解析失败逐操作报错（操作序号、类型、缺失字段），不透出行列号形式的原始报错。

计划按整体批准：任一步前置条件不满足则 `approvalAllowed=false`。`add_component` 缺可用 `clone_from`、`remove_component` 基线观测缺库身份/放置、`add_net` 成员引脚不存在等均展示为不满足的前置条件。纯 IR 标注可记录审批，仍不执行 EDA 写入。

### 8.2 apply、readback、rollback 与取消

可执行语义补丁为 `set_component_value`、`rename_net`、`remove_component`、`add_component`（需可用 `clone_from`）与 `add_net`；`annotate_semantic` 仅记录 IR。网络改名更新已审批映射中的全部导线，不允许隐式合并已有网络。原值及外部图元来自观测，模型不能指定原始 API 或图元 ID。

结构类物化契约：`remove_component` 以基线观测的库身份（libraryUuid/componentUuid）与放置作补偿，回滚按补偿数据经官方 BETA `create`+`modify` 重建（图元 ID 由 EDA 重新分配，语义内容不变）；基线观测缺库身份或放置时该步骤前置条件不满足。`add_component` 复用 `clone_from` 指向观测位号的库件身份，位置取源旁偏移（+200,+200），姿态随源；放置后设置位号与值，回滚删除新建器件。`add_net` 逐脚连线：既有网络（含同补丁内刚改名的网络，按旧名回溯）落到其导线端点或既有成员引脚，新网络链式相连、单脚画短桩；引脚坐标由执行器写入时按位号现场解析（引脚可能属于同补丁刚放置的元件），回滚为删除新建导线。新建类写入（`component_place`/`wire_create`）的确认必须带回新建图元 ID 作为回滚句柄；回滚删除按幂等处理（图元已不存在视为已恢复）。器件/导线 `create`/`delete` 均为官方 BETA 预览 API，真实客户端行为仍需验证。

验证分两种模式：仅属性改动的计划以整板快照哈希相等为准；含放置/删除/连线的计划正向写入改用逐操作结构断言（位号存在/移除、网络成员、值与改名），断言未全部成立保持 inconclusive 且不自动回滚；回滚方向一律以回到基线哈希为准。

`apply/readback/rollback` 会话请求均携带 `sessionId/planHash`。`apply` 首先执行新 inspect 并验证审批；执行器收到的逐图元 apply/rollback 请求包含 `project.read/project.write`、绑定的文档和计划，以及 `params: {kind,primitiveId,expected,value}`。扩展重新 get 目标并比较原值，调用官方 `modify(id,{name})` 或 `modify(id,{net})`；API 抛错或无确认返回为结果未知，不视为未写入。

每步调用前后保存 `observations/execution-<摘要>.json`，含原值、目标值、图元、pending/in_flight/written/restored/failed/unknown 状态。重复 apply 拒绝；重启后可读取原补偿句柄回滚。明确未写入的失败会停止后续步骤并逆序补偿已确认步骤；结果未知的写入禁止猜测重试或自动回滚。

写入后立即重新观测并分别持久化 `execution.write/readback/verification`，值为 `not_run/passed/failed/inconclusive`。只有完整回读匹配预期才验证通过；API 写入确认不等于设计成功。缺失或已知延迟的回读保持 inconclusive，允许独立 readback 重试，绝不据此自动回滚。此处验证为观测完整性与计划目标匹配，写后 DRC 仍需单独调用。

取消携带外层 `operation_started.requestId`。在途写入先收集结果，随后停止剩余修改并回读、保留补偿句柄；不丢弃迟到的写入确认。rollback 按逆序恢复已确认步骤，每步比较当前值与已写目标，拒绝覆盖人工修改；完成后再次回读基线。回滚 API 确认与回滚后验证仍是独立事实。

请求、审批、逐步执行、回读、验证、错误、取消与补偿均进入 ChangeSet 审计；最终结果包含完整审计并进入 EDA 会话。桌面「变更与审批」「会话与任务」「EDA 服务」读取同一状态及回滚句柄。

### v0.2.19 回滚恢复诊断

重试结果未知的单器件删除补偿时，先发送 `readback`，参数为 `requireIdle: true, statusOnly: true`，超时 5 秒。新版入口不调用 EDA API，返回 `mutationIdle`、`extensionVersion`、`activeMutation`（请求编号、操作、种类、起始时间、阶段）和 `payload.executorStatus: true`。空闲后再完整回读并核对删除后哈希，最后才执行补偿；完整回读仍须返回 `mutationIdle: true`。兼容 v0.2.18 在探针请求中返回完整观测。

缺少标记返回 `executor_idle_unsupported`，忙碌返回 `executor_busy`，分别记录 `rollback_executor_status`；任一恢复检查失败均不追加整页回读、不调用重建。探针超时保留未知状态，不自动释放写入锁。执行器版本取当前连接的 hello，若结果本身提供版本则优先采用。创建操作仍最多等待 120 秒，超时保留最后收到的阶段。

操作员触发 apply/readback/rollback 不调用模型，但会把对应 EDA 会话标记为 running，最终按验证结果标记 completed/failed，保留原 token 用量与引用。`operation_started.extensionVersion` 供面板显示实际加载的入口版本。

## 9. 兼容夹具与剩余验收

官方接口依据：[API 入口说明](https://prodocs.lceda.cn/cn/api/guide/invoke-apis.html)、[器件接口](https://prodocs.easyeda.com/cn/api/reference/pro-api.isch_primitivecomponent.html)、[导线坐标类型](https://prodocs.easyeda.com/cn/api/reference/pro-api.isch_primitivewire.getstate_line.html)、[DRC 返回值与详细模式](https://prodocs.easyeda.com/cn/api/reference/pro-api.sch_drc.check.html)。核对日期：2026-10-22。

| 场景 | 自动化夹具期望 | 真实客户端验收 |
| --- | --- | --- |
| 完整 SCH/DMT/SYS 模拟宿主 | 稳定快照、稳定外部引用、独立物理哈希 | 未执行 |
| namespace、字段缺失、返回 undefined、getter 抛错 | 对应面 inconclusive，保留其他证据 | 未执行 |
| 使用 this 的实例 getter | 绑定原始图元接收者后读取 | 未执行 |
| 导线扁平坐标、单段嵌套坐标、二维点列 | 归一坐标；无法解释的多分段显式 inconclusive | 未执行 |
| DRC 空数组、错误项、children 分组、未知形状/布尔/null | 仅已识别的空报告 passed；错误 failed；未知 inconclusive | 未执行 |
| 相同快照、替换图元 ID | 保留历史并标记 drifted | 未执行 |
| 过期基线、跨会话、篡改计划、观测不完整 | 拒绝批准，不出现写入成功 | 未执行 |
| 取消及迟到结果 | 审计取消、丢弃迟到结果、会话继续可用 | 未执行 |

`apiTypesVersion: 0.4.26` 是当前夹具声明，不是已确认的真实客户端版本。iframe MessageBus、WebSocket 回调/Blob、分组 DRC、属性及网络改名 API 的实际同步时序仍需在真实嘉立创 EDA 专业版验证。PCB 观测、新增/删除器件、新建网络与自动写后 DRC 尚未接通；不能视为 TASK-5-35 整体完成。写入 API 核对日期：2026-10-23，依据官方 [器件 modify](https://prodocs.lceda.cn/cn/api/reference/pro-api.sch_primitivecomponent.modify.html) 与 [导线 modify](https://prodocs.lceda.cn/cn/api/reference/pro-api.sch_primitivewire.modify.html)；回读使用重新观测，避免复用陈旧图元对象。
