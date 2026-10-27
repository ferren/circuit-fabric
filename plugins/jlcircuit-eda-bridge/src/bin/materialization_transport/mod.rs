use super::*;

pub(super) fn handle(
    stream: &TcpStream,
    state: &Arc<SharedState>,
    config: &Path,
    message: &Value,
    project: Option<&str>,
    inbound: &mpsc::Receiver<String>,
    backlog: &mut VecDeque<Value>,
) -> Result<(), String> {
    let operation = message["type"].as_str().unwrap_or("");
    let id = state.next_request_id();
    let outcome = (|| -> Result<Value, String> {
        let _gate = state.operation_gate.try_lock().map_err(|_| "另一个 EDA 操作仍在处理中。")?;
        let project = project.ok_or("请先选择项目。")?;
        let session = message["sessionId"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or("需要 sessionId。")?;
        let hash = message["planHash"].as_str().ok_or("需要 planHash。")?;
        let root = resolve_project_root(config, project)?;
        audit_session_note(
            config,
            project,
            session,
            &format!("{operation} 请求：planHash={hash}; requestId={id}"),
        )?;
        let lifecycle = OperationSession::start(&root, session)?;
        let result = (|| {
            send_json(
                stream,
                json!({"type":"operation_started","operation":operation,"requestId":id,
                "extensionVersion":state.executor_version()}),
            )?;
            let mut cancelled = false;
            jlcircuit_eda_bridge::materialization::execute(
                &root,
                project,
                session,
                hash,
                operation,
                |mut request| {
                    // Cancellation stops future mutations. An in-flight write is collected first.
                    collect_cancel(inbound, backlog, &id, &mut cancelled);
                    if cancelled
                        && matches!(
                            request["operation"].as_str(),
                            Some("apply" | "rollback" | "inspect")
                        )
                    {
                        return Err("cancelled".into());
                    }
                    let request_id = state.next_request_id();
                    let op = request["operation"].as_str().unwrap_or("").to_owned();
                    let request_kind = request["params"]["kind"].as_str().unwrap_or("").to_owned();
                    let status_probe = request["params"]["statusOnly"] == true;
                    request["type"] = json!("eda_operation");
                    request["requestId"] = json!(request_id);
                    request["protocolVersion"] = json!(operations::OPERATIONS_PROTOCOL_VERSION);
                    request["projectId"] = json!(project);
                    request["sessionId"] = json!(session);
                    request["scope"] = if op == "apply" || op == "rollback" {
                        json!(["project.read", "project.write"])
                    } else {
                        json!(["project.read"])
                    };
                    let executor =
                        state.lock_registry()?.stream.clone().ok_or("eda_executor_unavailable")?;
                    let extension_version = state.executor_version();
                    let (sender, receiver) = mpsc::channel();
                    state.lock_registry()?.pending.insert(request_id.clone(), sender);
                    let result = (|| {
                        send_json(&executor, request)?;
                        // 库件创建可能需要下载；属性/只读操作仍保留原来的超时。
                        let creating = matches!(op.as_str(), "apply" | "rollback")
                            && matches!(
                                (request_kind.as_str(), op.as_str()),
                                ("component_place", "apply") | ("component_delete", "rollback")
                            );
                        let deadline = Instant::now()
                            + if status_probe {
                                Duration::from_secs(5)
                            } else if creating {
                                Duration::from_secs(120)
                            } else {
                                OPERATION_TIMEOUT
                            };
                        let mut last_phase = "等待执行器响应".to_owned();
                        loop {
                            collect_cancel(inbound, backlog, &id, &mut cancelled);
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero() {
                                return Err(format!(
                                    "operation_timeout：operation={op}; kind={request_kind}; statusOnly={status_probe}; extensionVersion={extension_version:?}; phase={last_phase}；执行器结果未知"
                                ));
                            }
                            match receiver.recv_timeout(remaining.min(CANCEL_POLL_INTERVAL)) {
                                Ok(ExecutorEvent::Progress(mut event)) => {
                                    if let Some(phase) = event["phase"].as_str() {
                                        last_phase = phase.to_owned();
                                    }
                                    event["requestId"] = json!(id);
                                    send_json(stream, event)?;
                                }
                                Ok(ExecutorEvent::Result(mut reply)) => {
                                    if reply["operation"] != op || reply["requestId"] != request_id
                                    {
                                        return Err("执行器结果绑定不匹配。".into());
                                    }
                                    collect_cancel(inbound, backlog, &id, &mut cancelled);
                                    reply["cancelRequested"] = json!(cancelled);
                                    if reply["extensionVersion"].is_null() {
                                        reply["extensionVersion"] = json!(extension_version);
                                    }
                                    return Ok(reply);
                                }
                                Ok(ExecutorEvent::Error(error)) => return Err(error.to_string()),
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                Err(mpsc::RecvTimeoutError::Disconnected) => {
                                    return Err("eda_executor_unavailable".into());
                                }
                            }
                        }
                    })();
                    state.take_pending(&request_id);
                    result
                },
                now_seconds(),
            )
        })();
        lifecycle.finish(result.as_ref().is_ok_and(|reply| reply["ok"] == true))?;
        // Include the exact durable audit in the session so all three desktop views agree.
        if let Ok(reply) = &result {
            audit_session_note(config, project, session, &format!("{operation} 结果：{reply}"))?;
        } else if let Err(error) = &result {
            audit_session_note(config, project, session, &format!("{operation} 错误：{error}"))?;
        }
        result
    })();
    let mut reply = outcome.unwrap_or_else(|error| json!({"type":"operation_error","operation":operation,"code":"materialization_rejected","message":error,"inconclusive":["verification"]}));
    reply["requestId"] = json!(id);
    send_json(stream, reply)
}

// 操作员执行无需调用模型，但必须更新同一 EDA 会话的持久化运行状态。
struct OperationSession {
    storage: ProjectStorage,
    session: String,
}

impl OperationSession {
    fn start(root: &Path, session: &str) -> Result<Self, String> {
        let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
        let before = storage.load_session(session).map_err(|e| e.to_string())?.metadata.status;
        storage.resume_session(session).map_err(|e| e.to_string())?;
        append_event(
            &storage,
            session,
            SessionEventKind::StatusChanged { from: before, to: SessionStatus::Running },
        )?;
        Ok(Self { storage, session: session.to_owned() })
    }

    fn finish(self, succeeded: bool) -> Result<(), String> {
        let current = self.storage.load_session(&self.session).map_err(|e| e.to_string())?.metadata;
        let status = if succeeded { SessionStatus::Completed } else { SessionStatus::Failed };
        self.storage
            .complete_session(&self.session, current.usage, current.citations, status)
            .map_err(|e| e.to_string())?;
        append_event(
            &self.storage,
            &self.session,
            SessionEventKind::StatusChanged { from: current.status, to: status },
        )
    }
}

fn collect_cancel(
    inbound: &mpsc::Receiver<String>,
    backlog: &mut VecDeque<Value>,
    id: &str,
    cancelled: &mut bool,
) {
    while let Ok(raw) = inbound.try_recv() {
        if let Ok(message) = serde_json::from_str::<Value>(&raw) {
            if message["type"] == "cancel_operation" && message["requestId"] == id {
                *cancelled = true;
            } else {
                backlog.push_back(message);
            }
        }
    }
}
