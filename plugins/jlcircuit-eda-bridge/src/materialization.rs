//! Foreground materialization with durable per-primitive compensation checkpoints.
//! The transport owns the operation lock; no executor acknowledgement is a readback.
use std::path::Path;

use circuitfabric_contracts::{FactStatus, LogicalCircuitSnapshot, PatchOperation};
use circuitfabric_project::{
    ChangeSetAuditEntry, ChangeSetStage, ChangeSetStageStatus as Status, ProjectStorage,
    StoredChangeSet, TimelineObservation, TimelineSource,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{observation, plan::MaterializationPlan, review};

/// 结构类新建写入：执行器确认时必须带回新建图元 ID，作为回滚句柄。
const CREATE_KINDS: [&str; 2] = ["component_place", "wire_create"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Unit {
    kind: String,
    primitive_id: String,
    before: String,
    after: String,
    state: String,
    /// 结构类操作的执行器规格（放置参数 / 删除核对与补偿 / 连线锚点）。
    #[serde(default)]
    spec: Value,
    /// 新建图元回滚句柄：执行器确认写入时返回的图元 ID。
    #[serde(default)]
    created_id: Option<String>,
    /// 旧日志无此字段；恢复时必须结合审计确认方向。
    #[serde(default)]
    direction: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    plan_hash: String,
    units: Vec<Unit>,
}

fn stage(status: Status, detail: impl Into<String>) -> ChangeSetStage {
    ChangeSetStage { status, detail: detail.into() }
}

fn journal_path(root: &Path, hash: &str) -> std::path::PathBuf {
    observation::observations_dir(root).join(format!("execution-{}.json", &hash[7..]))
}

fn save_journal(root: &Path, journal: &Journal) -> Result<(), String> {
    observation::write_atomic(
        &journal_path(root, &journal.plan_hash),
        &serde_json::to_vec_pretty(journal).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn save(root: &Path, record: &StoredChangeSet) -> Result<(), String> {
    let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
    let mut merged = record.clone();
    if let Some(current) = storage
        .list_change_sets()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == record.id)
    {
        if !record.audit.starts_with(&current.audit) {
            merged.audit = current.audit;
            for entry in &record.audit {
                if !merged.audit.contains(entry) {
                    merged.audit.push(entry.clone());
                }
            }
        }
    }
    storage.save_change_set(&merged).map_err(|e| e.to_string())
}

fn audit(
    root: &Path,
    record: &mut StoredChangeSet,
    event: &str,
    detail: impl Into<String>,
    now: u64,
) -> Result<(), String> {
    record.audit.push(ChangeSetAuditEntry {
        timestamp_unix_seconds: now,
        actor: "bridge".into(),
        decision: event.into(),
        reason: detail.into(),
        observed_snapshot_hash: record.observed_snapshot_hash.clone(),
        rollback_handle: record.rollback_handle.clone(),
    });
    save(root, record)?;
    let plan = crate::plan::load_plan(root, &record.plan_hash)
        .map_err(|e| e.to_string())?
        .ok_or("审计缺少计划。")?;
    let storage = ProjectStorage::open(root).map_err(|e| e.to_string())?;
    match storage.load_session(&plan.session_id) {
        Ok(replay) if replay.metadata.category == circuitfabric_project::SessionCategory::Eda => {}
        Ok(_) => return Err("物化审计会话不是 EDA 类别。".into()),
        Err(circuitfabric_project::ProjectStorageError::SessionNotFound { .. }) => {
            storage
                .start_categorized_session(
                    circuitfabric_project::SessionSeed {
                        session_id: plan.session_id.clone(),
                        runtime_profile_id: "eda-operations".into(),
                        backend_id: Some("jlcircuit-eda".into()),
                    },
                    circuitfabric_project::SessionCategory::Eda,
                    None,
                )
                .map_err(|e| e.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    storage
        .append_session_event(
            &plan.session_id,
            &circuitfabric_project::SessionEvent {
                timestamp_unix_seconds: now,
                kind: circuitfabric_project::SessionEventKind::Note {
                    text: format!(
                        "ChangeSet {} · {}",
                        record.id,
                        serde_json::to_string(record.audit.last().expect("just appended"))
                            .map_err(|e| e.to_string())?
                    ),
                },
            },
        )
        .map_err(|e| e.to_string())
}

/// Persist a fresh observation, independently of the write result.
fn observe(
    root: &Path,
    plan: &MaterializationPlan,
    reply: Value,
    now: u64,
) -> Result<(String, bool, LogicalCircuitSnapshot), String> {
    if reply["ok"] != true {
        return Err("执行器未确认回读。".into());
    }
    let mut payload = reply["payload"].clone();
    if payload["document"] != plan.document {
        return Err("回读目标文档不匹配。".into());
    }
    payload["inconclusive"] =
        reply.get("inconclusive").cloned().unwrap_or(json!(["executorResult"]));
    let build = observation::build_observation(&payload, None).map_err(|e| e.to_string())?;
    let complete = build.inconclusive.is_empty()
        && build.validation.iter().all(|v| v.status == FactStatus::Passed);
    let hash = build.snapshot.snapshot_hash.clone();
    let record = json!({"schemaVersion":1,"kind":"inspect", "projectId":plan.project_id,"sessionId":plan.session_id,
        "receivedAt":observation::rfc3339_millis(now * 1000),"capturedAt":reply["capturedAt"],"observedPayload":payload,
        "snapshot":build.snapshot,"physicalProjection":build.physical_projection,"externalRefs":build.external_refs,
        "inconclusive":build.inconclusive,"diagnostics":build.diagnostics,"validation":build.validation});
    observation::persist_inspect_record(root, &record).map_err(|e| e.to_string())?;
    let change_set_id = format!("jlc-{}", &plan.plan_hash[7..]);
    ProjectStorage::open(root)
        .and_then(|storage| {
            storage.record_observed_snapshot(
                &build.snapshot,
                TimelineObservation {
                    source: TimelineSource::Readback,
                    observed_at_unix_millis: now * 1000,
                    session_id: Some(&plan.session_id),
                    change_set_id: Some(&change_set_id),
                },
            )
        })
        .map_err(|e| e.to_string())?;
    Ok((hash, complete, build.snapshot))
}

fn request(plan: &MaterializationPlan, operation: &str, params: Value) -> Value {
    json!({"operation":operation, "document":plan.document, "planHash":plan.plan_hash,
        "baselineSnapshotHash":plan.baseline_snapshot_hash, "params":params})
}

fn push_property_units(
    units: &mut Vec<Unit>,
    step: &crate::plan::PlanStep,
    kind: &str,
    before: &str,
    after: &str,
) -> Result<(), String> {
    if step.external_primitive_ids.is_empty() {
        return Err("计划缺少外部身份。".into());
    }
    for id in &step.external_primitive_ids {
        units.push(Unit {
            kind: kind.into(),
            primitive_id: id.clone(),
            before: before.into(),
            after: after.into(),
            state: "pending".into(),
            spec: Value::Null,
            created_id: None,
            direction: String::new(),
        });
    }
    Ok(())
}

fn units(plan: &MaterializationPlan) -> Result<Vec<Unit>, String> {
    let patch = plan.patch.as_ref().ok_or("计划缺少语义补丁。")?;
    let mut units = Vec::new();
    for (step, op) in plan.steps.iter().zip(&patch.operations) {
        match op {
            PatchOperation::SetComponentValue { value, .. } => {
                let before = step.rollback.data["value"].as_str().ok_or("计划缺少补偿原值。")?;
                push_property_units(&mut units, step, "component_value", before, value)?;
            }
            PatchOperation::RenameNet { name, .. } => {
                let before = step.rollback.data["name"].as_str().ok_or("计划缺少补偿原值。")?;
                push_property_units(&mut units, step, "wire_net", before, name)?;
            }
            PatchOperation::RemoveComponent { .. } => {
                if step.external_primitive_ids.is_empty() {
                    return Err("计划缺少外部身份。".into());
                }
                // 补偿数据（库身份/放置/原值）随规格下发，回滚据此重建。
                let mut spec = step.spec.clone();
                spec["restore"] = step.rollback.data.clone();
                for id in &step.external_primitive_ids {
                    units.push(Unit {
                        kind: "component_delete".into(),
                        primitive_id: id.clone(),
                        before: String::new(),
                        after: String::new(),
                        state: "pending".into(),
                        spec: spec.clone(),
                        created_id: None,
                        direction: String::new(),
                    });
                }
            }
            PatchOperation::AddComponent { .. } => {
                if step.spec.get("libraryUuid").and_then(Value::as_str).is_none() {
                    return Err("放置步骤缺少库身份规格。".into());
                }
                units.push(Unit {
                    kind: "component_place".into(),
                    primitive_id: step.logical_id.clone(),
                    before: String::new(),
                    after: String::new(),
                    state: "pending".into(),
                    spec: step.spec.clone(),
                    created_id: None,
                    direction: String::new(),
                });
            }
            PatchOperation::AddNet { .. } => {
                let connections = step
                    .spec
                    .get("connections")
                    .and_then(Value::as_array)
                    .ok_or("计划缺少连线锚点。")?;
                if connections.is_empty() {
                    return Err("新增网络缺少成员连线。".into());
                }
                for (index, connection) in connections.iter().enumerate() {
                    units.push(Unit {
                        kind: "wire_create".into(),
                        primitive_id: format!("{}#{index}", step.logical_id),
                        before: String::new(),
                        after: String::new(),
                        state: "pending".into(),
                        spec: json!({"net": step.spec["net"], "connection": connection}),
                        created_id: None,
                        direction: String::new(),
                    });
                }
            }
            // 纯 IR 标注不产生 EDA 写入单元。
            PatchOperation::AnnotateSemantic { .. } => {}
        }
    }
    if units.is_empty() || plan.steps.len() != patch.operations.len() {
        return Err("空计划或步骤与补丁不匹配。".into());
    }
    Ok(units)
}

// 仅恢复一次已确认删除、但补偿响应丢失的单元。其他未知写入保持阻塞。
// 需要新版执行器确认空闲，且完整新鲜观测等于成功删除后的现场。
fn reconcile_delete_rollback(
    root: &Path,
    plan: &MaterializationPlan,
    record: &mut StoredChangeSet,
    journal: &mut Journal,
    rpc: &mut impl FnMut(Value) -> Result<Value, String>,
    now: u64,
) -> Result<(), String> {
    if journal.units.len() != 1 {
        return Ok(());
    }
    let unit = &journal.units[0];
    if unit.kind != "component_delete" || !matches!(unit.state.as_str(), "unknown" | "in_flight") {
        return Ok(());
    }
    let first_rollback = record.audit.iter().position(|e| e.decision == "rollback_requested");
    let Some(first_rollback) = first_rollback else { return Ok(()) };
    if !matches!(unit.direction.as_str(), "" | "rollback")
        || record.execution.write.status != Status::Passed
        || !record.audit[..first_rollback].iter().any(|e| e.decision == "write_completed")
    {
        return Ok(());
    }
    let forward_hash = record.audit[..first_rollback]
        .iter()
        .rev()
        .find(|e| e.decision == "readback")
        .and_then(|e| e.observed_snapshot_hash.clone())
        .ok_or("缺少删除后的回读记录，不能恢复未知补偿。")?;
    let probe = rpc(request(plan, "readback", json!({"requireIdle":true,"statusOnly":true})))?;
    audit(
        root,
        record,
        "rollback_executor_status",
        json!({
            "extensionVersion":probe["extensionVersion"],"mutationIdle":probe["mutationIdle"],
            "activeMutation":probe["activeMutation"],"ok":probe["ok"],
            "message":probe["message"],"inconclusive":probe["inconclusive"]
        })
        .to_string(),
        now,
    )?;
    match probe["mutationIdle"].as_bool() {
        Some(true) => {}
        Some(false) => {
            return Err(format!(
                "executor_busy：执行器仍有在途写入，未再次重建器件。extensionVersion={}; activeMutation={}",
                probe["extensionVersion"], probe["activeMutation"]
            ));
        }
        None => {
            return Err(format!(
                "executor_idle_unsupported：当前执行器未返回 mutationIdle，不能安全重试。extensionVersion={}；请重新加载支持空闲探针的扩展入口。",
                probe["extensionVersion"]
            ));
        }
    }
    // 新版入口的探针不读取整页；兼容已返回完整观测的 v0.2.18。
    let reply = if probe["payload"]["executorStatus"] == true {
        rpc(request(plan, "readback", json!({"requireIdle":true})))?
    } else {
        probe
    };
    if reply["mutationIdle"] != true {
        return Err("executor_busy：完整观测期间执行器状态变化，未重建器件。".into());
    }
    let (hash, complete, snapshot) = observe(root, plan, reply, now)?;
    let reference = journal.units[0].spec["designator"].as_str().unwrap_or("");
    if !complete
        || hash != forward_hash
        || snapshot.components.iter().any(|c| c.reference == reference)
    {
        return Err(
            "新鲜现场与删除后的完整观测不一致；可能已重建或存在其他改动，拒绝重复创建。".into()
        );
    }
    journal.units[0].state = "written".into();
    journal.units[0].direction = "rollback".into();
    save_journal(root, journal)?;
    audit(
        root,
        record,
        "rollback_reconciled",
        "执行器空闲且完整回读与删除后现场一致；允许重试原补偿。",
        now,
    )
}

fn compensate(
    root: &Path,
    plan: &MaterializationPlan,
    journal: &mut Journal,
    rpc: &mut impl FnMut(Value) -> Result<Value, String>,
) -> Result<(), String> {
    // Unknown writes cannot be safely retried or reversed. Their handles remain for inspection.
    if journal.units.iter().any(|u| u.state == "in_flight" || u.state == "unknown") {
        return Err("存在结果未知的写入，拒绝自动回滚；请先核实 EDA 当前状态。".into());
    }
    for i in (0..journal.units.len()).rev() {
        if journal.units[i].state != "written" {
            continue;
        }
        let unit = journal.units[i].clone();
        journal.units[i].state = "in_flight".into();
        journal.units[i].direction = "rollback".into();
        save_journal(root, journal)?;
        // 属性写回原值；删除按补偿数据重建；新建图元按句柄删除。
        let params = if unit.kind == "component_delete" {
            json!({"kind":unit.kind,"primitiveId":unit.primitive_id,"expected":"","value":"","spec":unit.spec})
        } else if CREATE_KINDS.contains(&unit.kind.as_str()) {
            let Some(created) = unit.created_id.as_deref() else {
                journal.units[i].state = "unknown".into();
                save_journal(root, journal)?;
                return Err("新建图元缺少回滚句柄，拒绝自动回滚。".into());
            };
            json!({"kind":unit.kind,"primitiveId":created,"expected":"","value":"","spec":unit.spec})
        } else {
            json!({"kind":unit.kind,"primitiveId":unit.primitive_id,"expected":unit.after,"value":unit.before,"spec":Value::Null})
        };
        let response = rpc(request(plan, "rollback", params));
        match response {
            Ok(reply) if reply["ok"] == true && reply["payload"]["written"] == true => {
                journal.units[i].state = "restored".into()
            }
            Ok(reply) if reply["payload"]["writeStatus"] == "failed" => {
                journal.units[i].state = "written".into();
                save_journal(root, journal)?;
                return Err(format!("补偿被拒绝：{reply}"));
            }
            Err(error) if error == "cancelled" => {
                journal.units[i].state = "written".into();
                save_journal(root, journal)?;
                return Err("cancelled".into());
            }
            other => {
                journal.units[i].state = "unknown".into();
                save_journal(root, journal)?;
                return Err(format!("补偿结果未知：{other:?}"));
            }
        }
        save_journal(root, journal)?;
    }
    Ok(())
}

/// 含放置/删除/连线类操作的计划无法用整板哈希验证（放置元件的引脚以库件为准），
/// 改用逐操作结构断言；任何断言不满足都不判成功，也不触发自动回滚。
fn failed_checks(plan: &MaterializationPlan, observed: &LogicalCircuitSnapshot) -> Vec<String> {
    let Some(patch) = plan.patch.as_ref() else {
        return vec!["计划缺少语义补丁。".to_owned()];
    };
    let mut failed = Vec::new();
    for operation in &patch.operations {
        match operation {
            PatchOperation::SetComponentValue { component_id, value } => {
                if !observed
                    .components
                    .iter()
                    .any(|c| c.id == *component_id && c.value.as_deref() == Some(value.as_str()))
                {
                    failed.push(format!("{component_id} 的值未观测为 {value}"));
                }
            }
            PatchOperation::RenameNet { net_id, name } => {
                if !observed.nets.iter().any(|n| n.name.as_deref() == Some(name.as_str())) {
                    failed.push(format!("网络 {name} 未观测到"));
                }
                if let Some(old) = net_id.strip_prefix("net:") {
                    if !old.is_empty()
                        && observed.nets.iter().any(|n| n.name.as_deref() == Some(old))
                    {
                        failed.push(format!("旧网络名 {old} 仍存在"));
                    }
                }
            }
            PatchOperation::RemoveComponent { component_id } => {
                if observed.components.iter().any(|c| c.id == *component_id) {
                    failed.push(format!("{component_id} 仍存在"));
                }
            }
            PatchOperation::AddComponent { reference, .. } => {
                if !observed.components.iter().any(|c| c.reference == *reference) {
                    failed.push(format!("component:{reference} 未观测到放置"));
                }
            }
            PatchOperation::AddNet { name, pins } => {
                let net = observed.nets.iter().find(|n| n.id == format!("net:{name}"));
                if net.is_none() {
                    failed.push(format!("net:{name} 未观测到"));
                    continue;
                }
                for pin in pins {
                    if !net.is_some_and(|net| {
                        net.pins
                            .iter()
                            .any(|p| p.component_id == pin.component_id && p.pin_id == pin.pin_id)
                    }) {
                        failed
                            .push(format!("net:{name} 缺成员 {}:{}", pin.component_id, pin.pin_id));
                    }
                }
            }
            // 纯 IR 标注不产生可观测事实。
            PatchOperation::AnnotateSemantic { .. } => {}
        }
    }
    failed
}

fn readback(
    root: &Path,
    plan: &MaterializationPlan,
    record: &mut StoredChangeSet,
    expected: &str,
    rpc: &mut impl FnMut(Value) -> Result<Value, String>,
    now: u64,
) -> Result<(), String> {
    // Persist absence first; a crash or missing result cannot retain an old green check.
    record.execution.readback = stage(Status::Inconclusive, "等待新的 EDA 回读。");
    record.execution.verification = stage(Status::Inconclusive, "尚未取得完整匹配回读。");
    record.observed_snapshot_hash = None;
    save(root, record)?;
    let structural = plan.steps.iter().any(|step| {
        matches!(
            step.kind,
            crate::plan::StepKind::PlaceComponent
                | crate::plan::StepKind::DeleteComponent
                | crate::plan::StepKind::AddNet
        )
    });
    // 回滚方向以回到基线哈希为准；放置类计划只有正向写入改用结构断言
    // （放置元件的引脚以库件为准，目标哈希不可达）。
    let restoring = expected == record.base_snapshot_hash;
    match rpc(request(plan, "readback", json!({}))).and_then(|r| observe(root, plan, r, now)) {
        Ok((hash, complete, snapshot)) => {
            record.observed_snapshot_hash = Some(hash.clone());
            record.execution.readback = stage(
                if complete { Status::Passed } else { Status::Inconclusive },
                format!("snapshotHash={hash}"),
            );
            let (status, detail) = if !complete {
                (Status::Inconclusive, "回读不完整；不自动回滚已确认写入。".to_owned())
            } else if structural && !restoring {
                let failed = failed_checks(plan, &snapshot);
                if failed.is_empty() {
                    (Status::Passed, "结构断言全部成立（含放置/删除/连线操作）。".to_owned())
                } else {
                    (
                        Status::Inconclusive,
                        format!("结构断言未满足：{}；不自动回滚已确认写入。", failed.join("；")),
                    )
                }
            } else if hash == expected {
                (
                    Status::Passed,
                    if restoring {
                        "回滚后完整回读与基线一致。".to_owned()
                    } else {
                        "完整回读与预期一致。".to_owned()
                    },
                )
            } else {
                (
                    Status::Inconclusive,
                    "回读不完整或尚未同步到预期；不自动回滚已确认写入。".to_owned(),
                )
            };
            record.execution.verification = stage(status, detail);
            audit(
                root,
                record,
                "readback",
                format!("complete={complete}; expected={expected}; observed={hash}"),
                now,
            )?;
        }
        Err(error) => {
            audit(root, record, "readback_error", error, now)?;
        }
    }
    audit(
        root,
        record,
        "verification",
        format!(
            "{:?}: {}",
            record.execution.verification.status, record.execution.verification.detail
        ),
        now,
    )
}

/// Run under the bridge operation lock. The RPC must block until its result is known.
/// Every unit is checkpointed before and after calling the executor.
pub fn execute(
    root: &Path,
    project: &str,
    session: &str,
    hash: &str,
    operation: &str,
    mut rpc: impl FnMut(Value) -> Result<Value, String>,
    now: u64,
) -> Result<Value, String> {
    let plan = review::bound_plan(root, project, session, hash)?;
    let mut record = review::bound_change_set(root, &plan)?;
    audit(
        root,
        &mut record,
        &format!("{operation}_request"),
        format!("sessionId={session}; planHash={hash}"),
        now,
    )?;
    let result = execute_inner(root, project, session, hash, operation, &mut rpc, now);
    if let Err(error) = &result {
        let mut record = review::bound_change_set(root, &plan)?;
        audit(root, &mut record, &format!("{operation}_error"), error, now)?;
    }
    result
}

fn execute_inner(
    root: &Path,
    project: &str,
    session: &str,
    hash: &str,
    operation: &str,
    mut rpc: impl FnMut(Value) -> Result<Value, String>,
    now: u64,
) -> Result<Value, String> {
    let plan = review::bound_plan(root, project, session, hash)?;
    let mut record = review::bound_change_set(root, &plan)?;
    let path = journal_path(root, hash);
    let mut rollback_ok = None;
    if operation == "apply" {
        // Never overwrite a journal: interruption consumes approval and prohibits replay.
        if path.exists() {
            return Err("计划已开始执行，请回读或回滚；禁止重复 apply。".into());
        }
        let prepared = units(&plan)?;
        let fresh = match rpc(request(&plan, "inspect", json!({}))) {
            Ok(reply) => reply,
            Err(error) => {
                if error == "cancelled" {
                    record.execution.write = stage(Status::Failed, "写入前取消。");
                }
                audit(
                    root,
                    &mut record,
                    if error == "cancelled" { "cancelled" } else { "apply_error" },
                    &error,
                    now,
                )?;
                return Err(error);
            }
        };
        let (current, complete, _) = observe(root, &plan, fresh, now)?;
        if !complete {
            return Err("当前回读不完整，拒绝写入。".into());
        }
        review::authorize_apply(root, project, session, hash, &current)?;
        let mut journal = Journal { plan_hash: hash.into(), units: prepared };
        save_journal(root, &journal)?;
        record.rollback_handle = Some(format!("jlc-rollback:{}", &hash[7..]));
        record.execution.write = stage(Status::Inconclusive, "执行已开始，逐步确认写入。");
        record.execution.verification = stage(Status::Inconclusive, "等待写入后回读。");
        audit(root, &mut record, "apply_requested", format!("sessionId={session}"), now)?;
        let mut failed = false;
        for i in 0..journal.units.len() {
            let latest = review::bound_change_set(root, &plan)?;
            if latest
                .audit
                .iter()
                .rev()
                .find(|e| matches!(e.decision.as_str(), "approved" | "rejected"))
                .is_some_and(|e| e.decision == "rejected")
            {
                failed = true;
                record.execution.write = stage(Status::Failed, "审批已撤销，停止后续写入。");
                audit(
                    root,
                    &mut record,
                    "cancelled",
                    "审批已撤销；保留已完成步骤与补偿句柄。",
                    now,
                )?;
                break;
            }
            let unit = journal.units[i].clone();
            journal.units[i].state = "in_flight".into();
            journal.units[i].direction = "apply".into();
            save_journal(root, &journal)?;
            let reply = rpc(request(
                &plan,
                "apply",
                json!({"kind":unit.kind,"primitiveId":unit.primitive_id,"expected":unit.before,"value":unit.after,"spec":unit.spec}),
            ));
            match reply {
                Ok(reply) if reply["ok"] == true && reply["payload"]["written"] == true => {
                    // 新建类写入必须带回图元 ID 作为回滚句柄，否则结果按未知处理。
                    if CREATE_KINDS.contains(&unit.kind.as_str()) {
                        let Some(created) = reply["payload"]["createdId"].as_str() else {
                            journal.units[i].state = "unknown".into();
                            save_journal(root, &journal)?;
                            failed = true;
                            record.execution.write = stage(
                                Status::Inconclusive,
                                format!("step={i}; 新建图元未返回 ID，结果未知；剩余步骤未执行。"),
                            );
                            audit(
                                root,
                                &mut record,
                                "write_error",
                                format!("step={i}; {reply:?}"),
                                now,
                            )?;
                            break;
                        };
                        journal.units[i].created_id = Some(created.to_owned());
                    }
                    journal.units[i].state = "written".into();
                    save_journal(root, &journal)?;
                    audit(
                        root,
                        &mut record,
                        "step_written",
                        format!("step={i}; {}", unit.primitive_id),
                        now,
                    )?;
                    if reply["cancelRequested"] == true {
                        failed = true;
                        record.execution.write =
                            stage(Status::Failed, "已确认当前写入；已取消后续步骤，保留回滚句柄。");
                        audit(
                            root,
                            &mut record,
                            "cancelled",
                            "已收集在途结果，后续步骤未执行。",
                            now,
                        )?;
                        break;
                    }
                }
                reply => {
                    failed = true;
                    let explicit_failure =
                        reply.as_ref().is_ok_and(|r| r["payload"]["writeStatus"] == "failed");
                    let cancelled = reply.as_ref().is_err_and(|e| e == "cancelled");
                    journal.units[i].state =
                        if explicit_failure || cancelled { "failed" } else { "unknown" }.into();
                    save_journal(root, &journal)?;
                    record.execution.write = stage(
                        if explicit_failure || cancelled {
                            Status::Failed
                        } else {
                            Status::Inconclusive
                        },
                        format!("step={i}; {reply:?}; 剩余步骤未执行。"),
                    );
                    audit(
                        root,
                        &mut record,
                        if cancelled { "cancelled" } else { "write_error" },
                        format!("step={i}; {reply:?}"),
                        now,
                    )?;
                    if explicit_failure {
                        let compensation = compensate(root, &plan, &mut journal, &mut rpc);
                        audit(root, &mut record, "compensation", format!("{compensation:?}"), now)?;
                    }
                    break;
                }
            }
        }
        if !failed {
            record.execution.write = stage(Status::Passed, "全部步骤取得独立写入确认。");
            audit(root, &mut record, "write_completed", "全部步骤完成；尚不代表回读成功。", now)?;
        }
        let expected = if failed {
            record.base_snapshot_hash.clone()
        } else {
            record.target_snapshot_hash.clone()
        };
        readback(root, &plan, &mut record, &expected, &mut rpc, now)?;
    } else if operation == "readback" || operation == "rollback" {
        let mut journal: Journal =
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if journal.plan_hash != hash {
            return Err("回滚句柄与计划不匹配。".into());
        }
        let rollback = operation == "rollback";
        let mut recovery_blocked = false;
        if rollback {
            record.execution.readback = stage(Status::Inconclusive, "回滚开始，等待新的回读。");
            record.execution.verification = stage(Status::Inconclusive, "尚未验证回滚结果。");
            record.observed_snapshot_hash = None;
            audit(
                root,
                &mut record,
                "rollback_requested",
                "按补偿记录逆序回滚；每步核对当前值。",
                now,
            )?;
            let recovery =
                reconcile_delete_rollback(root, &plan, &mut record, &mut journal, &mut rpc, now);
            recovery_blocked = recovery.is_err();
            let result = recovery.and_then(|()| compensate(root, &plan, &mut journal, &mut rpc));
            rollback_ok = Some(result.is_ok());
            audit(
                root,
                &mut record,
                if result.is_ok() { "rollback_executed" } else { "rollback_error" },
                match &result {
                    Ok(()) => "补偿步骤执行完成，等待回读验证。".to_owned(),
                    Err(error) => error.clone(),
                },
                now,
            )?;
        }
        let restored = journal
            .units
            .iter()
            .all(|u| u.state == "restored" || u.state == "failed" || u.state == "pending");
        let rollback_attempted = record
            .audit
            .iter()
            .any(|entry| matches!(entry.decision.as_str(), "rollback_requested" | "compensation"));
        let expected = if rollback || restored || rollback_attempted {
            record.base_snapshot_hash.clone()
        } else {
            record.target_snapshot_hash.clone()
        };
        // 恢复检查未通过时没有发生新写入；不再重复慢速读整页或读取在途写入。
        if !recovery_blocked {
            readback(root, &plan, &mut record, &expected, &mut rpc, now)?;
        }
    } else {
        return Err("未知物化操作。".into());
    }
    let record = review::bound_change_set(root, &plan)?;
    let ok = rollback_ok.map_or_else(
        || record.verified(),
        |compensated| {
            compensated
                && record.execution.readback.status == Status::Passed
                && record.execution.verification.status == Status::Passed
        },
    );
    let failure = if ok {
        None
    } else {
        record
            .audit
            .iter()
            .rev()
            .find(|e| {
                matches!(e.decision.as_str(), "rollback_error" | "write_error" | "readback_error")
            })
            .map(|e| e.reason.clone())
    };
    Ok(json!({"type":format!("{operation}_result"),"operation":operation,"ok":ok,"failure":failure,
        "planHash":hash,"changeSetId":record.id,"execution":record.execution,"observedSnapshotHash":record.observed_snapshot_hash,
        "rollbackHandle":record.rollback_handle,"audit":record.audit}))
}
