# CircuitFabric

[English](README.md)

CircuitFabric 是一个以 AI 为原生能力的电路设计平台，具有语义核心、硬件文档证据体系，以及可插拔的 EDA 物化后端。

它不是嵌入单一 EDA 产品中的助手。CircuitFabric 位于外部智能体运行时（例如 Codex App Server）与嘉立创 EDA、KiCad、网表导出器、BOM 系统、仿真器等工程工具之间，提供受控的中间层。

```text
外部智能体运行时
        |
        v
电路语义核心 / Logical Circuit IR
        |
        v
融合与物化层
        |
        v
EDA、网表、BOM、仿真与验证插件
```

## 范围

- 将逻辑电路事实、语义意图、约束、证据和变更历史保持为与 EDA 厂商无关的内容。
- 将数据手册、参考电路、BOM 和设计说明等硬件文档作为可追溯的一等输入。
- 为 EDA 发起的智能体会话，通过来源感知检索提供项目已授权文档，使每条文档派生结论都可引用。
- 让外部智能体提出计划和补丁，而不授予其直接访问原始 EDA API 或持久化设计状态的权限。
- 通过具备版本的 EDA 后端插件物化已批准变更，并通过回读、DRC/ERC、视觉证据或仿真完成验证。

## 仓库结构

- `crates/`：领域契约与确定性服务；不依赖任何特定 EDA 或智能体运行时。
- `plugins/`：独立版本化的适配器，首先是嘉立创 EDA bridge 契约。
- `apps/`：产品壳，首先是 GPUI 桌面控制面。
- `assets/branding/`：项目自有应用美术资源，包括多尺寸 Windows 图标资源。
- [Architecture](docs/architecture.md) / [架构](docs/architecture.zh-CN.md)：目标架构、所有权、约束和插件边界。
- [Implementation plan](docs/implementation-plan.md) / [实施计划](docs/implementation-plan.zh-CN.md)：分阶段交付计划和验收标准。
- [Provider settings](docs/provider-settings.md)：多 Provider 配置与凭据边界。
- [TODO](TODO.md)：按执行顺序维护的实现待办。

## 当前状态

本仓库刻意从干净的架构与契约基线开始。首个实现里程碑是确定性的 Logical Circuit IR 快照存储，以及嘉立创 EDA 已观测设计导入器。嘉立创 EDA bridge 将在本仓库中独立实现：JLCircuit-Agent 仅作为行为参考，绝不是源码、构建或运行时依赖。

## 快速开始

安装 Rust 1.88 或更高版本后，在仓库根目录执行确定性检查：

```powershell
cargo fmt --all -- --check
cargo test --workspace --exclude circuitfabric-desktop
cargo clippy --workspace --exclude circuitfabric-desktop --all-targets -- -D warnings
```

桌面 crate 当前是带多 Provider 管理器的 GPUI 控制面骨架；准备好已锁定版本的 GPUI 依赖后，可使用 `--features native-ui` 构建原生界面。Provider 设置保存在系统应用数据目录中，只包含 API Key 环境变量名，不保存密钥值。嘉立创 EDA 扩展包可通过 `./scripts/package-jlc-extension.ps1` 重新生成，详见 [`artifacts/README.md`](artifacts/README.md)。

## 核心原则

1. 智能体只提出建议；语义核心才拥有事实。
2. 逻辑身份独立于 EDA 图元 ID 与物理布局。
3. 每个可执行变更都必须绑定快照、可审阅、可审计、可验证。
4. 后端写入若无法回读，状态为 `inconclusive`，而非成功。
5. 插件不得绕过语义核心、用户审批或验证策略。
