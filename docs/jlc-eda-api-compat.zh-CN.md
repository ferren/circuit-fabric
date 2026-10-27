# 嘉立创 EDA API 真实客户端兼容矩阵

本记录对应 TASK-5-35-4（548），整理日期为 2026-10-24。结论来自恢复的真实客户端截图、inspect 历史和探针日志；不是本轮重新执行全部验收。**只读链路已有证据，写入端到端验收尚未完成，不能据此宣布父任务 544 完成。**

## 环境和证据边界

2026-10-25 补充（分类修复已通过自动化测试，新扩展真实客户端采集仍待验证）：03:59:32Z 的真实记录再次返回 79 个对象，45 项无位号，`components` 不完整；schema/identity/topology 校验通过。此前“空 evidence 导致采集失败”的会话解释不成立，代码没有该判断条件。扩展 v0.2.17 改为按官方 [`getState_ComponentType`](https://prodocs.lceda.cn/en/api/reference/pro-api.isch_primitivecomponent.getstate_componenttype.html) 及[器件类型枚举](https://prodocs.lceda.cn/cn/api/reference/pro-api.esch_primitivecomponenttype.html) 区分普通元件和辅助符号：全量枚举保留，图框/网络标识/端口等归入 `symbols`，参与物理基线，不强求普通器件位号；类型未知、真正缺位号或字段缺失仍 inconclusive。当前旧记录没有采集此类型字段，不能据此声称新版本已在 V4.1.60 实测放行。须更新扩展及 Bridge 后重新采集、重新生成预览，旧计划不可复用。

新 Bridge 将缺失原因按类别和数量汇总到 `diagnostics`，传给界面和会话工具；`eda_inspect` 会明确标注读取已保存观测。DRC 的既有保守判定保留：无法解析完整报告时 `errors=0` 不表示通过，也不是此次 `components` 审批门禁的原因。

- 客户端：Windows，嘉立创 EDA 专业版 V4.1.60；本轮只读进程信息中的客户端构建为 `4.1.60.198f38ab`，与前次截图的 V4.1.60 一致。
- 环境探针：`isClient=true`、`isWeb=false`、`isJLCEDAProEdition=true`、`isEasyEDAProEdition=false`。Web 端、V4.2+ 客户端均未验收。
- 设计：`4寸墨水屏日历相框`，`Schematic1 / P1`；原理图观测的 pageUuid 为 `2b9370d3a9eae2c8`，CircuitFabric 项目 ID 为 `MojianC3`。
- 实测时间：inspect 为 2026-10-23 UTC；API 探针为 2026-10-24 UTC。以记录内 `capturedAt/collectedAt` 为准，日志外层 `t` 只有时分秒。
- 探针版本：0.1.1、0.1.3、0.1.4；原始记录在 `scripts/jlc-acceptance/probe_log.jsonl`。每条结论应保留探针版本和活动文档上下文。
- `eda.apiTypesVersion` 实测为 `null`，不能把模拟夹具的 `0.4.26` 当成宿主版本。
- 本轮从当前工作树重建扩展 v0.2.16，打包成功；**该新包未重新安装、未验收**。旧截图与记录没有已安装生产扩展和 Bridge 二进制的 SHA256，不能证明它们等同于当前工作树。

下表的“存在”仅表示 `typeof method === "function"`；“已调用”才表示获得调用结果。BETA 标签来自 2026-10-24 核对的官方文档，不代表最低客户端版本。任务引用的旧矩阵文件此前不存在于本工作树，本次新建本文件；原先标注为 v4.2+ 的完整方法清单尚未恢复，不能声称已逐项覆盖。今后添加方法必须同时记录官方版本依据和实测版本。

## 实测矩阵

| API / 场景 | 官方依据 / 版本边界 | V4.1.60 真实结果 | 适配与不确定性 |
| --- | --- | --- | --- |
| 入口 `eda` | [扩展运行时作用域](https://prodocs.lceda.cn/cn/api/guide/invoke-apis.html) | 入口探针可见，业务读取已调用 | 使用词法作用域 `eda`；保留 `globalThis.eda` 兼容回退 |
| iframe `eda` | 同上；此次仅测同一扩展 iframe | 探针报告 `edaPresent=true`、`edaInjectedAs="module-global eda"` | 证明未限定属性的 `eda` 可访问；探针没有排除同时存在 `globalThis.eda`，不可推断具体注入机制 |
| iframe 设计读取 | 未核定最低版本 | SCH 上下文 `sch_PrimitiveComponent.getAll()` 返回 79 项；后一次 PCB 上下文调用失败 | 不能把 PCB 上下文的失败解释为 iframe 没有读取权限；未测试 iframe 写入、跨扩展或所有权限组合 |
| `sys_MessageBus.push/pull` | [私有消息总线](https://prodocs.lceda.cn/cn/api/reference/pro-api.sys_messagebus.html) | 同一扩展 iframe → 入口收到消息，`busEcho="received"` | 首次曾超时，后续多次成功；反向、跨扩展和撤销外部交互授权未独立验收 |
| `sys_WebSocket.register/send/close` | 未核定最低版本 | 方法存在；入口报告通过本地 WebSocket 被服务端记录 | 未单独验收 close、断线重连、Blob 回调全部分支 |
| `sys_IFrame.openIFrame` | 未核定最低版本 | 探针 iframe 打开，有截图及返回消息 | 不代表独立脚本上下文也可调用 |
| `sys_Environment.isClient/isWeb/isJLCEDAProEdition/isEasyEDAProEdition` | [运行环境](https://prodocs.lceda.cn/cn/api/reference/pro-api.sys_environment.html) | 已调用，值见环境说明 | Web 端行为仍未验收 |
| `dmt_Project.getCurrentProjectInfo` / `dmt_Schematic.getCurrentSchematicPageInfo` | 未核定最低版本 | 方法存在；inspect 中取得工程和页身份 | 缺少工程/页身份应为 `document` inconclusive；缺失场景只有模拟测试 |
| `sch_PrimitiveComponent.getAll` / 图元 getter / `getAllPins` | 未核定最低版本 | 读取 79 个原始对象、178 个引脚；34 个有位号元件进入语义快照 | 图框、电源符号、网络端口等 45 项无位号，当前实现保留 `components` inconclusive，不应放宽审批门禁冒充完整 |
| `getState_pinType()` | [官方大小写](https://prodocs.lceda.cn/cn/api/reference/pro-api.isch_primitivepin.getstate_pintype.html) | 小写 p 的方法为 function，读取示例 `"IN"`；`getState_PinType` 为 undefined | 使用小写形式；历史 payload 还出现字符串 `"Undefined"`，不能和 JS undefined 混为一谈 |
| `sch_PrimitiveWire.getAll` / `sch_PrimitiveBus.getAll` | 未核定最低版本 | 方法存在；inspect payload 为 73 条导线、0 条总线 | 本次空总线集合不证明所有总线形状兼容 |
| `sch_Net.getAllNets` | 未核定最低版本 | 方法存在；历史 payload 的 nets 为空，语义快照从导线得到 13 个网络 | 不能据此认定完整 pin↔net 语义已验证；仍需非空网络返回形状的真实夹具 |
| `sch_Drc.check(true,false,true)` | [BETA](https://prodocs.lceda.cn/cn/api/reference/pro-api.sch_drc.check.html) | SCH 上下文返回 `[{"type":"warn","count":3}]`；PCB 上下文报 `doctype(3) not support check sch drc` | 汇总项没有 rule/明细，现有适配器返回 `inconclusive:["drc"]`，与已保存 Bridge 报告一致；不能显示 passed |
| `pcb_Drc.check(true,false,true)` | [BETA / 分组返回](https://prodocs.lceda.cn/cn/api/reference/pro-api.pcb_drc.check.html) | SCH 上下文报画布订阅错误；PCB 上下文返回数组，首项 keys 为 `count,list,name,title,visible`，`list` 中还有分组及 Netlist Error 叶子 | 真实分组键为 `list`，不能假定只有 children；目前只保存首项 JSON 的 600 字符摘要，完整 PCB 适配及验证仍未完成 |
| `dmt_EditorControl.getCurrentRenderedAreaImage` | [BETA / Blob 或 undefined](https://prodocs.lceda.cn/cn/api/reference/pro-api.dmt_editorcontrol.getcurrentrenderedareaimage.html) | 探针确认方法存在；未恢复到可靠的成功图像结果 | `canvasImage` 缺失应 inconclusive；函数存在不等于截图链路通过 |
| `sch_PrimitiveComponent.get/create/delete/modify`、`sch_PrimitiveWire.get/create/delete/modify` | 未核定最低版本；真实调用需审批 | 探针确认方法存在，未恢复到审批→写入→回读→回滚记录 | 元件/属性与导线/网络变更均仍待真实端到端验收 |
| v4.2+ 专属能力 / Web 端 | 未取得完整旧清单和对应宿主 | 未测 | 不把 4.1.60 的 method 存在性外推为 v4.2+ 或 Web 端兼容 |

PCB 的错误返回与 SCH 的成功返回共同说明调用依赖活动画布。现有生产 `runDrc()` 固定使用 SCH 命名空间；本表中的 PCB 探针结果不是生产 PCB DRC 已接通的证据。

## 两次 inspect 与 inconclusive

比较历史中 2026-10-23T13:00:13.030Z 与 2026-10-23T13:00:53.254Z 两条记录，三项哈希和 47 项外部引用列表完全相同：

```text
snapshot sha256:2e6ebcebddd280c1da4c9009f0ac3b1473c340a3dbefbbd99fdf46af72919efa
logical  sha256:4bf81a2edbe79d58bccaa7e764d036757550c872798648b19b0fcc8f1716fb61
physical sha256:f437cec94c3fe57c24d46def632303d1418f38a2f385b4c058816638815fe28a
```

两条记录均保留 `components` inconclusive，持久化的 `observation.completeness` 验证状态也是 inconclusive。稳定哈希不表示观测完整，也不代表审批或写入通过。

更早的一条记录哈希相同，外部引用却不同：较新记录的网络引用增加了 `pageUuid` 字段。现有证据不能确认当时运行的两个 Bridge 二进制相同，**不将跨实现版本的引用格式差异算作同版本稳定性通过**，也不把它误报为图元 ID 改变。

恢复记录的比较工具（Python 标准库，无网络调用、无 EDA 写入）支持显式输入路径：

```powershell
python scripts/jlc-acceptance/summarize_evidence.py --history <inspect-history目录> --probe-log scripts/jlc-acceptance/probe_log.jsonl --output <结果JSON路径>
```

输出包含输入 SHA256、全部比较结论、原始探针报告及其日志行号，供验收附件复核。原始记录、截图和汇总随本次问题评论的证据包交付；本文件本身不包含电路完整原始数据。

## 剩余验收

2026-10-25 评论 8470 与最新持久化审计进一步缩小范围：扩展实际版本为 `0.2.19`，空闲检查通过，重试在 `create` 等待阶段超时，随后完整回读仍为删除后现场。本机安装目录中的 SDK `pro-api/0.3.38.25cc6d44` 将图元中的工程器件 UUID 与器件来源库 UUID 组合返回；宿主 `pro-ui/4.1.62.5e01457f` 的创建实现，在找不到器件的分支不结束 Promise。提取这两个实际函数、使用受控假数据的离线复现确认了此行为；这不是实时客户端验收，也未证明当前工程库仍能取到 U1。SDK 与 UI 文件哈希随本次证据附件保存，不将资源目录版本当成运行中的客户端版本。

v0.2.20 在创建前通过 [`lib_Device.get`](https://prodocs.easyeda.com/en/api/reference/pro-api.lib_device.get.html) 精确核验原库，再核验当前绑定工程中的同一个 UUID；查不到、单次只读查询超过 5 秒、子部件不明确或活动文档变化时，在写入前失败。不会模糊搜索替代型号，也不释放已发出写入的在途锁。40 项扩展自动测试通过；现有 v0.2.19 Bridge 可继续使用，更新扩展并重启 EDA 后仍需在原计划上验证真实补偿、完整回读及引脚连接。

2026-10-25 新增真实运行证据：用户确认删除成功；持久审计中 `jlc-ff27330c5131cfebeffdacdae83486738ed6237ed854f08f2d280ffbd8e2c942` 的写入确认、完整回读和删除结构断言均通过。删除前基线 `cf3a0e78…77ff2`、删除后观测 `d563ea8a…2b0f1`；第一次补偿报 `operation_timeout：执行器结果未知`，之后日志为 `unknown`，再次回滚被保护机制拒绝。**不是 parent_snapshot_hash 为空导致；具体卡在库加载/创建/属性恢复/结果传输哪一步，旧记录不能判定。** 尚无真实恢复成功证据。

v0.2.18 与新 Bridge 增加补偿阶段提示、库件创建 120 秒等待、历史计划恢复、实际错误返回和受约束的未知补偿恢复；删除后的完整现场与执行器空闲必须重新确认。重建会恢复已采集 uniqueId/otherProperty。create/modify 参数依据[官方 create](https://prodocs.lceda.cn/cn/api/reference/pro-api.sch_primitivecomponent.create.html)和[官方 modify](https://prodocs.lceda.cn/cn/api/reference/pro-api.sch_primitivecomponent.modify.html)核对，修复包真实重试仍待验收，不能据模拟测试宣称全部属性、连接已恢复。

1. 更新 v0.2.18 与配套 Bridge、重启 EDA，选择原项目和原会话，在原计划上验证回滚重试及完整回读，记录阶段、审计与截图。旧 0.2.17 分类修复后的这次真实基线已完整，早期 components 不完整不再代表本次现场。
2. 使用可丢弃的测试设计继续验证元件属性和网络/导线各一次 preview→显式审批→apply→readback；单次元件删除已取得上述真实审计证据，其他变更类型仍未完成真实验收。
3. 在途取消、补偿回滚与回滚后基线匹配；保存完整审计、状态和截图。
4. 保存完整 SCH/PCB DRC 原始结构并补齐相应适配；补测视觉截图与真实不可用 API/字段缺失场景。
5. 恢复原 v4.2+ 方法清单，并在对应真实版本逐项补测；按需补 Web 端。
6. 完整验收后将最终记录发布父任务 544；当前只能交付部分证据，548 仍不可进入验收完成状态。
