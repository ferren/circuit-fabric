# 运行时集成验证记录

2026-09-06，Windows 本机。验证代码位于 `crates/circuitfabric-codex-runtime/tests/`，探针为 `examples/runtime_probe.rs`。

## 可复现命令

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p circuitfabric-desktop --features native-ui
cargo build -p circuitfabric-codex-runtime --example runtime_probe
python crates/circuitfabric-codex-runtime/tests/native_smoke.py
```

集成测试需要 PATH 上的 Codex、Claude Code、DSH 和 Python。测试启动本地 HTTP 模型协议服务和两个 Python MCP server，使用虚拟环境变量凭据，不访问远程模型。Node 运行时在受限沙箱内启动子进程可能报 EPERM；本次在常规用户权限下执行。

## 已通过

- 工作区自动化测试；包含 Provider 失效引用、停用引用、密钥字段校验、配置重载、保存失败保持原文件、未授权工具拒绝、启动前取消。
- 三种已安装运行时均完成配置 → 启动 → 模型请求 → 两个真实 MCP 工具调用 → 返回结果 → 清理。
- 两项技能的实际文件内容均出现在三种运行时的模型请求中。
- 显式 Provider 关联覆盖默认项，模型请求使用对应的另一服务路由和模型。
- Codex 图片请求实际包含图像数据，使用独立 Vision 路由和模型。
- 三种运行时在取消后返回失败，临时运行配置目录被清理。
- 原生桌面 feature 编译检查通过。工作区严格 Clippy 通过；启用 native-ui 的全量严格 Clippy 仍有既有界面代码警告，未将其报告为通过。

## 未完成的验收

- 远程模型服务：本进程未配置 OPENAI、Anthropic、DeepSeek 或项目既有的 LLM/Vision 环境变量；未执行真实远程推理。需要用户在本机配置凭据并提供环境变量名、服务地址和模型名，不能把密钥值写入任务评论。
- 原生界面人工验收：多 Provider 编辑、重启恢复、作用域切换、失败恢复和权限撤销的完整点击操作尚未录制。
- MCP 目前以 server 为授权单位，仅支持 stdio；逐工具权限编辑和 HTTP transport 尚未实现。
- 技能加载目前限于 SKILL.md 指令。依赖脚本、相对资源和技能原生发现机制尚未形成通用闭环。
- Claude/DSH 是每任务进程，尚无持久多轮原生会话；JLC bridge 通过历史文本重建 Codex 任务，结果完成后一次发送，没有逐 token 推送。
- 运行时任务目录为隔离目录；在用户指定项目目录直接执行以及对应的隐式配置防护仍需完善。
- 当前项目级持久化仅包括授权与指令，Provider、运行时与工具定义为全局。定义的项目级覆盖编辑尚未实现。

因此本次代码提供了真实调用链路和可复现测试，但不能据此将 issue 398 置为待审核或完成。
