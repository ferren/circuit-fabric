# CircuitFabric TODO

状态约定：`[x] 已确认` 表示已做出的架构或产品决策，尚不代表已完成代码实现；未标记的项目仍需实现并通过相应验收。

## Now: Phase 0 and Phase 1

- [x] 已确认：主要开发语言为 Rust；桌面控制台使用 GPUI + `gpui-component`，不承担原理图编辑或 EDA 内对话。
- [x] 已实现：建立 Rust workspace，以及核心契约、语义核心、快照存储、文档服务、插件 API、EDA bridge 和桌面控制面 crate 的初始边界。
- [ ] Add versioned Logical Circuit IR schemas and canonical JSON serialization.
- [ ] Implement snapshot store, `logicalHash`, `physicalHash`, lineage, and fixtures.
- [ ] Implement deterministic Schema, Identity, and Topology validation.
- [ ] Define `IRPatch`, `IRDiff`, `ChangeSet`, `MaterializationPlan`, and `VerificationReport` examples.
- [ ] Define and validate declarative Agent Runtime and EDA Backend plugin manifests.
- [ ] Add test policy: fixture inputs, expected snapshots, expected validation status, and golden diffs.
- [x] 已实现：加入 `.env.example`、本地数据约定、Rust 格式化/lint 配置和 CI 基线。
- [x] 已确认：桌面控制面管理项目、文档、服务、智能体/MCP、审计与用量；EDA backend/bridge 插件承担 EDA 内对话与上下文采集。
- [x] 已实现：Windows 平台适配层通过 GPUI 暴露的 `HWND` 设置标题栏与任务栏图标；不修改 GPUI 源码或本地缓存。
- [ ] Define the desktop control-plane and project-scoped configuration contracts for runtime endpoints, secret references, skills, MCPs, and usage.
- [x] 已实现：内存 ProjectWorkspace 可创建/列出项目，并只允许项目内已授权文档登记和证据检索。
- [x] 已实现：Codex App Server 本地 JSON-RPC 客户端、仅回环的 JLC WebSocket bridge，以及按嘉立创 `edaEsbuildExportName` IIFE 和 `.eext` 格式打包的 EDA 对话扩展第一条链路。

## Next: Documents and JLCircuit import

- [ ] Implement authorized Hardware Document source registration and content-hash index.
- [ ] Extract source-aware document fragments and citations from PDF/text/BOM/netlist inputs.
- [x] 已确认：EDA 发起的智能体会话可检索本项目已授权文档，且所有文档派生结论必须保留来源定位符并进入会话审计。
- [ ] Bind authorized document sources to projects and expose source-aware retrieval to EDA-originated agent sessions.
- [ ] Persist document citations used by an agent response in the session/task audit record.
- [ ] Define the JLCircuit bridge protocol as a plugin transport contract.
- [ ] Build an independent JLCircuit EDA backend/bridge plugin: EDA-local chat, context capture, task progress, and a read-only importer for components, pins, nets, wires, buses, DRC/ERC, and canvas evidence.
- [x] 已确认：JLCircuit-Agent 仅作可观察行为与兼容夹具的参考；不得作为本项目源码、构建或运行时依赖。
- [ ] Create a JLCircuit compatibility fixture matrix and mark missing fields `inconclusive`.

## Later: Fusion and materialization

- [ ] Implement baseline/target/readback reconciliation and identity conflict resolution.
- [ ] Implement JLCircuit preview/apply/readback/rollback behind snapshot-bound approval.
- [ ] Add write preconditions, delayed-readback handling, and structured verification reports.
- [ ] Add Codex App Server runtime host and runtime-neutral event stream.
- [ ] Add one second EDA importer before generalizing write support.
- [ ] Add plugin signing, permissions, isolation, lifecycle supervision, and management UI.
