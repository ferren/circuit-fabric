use circuitfabric_contracts::Project;
use circuitfabric_project::ProjectStorage;
use jlcircuit_eda_bridge::{materialization, observation, review};
use serde_json::{Value, json};
use std::path::PathBuf;

struct Design {
    root: PathBuf,
    payload: Value,
    hash: String,
}
impl Design {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cf-materialization-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        ProjectStorage::create(
            &root,
            Project { id: "p".into(), name: "测试".into(), description: None },
        )
        .unwrap();
        let payload = json!({"document":{"kind":"schematic","projectUuid":"eda","pageUuid":"page"},
            "components":[{"primitiveId":"r1","designator":"R1","name":"10k","footprint":"0603","x":0,"y":0,"rotation":0,"mirror":false,"pins":[{"number":"1","name":"A","x":0,"y":0}]}],
            "wires":[{"primitiveId":"w1","net":"VCC","points":[[0,0],[10,0]]}],"nets":[{"name":"VCC"}],"buses":[]});
        let built = observation::build_observation(&payload, None).unwrap();
        let hash = built.snapshot.snapshot_hash.clone();
        observation::persist_inspect_record(&root, &json!({"kind":"inspect","projectId":"p","sessionId":"s","receivedAt":"2026-10-23T00:00:00Z",
            "snapshot":built.snapshot,"externalRefs":built.external_refs,"validation":built.validation,"inconclusive":built.inconclusive,"physicalProjection":built.physical_projection,"observedPayload":payload})).unwrap();
        Self { root, payload, hash }
    }
    fn preview(&self, operations: Value, approve: bool) -> Value {
        let preview = review::preview(
            &self.root,
            "p",
            "s",
            &json!({"baselineSnapshotHash":self.hash,
            "patch":{"base_snapshot_hash":self.hash,"operations":operations,"rationale":"验收"}}),
            1,
        )
        .unwrap();
        assert_eq!(preview["approvalAllowed"], true, "{preview}");
        if approve {
            review::decide(&self.root,"p","s",&json!({"planHash":preview["planHash"],"baselineSnapshotHash":self.hash,"decision":"approved"}),2).unwrap();
        }
        preview
    }
    fn run(
        &self,
        preview: &Value,
        operation: &str,
        rpc: impl FnMut(Value) -> Result<Value, String>,
    ) -> Result<Value, String> {
        materialization::execute(
            &self.root,
            "p",
            "s",
            preview["planHash"].as_str().unwrap(),
            operation,
            rpc,
            3,
        )
    }
}
impl Drop for Design {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn property_patch() -> Value {
    json!([{"set_component_value":{"component_id":"component:R1","value":"47k"}}])
}
fn mixed_patch() -> Value {
    json!([{"set_component_value":{"component_id":"component:R1","value":"47k"}},{"rename_net":{"net_id":"net:VCC","name":"POWER"}}])
}
fn rpc(payload: &mut Value, request: &Value) -> Result<Value, String> {
    if matches!(request["operation"].as_str(), Some("inspect" | "readback")) {
        return Ok(json!({"ok":true,"payload":payload,"inconclusive":[]}));
    }
    let p = &request["params"];
    let current = if p["kind"] == "component_value" {
        &mut payload["components"][0]["name"]
    } else {
        &mut payload["wires"][0]["net"]
    };
    if *current != p["expected"] {
        return Ok(json!({"ok":false,"payload":{"writeStatus":"failed"}}));
    }
    *current = p["value"].clone();
    if p["kind"] == "wire_net" {
        payload["nets"][0]["name"] = p["value"].clone();
    }
    Ok(json!({"ok":true,"payload":{"written":true}}))
}

#[test]
fn component_and_network_preview_approval_apply_readback_and_restart_rollback() {
    let design = Design::new();
    let preview = design.preview(mixed_patch(), true);
    let mut payload = design.payload.clone();
    let applied = design.run(&preview, "apply", |r| rpc(&mut payload, &r)).unwrap();
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(payload["components"][0]["name"], "47k");
    assert_eq!(payload["wires"][0]["net"], "POWER");
    assert!(design.run(&preview, "apply", |_| panic!("replay must not call EDA")).is_err());
    let rolled_back = design.run(&preview, "rollback", |r| rpc(&mut payload, &r)).unwrap();
    assert_eq!(rolled_back["execution"]["verification"]["status"], "passed", "{rolled_back}");
    assert_eq!(payload, design.payload);
    let record = ProjectStorage::open(&design.root).unwrap().list_change_sets().unwrap().remove(0);
    for event in [
        "preview",
        "approved",
        "apply_requested",
        "step_written",
        "readback",
        "verification",
        "rollback_requested",
        "rollback_executed",
    ] {
        assert!(record.audit.iter().any(|e| e.decision == event), "{event}");
    }
}

#[test]
fn approval_and_fresh_baseline_are_both_required() {
    let design = Design::new();
    let preview = design.preview(property_patch(), false);
    let mut payload = design.payload.clone();
    assert!(
        design
            .run(&preview, "apply", |r| {
                assert_eq!(r["operation"], "inspect");
                rpc(&mut payload, &r)
            })
            .is_err()
    );
    review::decide(&design.root,"p","s",&json!({"planHash":preview["planHash"],"baselineSnapshotHash":design.hash,"decision":"approved"}),2).unwrap();
    payload["components"][0]["name"] = json!("external edit");
    assert!(
        design
            .run(&preview, "apply", |r| {
                assert_eq!(r["operation"], "inspect");
                rpc(&mut payload, &r)
            })
            .is_err()
    );
}

#[test]
fn absent_partial_and_delayed_readbacks_never_undo_confirmed_writes() {
    for mode in ["missing", "partial", "delayed"] {
        let design = Design::new();
        let preview = design.preview(property_patch(), true);
        let mut payload = design.payload.clone();
        let result = design
            .run(&preview, "apply", |r| {
                assert_ne!(r["operation"], "rollback");
                if r["operation"] == "readback" {
                    return match mode {
                        "missing" => Err("readback unavailable".into()),
                        "partial" => {
                            Ok(json!({"ok":true,"payload":payload,"inconclusive":["pins"]}))
                        }
                        _ => Ok(json!({"ok":true,"payload":design.payload,"inconclusive":[]})),
                    };
                }
                rpc(&mut payload, &r)
            })
            .unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["execution"]["write"]["status"], "passed");
        assert_eq!(result["execution"]["verification"]["status"], "inconclusive");
        assert_eq!(payload["components"][0]["name"], "47k");
        let retry = design.run(&preview, "readback", |r| rpc(&mut payload, &r)).unwrap();
        assert_eq!(retry["ok"], true, "{retry}");
    }
}

#[test]
fn partial_failure_stops_remaining_steps_and_reports_compensation() {
    let design = Design::new();
    let preview = design.preview(mixed_patch(), true);
    let mut payload = design.payload.clone();
    let mut writes = 0;
    let result = design
        .run(&preview, "apply", |r| {
            if r["operation"] == "apply" {
                writes += 1;
                if writes == 2 {
                    return Ok(json!({"ok":false,"payload":{"writeStatus":"failed"}}));
                }
            }
            rpc(&mut payload, &r)
        })
        .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(writes, 2);
    assert_eq!(payload, design.payload);
    assert!(
        result["audit"].as_array().unwrap().iter().any(
            |e| e["decision"] == "compensation" && e["reason"].as_str().unwrap().contains("Ok")
        )
    );
}

#[test]
fn cancellation_collects_in_flight_result_then_stops_and_allows_explicit_rollback() {
    let design = Design::new();
    let preview = design.preview(mixed_patch(), true);
    let mut payload = design.payload.clone();
    let mut writes = 0;
    let result = design
        .run(&preview, "apply", |r| {
            let mut result = rpc(&mut payload, &r)?;
            if r["operation"] == "apply" {
                writes += 1;
                result["cancelRequested"] = json!(true);
            }
            Ok(result)
        })
        .unwrap();
    assert_eq!(writes, 1);
    assert_eq!(result["ok"], false);
    assert_eq!(payload["components"][0]["name"], "47k");
    assert_eq!(payload["wires"][0]["net"], "VCC");
    assert!(result["audit"].as_array().unwrap().iter().any(|e| e["decision"] == "cancelled"));
    design.run(&preview, "rollback", |r| rpc(&mut payload, &r)).unwrap();
    assert_eq!(payload, design.payload);
}

#[test]
fn unknown_write_is_not_retried_or_compensated_and_external_edits_are_not_overwritten() {
    let design = Design::new();
    let preview = design.preview(property_patch(), true);
    let mut payload = design.payload.clone();
    let result = design
        .run(&preview, "apply", |r| {
            if r["operation"] == "apply" {
                Err("connection lost".into())
            } else {
                rpc(&mut payload, &r)
            }
        })
        .unwrap();
    assert_eq!(result["execution"]["write"]["status"], "inconclusive");
    let rollback = design
        .run(&preview, "rollback", |r| {
            assert_eq!(r["operation"], "readback");
            rpc(&mut payload, &r)
        })
        .unwrap();
    assert!(
        rollback["audit"].as_array().unwrap().iter().any(|e| e["decision"] == "rollback_error")
    );

    let design = Design::new();
    let preview = design.preview(property_patch(), true);
    let mut payload = design.payload.clone();
    design.run(&preview, "apply", |r| rpc(&mut payload, &r)).unwrap();
    payload["components"][0]["name"] = json!("external edit");
    design.run(&preview, "rollback", |r| rpc(&mut payload, &r)).unwrap();
    assert_eq!(payload["components"][0]["name"], "external edit");
}

#[test]
fn timed_out_delete_compensation_requires_idle_exact_readback_before_retry() {
    for mode in [
        "retry",
        "status_probe",
        "busy",
        "legacy_executor",
        "drift",
        "partial",
        "in_flight",
        "legacy_journal",
    ] {
        let mut design = Design::new();
        design.payload["components"][0]["libraryUuid"] = json!("lib");
        design.payload["components"][0]["componentUuid"] = json!("device");
        let build = observation::build_observation(&design.payload, None).unwrap();
        design.hash = build.snapshot.snapshot_hash.clone();
        observation::persist_inspect_record(&design.root, &json!({"kind":"inspect","projectId":"p","sessionId":"s",
            "snapshot":build.snapshot,"externalRefs":build.external_refs,"validation":build.validation,"inconclusive":[],
            "physicalProjection":build.physical_projection,"observedPayload":design.payload})).unwrap();
        let preview =
            design.preview(json!([{"remove_component":{"component_id":"component:R1"}}]), true);
        let mut payload = design.payload.clone();
        let applied = design
            .run(&preview, "apply", |r| {
                if r["operation"] == "apply" {
                    payload["components"] = json!([]);
                    return Ok(json!({"ok":true,"payload":{"written":true}}));
                }
                rpc(&mut payload, &r)
            })
            .unwrap();
        assert_eq!(applied["ok"], true, "{applied}");
        let restored_review =
            review::latest_session_review(&design.root, "p", "s").unwrap().unwrap();
        assert_eq!(restored_review["started"], true);
        assert_eq!(restored_review["approved"], false);
        assert_eq!(restored_review["planHash"], preview["planHash"]);
        assert!(review::latest_session_review(&design.root, "p", "other").unwrap().is_none());
        let failed = design
            .run(&preview, "rollback", |r| {
                if r["operation"] == "rollback" {
                    return Err("operation_timeout：执行器结果未知".into());
                }
                rpc(&mut payload, &r)
            })
            .unwrap();
        assert_eq!(failed["ok"], false);
        assert!(failed["failure"].as_str().unwrap().contains("operation_timeout"));
        // 模拟进程中断以及旧版本日志，不改真实工程。
        let journal = design
            .root
            .join("observations")
            .join(format!("execution-{}.json", &preview["planHash"].as_str().unwrap()[7..]));
        if matches!(mode, "in_flight" | "legacy_journal") {
            let mut value: Value =
                serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
            if mode == "in_flight" {
                value["units"][0]["state"] = json!("in_flight");
            } else {
                value["units"][0].as_object_mut().unwrap().remove("direction");
            }
            std::fs::write(&journal, serde_json::to_vec(&value).unwrap()).unwrap();
        }
        if mode == "drift" {
            payload["wires"][0]["net"] = json!("EXTERNAL");
        }
        let mut creates = 0;
        let mut reads = 0;
        let recovered = design
            .run(&preview, "rollback", |r| {
                if r["operation"] == "rollback" {
                    creates += 1;
                    payload = design.payload.clone();
                    payload["components"][0]["primitiveId"] = json!("new-r1");
                    return Ok(json!({"ok":true,"payload":{"written":true,"createdId":"new-r1"}}));
                }
                reads += 1;
                if mode == "status_probe" && r["params"]["statusOnly"] == true {
                    return Ok(json!({"ok":true,"mutationIdle":true,"extensionVersion":"0.2.19",
                        "payload":{"executorStatus":true}}));
                }
                let mut result = rpc(&mut payload, &r)?;
                if r["params"]["requireIdle"] == true {
                    if mode != "legacy_executor" {
                        result["mutationIdle"] = json!(mode != "busy");
                    }
                    if mode == "partial" {
                        result["inconclusive"] = json!(["components"]);
                    }
                }
                Ok(result)
            })
            .unwrap();
        let recoverable = matches!(mode, "retry" | "status_probe" | "in_flight" | "legacy_journal");
        assert_eq!(recovered["ok"], recoverable, "{mode}: {recovered}");
        assert_eq!(creates, usize::from(recoverable), "{mode}");
        if matches!(mode, "busy" | "legacy_executor") {
            assert_eq!(reads, 1, "不追加整页回读");
            let code = if mode == "busy" { "executor_busy" } else { "executor_idle_unsupported" };
            assert!(recovered["failure"].as_str().unwrap().contains(code));
            assert!(
                recovered["audit"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["decision"] == "rollback_executor_status")
            );
        }
        let context = review::design_context(&design.root, "p").unwrap().unwrap();
        assert!(context["changeSets"][0]["lastError"].is_object());
        assert!(context["changeSets"][0]["rollbackNote"].as_str().unwrap().contains("不依赖"));
    }
}
