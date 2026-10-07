# 用量与审计核查清单

核查日期：2026-10-06。实现沿用桌面的 `application/` 与 `ui/` 分层。

| 核查项 | 已落地行为 | 来源与实际边界 |
| --- | --- | --- |
| 运行时 Token | Codex 接收匹配 thread/turn 的 `thread/tokenUsage/updated` 累计快照；重复通知替换，不累加。Claude 读取结果 JSON 的输入、缓存读取、缓存创建和输出计数。 | 仅使用适配器报告，Dsh 文本协议没有 Token 报告，标为未报告。没有估算成本或 Token。 |
| 持久化 | `application/runtime_session.rs` 统一保存用户/模型摘录、用量事件、终态和会话元数据；失败/取消保留已经报告的用量。任务工作线程先保存再交回 UI。 | `sessions/*.md`；仅已打开项目的调用进入项目统计。无项目调用或会话文件创建失败的调用不能归属项目，UI 明示失败。异常退出仍可能留下 running 会话，未报告部分无法恢复。 |
| 文档提取 | 每次实际 Codex 提取调用建立独立会话，保留 document ID、文档哈希、类别和页码；复用检查点不会再建会话或重复计数；结束后刷新项目投影。 | Codex 提取步骤已覆盖。独立 Jev evaluate 判断调用仍不纳入此运行时账本：其 MCP 元数据不提供可靠的计数可用性标记，失败/部分响应也不能提供完整消耗。不能把这些用量显示成真实零值。 |
| 未报告与零值 | 非零历史计数或显式用量事件标为已报告；显式零用量事件保持 true；无报告保持 false。页面显示未报告会话数，CSV 带 `usage_reported`。 | 兼容 schema-v1；只有零元数据且没有用量事件的历史会话无法判定是否报告过零，因此保守视为未报告。 |
| 周期和聚合 | 全部、最近 7 天、最近 30 天；按项目、记录的 Provider 配置 ID、运行时分别聚合；统计、四种图表和用量导出共用同一筛选集合。 | 最近周期是相对当前时刻的滚动区间，排除未来时间；用量以完成时间归属，未完成会话用开始时间，按 UTC 分日。旧 `runtimeProfileId` 仅保留记录值，不反推厂商/模型。 |
| 图表 | 每日输入/输出为双系列折线图，共用纵轴，横轴按 UTC 日期实际间隔绘制；项目、Provider、运行时分别为环形图，图例显示总量与占比。图表位于文字明细前。 | 空数据和非空全零记录都绘制网格、日期刻度、零值折线与数据点，以及灰色空环、0 tokens 和明确提示；重合线条用不同线宽与点大小保留两种颜色。缺失日期补展示用零值，不插入业务记录或 CSV。环形图只对正数绘制占比；全零灰环不代表虚构类别。比例先按最大值归一化，极大计数不溢出。未报告会话的默认计数不代表实测零，完整性由页面提示与导出标记说明。 |
| 会话审计 | 投影开始/完成元数据和严格时间戳事件头，保留项目、会话、配置 ID、backend ID、会话文件名与 body 行号。 | 用户/模型正文中的 approval、rollback 等字样不生成事件。运行时摘录最多 2000 字符，CSV 完整导出的是已持久化记录，不能恢复未记录的原始长文本。 |
| 审批审计 | 加载 ChangeSet `audit[]` 的已保存决策，保留 actor、reason、观测哈希、回滚句柄及 base/target/plan 哈希。 | `logic/changesets/<id>.json#/audit/<index>`。源结构没有会话/Provider/运行时关联时，显式使用 `not-recorded`，不推断身份。 |
| 物化与验证 | 非 NotRun 的 write/readback/verification 作为只读阶段快照，保留状态、detail 和计划哈希上下文。 | `#/execution/<stage>`；源结构没有事件时间，`timestamp_known=false`，UI 明示未知时间，CSV 时间留空；只在“全部”周期出现，不能伪造为 1970 年事件或当前事件。语义快照的 constraints 是当前验证事实，仍由语义页面展示，不冒充历史审计事件。 |
| 回滚 | 只投影明确记录的回滚决策/事件；句柄可用不会生成“已回滚”记录。 | 当前工程没有可自动补回的独立回滚执行历史；只有后端写入明确事件才有回滚行。 |
| 来源筛选与完整导出 | 审计按类别、周期和文本筛选；文本覆盖 summary、各身份、source type/ID、locator。用量文本也支持会话和来源路径。两份 CSV 保留来源标识和定位信息，正确转义逗号、引号、换行。 | 审计每页 50 条，筛选变化重置页码、数据缩减夹紧页码；导出使用全部筛选结果，不受当前页或原 200 条上限影响。 |
| 只读与缺失来源 | 审计页面没有编辑、删除、改写入口。会话 replay 读取失败保留已有用量并显示审计缺失诊断。 | 只读投影与导出前后来源文件字节保持一致。底层是本地文件，不提供签名、防篡改链或 WORM 存储；执行阶段快照可被后端更新，因此不能保证完整事件历史。 |

主要实现：`application/usage_audit.rs`（查询/聚合/来源/CSV/分页范围）、`application/project_data.rs`（来源装配）、`application/runtime_session.rs`（运行时持久化）、`ui/usage.rs`（筛选/分页/导出）、`ui/usage_charts.rs`（折线/环形图）、`ui/runtime.rs` 和 `ui/documents.rs`（调用接入）。

菜单交付状态已更新：`Usage & audit` 不再显示 TODO。GPUI 回归检查空数据、非空全零、正数和筛选无结果，并检查两条折线实际绘制出的彩色数据点，避免只验证容器高度；另验证共同刻度、日期间隔和极大计数的环形比例。

回归覆盖：空集合、实测零与未报告、极大计数、周期边界和未来时间、所有图表与同一筛选集合一致、身份与定位保留、未知时间阶段、回滚句柄不产生回滚、超过 200 条的分页/完整导出、CSV 特殊字符、来源缺失诊断、失败调用用量落盘、文件重开和来源字节不变，以及实际 GPUI 的空/零图表和筛选后的页码重置。

验证命令：

```powershell
cargo test -p circuitfabric-desktop -p circuitfabric-codex-runtime -p circuitfabric-project --lib --bins --tests
cargo test -p circuitfabric-desktop --features ui-test-support --bins
python scripts/check-desktop-boundaries.py
cargo build -p circuitfabric-codex-runtime --example runtime_probe
python crates/circuitfabric-codex-runtime/tests/native_smoke.py --usage-only codex claude dsh
```

真实已安装适配器对本地确定性模型服务的用量验证结果：Codex 报告输入 10 / 输出 2，Claude 报告输入 10 / 输出 2，Dsh 运行成功但 `usage=None`。没有访问付费模型服务。

本地回归结果：三个相关 crate 的应用/运行时/项目及集成测试共 138 项通过，1 项需要独立 Jev 环境的原有测试跳过；本次图表修正后，启用原生 UI 的桌面测试 54 项通过；桌面依赖/滚动边界检查和差异空白检查通过。

完整 MCP smoke 的实际限制：Codex 的工具及计数检查通过；当前 Claude 向本地模型发送的工具清单为空，完整 smoke 在 MCP 工具暴露断言失败。启用 MCP 的 Dsh 也出现 fixture MCP 初始同步失败。`--usage-only` 明确关闭 MCP 授权，只验证运行时计数；不替代完整工具能力验证。
