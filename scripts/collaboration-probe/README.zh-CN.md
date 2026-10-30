# 原生协作能力探测

评论 9124 的专用工程增补：`eda-host/content.ts`、`archive.ts`、`dirty.ts` 均默认禁止写入；先只读核对 fixture UUID，再在一次性 artifact 副本中启用门禁。每次粘贴前须用窗口截图确认原生脚本编辑器确实打开，不能仅凭窗口标题。脚本可能打开多个原生导出对话框；`save_native_dialog.ps1 -WindowHandle <已核实句柄>` 可明确选择，禁止覆盖既有证据。导入未知结果不可盲目重跑。

复核新宿主证据：`python scripts/collaboration-probe/verify_content.py --dir artifacts/collaboration-584-ready`。包括两个内容标记在新 EDA 进程中的回读，以及未保存编辑被 closeDocument 丢弃的反例。后者不应解释为保存/保留/取消支持。归档导入当前被外部交互权限拒绝；Claude Z.ai 当前进程缺少认证注入。三者仍关闭阶段门禁。新脚本以同一类型包编译：`npm --prefix scripts/collaboration-probe/eda-host run build`。

探测依赖本机已安装的运行时与现有登录，不复制凭据。CLI 自身会写会话及配置缓存；在受限环境需允许该操作。结果保留实际版本、原始输出和退出码，脚本退出 0 只表示采集结束，不表示每项能力或阶段门禁通过。

## 运行时

```powershell
python scripts/collaboration-probe/model_probe.py --out artifacts/cf584-codex-image-resume
python scripts/collaboration-probe/codex_interaction_probe.py --out artifacts/cf584-codex-interaction
python scripts/collaboration-probe/verify_interaction.py artifacts/cf584-codex-interaction/rpc.jsonl --negative-controls
python scripts/collaboration-probe/other_runtime_probe.py --runtime claude --out artifacts/cf584-claude
python scripts/collaboration-probe/other_runtime_probe.py --runtime kimi --out artifacts/cf584-kimi
python scripts/collaboration-probe/other_runtime_probe.py --runtime dsh --out artifacts/cf584-dsh
```

Codex 图像/恢复探测发送确定性三色图及随机标记，第二个进程恢复原会话。交互探测收到真实原生问题后才生成回答，通过原 request ID 回传；另起一轮真实生成，收到文本 delta 后取消，必须读到同一 turn 的 interrupted。两个测试互相独立，App Server 的临时线程不冒充持久会话恢复。使用安装二进制生成的实验 schema 校对消息格式，并参考 [OpenAI App Server 官方文档](https://learn.chatgpt.com/docs/app-server)。原生协议通过不表示 CircuitFabric 或 EDA 的问答界面已接入。

Claude 使用 safe mode、空工具、strict MCP，只发测试文本；失败时保留认证错误，不自动登录、更换账号或读取保险库。Kimi 使用独立目录并要求模型不使用工具；采集原生事件，不宣称已验证工具隔离。Kimi 若输出 session.resume_hint，就按该会话 ID 在原 cwd 恢复，禁止选择用户的最近会话。只恢复已有探测结果、不重跑首轮时：

```powershell
python scripts/collaboration-probe/other_runtime_probe.py --runtime kimi --out artifacts/cf584-kimi-resume --resume-result artifacts/cf584-kimi/result.json
```

DSH 保留既有 Provider 配置，以临时 patch 禁用工程文件、shell、web、子智能体等工具；只验证 headless 单次模型调用。不能把没有恢复测试当成不支持原生恢复。

所有模型进程有超时并在前台收集；Windows 超时只回收本次启动的准确 PID 树。不要上传认证配置、保险库或完整用户会话；测试输出仍须检查敏感内容后才附到议题。

## EDA

见 `eda-host/README.zh-CN.md`。已有测试工程 `CF584-Lifecycle-20261030`，不要重跑创建。`readonly.ts` 只读，`lifecycle.ts` 默认禁止切换，`reopen.ts` 绑定既有工程 UUID；打开工程可能丢弃未保存编辑，必须先核实现场。原始生命周期记录可通过 `verify_lifecycle.py <证据目录>` 交叉校验。该校验不能代替重启、重新导入、多实例及未保存分支的现场测试。
