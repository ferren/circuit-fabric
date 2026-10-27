(function () {
"use strict";
  var cfSocket = null;
  var cfReady = false;
  var cfProject = "";
  var cfPendingProject = "";
  var cfMessage = null;
  var cfBusy = false;
  var cfTimer = null;
  var cfStartedAt = 0;
  var cfModelRequests = 0;
  var cfSession = "compat-" + new Date().getTime() + "-" + Math.floor(Math.random() * 1000000);
  var projectSelect = document.getElementById("projectSelect");
  var sessionSelect = document.getElementById("sessionSelect");
  var bridgeInput = document.getElementById("bridge");
  var sendButton = document.getElementById("send");
  var refreshButton = document.getElementById("refresh");
  var inspectButton = document.getElementById("inspectBtn");
  var drcButton = document.getElementById("drcBtn");
  var captureButton = document.getElementById("captureBtn");
  var instruction = document.getElementById("instruction");
  var cfOperationKind = null;
  var cfOperationId = null;
  var cfReview = null;
  var cfReviewPending = false;

  sendButton.disabled = true;
  projectSelect.disabled = true;
  sessionSelect.disabled = true;
  inspectButton.disabled = true;
  drcButton.disabled = true;
  captureButton.disabled = true;

  function setConnection(kind, text) {
    document.getElementById("dot").className = "dot " + kind;
    document.getElementById("connection").textContent = text;
  }
  function resetOptions(select, label) {
    select.innerHTML = "";
    select.value = "";
    var option = document.createElement("option");
    option.value = "";
    option.textContent = label;
    select.appendChild(option);
  }
  function addOption(select, value, label) {
    var option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    select.appendChild(option);
  }
  function hasOption(select, value) {
    for (var i = 0; i < select.children.length; i += 1) {
      if (select.children[i].value === value) return true;
    }
    return false;
  }
  function setControls() {
    sendButton.disabled = !cfReady || !cfProject || cfBusy;
    projectSelect.disabled = !cfReady || cfBusy;
    sessionSelect.disabled = !cfReady || !cfProject || cfBusy;
    inspectButton.disabled = !cfReady || !cfProject || cfBusy;
    drcButton.disabled = !cfReady || !cfProject || cfBusy;
    captureButton.disabled = !cfReady || !cfProject || cfBusy;
    document.getElementById("clear").disabled = cfBusy;
    document.getElementById("approvePlan").disabled = !cfReady || cfBusy || cfReviewPending || !cfReview || !cfReview.approvalAllowed;
    document.getElementById("rejectPlan").disabled = !cfReady || cfBusy || cfReviewPending || !cfReview;
    document.getElementById("applyPlan").disabled = !cfReady || cfBusy || !cfReview || !cfReview.approved || !!cfReview.started;
    document.getElementById("readbackPlan").disabled = !cfReady || cfBusy || !cfReview || !cfReview.started;
    document.getElementById("rollbackPlan").disabled = !cfReady || cfBusy || !cfReview || !cfReview.rollbackHandle;
    document.getElementById("cancelPlan").disabled = !cfBusy || !cfOperationId;
  }
  function operationLabel(kind) {
    if (kind === "inspect") return "观测当前设计";
    if (kind === "drc") return "DRC 检查";
    if (kind === "visual_capture") return "画布截图";
    return kind || "EDA 操作";
  }
  function shortHash(value) {
    var text = String(value || "");
    if (text.indexOf("sha256:") === 0) text = text.slice("sha256:".length);
    return text.slice(0, 16);
  }
  function formatInconclusive(list) {
    if (!list || !list.length) return "无";
    return list.join("、");
  }
  function matchesOperation(message) {
    if (!message || message.requestId === undefined || message.requestId === null) return false;
    if (cfOperationId !== null) return message.requestId === cfOperationId;
    return cfBusy && cfOperationKind !== null;
  }
  function finishOperation(kind, failed, phaseText) {
    stopTimer();
    cfBusy = false;
    cfOperationKind = null;
    cfOperationId = null;
    setControls();
    document.getElementById("live").className = failed ? "live failed" : "live done";
    document.getElementById("liveState").textContent = failed ? "当前状态：执行失败" : "当前状态：执行完成";
    document.getElementById("livePhase").textContent = phaseText || (operationLabel(kind) + (failed ? "失败" : "已完成"));
  }
  function renderInspectResult(message) {
    var lines = [];
    lines.push("观测完成：元件 " + (message.components != null ? message.components : "?") + " 个 · 网络 " + (message.nets != null ? message.nets : "?") + " 个");
    lines.push("快照 " + shortHash(message.snapshotHash) + " · 逻辑 " + shortHash(message.logicalHash) + " · 物理 " + shortHash(message.physicalHash));
    lines.push("不确定面：" + formatInconclusive(message.inconclusive));
    var diagnostics = message.diagnostics || [], i, diagnostic;
    for (i = 0; i < diagnostics.length && i < 5; i += 1) {
      diagnostic = diagnostics[i] || {};
      lines.push("原因：" + (diagnostic.message || diagnostic.code || "采集不完整") +
        (diagnostic.count ? "（" + diagnostic.count + " 项）" : "") +
        (diagnostic.examples && diagnostic.examples.length ? " 例如：" + diagnostic.examples.join("、") : ""));
    }
    addMessage("assistant", lines.join("\n"));
    finishOperation("inspect", false, "观测已完成，快照由 Bridge 校验并持久化");
    if (message.inconclusive && message.inconclusive.length) {
      document.getElementById("live").className = "live uncertain";
      document.getElementById("liveState").textContent = "当前状态：观测不完整";
    }
  }
  function renderDrcResult(message) {
    var statusText = message.status === "passed" ? "通过" : message.status === "failed" ? "未通过" : "不确定（无法判定）";
    var lines = [];
    var errors = message.errors || [];
    var i, item;
    lines.push("DRC/ERC 检查：" + statusText);
    lines.push("问题 " + errors.length + " 条");
    if (message.status === "inconclusive" && !errors.length) lines.push("未取得完整的可解析报告，0 条不代表 DRC 通过。");
    for (i = 0; i < errors.length && i < 5; i += 1) {
      item = errors[i] || {};
      lines.push("- [" + (item.type || "?") + "] " + (item.rule || "?") + (item.net ? " · 网络 " + item.net : ""));
    }
    if (message.inconclusive && message.inconclusive.length) lines.push("不确定面：" + formatInconclusive(message.inconclusive));
    addMessage("assistant", lines.join("\n"));
    finishOperation("drc", false, "DRC/ERC 检查结束：" + statusText);
    if (message.status !== "passed") {
      document.getElementById("live").className = message.status === "failed" ? "live failed" : "live uncertain";
      document.getElementById("liveState").textContent = "当前状态：" + statusText;
    }
  }
  function renderPreview(message) {
    if (message.sessionId && message.sessionId !== cfSession) return;
    cfReview = message;
    cfReviewPending = false;
    var plan = message.plan || {};
    var lines = ["计划 " + shortHash(message.planHash), "基线 " + shortHash(message.baselineSnapshotHash)];
    var i, j, steps = plan.steps || [], diff = plan.diff || [], risks = plan.risks || [];
    for (i = 0; i < diff.length; i += 1) lines.push(diff[i].detail || diff[i].subject || "");
    for (i = 0; i < risks.length; i += 1) lines.push("风险：" + risks[i]);
    for (i = 0; i < steps.length; i += 1) {
      for (j = 0; steps[i].preconditions && j < steps[i].preconditions.length; j += 1) {
        if (!steps[i].preconditions[j].satisfied) lines.push("前置条件未满足：" + steps[i].preconditions[j].description);
      }
    }
    var warnings = plan.rollbackWarnings || [];
    var warningBox = document.getElementById("rollbackWarnings");
    var warningLines = ["【回滚提醒】以下删除可能无法自动恢复。批准即表示已知晓此风险。"];
    for (i = 0; i < warnings.length; i += 1) warningLines.push("· " + warnings[i]);
    warningBox.textContent = warnings.length ? warningLines.join("\n") : "";
    warningBox.style.display = warnings.length ? "" : "none";
    document.getElementById("approvePlan").textContent = warnings.length ? "已知晓回滚风险，批准计划（不执行）" : "批准计划（不执行）";
    lines.push("具备补偿计划：" + (!plan.rollbackable ? "否（见回滚提醒）" : warnings.length ? "部分（见回滚提醒）" : "是（实际恢复需回读验证）"));
    lines.push(message.started ? "此计划已执行；可回读或请求回滚。" : "写入：未执行；回读：未执行；设计验证：不确定。");
    lines.push(message.approvalAllowed ? "可以记录审批决定；批准后需单独执行。" : "前置条件未满足，不能批准。");
    document.getElementById("planDetails").textContent = lines.join("\n");
    document.getElementById("planReview").style.display = "";
    setControls();
  }
  function decidePlan(decision) {
    if (!cfReview || cfBusy || cfReviewPending || !cfReady || (decision === "approved" && !cfReview.approvalAllowed)) return;
    cfReviewPending = true;
    setControls();
    cfSocket.send(JSON.stringify({type:"approve_plan", sessionId:cfSession, planHash:cfReview.planHash,
      baselineSnapshotHash:cfReview.baselineSnapshotHash, decision:decision}));
  }
  function renderCaptureResult(message) {
    var evidence = message.evidence || {};
    var bytes = evidence.bytes != null ? evidence.bytes + " 字节" : "字节数未知";
    addMessage("assistant", "画布截图已采集\n证据 " + shortHash(evidence.contentHash) + " · " + (evidence.mime || "?") + " · " + bytes);
    finishOperation("visual_capture", false, "画布截图已保存为证据");
  }
  function renderOperationError(message) {
    var detail = message.message || "EDA 操作失败";
    addMessage("error", "EDA 操作失败：" + detail);
    finishOperation(message.operation, true, detail);
  }
  function startOperation(kind) {
    if (cfBusy || !cfReady || !cfProject || !cfSocket || cfSocket.readyState !== 1) return;
    cfBusy = true;
    cfOperationKind = kind;
    cfOperationId = null;
    cfStartedAt = new Date().getTime();
    updateElapsed();
    cfTimer = setInterval(updateElapsed, 100);
    setControls();
    document.getElementById("live").className = "live running";
    document.getElementById("liveState").textContent = "当前状态：执行中";
    document.getElementById("livePhase").textContent = operationLabel(kind) + "：等待执行器响应…";
    var request = {type:kind, sessionId:cfSession, document:{kind:"schematic"}};
    if (kind === "apply" || kind === "readback" || kind === "rollback") {
      if (!cfReview) { finishOperation(kind, true, "缺少计划"); return; }
      request.planHash = cfReview.planHash;
      if (kind === "apply") cfReview.started = true;
    }
    cfSocket.send(JSON.stringify(request));
  }
  function addMessage(role, text) {
    var welcome = document.getElementById("welcome");
    var conversation = document.getElementById("conversation");
    var row = document.createElement("article");
    var bubble = document.createElement("div");
    welcome.style.display = "none";
    row.className = "message " + role;
    bubble.className = "bubble";
    bubble.textContent = text;
    row.appendChild(bubble);
    conversation.appendChild(row);
    conversation.scrollTop = conversation.scrollHeight;
    return bubble;
  }
  function showError(text) {
    stopTimer();
    addMessage("error", text);
    document.getElementById("live").className = "live failed";
    document.getElementById("liveState").textContent = "当前状态：执行失败";
    document.getElementById("livePhase").textContent = text;
  }
  function updateElapsed() {
    document.getElementById("elapsed").textContent = ((new Date().getTime() - cfStartedAt) / 1000).toFixed(1) + "s";
  }
  function stopTimer() {
    if (cfTimer !== null) {
      clearInterval(cfTimer);
      cfTimer = null;
      updateElapsed();
    }
  }
  function resetSelection() {
    cfReview = null;
    cfReviewPending = false;
    document.getElementById("planReview").style.display = "none";
    cfProject = "";
    cfPendingProject = "";
    resetOptions(projectSelect, "连接后选择项目");
    resetOptions(sessionSelect, "新会话");
    setControls();
  }
  function requestProjects() {
    if (!cfSocket || cfSocket.readyState !== 1) return;
    cfSocket.send(JSON.stringify({type:"list_projects"}));
  }
  function pushBridgeAddress() {
    try {
      if (typeof eda !== "undefined" && eda && eda.sys_MessageBus && typeof eda.sys_MessageBus.push === "function") {
        eda.sys_MessageBus.push("circuitfabric.bridge", bridgeInput.value);
      }
    } catch (ignored) {
      // iframe 与扩展入口之间的 MessageBus 互通未经官方背书，失败时入口使用默认地址。
    }
  }
  function chooseProject(id) {
    if (cfBusy || !cfSocket || cfSocket.readyState !== 1 || !id) return;
    cfProject = "";
    cfPendingProject = id;
    resetOptions(sessionSelect, "新会话");
    setControls();
    cfSocket.send(JSON.stringify({type:"select_project",projectId:id}));
  }
  function newSession() {
    if (cfBusy) return;
    cfSession = "compat-" + new Date().getTime() + "-" + Math.floor(Math.random() * 1000000);
    cfMessage = null;
    cfModelRequests = 0;
    cfOperationKind = null;
    cfOperationId = null;
    cfReview = null;
    cfReviewPending = false;
    document.getElementById("planReview").style.display = "none";
    document.getElementById("live").className = "live";
    document.getElementById("liveState").textContent = "当前状态：就绪";
    document.getElementById("livePhase").textContent = "已创建新的本地会话";
    document.getElementById("elapsed").textContent = "--";
    document.getElementById("metrics").textContent = "模型请求 0 · Token 尚无统计";
    document.getElementById("session").textContent = "当前会话：" + cfSession;
    var conversation = document.getElementById("conversation");
    var welcome = document.getElementById("welcome");
    conversation.innerHTML = "";
    conversation.appendChild(welcome);
    welcome.style.display = "";
  }
  function requestSessions() {
    if (cfReady && cfProject && !cfBusy) cfSocket.send(JSON.stringify({type:"list_sessions"}));
  }
  function sessionLabel(session) {
    var shortId = String(session.sessionId || "").slice(0, 12);
    if (session.title) return session.title + " · " + shortId;
    return session.startedAt + " · " + session.status + " · " + shortId;
  }
  function renderSessions(sessions) {
    var i;
    resetOptions(sessionSelect, "新会话");
    for (i = 0; sessions && i < sessions.length; i += 1) {
      if (!sessions[i] || !sessions[i].sessionId) continue;
      addOption(sessionSelect, sessions[i].sessionId, sessionLabel(sessions[i]));
    }
    sessionSelect.value = hasOption(sessionSelect, cfSession) ? cfSession : "";
  }
  function renderProjects(projects) {
    var i, project;
    resetOptions(projectSelect, (projects && projects.length) ? "选择项目" : "暂无已注册项目");
    for (i = 0; projects && i < projects.length; i += 1) {
      project = projects[i];
      if (!project || !project.id) continue;
      addOption(projectSelect, project.id, (project.name || project.id) + " (" + project.id + ")");
    }
    // A refresh (e.g. opening the dropdown) must not drop the current choice.
    projectSelect.value = cfProject || cfPendingProject;
  }
  function connect() {
    var ws;
    if (cfSocket && (cfSocket.readyState === 0 || cfSocket.readyState === 1)) return;
    setConnection("", "正在连接");
    try {
      ws = new WebSocket(bridgeInput.value);
    } catch (error) {
      setConnection("error", "Bridge 不可用");
      showError("无法创建 bridge 连接：" + String(error));
      return;
    }
    cfSocket = ws;
    ws.onopen = function () {
      if (cfSocket !== ws) return;
      ws.send(JSON.stringify({type:"hello",protocolVersion:1}));
    };
    ws.onmessage = function (event) {
      var message, detail;
      if (cfSocket !== ws) return;
      try { message = JSON.parse(event.data); } catch (ignored) { return; }
      if (message.type === "hello_ack") {
        cfReady = true;
        setConnection("connected", "Bridge 已连接");
        pushBridgeAddress();
        setControls();
        requestProjects();
      } else if (message.type === "projects") {
        renderProjects(message.projects);
      } else if (message.type === "project_selected") {
        cfProject = message.projectId || "";
        cfPendingProject = "";
        if (hasOption(projectSelect, cfProject)) projectSelect.value = cfProject;
        setControls();
        newSession();
        resetOptions(sessionSelect, "新会话");
        requestSessions();
      } else if (message.type === "sessions" && message.projectId === cfProject) {
        renderSessions(message.sessions);
      } else if (message.type === "session_loaded" && message.projectId === cfProject) {
        if (cfBusy) return;
        newSession();
        cfSession = message.sessionId;
        if (hasOption(sessionSelect, cfSession)) sessionSelect.value = cfSession;
        document.getElementById("session").textContent = "当前会话：" + cfSession;
        addMessage("assistant", message.body || "此会话尚无已保存消息。");
        if (message.review) renderPreview(message.review);
        document.getElementById("livePhase").textContent = "历史已加载；输入消息后继续此会话";
      } else if (message.type === "chat_started") {
        document.getElementById("livePhase").textContent = "Codex 正在处理请求…";
      } else if (message.type === "chat_delta") {
        if (!cfMessage) cfMessage = addMessage("assistant", "");
        cfMessage.textContent += message.delta || "";
      } else if (message.type === "chat_completed") {
        stopTimer();
        cfBusy = false;
        setControls();
        document.getElementById("live").className = "live done";
        document.getElementById("liveState").textContent = "当前状态：执行完成";
        document.getElementById("livePhase").textContent = "最近一次请求已完成";
        cfMessage = null;
        requestSessions();
      } else if (message.type === "operation_started") {
        if (cfBusy && cfOperationKind === message.operation) {
          cfOperationId = message.requestId || null;
          setControls();
          document.getElementById("livePhase").textContent = operationLabel(message.operation) + "：已开始";
          if (message.operation === "apply" || message.operation === "readback" || message.operation === "rollback") {
            addMessage("assistant", operationLabel(message.operation) + "，执行器版本：" + (message.extensionVersion || "未提供"));
          }
        }
      } else if (message.type === "operation_progress") {
        if (matchesOperation(message) && message.phase) {
          document.getElementById("livePhase").textContent = operationLabel(message.operation) + "：" + message.phase;
        }
      } else if (message.type === "inspect_result") {
        if (matchesOperation(message)) renderInspectResult(message);
      } else if (message.type === "drc_result") {
        if (matchesOperation(message)) renderDrcResult(message);
      } else if (message.type === "read_context_result") {
        if (matchesOperation(message)) {
          var context = message.context || {};
          addMessage("assistant", "当前上下文\n工程：" + (context.projectName || "未知") + "\n页面：" + (context.pageName || "未知") + "\n不确定面：" + formatInconclusive(message.inconclusive));
          finishOperation("read_context", false, "上下文已读取");
        }
      } else if (message.type === "visual_capture_result") {
        if (matchesOperation(message)) renderCaptureResult(message);
      } else if (message.type === "preview_result") {
        renderPreview(message);
      } else if (message.type === "approval_result" && cfReview && message.planHash === cfReview.planHash) {
        cfReviewPending = false;
        cfReview.approvalAllowed = false;
        cfReview.approved = message.decision === "approved";
        addMessage("assistant", message.decision === "approved" ? "计划审批已记录。EDA 写入尚未执行。" : "计划已拒绝。");
        setControls();
      } else if (message.type === "apply_result" || message.type === "readback_result" || message.type === "rollback_result") {
        if (matchesOperation(message) && cfReview && message.planHash === cfReview.planHash) {
          cfReview.rollbackHandle = message.rollbackHandle;
          var stages = message.execution || {};
          var summary = "write=" + (stages.write || {}).status + " · readback=" + (stages.readback || {}).status + " · verification=" + (stages.verification || {}).status;
          document.getElementById("planDetails").textContent += "\n" + summary;
          addMessage("assistant", summary);
          if (message.failure) {
            addMessage("error", "操作未完成：" + message.failure);
            document.getElementById("planDetails").textContent += "\n原因：" + message.failure;
          }
          finishOperation(message.operation, false, summary);
          if (!message.ok) {
            document.getElementById("live").className = "live uncertain";
            document.getElementById("liveState").textContent = "当前状态：不确定或失败，请查看三项独立状态";
          }
        }
      } else if (message.type === "proposal_error") {
        addMessage("error", "候选补丁未通过预览：" + message.message);
      } else if (message.type === "operation_error") {
        if (message.operation === "approve_plan" || message.operation === "preview") {
          cfReviewPending = false;
          addMessage("error", message.message || "计划请求失败");
          setControls();
        } else if (matchesOperation(message)) renderOperationError(message);
      } else if (message.type === "error") {
        cfBusy = false;
        detail = message.message || "Bridge 请求失败";
        if (message.requestType === "select_project") {
          cfProject = "";
          cfPendingProject = "";
          projectSelect.value = "";
        }
        setControls();
        showError(detail);
        cfMessage = null;
      }
    };
    ws.onerror = function () {
      if (cfSocket === ws) {
        if (cfBusy) showError("Bridge 连接出错，请重新连接后重试。");
        cfBusy = false;
        cfReady = false;
        resetSelection();
        setConnection("error", "Bridge 不可用");
      }
    };
    ws.onclose = function () {
      if (cfSocket === ws) {
        if (cfBusy) showError("Bridge 连接已断开，请重新连接后重试。");
        cfReady = false;
        cfBusy = false;
        resetSelection();
        setConnection("", "已断开");
      }
    };
  }
  function reconnect() {
    if (cfBusy) showError("已重新连接；如需继续请求，请选择项目后重试。");
    cfBusy = false;
    cfMessage = null;
    cfOperationKind = null;
    cfOperationId = null;
    pushBridgeAddress();
    if (cfSocket) { try { cfSocket.close(); } catch (ignored) {} }
    cfSocket = null;
    cfReady = false;
    resetSelection();
    connect();
  }
  function send() {
    var text = instruction.value.replace(/^\s+|\s+$/g, "");
    if (!text || cfBusy || !cfReady || !cfProject || !cfSocket || cfSocket.readyState !== 1) return;
    cfBusy = true;
    cfMessage = null;
    cfStartedAt = new Date().getTime();
    cfModelRequests += 1;
    updateElapsed();
    cfTimer = setInterval(updateElapsed, 100);
    document.getElementById("metrics").textContent = "模型请求 " + cfModelRequests + " · Token 由 Codex App Server 统计";
    setControls();
    document.getElementById("live").className = "live running";
    document.getElementById("liveState").textContent = "当前状态：执行中";
    document.getElementById("livePhase").textContent = "Codex 正在处理请求…";
    addMessage("user", text);
    instruction.value = "";
    cfSocket.send(JSON.stringify({type:"chat",sessionId:cfSession,text:text}));
  }
  projectSelect.onmousedown = function () {
    if (cfReady && !cfBusy) requestProjects();
  };
  projectSelect.onchange = function () {
    chooseProject(projectSelect.value);
  };
  sessionSelect.onchange = function () {
    var id = sessionSelect.value;
    if (cfBusy) return;
    if (!id) {
      newSession();
      return;
    }
    if (cfSocket && cfSocket.readyState === 1) {
      cfSocket.send(JSON.stringify({type:"load_session",sessionId:id}));
    }
  };
  document.getElementById("clear").onclick = newSession;
  refreshButton.onclick = reconnect;
  sendButton.onclick = send;
  inspectButton.onclick = function () { startOperation("inspect"); };
  drcButton.onclick = function () { startOperation("drc"); };
  captureButton.onclick = function () { startOperation("visual_capture"); };
  document.getElementById("approvePlan").onclick = function () { decidePlan("approved"); };
  document.getElementById("rejectPlan").onclick = function () { decidePlan("rejected"); };
  document.getElementById("applyPlan").onclick = function () { startOperation("apply"); };
  document.getElementById("readbackPlan").onclick = function () { startOperation("readback"); };
  document.getElementById("rollbackPlan").onclick = function () { startOperation("rollback"); };
  document.getElementById("cancelPlan").onclick = function () {
    if (cfSocket && cfOperationId) cfSocket.send(JSON.stringify({type:"cancel_operation", requestId:cfOperationId}));
  };
  bridgeInput.onchange = reconnect;
  instruction.onkeydown = function (event) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing && event.keyCode !== 229) {
      event.preventDefault();
      send();
    }
  };
  resetOptions(projectSelect, "连接后选择项目");
  resetOptions(sessionSelect, "新会话");
  newSession();
  setTimeout(connect, 100);
}());
