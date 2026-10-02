# 技能与 MCP 管理、授权和验收

2026-10-02，Windows 原生 GPUI 应用，任务 #490。工具定义和授权复用现有运行时配置、项目存储和保险库；Provider 与运行时设置页未重做。

## 操作流程

进入「智能体与工具 → 技能与 MCP 授权」。资源管理使用独立的「技能」「MCP server」标签、搜索列表和选中详情；「返回运行时与 Provider」返回原来的资源导航。

技能：点击「添加技能」，选择实际目录或 `SKILL.md`，在弹窗核对标识、来源、说明和内容预览，再保存。标识优先读取 frontmatter 的 `name`，否则使用目录名；拒绝错误文件名、空文件、超过 256 KiB 的内容、非法标识和来自另一目录的重复标识。编辑可以更换相同标识的来源文件，保持启停状态；更换标识需另行添加。预览不授予加载权限，停用技能也可以预览。

MCP：点击「添加 MCP server」，在弹窗配置名称、唯一标识、启动程序、字符串 JSON 参数数组和认证变量名。当前只支持 stdio；程序不经过 shell 执行。参数中的常见明文认证形式、无效变量名及引用变量中已知的密钥值会被拒绝。已保存旧条目没有显示名称时显示标识。编辑保留标识与启停状态，Jev 编辑进入其已有配置弹窗，始终使用同一条 `typesafe-jev` 定义及同一授权列表。

选择资源后，分别在「全局」「当前项目」点击「应用到所选范围」。保存定义、启用资源和授权是三个独立步骤，保存不自动授权。未选择项目时使用全局权限；选择项目后，最终权限为 **全局授权 ∩ 当前项目授权 ∩ 已存在且启用的定义**。项目不能扩大全局权限。详情同时展示两个授权来源和最终权限。

技能通过「验证加载」实际读取指令。任务启动时将指令注入运行时。当前只支持 `SKILL.md` 指令，附属脚本和资源不复制、不自动运行，相对资源没有通用加载保证。

MCP 通过「连接并发现工具」执行真实 initialize / tools/list，显示实际工具名称、说明及 schema；选择真实工具、填写 JSON 对象后点击「调用工具」。每次发现和调用建立新的连接；“连接验证成功”是最近一次验证结果，不表示常驻连接。未授权、停用、缺失程序、协议错误、服务端 `isError` 或超时均显示失败。

编辑、添加和删除确认都在弹窗完成，保存按钮留在弹窗内；保存失败保留输入，取消丢弃草稿，下次打开重新读取保存值。列表切换不会保存。局部保存仅合并 catalog 或授权，不提交其他页面的 Provider、Bridge、目录或运行时草稿。

配置/权限变更成功或项目切换会取消普通任务、数据提取及 MCP 操作，清空旧验证状态；旧异步结果通过版本校验不能回写为成功。MCP 取消会终止该操作的子进程树。新任务读取新配置；已经完成的外部副作用不能回滚。bridge 对配置变更保留原有监测取消，并复用同一交集计算；运行时入口再次限制调用方传入的权限不得超出保存的全局授权。

删除清除定义、全局授权及已恢复到当前应用的项目授权，保留源文件。项目文件分属不同目录，清理不是跨文件事务：某项目写入失败会保留定义、报告失败及已完成的局部撤销，可重试。未注册/无法恢复的项目文件无法枚举，可能保留旧标识；没有全局授权时不生效，重新引入同一标识前应复核这些旧项目。内置 Jev 删除留有持久化标记，重启不自动复活。

## 自动化与真实运行记录

运行过的命令：

```powershell
cargo test -p circuitfabric-codex-runtime -p circuitfabric-desktop -p jlcircuit-eda-bridge
cargo check -p circuitfabric-desktop --features native-ui
cargo build -p circuitfabric-desktop --features native-ui
cargo build -p circuitfabric-codex-runtime --example runtime_probe
python crates/circuitfabric-codex-runtime/tests/native_smoke.py mcp codex
cargo fmt --all -- --check
```

针对性新增测试见 `tests/resource_workflow.rs`、`tools.rs` 和桌面 `settings_persistence.rs`：

最新回归结果：三个包合计 72 项测试通过、1 项 Jev 外部依赖测试忽略；原生界面 check/build、格式检查和 diff 空白检查通过。

- 两项真实技能文件和两个独立 Python stdio server，实际发现 echo、调用并检查各自调用日志，保存后重新加载。
- 全局/项目交集、两个项目互斥授权、未授权、撤销、停用、缺失程序和非法配置。
- 正在运行的慢 server 实际启动后取消，三秒内返回取消失败而非成功，不等待旧超时。
- 技能导入后内容变空、MCP server 返回空调用结果，均必须失败，不能显示成功。
- 文件名、技能标识冲突、明文认证及保险库已知密钥值检查；删除 Jev 后不随重启恢复。
- 局部保存不提交其他页无效草稿；读取、校验、写入失败保持保存文件；删除不改变 Provider 配置。

实际 Codex CLI 通过本地确定性模型协议服务运行，两项技能出现在模型请求中，两组 MCP 工具均被调用并写入独立日志；Provider 路由、Vision 与三种运行时的取消清理也通过。**模型响应是本地协议 fixture，不是远程推理验证**。MCP 使用实际进程和实际 JSON-RPC 请求，不以界面状态或假工具清单代替连接与调用。

## 原生界面验收

使用 `scripts/validate-tool-management.py` 启动隔离 APPDATA 配置，不接触用户原有配置或密钥。脚本只操作其自身 PID 的 CircuitFabric 窗口，在结束前关闭子进程。默认准备 fixture；`--reuse` 保留保存状态用于重启验收。项目 fixture 使用实际 `ProjectStorage` 和 `ProjectRegistry` 创建：

```powershell
cargo run -p circuitfabric-desktop --example prepare_tool_validation -- target/tool-management-native
python scripts/validate-tool-management.py --reuse
```

| 原生场景 | 观察结果 |
| --- | --- |
| 35 项技能、120 行长详情 | 列表和详情可分别滚动，另一框的位置不随之改变 |
| 技能验证加载 | 实际读取 skill-00，共 8007 字节指令；预览与权限分开显示 |
| 原生技能导入和双范围应用 | 从实际 native-imported 目录导入，预览 frontmatter 与内容，分别应用全局/native-b，实际加载 155 字节指令 |
| 两个 server 同时存在 | first、second 逐项管理，工具清单来自真实连接 |
| 两个 server 真实调用 | first 写入 `CF_NATIVE_FIRST`，second 写入 `CF_NATIVE_SECOND`，各自日志记录 echo 调用 |
| 未授权、撤销、停用 | 显示禁止/失败；成功撤销清除旧工具与验证结果 |
| 原生保存失败 | 将隔离 runtime.json 设为只读，保存报拒绝访问；弹窗未关闭，输入和保存/取消按钮保留 |
| 重启恢复 | first 全局授权恢复；连接验证记录不冒充持久连接 |
| 项目切换 | native-a 允许 first；切到 native-b 后 first 被拒绝，second 可独立应用并调用 |
| 删除 | second 消失，其全局及 native-b 授权移除；first 保留 |
| 新增 MCP | 弹窗新建 third，保存定义后仍显示未应用/禁止；列表恢复为两个条目 |
| 小窗口 | 2200×1300 物理像素、200% 缩放（约 1100×650 逻辑像素），操作按钮可达，详情在框内滚动 |

截图与调用日志通过 issue 附件交付；本机 `target/` 路径不是共享链接。

## 尚未通过或未验证的部分

- 全套原生 runtime smoke 中，Claude 返回了模型文本，但工具 schema 为空；不能报告 Claude 技能/MCP 闭环通过。DSH 在两个 MCP client 的初始连接/同步阶段失败。两者取消与进程清理通过，整体运行时验收仍由父任务 #398 跟踪。
- 远程模型、Jev 的真实远程 evaluate、JLC EDA 插件实际连接未在此次运行验证；没有使用用户密钥。stdio 以外的传输和技能脚本/附属资源不在当前实现支持范围内。
- 严格 Clippy 没有全部通过：现有 `lib.rs` 文档、project storage 文档、`usage_audit.rs` 字段命名及 `pdf_zoom.rs` 浮点比较告警仍在。仅 runtime 包在允许其既有 doc_markdown 告警时的严格检查通过，不能替代全仓严格检查。
- 新增原生验收脚本是交互检查工具，依赖 Windows 桌面、Pillow 和焦点权限；坐标受窗口与 DPI 影响，截图内容经过检查后才作为记录。无头环境不具备同等原生验收证据。
