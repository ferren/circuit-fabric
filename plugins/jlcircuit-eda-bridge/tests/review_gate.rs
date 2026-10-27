//! 子任务 D 的 apply 桩：准入失败时绝不调用写入，所有状态都从磁盘重读。
use std::path::PathBuf;

use circuitfabric_contracts::Project;
use circuitfabric_project::{ChangeSetAuditEntry, ChangeSetStageStatus, ProjectStorage};
use jlcircuit_eda_bridge::{approval, observation, plan, review};
use serde_json::{Value, json};

struct Design {
    root: PathBuf,
    payload: Value,
    hash: String,
}

impl Design {
    fn new() -> Self {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "circuitfabric-review-gate-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        ProjectStorage::create(
            &root,
            Project { id: "p1".into(), name: "测试".into(), description: None },
        )
        .unwrap();
        let payload = json!({
            "document": {"kind":"schematic", "projectUuid":"eda-project", "pageUuid":"page-1"},
            "components": [{"primitiveId":"prim-r1", "designator":"R1", "name":"10k",
                "footprint":"0603", "x":0, "y":0, "rotation":0, "mirror":false,
                "pins":[{"number":"1", "name":"A", "x":0, "y":0}]}],
            "wires":[], "nets":[], "buses":[]
        });
        let mut design = Self { root, payload, hash: String::new() };
        design.observe(&[]);
        design
    }

    fn observe(&mut self, missing: &[&str]) {
        let mut payload = self.payload.clone();
        payload["inconclusive"] = json!(missing);
        let build = observation::build_observation(&payload, None).unwrap();
        self.hash = build.snapshot.snapshot_hash.clone();
        let record = json!({"schemaVersion":1, "kind":"inspect", "projectId":"p1", "sessionId":"s1",
            // 故意保持同毫秒：最新基线必须由持久化次序决胜。
            "receivedAt":"2026-10-23T01:00:00.000Z", "observedPayload":payload,
            "snapshot":build.snapshot, "externalRefs":build.external_refs,
            "inconclusive":build.inconclusive, "validation":build.validation,
            "physicalProjection":build.physical_projection});
        observation::persist_inspect_record(&self.root, &record).unwrap();
    }

    fn preview(&self) -> Value {
        review::preview(&self.root, "p1", "s1", &json!({"baselineSnapshotHash":self.hash,
            "patch":{"base_snapshot_hash":self.hash, "operations":[{"annotate_semantic":{"key":"domain", "value":"power"}}],
                "rationale":"标注用途"}}), 1).unwrap()
    }

    fn decide(&self, preview: &Value, decision: &str) -> Result<Value, String> {
        review::decide(
            &self.root,
            "p1",
            "s1",
            &json!({"planHash":preview["planHash"],
            "baselineSnapshotHash":preview["baselineSnapshotHash"], "decision":decision}),
            2,
        )
    }

    fn removal_with_executor_stub(&self) -> Value {
        let preview = review::preview(&self.root, "p1", "s1", &json!({"baselineSnapshotHash":self.hash,
            "patch":{"base_snapshot_hash":self.hash, "operations":[{"remove_component":{"component_id":"component:R1"}}],
                "rationale":"删除电阻"}}), 1).unwrap();
        // 夹具缺少补偿数据：只醒目提醒、不阻止批准。
        assert_eq!(preview["approvalAllowed"], true, "{preview}");
        assert!(preview["plan"]["rollbackWarnings"].as_array().is_some_and(|w| !w.is_empty()));

        // 仅在夹具中模拟 D 已实现删除与补偿，验证高风险计划的准入契约。
        let mut plan =
            plan::load_plan(&self.root, preview["planHash"].as_str().unwrap()).unwrap().unwrap();
        for precondition in &mut plan.steps[0].preconditions {
            precondition.satisfied = true;
        }
        plan.plan_hash = plan::plan_digest(&plan);
        plan::persist_plan(&self.root, &plan).unwrap();
        let storage = ProjectStorage::open(&self.root).unwrap();
        let mut record = storage
            .list_change_sets()
            .unwrap()
            .into_iter()
            .find(|record| record.plan_hash == preview["planHash"])
            .unwrap();
        record.id = format!("jlc-{}", &plan.plan_hash[7..]);
        record.plan_hash = plan.plan_hash.clone();
        record.preview_validation.status = ChangeSetStageStatus::Passed;
        storage.save_change_set(&record).unwrap();
        json!({"planHash":plan.plan_hash, "baselineSnapshotHash":self.hash, "changeSetId":record.id})
    }

    fn apply_stub(&self, preview: &Value, writes: &mut u32) -> Result<(), String> {
        review::authorize_apply(
            &self.root,
            "p1",
            "s1",
            preview["planHash"].as_str().unwrap(),
            &self.hash,
        )?;
        *writes += 1;
        Ok(())
    }
}

impl Drop for Design {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[test]
fn explicit_approval_binds_hashes_and_latest_rejection_stops_the_apply_stub() {
    let design = Design::new();
    let preview = design.preview();
    assert_eq!(preview["approvalAllowed"], true);
    let mut writes = 0;
    assert!(design.apply_stub(&preview, &mut writes).is_err());
    assert_eq!(writes, 0);
    design.decide(&preview, "approved").unwrap();
    let persisted =
        approval::approval_for(&design.root, preview["planHash"].as_str().unwrap()).unwrap();
    assert_eq!(persisted.baseline_snapshot_hash, design.hash);
    assert_eq!(persisted.plan_hash, preview["planHash"]);
    assert_eq!(persisted.decision, approval::ApprovalDecision::Approved);
    design.apply_stub(&preview, &mut writes).unwrap();
    assert_eq!(writes, 1);

    // 桌面端通过同一 ChangeSet 撤销；即使 Bridge 的旁路文件仍为 approved 也不可执行。
    let storage = ProjectStorage::open(&design.root).unwrap();
    storage
        .record_change_set_decision(
            preview["changeSetId"].as_str().unwrap(),
            Some(&design.hash),
            ChangeSetAuditEntry {
                timestamp_unix_seconds: 3,
                actor: "desktop operator".into(),
                decision: "rejected".into(),
                reason: "撤销".into(),
                observed_snapshot_hash: Some(design.hash.clone()),
                rollback_handle: None,
            },
        )
        .unwrap();
    assert!(design.apply_stub(&preview, &mut writes).is_err());
    assert_eq!(writes, 1);
    let record = &storage.list_change_sets().unwrap()[0];
    assert_eq!(
        record.audit.iter().map(|entry| entry.decision.as_str()).collect::<Vec<_>>(),
        ["preview", "approved", "rejected"]
    );
    assert_eq!(record.execution, Default::default());
}

#[test]
fn baseline_drift_after_approval_never_reaches_the_write_stub() {
    let mut design = Design::new();
    let preview = design.removal_with_executor_stub();
    design.decide(&preview, "approved").unwrap();
    let old_hash = design.hash.clone();
    design.payload["components"][0]["x"] = json!(100);
    design.observe(&[]);
    assert_ne!(design.hash, old_hash);
    assert_eq!(
        observation::latest_inspect_snapshot_hash(&design.root).unwrap(),
        Some(design.hash.clone())
    );
    let mut writes = 0;
    assert!(design.apply_stub(&preview, &mut writes).unwrap_err().contains("漂移"));
    assert!(
        review::authorize_apply(
            &design.root,
            "p1",
            "s1",
            preview["planHash"].as_str().unwrap(),
            &old_hash
        )
        .is_err(),
        "调用方也不能拿旧哈希绕过新观测"
    );
    assert!(design.decide(&preview, "approved").is_err());
    assert_eq!(writes, 0);
}

#[test]
fn changed_external_identity_rejects_approved_write_even_if_snapshot_hash_is_unchanged() {
    let mut design = Design::new();
    let preview = design.removal_with_executor_stub();
    let mut writes = 0;
    assert!(design.apply_stub(&preview, &mut writes).is_err());
    design.decide(&preview, "approved").unwrap();
    design.apply_stub(&preview, &mut writes).unwrap();
    assert_eq!(writes, 1);
    design.payload["components"][0]["primitiveId"] = json!("replacement-r1");
    design.observe(&[]);
    assert_eq!(preview["baselineSnapshotHash"], design.hash);
    assert!(design.apply_stub(&preview, &mut writes).unwrap_err().contains("身份映射"));
    assert_eq!(writes, 1, "身份漂移不得触发第二次写入");
}

#[test]
fn incomplete_current_observation_invalidates_approval_even_with_the_same_hash() {
    let mut design = Design::new();
    let preview = design.preview();
    design.decide(&preview, "approved").unwrap();
    design.observe(&["nets"]);
    assert_eq!(preview["baselineSnapshotHash"], design.hash);
    let mut writes = 0;
    assert!(design.apply_stub(&preview, &mut writes).is_err());
    assert_eq!(writes, 0);
}

#[test]
fn tampered_plan_and_cross_session_or_project_cannot_reuse_approval() {
    let design = Design::new();
    let preview = design.preview();
    design.decide(&preview, "approved").unwrap();
    let hash = preview["planHash"].as_str().unwrap();
    for (project, session) in [("p2", "s1"), ("p1", "s2")] {
        assert!(
            review::authorize_apply(&design.root, project, session, hash, &design.hash).is_err()
        );
    }
    assert!(
        review::authorize_apply(&design.root, "p1", "s1", "sha256:../../outside", &design.hash)
            .is_err()
    );
    let mut plan = plan::load_plan(&design.root, hash).unwrap().unwrap();
    plan.plan_hash = format!("sha256:{}", "0".repeat(64));
    // planDigest 会清空自引用字段，故还必须单独校验 planHash 本身。
    std::fs::write(plan::plan_path(&design.root, hash), serde_json::to_vec(&plan).unwrap())
        .unwrap();
    let mut writes = 0;
    assert!(design.apply_stub(&preview, &mut writes).is_err());
    assert!(design.decide(&preview, "approved").is_err());
    assert_eq!(writes, 0);
}

#[test]
fn changeset_binding_preview_validation_and_execution_state_are_checked() {
    let design = Design::new();
    let preview = design.preview();
    design.decide(&preview, "approved").unwrap();
    let storage = ProjectStorage::open(&design.root).unwrap();
    let original = storage.list_change_sets().unwrap().remove(0);
    for field in ["plan", "baseline", "target", "audit", "validation", "execution"] {
        let mut record = original.clone();
        match field {
            "plan" => record.plan_hash = "different".into(),
            "baseline" => record.base_snapshot_hash = "different".into(),
            "target" => record.target_snapshot_hash = "different".into(),
            "audit" => {
                record.audit.last_mut().unwrap().observed_snapshot_hash = Some("different".into())
            }
            "validation" => record.preview_validation.status = ChangeSetStageStatus::NotRun,
            "execution" => record.execution.write.status = ChangeSetStageStatus::Passed,
            _ => unreachable!(),
        }
        storage.save_change_set(&record).unwrap();
        let mut writes = 0;
        assert!(design.apply_stub(&preview, &mut writes).is_err(), "{field}");
        assert_eq!(writes, 0);
    }
}
