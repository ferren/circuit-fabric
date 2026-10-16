# 运行时集成验证记录

2026-09-06，Windows 本机。验证代码位于 `crates/circuitfabric-codex-runtime/tests/`，探针为 `examples/runtime_probe.rs`。

## 2026-10-16 DSH + Z.ai 报错修复（issue #525）

用户将 Z.ai Provider 绑定到 DSH 后得到 `dsh: SERVER: DeepSeek API error (HTTP 500)`。核对 `@deepseek-ai/dsh` 的 `dsh-llm-deepseek` 适配器源码（`lib/index.js`）：认证为 `Bearer` 头，请求固定发往 `{baseURL}/chat/completions`。随附 Z.ai Provider 的 `https://api.z.ai/api/v1` 只服务 OpenAI Responses（Codex 路由），`/api/v1/chat/completions` 被网关以 HTTP 500 拒绝，与 Jev 文档"不要使用 `/api/v1`"的结论一致。

`execution.rs` 的 `dsh_command` 现在先经 `dsh_endpoint` 校验：`api.z.ai` 主机上仅放行两个文档化 Chat Completions 基址——`/api/coding/paas/v4`（编程包密钥）与 `/api/paas/v4`（按量付费密钥）——其余（含 `/api/v1`、`/api/anthropic`）在任务启动前报错并列出两个端点；非 z.ai 地址原样透传。与 Claude 的按调用重写不同，这里不做自动改写：两个 chat 端点按密钥计费方式区分，猜错会在服务商侧返回 1113/401，比明确报错更难排查。启动时校验同时覆盖显式绑定与默认 Provider。

验证结果（Windows）：

- `cargo test -p circuitfabric-codex-runtime`：72 通过、1 忽略，新增用例覆盖 z.ai 非 chat 地址拒绝（错误包含两个端点）与文档化地址、DeepSeek、自定义网关的透传及 `runtime.patch.yml` 中的 `baseURL` 断言。
- 远端边界不变：未用真实 z.ai 密钥复验 `/api/v1/chat/completions` 的 500（结论依据适配器源码、仓库既有 live 验证记录与用户报错）；用户按密钥类型改用对应端点后需在桌面端实测。

## 2026-10-16 Claude Code + Z.ai 报错修复（issue #522）

用户在 Claude Code 端点选择 Z.ai 后出现通用运行时错误。此前的本地 fixture 成功不能证明默认 Z.ai 地址能用于 Claude：旧适配器只移除 `/v1`，将默认 `https://api.z.ai/api/v1` 变成 `/api`，而 [Z.ai 官方 Claude Code 配置](https://docs.z.ai/devpack/tool/claude)要求 `/api/anthropic` 和 `ANTHROPIC_AUTH_TOKEN`。

`execution.rs` 现在仅对 `api.z.ai` 上已知的 Responses/Coding Plan 地址选择 Anthropic 路由及认证变量，保留共享 Provider 的保存配置与 Codex 路由。自定义网关不会按 Provider 名称或 ID 重定向。失败路径读取 Claude stdout JSON 的 `result`/`errors`，同时保留 stderr，脱敏后显示；错误前缀只包装一次。当前 Claude 2.1.296 对 401 也会执行十次重试，其退避超过应用的 180 秒超时，导致最终 JSON 错误丢失，因此将子进程重试上限设为两次。

验证结果（Windows，Claude 2.1.296）：

- `cargo test -p circuitfabric-codex-runtime`：71 通过、1 忽略，包括 Z.ai 路由/凭据隔离、共享配置不变、结构化错误提取和脱敏。
- `cargo clippy -p circuitfabric-codex-runtime --all-targets -- -D warnings`：通过。
- `cargo build -p circuitfabric-codex-runtime --example runtime_probe` 后执行完整 `native_smoke.py`：MCP/Codex/Claude/DSH、技能与授权工具调用、Provider/Vision 分流、取消清理全部通过；新增 GLM 模型的 `/api/anthropic/v1/messages` 请求及 HTTP 401 原因回传、密钥脱敏检查均通过。Claude 请求可带 `?beta=true`，路径断言按解析后的 URL 检查。
- `cargo build -p circuitfabric-desktop --features native-ui`：通过，桌面开发版已重建。

完整 smoke 须在允许 CLI 创建 MCP 子进程的环境运行；受限沙箱会以 `EPERM` 阻止启动，首次失败已在沙箱外复验通过。所有模型服务仍是本地 fixture，未解锁用户保险库或验证真实 Z.ai 订阅、配额及密钥；远端验收需重启新开发版，在桌面端解锁后再次测试 Claude Code + Z.ai。

## 2026-10-14 Claude Code 运行时复验（issue #522）

针对 2026-10-03 记录的"Claude 工具 schema 为空、DSH MCP 初始同步失败"，在当前 CLI（claude 2.1.292 / dsh 0.1.2-rc.1 / codex 0.162.1）下复验，两项均不再复现：

```powershell
cargo test -p circuitfabric-codex-runtime          # 66 通过，1 忽略
cargo build -p circuitfabric-codex-runtime --example runtime_probe
python crates/circuitfabric-codex-runtime/tests/native_smoke.py   # mcp/codex/claude/dsh 全部 PASS
```

完整 smoke 结果：Claude 的模型请求包含且仅包含两个授权 MCP 工具（`mcp__first__echo`、`mcp__second__echo`，无内置工具），技能指令随请求下发，两个 fixture MCP server 均被真实调用并回传结果，`CF_USAGE_OK` 计数通过；DSH 的 MCP 同步与闭环同样通过。Provider 绑定路由、Vision 分流、三运行时取消与临时目录清理全部 PASS。此前 `--usage-only` 只是绕过 Claude MCP 断言的临时手段，现不再需要。

边界不变：仍以本地 fixture 模型服务验证，未做真实远程推理（本机密钥在加密保险库中，需用户解锁后由桌面端发起）；Claude/DSH/Kimi 仍为每任务一次性进程，流式与原生多轮会话仅 Codex 实现。

## 2026-10-03 工作区编译修复与补交

技能/MCP 子任务 #490 的运行时、桌面界面、bridge、测试和操作说明此前留在工作区，未包含在工作目录提交 `604f30d` 中，本次一起补交。原生界面首次构建报新接口不存在，而接口实际已在源码中：先前隔离工作副本与当前工作区共用了 `target`，依赖产物沿用了旧源码版本。执行以下命令后，当前工作区原生构建通过，三个包回归共 74 项通过、1 项 Jev 外部依赖测试忽略。

```powershell
cargo clean -p circuitfabric-codex-runtime -p circuitfabric-document-opener
cargo build -p circuitfabric-desktop --features native-ui
cargo test -p circuitfabric-codex-runtime -p circuitfabric-desktop -p jlcircuit-eda-bridge
```

隔离工作副本验证必须使用该副本自己的默认 `target` 或独立的 `CARGO_TARGET_DIR`，不能指向用户工作区的 `target`。本次不改动其他任务的 PDF 文本选择/原文定位源码。技能/MCP 的原生记录与 Claude/DSH 尚未通过的链路见 [子功能验收说明](skills-mcp-management.md)；以下 2026-09-06 记录是历史验证，不能覆盖最新未完成项。

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
