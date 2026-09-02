# CircuitFabric 嘉立创 EDA 扩展

此扩展独立实现，不依赖或包含 `JLCircuit-Agent` 源码。安装后从 EDA 顶部菜单打开 **CircuitFabric**，填写项目 ID，并连接到本机 `ws://127.0.0.1:49630/bridge`。

先在桌面端保存 App Server 设置，再运行：

```powershell
cargo run -p jlcircuit-eda-bridge --bin circuitfabric-jlc-bridge
```

扩展包不保存 API Key；bridge 只接受 `127.x.x.x` 回环连接，Codex 认证仍由本机 `codex app-server` 处理。
