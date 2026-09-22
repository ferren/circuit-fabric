# CircuitFabric 嘉立创 EDA 扩展

此扩展独立实现，不依赖或包含 `JLCircuit-Agent` 源码。安装后从 EDA 顶部菜单打开 **CircuitFabric**，自动连接本机 `ws://127.0.0.1:49630/bridge`，获取项目列表，再从下拉框选择项目（显示名称和 ID），无需手输 ID。

连接不依赖项目注册：列表为空也保持连接。在桌面端创建或打开项目后点击「刷新项目」，选中项目并收到确认后即可发送请求。切换项目会创建新会话；失效项目会显示具体原因，可刷新重选，无需断开连接。

协议顺序：`hello`（不带项目 ID）→ `hello_ack` → `list_projects` → `projects`（`id` / `name`）→ `select_project` → `project_selected` → `chat`。请同时更新 bridge 二进制和扩展包；旧版 bridge 不支持此流程。

先在桌面端保存 App Server 设置，再运行：

```powershell
cargo run -p jlcircuit-eda-bridge --bin circuitfabric-jlc-bridge
```

扩展包不保存 API Key；bridge 只接受 `127.x.x.x` 回环连接，Codex 认证仍由本机 `codex app-server` 处理。

v0.2.5-baseline 是已确认能在 EDA 中打开的对照包。v0.2.10 的 `index.html` 与该基线逐字节相同，只额外引用一个独立的 ES5 `compat.js`；即使项目选择脚本无法加载，基础窗口仍保持基线行为。连接成功后，项目按钮由普通原生 DOM 节点生成。打包器显式用 ZIP 规范的正斜杠路径（如 `iframe/index.html`），不依赖 Windows 的反斜杠路径。

入口源码位于 `src/index.js`；运行 `powershell -NoProfile -File scripts/package-jlc-extension.ps1` 会先生成 `dist/index.js` 再打包，避免依赖本机残留的构建文件。入口、兼容语法和连接逻辑可用 `node --test plugins/jlcircuit-eda-extension/tests/*.test.cjs` 验证；这些测试使用模拟宿主，不能替代 EDA 内的安装验收。
