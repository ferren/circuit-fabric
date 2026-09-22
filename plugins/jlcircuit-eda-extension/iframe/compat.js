(function () {
"use strict";
try {
  var cfSocket = null;
  var cfReady = false;
  var cfProject = "";
  var cfMessage = null;
  var cfSession = "compat-" + new Date().getTime() + "-" + Math.floor(Math.random() * 1000000);
  var projectInput = document.getElementById("project");
  var bridgeInput = document.getElementById("bridge");
  var sendButton = document.getElementById("send");
  var refreshButton = document.getElementById("refresh");
  var instruction = document.getElementById("instruction");
  var settings = projectInput.parentNode;
  var projectList = document.createElement("div");
  var projectHint = document.createElement("p");
  var listButton = document.createElement("button");

  projectList.id = "circuitfabric-project-list";
  projectList.style.margin = "6px 0";
  projectHint.id = "circuitfabric-project-hint";
  projectHint.className = "hint";
  projectHint.textContent = "正在连接 bridge，连接后显示项目列表。";
  listButton.type = "button";
  listButton.className = "secondary";
  listButton.textContent = "刷新项目";
  projectInput.value = "";
  projectInput.readOnly = true;
  projectInput.placeholder = "连接后选择项目";
  sendButton.disabled = true;
  settings.appendChild(listButton);
  settings.appendChild(projectList);
  settings.appendChild(projectHint);

  function setConnection(kind, text) {
    document.getElementById("dot").className = "dot " + kind;
    document.getElementById("connection").textContent = text;
  }
  function setControls() {
    sendButton.disabled = !cfReady || !cfProject;
    listButton.disabled = !cfReady;
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
    addMessage("error", text);
    document.getElementById("live").className = "live failed";
    document.getElementById("liveState").textContent = "当前状态：执行失败";
    document.getElementById("livePhase").textContent = text;
  }
  function clearProjects() {
    projectList.innerHTML = "";
    cfProject = "";
    projectInput.value = "";
    setControls();
  }
  function requestProjects() {
    if (!cfSocket || cfSocket.readyState !== 1) return;
    clearProjects();
    projectHint.textContent = "正在获取项目列表…";
    cfSocket.send(JSON.stringify({type:"list_projects"}));
  }
  function chooseProject(id, label) {
    if (!cfSocket || cfSocket.readyState !== 1) return;
    cfProject = "";
    projectInput.value = label;
    projectHint.textContent = "正在选择项目…";
    setControls();
    cfSocket.send(JSON.stringify({type:"select_project",projectId:id}));
  }
  function renderProjects(projects) {
    var i, project, label, button;
    clearProjects();
    if (!projects || !projects.length) {
      projectHint.textContent = "暂无已注册项目；请在 CircuitFabric 创建或打开项目后刷新列表。";
      return;
    }
    for (i = 0; i < projects.length; i += 1) {
      project = projects[i];
      if (!project || !project.id) continue;
      label = (project.name || project.id) + " (" + project.id + ")";
      button = document.createElement("button");
      button.type = "button";
      button.className = "secondary";
      button.style.margin = "0 6px 6px 0";
      button.textContent = label;
      button.onclick = (function (id, text) {
        return function () { chooseProject(id, text); };
      }(project.id, label));
      projectList.appendChild(button);
    }
    projectHint.textContent = "请选择一个项目后发送请求。";
  }
  function disconnectOldPageSocket() {
    if (typeof socket !== "undefined" && socket) {
      try { socket.close(); } catch (ignored) {}
    }
  }
  function connect() {
    var ws;
    if (cfSocket && (cfSocket.readyState === 0 || cfSocket.readyState === 1)) return;
    setConnection("", "正在连接");
    try {
      ws = new WebSocket(bridgeInput.value);
    } catch (error) {
      setConnection("error", "Bridge 不可用");
      projectHint.textContent = "无法创建 bridge 连接：" + String(error);
      return;
    }
    cfSocket = ws;
    ws.onopen = function () {
      ws.send(JSON.stringify({type:"hello",protocolVersion:1}));
    };
    ws.onmessage = function (event) {
      var message, detail;
      try { message = JSON.parse(event.data); } catch (ignored) { return; }
      if (message.type === "hello_ack") {
        cfReady = true;
        setConnection("connected", "Bridge 已连接");
        setControls();
        requestProjects();
      } else if (message.type === "projects") {
        renderProjects(message.projects);
      } else if (message.type === "project_selected") {
        cfProject = message.projectId || "";
        projectInput.value = cfProject;
        projectHint.textContent = "已选择项目：" + cfProject;
        setControls();
      } else if (message.type === "chat_started") {
        document.getElementById("livePhase").textContent = "Codex 正在处理请求…";
      } else if (message.type === "chat_delta") {
        if (!cfMessage) cfMessage = addMessage("assistant", "");
        cfMessage.textContent += message.delta || "";
      } else if (message.type === "chat_completed") {
        document.getElementById("live").className = "live done";
        document.getElementById("liveState").textContent = "当前状态：执行完成";
        document.getElementById("livePhase").textContent = "最近一次请求已完成";
        cfMessage = null;
      } else if (message.type === "error") {
        detail = message.message || "Bridge 请求失败";
        if (message.requestType === "select_project") {
          cfProject = "";
          projectInput.value = "";
          setControls();
        }
        projectHint.textContent = detail;
        showError(detail);
        cfMessage = null;
      }
    };
    ws.onerror = function () {
      if (cfSocket === ws) {
        cfReady = false;
        cfProject = "";
        setControls();
        setConnection("error", "Bridge 不可用");
        projectHint.textContent = "无法连接 bridge；请检查 EDA 服务。";
      }
    };
    ws.onclose = function () {
      if (cfSocket === ws) {
        cfReady = false;
        cfProject = "";
        setControls();
        setConnection("", "已断开");
      }
    };
  }
  function reconnect() {
    if (cfSocket) { try { cfSocket.close(); } catch (ignored) {} }
    cfSocket = null;
    cfReady = false;
    clearProjects();
    connect();
  }
  function send() {
    var text = instruction.value.replace(/^\s+|\s+$/g, "");
    if (!text || !cfReady || !cfProject || !cfSocket || cfSocket.readyState !== 1) return;
    document.getElementById("live").className = "live running";
    document.getElementById("liveState").textContent = "当前状态：执行中";
    document.getElementById("livePhase").textContent = "Codex 正在处理请求…";
    addMessage("user", text);
    instruction.value = "";
    cfSocket.send(JSON.stringify({type:"chat",sessionId:cfSession,text:text}));
  }
  listButton.onclick = requestProjects;
  refreshButton.onclick = reconnect;
  sendButton.onclick = send;
  bridgeInput.onchange = reconnect;
  instruction.onkeydown = function (event) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  };
  disconnectOldPageSocket();
  setTimeout(connect, 100);
} catch (error) {
  /* The v0.2.5 page remains usable if optional project selection cannot load. */
}
}());