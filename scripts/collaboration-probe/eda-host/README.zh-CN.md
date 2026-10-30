# EDA 原生阶段 1 探测

此目录仅用于现场能力探测，不实现后续工程管理 UI 或生产写入命令。

```powershell
npm ci --prefix scripts/collaboration-probe/eda-host
npm run build --prefix scripts/collaboration-probe/eda-host
```

编译产物在 `artifacts/cf584-native-script/`。在 EDA「高级 → 运行脚本」中导入或粘贴 `readonly.js`，执行后通过原生保存对话框导出 JSON。直接写工作区需要宿主外部交互权限，失败时会使用保存对话框，不更改权限设置。不要将 SDK 模拟结果替代此 JSON。

已核对的类型包为 `@jlceda/pro-api-types 0.4.27`，声明 SHA-256 `3e91ca9432bc6a640d5a7983c044ec63630ca94e5398744e17e0ac162674d5aa`，TypeScript `5.9.3`。2026-10-30 实测宿主报告 `4.1.78`，安装资源为 `pro-api/0.3.53.61b2011d`；npm 无同名 0.3.53 声明包。类型检查版本与宿主版本不同，所有 `@beta` 调用须以实际返回为准。

`readonly.ts` 读取当前工程树、所有原理图与图页、当前图页前后身份和编辑器树。不会创建、保存或修改设计。输出的同名 P1 图页有不同父原理图和 UUID，不能按名称合并。

`lifecycle.ts` 是待确认的独立工程测试脚本，**默认 `ALLOW_SWITCH_AFTER_SAVE=false`，在任何写入前停止**。SDK 的 `openProject` JSDoc 明确警告会直接丢弃原先工程所有未保存数据，未找到可核实的 dirty 查询，因此必须先由用户确认已保存并允许切换；不能把普通“已连接”视为此确认。只有取得此确认后才能修改门闩、重新编译并运行。目标工程为 `CF584-Lifecycle-20261030`，目标目录为当前仓库 `artifacts/cf584-native-projects`，初始工程 UUID 也受固定守卫保护。更换工作区时先审查并调整这两个路径和 UUID。

测试脚本记录创建/打开/图页创建/改名/保存/重开结果，最后导出原生工程和 JSON；不删除、不覆盖、不跨工程移动，不在未知写入结果后自动重放或切回。运行前检查是否已有同名探测工程，已有或结果未知时应回读协调，禁止直接重跑创建。未验证保存后的重启身份、编辑器同步、云端同步、未保存分支与原生检查点，不应标支持。

`run_native_script.ps1` 是针对本机已目视检查的 EDA 脚本编辑器布局的辅助输入工具，使用前须确认该编辑器已经打开且尺寸匹配。它会检查前台进程和窗口尺寸、暂存并恢复剪贴板；不适用于未知布局。
