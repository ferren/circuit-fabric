"use strict";
/* CircuitFabric JLC EDA extension entry point. No JLCircuit-Agent code is used. */
var edaEsbuildExportName = (() => {
function edaApi() {
  // JLC EDA supplies `eda` as a module-scoped global. Some versions additionally
  // expose it on globalThis, so retain that fallback for compatibility.
  return typeof eda !== "undefined" ? eda : globalThis.eda;
}

function notify(message) {
  try {
    const api = edaApi();
    if (api && api.sys_Message && api.sys_Message.showToastMessage) {
      api.sys_Message.showToastMessage(message, "info");
      return;
    }
    api?.sys_Dialog?.showInformationMessage?.(message, "CircuitFabric");
  } catch {
    // The original failure must not escape a menu callback and disappear silently.
  }
}

async function openCircuitFabricAssistant() {
  try {
    const api = edaApi();
    if (!api?.sys_IFrame?.openIFrame) {
      notify("当前嘉立创 EDA 运行时不提供 SYS_IFrame，无法打开 CircuitFabric 助手。");
      return;
    }
    const opened = await api.sys_IFrame.openIFrame(
      "/iframe/index.html",
      760,
      620,
      "circuitfabric-assistant",
      {
        title: "CircuitFabric 助手",
        maximizeButton: true,
        minimizeButton: true,
        minimizeStyle: "collapsed",
      },
    );
    if (opened === false) {
      notify("CircuitFabric 助手窗口打开失败：扩展包中缺少 iframe/index.html。");
    }
  } catch (error) {
    notify(`CircuitFabric 助手无法打开：${String(error)}`);
  }
}
return { openCircuitFabricAssistant };
})();
