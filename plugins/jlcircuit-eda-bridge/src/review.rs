//! 受控预览与审批：不连接执行器、不修改 EDA，审批绝不表示写入或回读成功。

use std::path::Path;

use circuitfabric_contracts::{FactStatus, IrPatch, LogicalCircuitSnapshot, PatchOperation};
use circuitfabric_project::{
    ChangeSetAuditEntry, ChangeSetStage, ChangeSetStageStatus, ProjectStorage, StoredChangeSet,
};
use serde_json::{Value, json};

use crate::{approval, fusion, observation, plan};

/// 在文件名中使用外部哈希前必须验证，避免把消息中的哈希解释为路径。
pub fn valid_hash(hash: &str) -> bool {
    hash.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// 支持的候选补丁操作类型；解析错误时向模型给出可自我纠正的提示。
const SUPPORTED_OPERATIONS: [&str; 6] = [
    "set_component_value",
    "rename_net",
    "add_component",
    "remove_component",
    "add_net",
    "annotate_semantic",
];

/// 解析候选补丁 JSON（文本或已解析值）。逐操作定位错误：说明操作序号、
/// 类型与缺失字段，不透出行列号形式的原始 serde 报错。
pub(crate) fn parse_patch(candidate: Value) -> Result<IrPatch, String> {
    let operations = candidate
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("候选补丁缺少 operations 数组。")?;
    let mut parsed = Vec::with_capacity(operations.len());
    for (index, operation) in operations.iter().enumerate() {
        let Some(fields) = operation.as_object() else {
            return Err(format!("operations[{index}] 必须是单键操作对象。"));
        };
        if fields.len() != 1 {
            return Err(format!(
                "operations[{index}] 必须是单键操作对象，实际含 {} 个键。",
                fields.len()
            ));
        }
        let kind = fields.keys().next().expect("已校验单键");
        if !SUPPORTED_OPERATIONS.contains(&kind.as_str()) {
            return Err(format!(
                "operations[{index}]：不支持的操作类型 {kind}，支持 {}。",
                SUPPORTED_OPERATIONS.join(" / ")
            ));
        }
        let operation: PatchOperation = serde_json::from_value(operation.clone())
            .map_err(|e| format!("operations[{index}]（{kind}）无效：{e}"))?;
        parsed.push(operation);
    }
    Ok(IrPatch {
        base_snapshot_hash: candidate
            .get("base_snapshot_hash")
            .and_then(Value::as_str)
            .ok_or("候选补丁缺少 base_snapshot_hash。")?
            .to_owned(),
        operations: parsed,
        rationale: candidate
            .get("rationale")
            .and_then(Value::as_str)
            .ok_or("候选补丁缺少 rationale。")?
            .to_owned(),
    })
}

/// 文本补丁代码块标记。模型的唯一提交通道是 `agent_tools` 的三个受控工具；
/// 此标记仅用于识别模型误用的文本补丁并给出纠正提示，不再具有提交效力。
pub const PATCH_FENCE_MARKER: &str = "```circuitfabric-patch";

/// 解析文本中的候选补丁块（供测试与误用检测；工具面才是有效提交通道）。
pub fn candidate_from_text(text: &str) -> Result<Option<IrPatch>, String> {
    let marker = PATCH_FENCE_MARKER;
    let Some((_, rest)) = text.split_once(marker) else {
        return Ok(None);
    };
    let (body, tail) = rest.split_once("```").ok_or("候选补丁代码块未闭合。")?;
    if tail.contains(marker) {
        return Err("一次回复只能提交一个候选补丁。".into());
    }
    let candidate: Value =
        serde_json::from_str(body.trim()).map_err(|e| format!("候选补丁不是有效 JSON：{e}"))?;
    parse_patch(candidate).map(Some)
}

/// 会话上下文只含语义快照和文档绑定，不把外部图元映射/API 传给模型。
pub fn design_context(root: &Path, project_id: &str) -> Result<Option<Value>, String> {
    let Some(hash) = observation::latest_inspect_snapshot_hash(root).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let record = load_baseline(root, &hash)?;
    if record["projectId"] != project_id {
        return Err("观测不属于会话项目。".into());
    }
    let diagnostics = record["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| observation::component_diagnostics(&record["observedPayload"]));
    let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
    let mut changes = storage.list_change_sets().map_err(|e| e.to_string())?;
    changes
        .sort_by_key(|r| std::cmp::Reverse(r.audit.last().map_or(0, |e| e.timestamp_unix_seconds)));
    let changes: Vec<Value> = changes.iter().filter(|r| r.id.starts_with("jlc-")).take(5).map(|r| {
        let error = r.audit.iter().rev().find(|e| e.decision.ends_with("_error"));
        json!({"changeSetId":r.id,"planHash":r.plan_hash,"execution":r.execution,
            "rollbackHandle":r.rollback_handle,"lastError":error,
            "rollbackNote":"按原计划补偿，不依赖 parent_snapshot_hash；rollbackable 仅表示计划有补偿数据，不保证已完整恢复。以执行审计和新鲜回读为准，不猜测失败原因。"})
    }).collect();
    Ok(Some(json!({"baselineSnapshotHash": hash, "snapshot": record["snapshot"],
        "inconclusive": record["inconclusive"], "validation": record["validation"],
        "diagnostics": diagnostics, "capturedAt":record["capturedAt"],"changeSets":changes,
        "observationSource":"persisted",
        "observationNote":"这是最近已保存的观测，不是新调用 EDA；以 diagnostics/validation 解释不完整原因，不能从元件 evidence 为空推断采集失败。需要新采集时请点击扩展中的观测当前设计。"})))
}

/// 恢复历史会话最近的已执行计划，仅开放回读/回滚，不恢复写入授权。
pub fn latest_session_review(
    root: &Path,
    project: &str,
    session: &str,
) -> Result<Option<Value>, String> {
    let mut records =
        ProjectStorage::open(root).and_then(|s| s.list_change_sets()).map_err(|e| e.to_string())?;
    records
        .sort_by_key(|r| std::cmp::Reverse(r.audit.last().map_or(0, |e| e.timestamp_unix_seconds)));
    for record in records {
        if !record.id.starts_with("jlc-") || record.rollback_handle.is_none() {
            continue;
        }
        let Some(plan) = plan::load_plan(root, &record.plan_hash).map_err(|e| e.to_string())?
        else {
            continue;
        };
        if plan.project_id != project || plan.session_id != session {
            continue;
        }
        return Ok(Some(
            json!({"type":"preview_result","sessionId":session,"planHash":record.plan_hash,
            "baselineSnapshotHash":record.base_snapshot_hash,"changeSetId":record.id,
            "plan":plan,"approvalAllowed":false,"approved":false,"started":true,
            "rollbackHandle":record.rollback_handle,"execution":record.execution}),
        ));
    }
    Ok(None)
}

pub fn load_baseline(root: &Path, hash: &str) -> Result<Value, String> {
    if !valid_hash(hash) {
        return Err("无效的 baselineSnapshotHash。".into());
    }
    let path = observation::observations_dir(root).join(format!("{}.json", &hash[7..]));
    let record: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let snapshot: LogicalCircuitSnapshot =
        serde_json::from_value(record["snapshot"].clone()).map_err(|e| e.to_string())?;
    if record["kind"] != "inspect"
        || snapshot.snapshot_hash != hash
        || circuitfabric_core::stamp_snapshot(&snapshot).map_err(|e| e.to_string())?.snapshot_hash
            != hash
    {
        return Err("观测记录内容与基线哈希不匹配。".into());
    }
    Ok(record)
}

fn observation_complete(record: &Value) -> bool {
    record["inconclusive"].as_array().is_some_and(Vec::is_empty)
        && record["validation"].as_array().is_some_and(|items| {
            !items.is_empty() && items.iter().all(|item| item["status"] == "passed")
        })
}

pub(crate) fn bound_plan(
    root: &Path,
    project_id: &str,
    session_id: &str,
    hash: &str,
) -> Result<plan::MaterializationPlan, String> {
    if !valid_hash(hash) {
        return Err("无效的 planHash。".into());
    }
    let plan = plan::load_plan(root, hash).map_err(|e| e.to_string())?.ok_or("计划不存在。")?;
    if plan.plan_hash != hash
        || plan::plan_digest(&plan) != hash
        || plan.schema_version != 1
        || plan.backend != plan::PLAN_BACKEND
        || plan.project_id != project_id
        || plan.session_id != session_id
        || plan.steps.is_empty()
    {
        return Err("计划哈希、版本、后端或项目/会话绑定不匹配。".into());
    }
    Ok(plan)
}

pub(crate) fn bound_change_set(
    root: &Path,
    plan: &plan::MaterializationPlan,
) -> Result<StoredChangeSet, String> {
    let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
    if storage.manifest().project.id != plan.project_id {
        return Err("计划与项目存储不匹配。".into());
    }
    let id = format!("jlc-{}", &plan.plan_hash[7..]);
    let record = storage
        .list_change_sets()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or("计划缺少 ChangeSet 审计记录。")?;
    if record.plan_hash != plan.plan_hash
        || record.base_snapshot_hash != plan.baseline_snapshot_hash
    {
        return Err("ChangeSet 的计划或基线绑定不匹配。".into());
    }
    Ok(record)
}

/// apply 的只读准入校验，供子任务 D 在写入前、持有操作锁时调用。
/// `current_snapshot_hash` 必须来自执行器刚完成的 inspect 并已持久化，
/// 不能使用请求方声明的哈希。返回计划只表示准入通过，不代表任何写入成功。
///
/// # Errors
///
/// 无显式批准、绑定不匹配、基线/身份漂移、观测不完整或已开始执行时拒绝。
pub fn authorize_apply(
    root: &Path,
    project_id: &str,
    session_id: &str,
    plan_hash: &str,
    current_snapshot_hash: &str,
) -> Result<plan::MaterializationPlan, String> {
    let plan = bound_plan(root, project_id, session_id, plan_hash)?;
    let current = observation::latest_inspect_snapshot_hash(root).map_err(|e| e.to_string())?;
    if current_snapshot_hash != plan.baseline_snapshot_hash
        || current.as_deref() != Some(current_snapshot_hash)
    {
        return Err("基线已漂移，拒绝执行；请重新 inspect、preview 并审批。".into());
    }
    let baseline = load_baseline(root, current_snapshot_hash)?;
    let document = baseline.pointer("/observedPayload/document").unwrap_or(&baseline["document"]);
    if baseline["projectId"] != project_id
        || document != &plan.document
        || !observation_complete(&baseline)
        || !plan.preconditions_satisfied()
    {
        return Err("当前观测、目标文档或计划前置条件不满足，拒绝执行。".into());
    }
    let record = bound_change_set(root, &plan)?;
    // ChangeSet 是桌面端与 Bridge 共用的审批事实源；旁路 approval 文件不能覆盖最新拒绝。
    if !record.approval_allowed(Some(current_snapshot_hash))
        || !record
            .audit
            .iter()
            .rev()
            .find(|entry| matches!(entry.decision.as_str(), "approved" | "rejected" | "cancelled"))
            .is_some_and(|entry| {
                entry.decision == "approved"
                    && entry.observed_snapshot_hash.as_deref() == Some(current_snapshot_hash)
            })
    {
        return Err("计划尚未显式批准、批准绑定不匹配或执行已开始。".into());
    }
    let patch = plan.patch.as_ref().ok_or("计划缺少已校验的 IRPatch。")?;
    if patch.base_snapshot_hash != current_snapshot_hash
        || target_snapshot(&baseline, patch)?.snapshot_hash != record.target_snapshot_hash
    {
        return Err("IRPatch 或目标快照与审批记录不匹配。".into());
    }
    // 外部图元 ID 不进入语义快照哈希，因此需独立防止同内容下的引用漂移。
    let fusion = fusion::build_fusion_view(
        &fusion::load_inspect_observations(root).map_err(|e| e.to_string())?,
    );
    for step in &plan.steps {
        if step.external_primitive_ids.is_empty() && step.kind != plan::StepKind::DeleteComponent {
            continue;
        }
        let stable = fusion.mapping_for(&step.logical_id).is_some_and(|mapping| {
            mapping.status.is_stable()
                && mapping.present_in_latest
                && mapping.primitive_ids == step.external_primitive_ids
                && mapping.page_uuid.as_deref() == plan.document["pageUuid"].as_str()
        });
        if !stable {
            return Err(format!("{} 的外部身份映射已漂移或冲突，拒绝执行。", step.logical_id));
        }
    }
    Ok(plan)
}

fn apply_candidate(
    baseline: &LogicalCircuitSnapshot,
    patch: &IrPatch,
) -> Result<LogicalCircuitSnapshot, String> {
    if patch.operations.is_empty() {
        return Err("候选补丁不能为空。".into());
    }
    let mut target = baseline.clone();
    for operation in &patch.operations {
        match operation {
            PatchOperation::SetComponentValue { component_id, value } => {
                target
                    .components
                    .iter_mut()
                    .find(|c| c.id == *component_id)
                    .ok_or("属性变更的元件不存在。")?
                    .value = Some(value.clone());
            }
            PatchOperation::RenameNet { net_id, name } => {
                if name.trim().is_empty()
                    || target.nets.iter().any(|n| n.name.as_ref() == Some(name))
                {
                    return Err("目标网络名称为空或已存在，禁止隐式合并网络。".into());
                }
                let net = target.nets.iter_mut().find(|n| n.id == *net_id).ok_or("网络不存在。")?;
                net.id = format!("net:{name}");
                net.name = Some(name.clone());
            }
            PatchOperation::AddComponent { reference, value, pins, .. } => {
                // 逻辑 ID 与证据由系统按观测约定派生，模型不提供快照内部字段。
                target.components.push(circuitfabric_contracts::Component {
                    id: format!("component:{reference}"),
                    reference: reference.clone(),
                    value: value.clone(),
                    pins: pins.clone(),
                    evidence: Vec::new(),
                });
            }
            PatchOperation::RemoveComponent { component_id } => {
                if !target.components.iter().any(|c| c.id == *component_id) {
                    return Err(format!("元件 {component_id} 不存在。"));
                }
                target.components.retain(|c| c.id != *component_id);
                for net in &mut target.nets {
                    net.pins.retain(|pin| pin.component_id != *component_id);
                }
            }
            PatchOperation::AddNet { name, pins } => {
                if name.trim().is_empty() {
                    return Err("新增网络名称为空。".into());
                }
                let id = format!("net:{name}");
                match target.nets.iter_mut().find(|net| net.id == id) {
                    // 显式挂到既有网络：追加成员并去重，不合并其他网络。
                    Some(net) => {
                        for pin in pins {
                            if !net.pins.iter().any(|existing| {
                                existing.component_id == pin.component_id
                                    && existing.pin_id == pin.pin_id
                            }) {
                                net.pins.push(pin.clone());
                            }
                        }
                    }
                    None => target.nets.push(circuitfabric_contracts::Net {
                        id,
                        name: Some(name.clone()),
                        pins: pins.clone(),
                    }),
                }
            }
            PatchOperation::AnnotateSemantic { key, value } => {
                target.application_semantics.insert(key.clone(), value.clone());
            }
        }
    }
    target.components.sort_by(|a, b| a.id.cmp(&b.id));
    target.nets.sort_by(|a, b| a.id.cmp(&b.id));
    target.parent_snapshot_hash = Some(baseline.snapshot_hash.clone());
    let target = circuitfabric_core::stamp_snapshot(&target).map_err(|e| e.to_string())?;
    let failures: Vec<_> = circuitfabric_core::validate_snapshot(&target)
        .into_iter()
        .filter(|result| result.status == FactStatus::Failed)
        .collect();
    if !failures.is_empty() {
        // 逐条列出失效对象（如重复或悬空的逻辑 ID），让模型能自我纠正。
        let subjects = failures
            .iter()
            .map(|result| result.subject_refs.join(", "))
            .collect::<Vec<_>>()
            .join("；");
        return Err(format!("候选补丁未通过身份/拓扑校验：{subjects}。"));
    }
    Ok(target)
}

/// Predict physical changes using the same observation projection as readback.
pub(crate) fn target_snapshot(
    record: &Value,
    patch: &IrPatch,
) -> Result<LogicalCircuitSnapshot, String> {
    let baseline = serde_json::from_value(record["snapshot"].clone()).map_err(|e| e.to_string())?;
    let mut target = apply_candidate(&baseline, patch)?;
    let mut payload = record["observedPayload"].clone();
    let mut subjects = std::collections::BTreeSet::new();
    for op in &patch.operations {
        match op {
            PatchOperation::SetComponentValue { component_id, .. } => {
                if !subjects.insert(component_id.clone()) {
                    return Err("一次计划不得重复修改同一对象。".into());
                }
            }
            PatchOperation::RenameNet { net_id, name } => {
                if !subjects.insert(net_id.clone()) {
                    return Err("一次计划不得重复修改同一对象。".into());
                }
                let old = net_id.strip_prefix("net:").ok_or("无效网络身份。")?;
                for wire in payload["wires"].as_array_mut().ok_or("缺少导线观测。")? {
                    if wire["net"] == old {
                        wire["net"] = json!(name);
                    }
                }
                for net in payload["nets"].as_array_mut().ok_or("缺少网络观测。")? {
                    if net["name"] == old {
                        net["name"] = json!(name);
                    }
                }
            }
            _ => {}
        }
    }
    if patch.operations.iter().any(|op| matches!(op, PatchOperation::RenameNet { .. })) {
        target.physical_hash = Some(
            observation::build_observation(&payload, None)
                .map_err(|e| e.to_string())?
                .physical_hash,
        );
    }
    circuitfabric_core::stamp_snapshot(&target).map_err(|e| e.to_string())
}

fn audit(
    actor: &str,
    decision: &str,
    reason: String,
    hash: &str,
    unix_seconds: u64,
) -> ChangeSetAuditEntry {
    ChangeSetAuditEntry {
        timestamp_unix_seconds: unix_seconds,
        actor: actor.into(),
        decision: decision.into(),
        reason,
        observed_snapshot_hash: Some(hash.into()),
        rollback_handle: None,
    }
}

/// 对已观测的当前基线预览语义补丁。模型不能指定外部图元或 API 方法。
pub fn preview(
    root: &Path,
    project_id: &str,
    session_id: &str,
    message: &Value,
    unix_seconds: u64,
) -> Result<Value, String> {
    let hash =
        message["baselineSnapshotHash"].as_str().ok_or("preview 需要 baselineSnapshotHash。")?;
    let record = load_baseline(root, hash)?;
    if record["projectId"] != project_id {
        return Err("基线不属于当前项目。".into());
    }
    if observation::latest_inspect_snapshot_hash(root).map_err(|e| e.to_string())?.as_deref()
        != Some(hash)
    {
        return Err("基线已过期，请重新 inspect。".into());
    }
    let document = record
        .pointer("/observedPayload/document")
        .cloned()
        .unwrap_or_else(|| record["document"].clone());
    if let Some(request_document) = message.get("document") {
        for field in ["kind", "projectUuid", "pageUuid"] {
            if request_document.get(field).is_some_and(|value| value != &document[field]) {
                return Err("预览目标文档与观测基线不匹配。".into());
            }
        }
    }
    let patch = parse_patch(message["patch"].clone())?;
    if patch.base_snapshot_hash != hash {
        return Err("IRPatch 与请求基线不匹配。".into());
    }
    let baseline: LogicalCircuitSnapshot =
        serde_json::from_value(record["snapshot"].clone()).map_err(|e| e.to_string())?;
    let target = target_snapshot(&record, &patch)?;
    let (_, fusion) = fusion::rebuild_and_persist(root).map_err(|e| e.to_string())?;
    let observed = record.pointer("/observedPayload").cloned().unwrap_or(Value::Null);
    let mut plan = plan::build_plan(&patch, hash, &baseline, &observed, &fusion);
    plan.project_id = project_id.into();
    plan.session_id = session_id.into();
    plan.document = document;
    let complete = observation_complete(&record);
    if !complete {
        plan.risks.push("基线存在缺失观测或校验失败，禁止批准此计划。".into());
        for step in &mut plan.steps {
            step.preconditions.push(plan::Precondition {
                description: "基线观测完整且校验通过。".into(),
                satisfied: false,
            });
        }
    }
    let can_approve = complete && plan.preconditions_satisfied();
    plan.plan_hash = plan::plan_digest(&plan);
    plan::persist_plan(root, &plan).map_err(|e| e.to_string())?;
    let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
    let id = format!("jlc-{}", &plan.plan_hash[7..]);
    // 重复预览必须保持已有决定与执行记录，不能重置审计。
    if !storage.list_change_sets().map_err(|e| e.to_string())?.iter().any(|cs| cs.id == id) {
        let mut diff: Vec<_> = plan.diff.iter().map(|entry| entry.detail.clone()).collect();
        diff.extend(plan.risks.iter().map(|risk| format!("风险：{risk}")));
        diff.push(format!("可安全回滚：{}", plan.rollbackable));
        storage
            .save_change_set(&StoredChangeSet {
                schema_version: 1,
                id: id.clone(),
                base_snapshot_hash: hash.into(),
                target_snapshot_hash: target.snapshot_hash,
                plan_hash: plan.plan_hash.clone(),
                ir_diff: diff,
                evidence: Vec::new(),
                preview_validation: ChangeSetStage {
                    status: if can_approve {
                        ChangeSetStageStatus::Passed
                    } else {
                        ChangeSetStageStatus::Failed
                    },
                    detail: "候选 IR、观测完整性与物化前置条件校验；未执行 EDA 写入。".into(),
                },
                execution: Default::default(),
                observed_snapshot_hash: Some(hash.into()),
                rollback_handle: None,
                audit: vec![audit(
                    "bridge",
                    "preview",
                    format!("sessionId={session_id}"),
                    hash,
                    unix_seconds,
                )],
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(json!({ "type": "preview_result", "ok": true, "planHash": plan.plan_hash,
        "baselineSnapshotHash": hash, "changeSetId": id, "plan": plan, "approvalAllowed": can_approve,
        "write": "not_run", "readback": "not_run", "verification": "inconclusive" }))
}

/// 只接收操作员的显式决定，记录绑定；绝不调用执行器。
pub fn decide(
    root: &Path,
    project_id: &str,
    session_id: &str,
    message: &Value,
    unix_seconds: u64,
) -> Result<Value, String> {
    let hash = message["planHash"].as_str().ok_or("无效的 planHash。")?;
    let plan = bound_plan(root, project_id, session_id, hash)?;
    bound_change_set(root, &plan)?;
    if message["baselineSnapshotHash"] != plan.baseline_snapshot_hash {
        return Err("审批基线与计划不匹配。".into());
    }
    let decision = match message["decision"].as_str() {
        Some("approved") => approval::ApprovalDecision::Approved,
        Some("rejected") => approval::ApprovalDecision::Rejected,
        _ => return Err("decision 必须为 approved 或 rejected。".into()),
    };
    let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
    let current = observation::latest_inspect_snapshot_hash(root).map_err(|e| e.to_string())?;
    if decision == approval::ApprovalDecision::Approved {
        let baseline = load_baseline(root, &plan.baseline_snapshot_hash)?;
        if !observation_complete(&baseline) || !plan.preconditions_satisfied() {
            return Err("当前观测不完整或计划前置条件未满足，不能批准。".into());
        }
    }
    let id = format!("jlc-{}", &hash[7..]);
    storage
        .record_change_set_decision(
            &id,
            current.as_deref(),
            audit(
                "eda operator",
                decision.as_str(),
                message["reason"].as_str().unwrap_or("EDA 预览面板显式决定").into(),
                &plan.baseline_snapshot_hash,
                unix_seconds,
            ),
        )
        .map_err(|e| e.to_string())?;
    approval::persist_approval(
        root,
        &approval::ApprovalRecord::new(
            hash,
            &plan.baseline_snapshot_hash,
            decision,
            observation::rfc3339_millis(unix_seconds * 1000),
            session_id,
            project_id,
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(
        json!({ "type": "approval_result", "ok": true, "planHash": hash, "decision": decision.as_str(), "changeSetId": id }),
    )
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::{Component, Net, Pin, PinRef, SnapshotAuthority};

    use super::*;

    #[test]
    fn model_output_only_accepts_one_semantic_candidate() {
        assert!(candidate_from_text("纯文字建议").unwrap().is_none());
        let valid = "```circuitfabric-patch\n{\"base_snapshot_hash\":\"sha256:b\",\"operations\":[{\"annotate_semantic\":{\"key\":\"domain\",\"value\":\"power\"}}],\"rationale\":\"用途\"}\n```";
        assert!(candidate_from_text(valid).unwrap().is_some());
        assert!(candidate_from_text(&format!("{valid}\n{valid}")).is_err());
        assert!(candidate_from_text("```circuitfabric-patch\n{}").is_err());
        assert!(candidate_from_text("```circuitfabric-patch\n{\"base_snapshot_hash\":\"b\",\"rationale\":\"raw\",\"operations\":[{\"eda_api\":{\"method\":\"delete\"}}]}\n```").is_err());
    }

    #[test]
    fn candidate_parse_errors_locate_the_operation_and_expected_shape() {
        // 缺失字段：说明操作序号、类型与字段名，不透出行列号。
        let missing = candidate_from_text(
            "```circuitfabric-patch\n{\"base_snapshot_hash\":\"sha256:b\",\"operations\":[{\"rename_net\":{\"net_id\":\"net:VCC\",\"name\":\"POWER\"}},{\"add_component\":{\"reference\":\"J1\"}}],\"rationale\":\"r\"}\n```",
        )
        .unwrap_err();
        assert!(missing.contains("operations[1]（add_component）"), "{missing}");
        assert!(missing.contains("missing field `pins`"), "{missing}");

        // 未知操作类型：列出全部支持的操作。
        let unsupported = candidate_from_text(
            "```circuitfabric-patch\n{\"base_snapshot_hash\":\"sha256:b\",\"operations\":[{\"eda_api\":{\"method\":\"delete\"}}],\"rationale\":\"r\"}\n```",
        )
        .unwrap_err();
        assert!(unsupported.contains("不支持的操作类型 eda_api"), "{unsupported}");
        assert!(unsupported.contains("add_component"), "{unsupported}");

        // 多键操作对象与缺失顶层字段分别给出明确错误。
        let multi_key = parse_patch(json!({"base_snapshot_hash":"sha256:b","rationale":"r",
            "operations":[{"add_component":{"reference":"J1"},"remove_component":{"component_id":"component:U1"}}]}))
        .unwrap_err();
        assert!(multi_key.contains("单键操作对象"), "{multi_key}");
        assert!(parse_patch(json!({"operations":[],"rationale":"r"})).is_err());
    }

    #[test]
    fn flat_author_payloads_derive_identity_and_join_existing_nets() {
        let mut baseline = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        baseline.snapshot_hash = "sha256:baseline".to_owned();
        baseline.components.push(Component {
            id: "component:R9".to_owned(),
            reference: "R9".to_owned(),
            value: Some("10k".to_owned()),
            pins: vec![
                Pin { id: "1".to_owned(), name: "A".to_owned() },
                Pin { id: "2".to_owned(), name: "B".to_owned() },
            ],
            evidence: Vec::new(),
        });
        // 未命名地网：成员 R9.2。
        baseline.nets.push(Net {
            id: "net:".to_owned(),
            name: None,
            pins: vec![PinRef { component_id: "component:R9".to_owned(), pin_id: "2".to_owned() }],
        });
        let record = json!({
            "snapshot": baseline,
            "observedPayload": {
                "document": {"kind":"schematic", "projectUuid":"eda-project", "pageUuid":"page-1"},
                "components": [], "wires": [], "nets": [], "buses": []
            }
        });
        let patch = parse_patch(json!({
            "base_snapshot_hash": "sha256:baseline",
            "operations": [
                {"rename_net": {"net_id": "net:", "name": "GND"}},
                {"add_component": {"reference": "J1", "value": "HOST_SPI_8P",
                    "pins": [{"id": "1", "name": "VCC"}, {"id": "2", "name": "GND"}]}},
                {"add_net": {"name": "GND", "pins": [{"component_id": "component:J1", "pin_id": "2"}]}},
                {"add_net": {"name": "GND", "pins": [{"component_id": "component:J1", "pin_id": "2"}]}},
                {"add_net": {"name": "BS", "pins": [{"component_id": "component:R9", "pin_id": "1"}]}}
            ],
            "rationale": "无源转接板"
        }))
        .unwrap();
        let target = target_snapshot(&record, &patch).unwrap();

        // 新增元件身份由系统派生。
        let j1 = target
            .components
            .iter()
            .find(|component| component.id == "component:J1")
            .expect("派生 component:J1");
        assert_eq!(j1.reference, "J1");
        assert!(j1.evidence.is_empty());

        // 改名后的既有网络被显式挂接且成员去重；新网络独立创建。
        let gnd = target.nets.iter().find(|net| net.id == "net:GND").expect("net:GND");
        assert_eq!(gnd.name.as_deref(), Some("GND"));
        assert_eq!(
            gnd.pins,
            vec![
                PinRef { component_id: "component:R9".to_owned(), pin_id: "2".to_owned() },
                PinRef { component_id: "component:J1".to_owned(), pin_id: "2".to_owned() },
            ],
            "挂接成员去重且保留原成员"
        );
        let bs = target.nets.iter().find(|net| net.id == "net:BS").expect("net:BS");
        assert_eq!(bs.pins.len(), 1);
    }

    #[test]
    fn topology_validation_failures_name_the_broken_subject() {
        let mut baseline = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        baseline.snapshot_hash = "sha256:baseline".to_owned();
        let record = json!({
            "snapshot": baseline,
            "observedPayload": {
                "document": {"kind":"schematic", "projectUuid":"eda-project", "pageUuid":"page-1"},
                "components": [], "wires": [], "nets": [], "buses": []
            }
        });
        let patch = parse_patch(json!({
            "base_snapshot_hash": "sha256:baseline",
            "operations": [{"add_net": {"name": "BS",
                "pins": [{"component_id": "component:GHOST", "pin_id": "1"}]}}],
            "rationale": "悬空引脚"
        }))
        .unwrap();
        let error = target_snapshot(&record, &patch).unwrap_err();
        assert!(error.contains("身份/拓扑校验"), "{error}");
        assert!(error.contains("component:GHOST:1"), "{error}");
    }
}
