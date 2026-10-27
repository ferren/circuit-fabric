//! Bridge 操作协议端到端测试（`docs/bridge-operations-protocol.zh-CN.md`）：
//! 执行器通道握手、`inspect`/`drc`/`visual_capture` 全链路、观测持久化与快照
//! 稳定性、取消、执行器断开与能力诚实化。

mod common;

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use circuitfabric_project::ProjectStorage;
use common::{RunningBridge, WsClient, write_registry};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// 测试夹具
// ---------------------------------------------------------------------------

fn executor_hello() -> Value {
    json!({
        "type": "executor_hello",
        "protocolVersion": 1,
        "extensionId": "circuitfabric-jlc-eda",
        "extensionVersion": "0.0.1-test",
        "eda": { "edition": "jlceda-pro", "client": true, "web": false, "apiTypesVersion": "0.4.26" },
    })
}

/// 连接 `/eda-operations` 并完成 `executor_hello` 握手。
fn connect_executor(address: &str) -> WsClient {
    let mut executor = WsClient::connect(address, "/eda-operations");
    executor.send(&executor_hello());
    let ack = executor.recv();
    assert_eq!(ack["type"], "executor_hello_ack");
    assert_eq!(ack["protocolVersion"], 1);
    assert_eq!(ack["bridge"], "CircuitFabric");
    assert_eq!(
        ack["operations"],
        json!([
            "inspect",
            "read_context",
            "drc",
            "visual_capture",
            "apply",
            "readback",
            "rollback"
        ])
    );
    assert_eq!(ack["unsupportedOperations"], json!([]));
    executor
}

fn select_project(client: &mut WsClient, project_id: &str) {
    let reply = client.exchange(&json!({"type":"select_project","projectId":project_id}));
    assert_eq!(reply["type"], "project_selected", "unexpected reply: {reply}");
}

/// 注册项目并创建项目存储，返回项目根目录。
fn register_project(bridge: &RunningBridge, project_id: &str) -> PathBuf {
    write_registry(&bridge.dir, project_id);
    let root = bridge.dir.join("project-root");
    std::fs::create_dir_all(&root).unwrap();
    ProjectStorage::create(
        &root,
        circuitfabric_contracts::Project {
            id: project_id.into(),
            name: "Power Supply".into(),
            description: None,
        },
    )
    .unwrap();
    root
}

/// observation.rs 测试同款 inspect 观测载荷：2 个元件、VCC 网络。
/// 元件含库身份与放置（删除补偿/放置来源所需）。
fn inspect_payload() -> Value {
    json!({
        "document": { "kind": "schematic", "projectUuid": "p1", "projectName": "电源",
                      "pageUuid": "page-1", "pageName": "Sheet1" },
        "components": [
            { "primitiveId": "prim-r1", "designator": "R1", "name": "10k", "footprint": "0603",
              "libraryUuid": "lib-1", "componentUuid": "cmp-r1",
              "x": 0, "y": 0, "rotation": 90, "mirror": false,
              "pins": [ { "number": "1", "name": "A", "x": 0, "y": 0 },
                        { "number": "2", "name": "B", "x": 0, "y": 10 } ] },
            { "primitiveId": "prim-r2", "designator": "R2", "name": "1k", "footprint": "0603",
              "libraryUuid": "lib-1", "componentUuid": "cmp-r2",
              "x": 40, "y": 0, "rotation": 0, "mirror": false,
              "pins": [ { "number": "1", "name": "A", "x": 40, "y": 10 },
                        { "number": "2", "name": "B", "x": 40, "y": 0 } ] }
        ],
        "wires": [ { "primitiveId": "w1", "net": "VCC", "points": [[0, 0], [40, 0]] } ],
        "buses": [ { "primitiveId": "b1", "name": "DBUS" } ],
        "nets": [ { "name": "VCC", "wires": [ { "name": "w1", "id": "w1", "pageName": "Sheet1", "pageUuid": "page-1" } ] } ]
    })
}

fn operation_result(request_id: &str, operation: &str, payload: &Value) -> Value {
    json!({
        "type": "operation_result",
        "requestId": request_id,
        "operation": operation,
        "ok": true,
        "capturedAt": "2026-10-22T08:00:00.000Z",
        "payload": payload,
        "inconclusive": [],
    })
}

fn inspect_request() -> Value {
    json!({"type":"inspect","sessionId":"s1","document":{"kind":"schematic"}})
}

#[test]
fn classifying_symbols_fixes_the_baseline_without_bypassing_approval_guards() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "p1");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    session.exchange(&json!({"type":"hello"}));
    select_project(&mut session, "p1");
    let mut legacy = inspect_payload();
    let mut flag = legacy["components"][0].clone();
    flag["designator"] = Value::Null;
    flag["primitiveId"] = json!("symbol-gnd");
    flag["name"] = json!("GND");
    flag["pins"] = json!([]);
    legacy["components"].as_array_mut().unwrap().push(flag.clone());
    let incomplete = complete_inspect(&mut session, &mut executor, &legacy, &["components"]);
    assert_eq!(incomplete["diagnostics"][0]["code"], "missing_designator");
    let request_preview = |hash: &Value| {
        json!({"type":"preview","sessionId":"s1", "baselineSnapshotHash":hash,
        "patch":{"base_snapshot_hash":hash,"operations":[{"remove_component":{"component_id":"component:R1"}}],"rationale":"只删除 R1"}})
    };
    let old_preview = session.exchange(&request_preview(&incomplete["snapshotHash"]));
    assert_eq!(old_preview["approvalAllowed"], false);
    let old_approval = json!({"type":"approve_plan","sessionId":"s1","planHash":old_preview["planHash"],
        "baselineSnapshotHash":incomplete["snapshotHash"],"decision":"approved"});
    assert_eq!(session.exchange(&old_approval)["code"], "review_rejected");
    let mut fixed = inspect_payload();
    flag["componentType"] = json!("netflag");
    fixed["symbols"] = json!([flag]);
    let complete = complete_inspect(&mut session, &mut executor, &fixed, &[]);
    assert_eq!(complete["inconclusive"], json!([]));
    assert_eq!(complete["diagnostics"], json!([]));
    assert_eq!(complete["components"], 2);
    assert_ne!(complete["snapshotHash"], incomplete["snapshotHash"]);
    assert_eq!(session.exchange(&old_approval)["code"], "review_rejected", "旧预览不得复用");
    let preview = session.exchange(&request_preview(&complete["snapshotHash"]));
    assert_eq!(preview["approvalAllowed"], true);
    assert_eq!(preview["write"], "not_run");
    let record: Value = serde_json::from_slice(
        &std::fs::read(
            root.join("observations")
                .join(format!("{}.json", &complete["snapshotHash"].as_str().unwrap()[7..])),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(record["observedPayload"]["symbols"].as_array().unwrap().len(), 1);
    assert_eq!(record["diagnostics"], json!([]));
}

fn observations(root: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(root.join("observations"))
        .expect("observations directory")
        .map(|entry| entry.expect("entry").path())
        .collect()
}

fn complete_inspect(
    session: &mut WsClient,
    executor: &mut WsClient,
    payload: &Value,
    missing: &[&str],
) -> Value {
    session.send(&inspect_request());
    assert_eq!(session.recv()["type"], "operation_started");
    let request = executor.recv();
    assert_eq!(request["operation"], "inspect", "预览不得向执行器发送操作");
    let mut result = operation_result(request["requestId"].as_str().unwrap(), "inspect", payload);
    result["inconclusive"] = json!(missing);
    executor.send(&result);
    let reply = session.recv();
    assert_eq!(reply["type"], "inspect_result", "{reply}");
    reply
}

fn annotation_preview(hash: &Value) -> Value {
    json!({"type":"preview", "sessionId":"s1", "baselineSnapshotHash":hash,
        "patch":{"base_snapshot_hash":hash,"operations":[{"annotate_semantic":{"key":"domain","value":"power"}}],"rationale":"标注用途"}})
}

fn approved_mutation(session: &mut WsClient, hash: &Value) -> Value {
    let preview =
        session.exchange(&json!({"type":"preview","sessionId":"s1","baselineSnapshotHash":hash,
        "patch":{"base_snapshot_hash":hash,"rationale":"属性与导线变更","operations":[
            {"set_component_value":{"component_id":"component:R1","value":"47k"}},
            {"rename_net":{"net_id":"net:VCC","name":"POWER"}}]}}));
    assert_eq!(preview["approvalAllowed"], true, "{preview}");
    let approval=session.exchange(&json!({"type":"approve_plan","sessionId":"s1","planHash":preview["planHash"],"baselineSnapshotHash":hash,"decision":"approved"}));
    assert_eq!(approval["decision"], "approved", "{approval}");
    preview
}

fn reply_to(executor: &mut WsClient, request: &Value, payload: &Value) {
    executor.send(&operation_result(
        request["requestId"].as_str().unwrap(),
        request["operation"].as_str().unwrap(),
        payload,
    ));
}

#[test]
fn approved_property_and_wire_changes_apply_readback_and_rollback_over_websocket() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let baseline = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);
    let preview = approved_mutation(&mut session, &baseline["snapshotHash"]);
    let storage = ProjectStorage::open(&root).unwrap();
    let usage = circuitfabric_project::SessionUsage { input_tokens: 123, output_tokens: 45 };
    let citations = vec!["keep-existing-reference".to_owned()];
    storage
        .complete_session(
            "s1",
            usage,
            citations.clone(),
            circuitfabric_project::SessionStatus::Completed,
        )
        .unwrap();
    session.send(&json!({"type":"apply","sessionId":"s1","planHash":preview["planHash"]}));
    assert_eq!(session.recv()["type"], "operation_started");
    let request = executor.recv();
    assert_eq!(request["operation"], "inspect");
    assert_eq!(
        storage.load_session("s1").unwrap().metadata.status,
        circuitfabric_project::SessionStatus::Running
    );
    reply_to(&mut executor, &request, &inspect_payload());
    for kind in ["component_value", "wire_net"] {
        let request = executor.recv();
        assert_eq!(request["operation"], "apply");
        assert_eq!(request["params"]["kind"], kind);
        assert_eq!(request["planHash"], preview["planHash"]);
        assert!(request["scope"].as_array().unwrap().contains(&json!("project.write")));
        reply_to(&mut executor, &request, &json!({"written":true}));
    }
    let mut changed = inspect_payload();
    changed["components"][0]["name"] = json!("47k");
    changed["wires"][0]["net"] = json!("POWER");
    changed["nets"][0]["name"] = json!("POWER");
    let request = executor.recv();
    assert_eq!(request["operation"], "readback");
    reply_to(&mut executor, &request, &changed);
    let applied = session.recv();
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(
        storage.load_session("s1").unwrap().metadata.status,
        circuitfabric_project::SessionStatus::Completed
    );
    session.send(&json!({"type":"rollback","sessionId":"s1","planHash":preview["planHash"]}));
    assert_eq!(session.recv()["type"], "operation_started");
    for kind in ["wire_net", "component_value"] {
        let request = executor.recv();
        assert_eq!(request["operation"], "rollback");
        assert_eq!(
            storage.load_session("s1").unwrap().metadata.status,
            circuitfabric_project::SessionStatus::Running
        );
        assert_eq!(request["params"]["kind"], kind);
        reply_to(&mut executor, &request, &json!({"written":true}));
    }
    let request = executor.recv();
    assert_eq!(request["operation"], "readback");
    reply_to(&mut executor, &request, &inspect_payload());
    let rolled_back = session.recv();
    assert_eq!(rolled_back["ok"], true, "{rolled_back}");
    let metadata = storage.load_session("s1").unwrap().metadata;
    assert_eq!(metadata.status, circuitfabric_project::SessionStatus::Completed);
    assert_eq!(metadata.usage, usage);
    assert_eq!(metadata.citations, citations);
    // 验证失败也必须结束运行态，且不能把旧成功回读当作本次结果。
    session.send(&json!({"type":"readback","sessionId":"s1","planHash":preview["planHash"]}));
    assert_eq!(session.recv()["type"], "operation_started");
    let request = executor.recv();
    assert_eq!(
        storage.load_session("s1").unwrap().metadata.status,
        circuitfabric_project::SessionStatus::Running
    );
    reply_to(&mut executor, &request, &changed);
    assert_eq!(session.recv()["ok"], false);
    assert_eq!(
        storage.load_session("s1").unwrap().metadata.status,
        circuitfabric_project::SessionStatus::Failed
    );
    let storage = ProjectStorage::open(&root).unwrap();
    let body = storage.load_session("s1").unwrap().body;
    for event in ["apply 请求", "step_written", "readback", "verification", "rollback_executed"] {
        assert!(body.contains(event), "{event}");
    }
}

/// 结构类写入的模拟执行器：按规格真实地变更观测载荷（删除/放置/连线/重建）。
fn structural_rpc(payload: &mut Value, request: &Value) -> Result<Value, String> {
    if matches!(request["operation"].as_str(), Some("inspect" | "readback")) {
        return Ok(json!({"ok":true,"payload":payload.clone(),"inconclusive":[]}));
    }
    let p = &request["params"];
    let kind = p["kind"].as_str().unwrap_or_default();
    let rollback = request["operation"] == "rollback";
    let fail = |message: &str| {
        Ok(json!({"ok":false,"payload":{"written":false,"writeStatus":"failed","message":message}}))
    };
    let pin_xy =
        |payload: &Value, designator: &str, pin: &str| -> Option<(i64, i64)> {
            payload["components"].as_array()?.iter().find(|c| c["designator"] == designator)?
            ["pins"].as_array()?.iter().find(|entry| entry["number"] == pin)
            .map(|entry| (entry["x"].as_i64().unwrap_or(0), entry["y"].as_i64().unwrap_or(0)))
        };
    match (kind, rollback) {
        // 属性写入与恢复：CAS 语义，当前值须等于期望值。
        ("component_value" | "wire_net", _) => {
            let current = if kind == "component_value" {
                &mut payload["components"][0]["name"]
            } else {
                &mut payload["wires"][0]["net"]
            };
            if *current != p["expected"] {
                return Ok(
                    json!({"ok":false,"payload":{"written":false,"writeStatus":"failed","message":"属性漂移"}}),
                );
            }
            *current = p["value"].clone();
            if kind == "wire_net" {
                payload["nets"][0]["name"] = p["value"].clone();
            }
            Ok(json!({"ok":true,"payload":{"written":true}}))
        }
        ("component_delete", false) => {
            let id = p["primitiveId"].as_str().unwrap_or_default();
            let designator = p["spec"]["designator"].as_str().unwrap_or_default();
            let components = payload["components"].as_array_mut().unwrap();
            match components
                .iter()
                .position(|c| c["primitiveId"] == id && c["designator"] == designator)
            {
                Some(index) => {
                    components.remove(index);
                    Ok(json!({"ok":true,"payload":{"written":true}}))
                }
                None => fail("目标元件不存在或身份漂移"),
            }
        }
        ("component_delete", true) => {
            // 回滚 = 用补偿数据重建（图元 ID 由 EDA 重新分配）。
            let restore = p["spec"]["restore"].clone();
            if restore["designator"].is_null() {
                return fail("缺少补偿数据");
            }
            let mut restored = restore;
            let designator = restored["designator"].as_str().unwrap_or_default().to_owned();
            restored["primitiveId"] = json!(format!("restored-{designator}"));
            payload["components"].as_array_mut().unwrap().push(restored.clone());
            Ok(json!({"ok":true,"payload":{"written":true,"createdId":restored["primitiveId"]}}))
        }
        ("component_place", false) => {
            let spec = &p["spec"];
            let x = spec["x"].as_i64().unwrap_or_default();
            let y = spec["y"].as_i64().unwrap_or_default();
            let created = json!({ "primitiveId": "gge-J1", "designator": spec["designator"],
                "name": spec["name"], "footprint": "HDR-8P",
                "libraryUuid": spec["libraryUuid"], "componentUuid": spec["componentUuid"],
                "x": x, "y": y, "rotation": spec["rotation"], "mirror": spec["mirror"],
                "pins": [ { "number": "1", "name": "VCC", "x": x, "y": y + 10 },
                          { "number": "2", "name": "GND", "x": x + 10, "y": y } ] });
            payload["components"].as_array_mut().unwrap().push(created.clone());
            Ok(json!({"ok":true,"payload":{"written":true,"createdId":"gge-J1"}}))
        }
        ("wire_create", false) => {
            let connection = &p["spec"]["connection"];
            let Some((px, py)) = pin_xy(
                payload,
                connection["pin"]["designator"].as_str().unwrap_or_default(),
                connection["pin"]["pin"].as_str().unwrap_or_default(),
            ) else {
                return fail("引脚坐标不可解析");
            };
            let (ax, ay) = if let (Some(x), Some(y)) =
                (connection["anchor"]["x"].as_i64(), connection["anchor"]["y"].as_i64())
            {
                (x, y)
            } else if let Some(pin) = connection["anchor"]["pin"].as_object() {
                let Some((x, y)) = pin_xy(
                    payload,
                    pin["designator"].as_str().unwrap_or_default(),
                    pin["pin"].as_str().unwrap_or_default(),
                ) else {
                    return fail("锚点引脚坐标不可解析");
                };
                (x, y)
            } else {
                (px + 100, py) // 短桩
            };
            let id = format!("wire-new-{}", payload["wires"].as_array().map_or(0, Vec::len));
            payload["wires"].as_array_mut().unwrap().push(json!({
                "primitiveId": id, "net": p["spec"]["net"], "points": [[px, py], [ax, ay]] }));
            Ok(json!({"ok":true,"payload":{"written":true,"createdId":id}}))
        }
        // 新建类回滚 = 删除新建图元；图元已不存在视为已达成（幂等）。
        ("component_place" | "wire_create", true) => {
            let id = p["primitiveId"].as_str().unwrap_or_default();
            for field in ["components", "wires"] {
                if let Some(items) = payload[field].as_array_mut() {
                    items.retain(|item| item["primitiveId"] != id);
                }
            }
            Ok(json!({"ok":true,"payload":{"written":true}}))
        }
        _ => fail("不支持的写入类型"),
    }
}

#[test]
fn structural_patch_previews_approves_applies_reads_back_and_rolls_back_over_websocket() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let baseline = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);

    // 模型提交的扁平载荷：clone_from 指向观测位号复用库件身份；
    // 删除以观测库身份补偿，新增网络逐脚连线。
    let preview = session.exchange(&json!({"type":"preview", "sessionId":"s1",
    "baselineSnapshotHash":baseline["snapshotHash"],
    "patch":{"base_snapshot_hash":baseline["snapshotHash"], "rationale":"无源转接板",
        "operations":[
            {"remove_component":{"component_id":"component:R2"}},
            {"rename_net":{"net_id":"net:VCC","name":"GND"}},
            {"add_component":{"reference":"J1","value":"HOST_SPI_8P","clone_from":"R1",
                "pins":[{"id":"1","name":"VCC"},{"id":"2","name":"GND"}]}},
            {"add_net":{"name":"GND","pins":[{"component_id":"component:J1","pin_id":"2"}]}},
            {"add_net":{"name":"BS","pins":[{"component_id":"component:J1","pin_id":"1"}]}}
        ]}}));
    assert_eq!(preview["type"], "preview_result", "{preview}");
    assert_eq!(preview["approvalAllowed"], true, "{preview}");
    let risks = preview["plan"]["risks"].to_string();
    assert!(!risks.contains("不存在的引脚"), "同补丁新增的 J1 引脚必须可解析：{risks}");
    let diff = preview["plan"]["diff"].to_string();
    assert!(diff.contains("新增元件 J1"), "{diff}");
    assert!(diff.contains("挂接"), "{diff}");
    assert!(diff.contains("复用 R1 库件"), "{diff}");

    // 放置/删除规格随计划落盘，供执行器使用。
    let plan_hash = preview["planHash"].as_str().unwrap();
    let persisted = jlcircuit_eda_bridge::plan::load_plan(&root, plan_hash).unwrap().unwrap();
    assert_eq!(persisted.steps.len(), 5);
    let place = persisted.steps.iter().find(|step| step.logical_id == "component:J1").unwrap();
    assert_eq!(place.spec["libraryUuid"], "lib-1");
    assert_eq!(place.spec["designator"], "J1");
    let delete = persisted.steps.iter().find(|step| step.logical_id == "component:R2").unwrap();
    assert_eq!(delete.rollback.data["libraryUuid"], "lib-1");
    assert_eq!(delete.rollback.data["designator"], "R2");

    let approval = session.exchange(&json!({"type":"approve_plan","sessionId":"s1",
        "planHash":preview["planHash"],"baselineSnapshotHash":baseline["snapshotHash"],
        "decision":"approved"}));
    assert_eq!(approval["decision"], "approved", "{approval}");

    session.send(&json!({"type":"apply","sessionId":"s1","planHash":preview["planHash"]}));
    assert_eq!(session.recv()["type"], "operation_started");
    // 写入前新 inspect。
    let mut request = executor.recv();
    assert_eq!(request["operation"], "inspect");
    reply_to(&mut executor, &request, &inspect_payload());
    let mut payload = inspect_payload();
    // 五个写入单元：删除 R2、导线改名、放置 J1、GND 挂接、BS 新建。
    for expected in
        ["component_delete", "wire_net", "component_place", "wire_create", "wire_create"]
    {
        request = executor.recv();
        assert_eq!(request["operation"], "apply", "{request}");
        assert_eq!(request["params"]["kind"], expected, "{request}");
        assert!(request["scope"].as_array().unwrap().contains(&json!("project.write")));
        let reply = structural_rpc(&mut payload, &request).unwrap();
        assert_eq!(reply["payload"]["written"], true, "{request}");
        executor.send(&json!({"type":"operation_result","requestId":request["requestId"],
            "operation":"apply","ok":true,"capturedAt":"2026-10-23T08:00:00.000Z",
            "payload":reply["payload"],"inconclusive":[]}));
    }
    let readback_request = executor.recv();
    assert_eq!(readback_request["operation"], "readback");
    executor.send(&json!({"type":"operation_result","requestId":readback_request["requestId"],
        "operation":"readback","ok":true,"capturedAt":"2026-10-23T08:00:01.000Z",
        "payload":payload,"inconclusive":[]}));
    let applied = session.recv();
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(applied["execution"]["write"]["status"], "passed", "{applied}");
    assert_eq!(applied["execution"]["verification"]["status"], "passed", "{applied}");
    assert_eq!(
        applied["execution"]["verification"]["detail"],
        "结构断言全部成立（含放置/删除/连线操作）。"
    );

    // 回滚：逆序删除新建导线与元件、恢复改名、按补偿数据重建 R2，回到基线。
    session.send(&json!({"type":"rollback","sessionId":"s1","planHash":preview["planHash"]}));
    assert_eq!(session.recv()["type"], "operation_started");
    // 从 apply 后的设计状态开始逆推。
    let mut payload = {
        let mut p = inspect_payload();
        p["components"].as_array_mut().unwrap().retain(|c| c["designator"] != "R2");
        p["components"].as_array_mut().unwrap().push(json!({
            "primitiveId": "gge-J1", "designator": "J1", "name": "HOST_SPI_8P", "footprint": "HDR-8P",
            "libraryUuid": "lib-1", "componentUuid": "cmp-r1", "x": 200, "y": 200, "rotation": 90, "mirror": false,
            "pins": [ { "number": "1", "name": "VCC", "x": 200, "y": 210 },
                      { "number": "2", "name": "GND", "x": 210, "y": 200 } ] }));
        p["wires"][0]["net"] = json!("GND");
        p["nets"][0]["name"] = json!("GND");
        p["wires"]
            .as_array_mut()
            .unwrap()
            .push(json!({"primitiveId":"wire-new-1","net":"GND","points":[[210,200],[0,0]]}));
        p["wires"]
            .as_array_mut()
            .unwrap()
            .push(json!({"primitiveId":"wire-new-2","net":"BS","points":[[200,210],[300,210]]}));
        p
    };
    for expected in
        ["wire_create", "wire_create", "component_place", "wire_net", "component_delete"]
    {
        request = executor.recv();
        assert_eq!(request["operation"], "rollback", "{request}");
        assert_eq!(request["params"]["kind"], expected, "{request}");
        let reply = structural_rpc(&mut payload, &request).unwrap();
        assert_eq!(reply["payload"]["written"], true, "{request}");
        executor.send(&json!({"type":"operation_result","requestId":request["requestId"],
            "operation":"rollback","ok":true,"capturedAt":"2026-10-23T08:00:02.000Z",
            "payload":reply["payload"],"inconclusive":[]}));
    }
    let readback_request = executor.recv();
    assert_eq!(readback_request["operation"], "readback");
    executor.send(&json!({"type":"operation_result","requestId":readback_request["requestId"],
        "operation":"readback","ok":true,"capturedAt":"2026-10-23T08:00:03.000Z",
        "payload":payload,"inconclusive":[]}));
    let rolled_back = session.recv();
    assert_eq!(rolled_back["ok"], true, "{rolled_back}");
    assert_eq!(rolled_back["execution"]["verification"]["detail"], "回滚后完整回读与基线一致。");

    let storage = ProjectStorage::open(&root).unwrap();
    let record = storage
        .list_change_sets()
        .unwrap()
        .into_iter()
        .find(|record| record.plan_hash == plan_hash)
        .expect("ChangeSet");
    for event in [
        "preview",
        "approved",
        "apply_requested",
        "step_written",
        "write_completed",
        "readback",
        "rollback_requested",
        "rollback_executed",
    ] {
        assert!(record.audit.iter().any(|entry| entry.decision == event), "{event}");
    }
}

#[test]
fn mixed_passive_adapter_patch_previews_with_flat_author_payloads() {
    let bridge = RunningBridge::spawn();
    register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let baseline = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);

    // clone_from 缺失或不可解析时，放置步骤前置条件不满足，计划整体不可批准。
    let preview = session.exchange(&json!({"type":"preview", "sessionId":"s1",
    "baselineSnapshotHash":baseline["snapshotHash"],
    "patch":{"base_snapshot_hash":baseline["snapshotHash"], "rationale":"无源转接板",
        "operations":[
            {"remove_component":{"component_id":"component:R2"}},
            {"rename_net":{"net_id":"net:VCC","name":"GND"}},
            {"add_component":{"reference":"J1","value":"HOST_SPI_8P",
                "pins":[{"id":"1","name":"VCC"},{"id":"2","name":"GND"}]}},
            {"add_net":{"name":"GND","pins":[{"component_id":"component:J1","pin_id":"2"}]}}
        ]}}));
    assert_eq!(preview["type"], "preview_result", "{preview}");
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["approvalAllowed"], false, "{preview}");
    let risks = preview["plan"]["risks"].to_string();
    assert!(risks.contains("clone_from"), "{risks}");
}

#[test]
fn cancellation_during_a_write_keeps_its_result_and_skips_later_mutations() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let baseline = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);
    let preview = approved_mutation(&mut session, &baseline["snapshotHash"]);
    session.send(&json!({"type":"apply","sessionId":"s1","planHash":preview["planHash"]}));
    let started = session.recv();
    let request = executor.recv();
    reply_to(&mut executor, &request, &inspect_payload());
    let request = executor.recv();
    assert_eq!(request["operation"], "apply");
    session.send(&json!({"type":"cancel_operation","requestId":started["requestId"]}));
    executor.send(&json!({"type":"operation_progress","requestId":request["requestId"],"operation":"apply","phase":"write_in_flight","completed":0,"total":1}));
    assert_eq!(session.recv()["type"], "operation_progress");
    reply_to(&mut executor, &request, &json!({"written":true}));
    let next = executor.recv();
    assert_eq!(next["operation"], "readback", "剩余网络写入必须停止");
    let mut changed = inspect_payload();
    changed["components"][0]["name"] = json!("47k");
    reply_to(&mut executor, &next, &changed);
    let result = session.recv();
    assert_eq!(result["ok"], false, "{result}");
    let record = ProjectStorage::open(&root).unwrap().list_change_sets().unwrap().remove(0);
    assert!(record.audit.iter().any(|e| e.decision == "step_written"));
    assert!(record.audit.iter().any(|e| e.decision == "cancelled"));
    assert!(record.rollback_handle.is_some());
}

#[test]
fn preview_and_explicit_approval_persist_without_claiming_execution() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let baseline = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);
    drop(executor); // preview/approval must work without an EDA executor.
    let request = annotation_preview(&baseline["snapshotHash"]);
    let preview = session.exchange(&request);
    assert_eq!(preview["type"], "preview_result", "{preview}");
    assert_eq!(preview["approvalAllowed"], true);
    assert_eq!(preview["write"], "not_run");
    assert_eq!(preview["readback"], "not_run");
    assert_eq!(preview["verification"], "inconclusive");
    assert_eq!(preview["plan"]["document"]["pageUuid"], "page-1");
    let decision = json!({"type":"approve_plan", "sessionId":"s1", "planHash":preview["planHash"],
        "baselineSnapshotHash":baseline["snapshotHash"], "decision":"approved"});
    assert_eq!(session.exchange(&decision)["decision"], "approved");
    let again = session.exchange(&request);
    assert_eq!(again["planHash"], preview["planHash"]);
    let storage = ProjectStorage::open(&root).unwrap();
    let records = storage.list_change_sets().unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.decision().unwrap().decision, "approved", "重复预览不能擦除审批");
    assert_eq!(record.execution, Default::default(), "审批不得伪造执行状态");
    assert_eq!(storage.list_logical_snapshots().unwrap().len(), 1);
    assert!(storage.load_session("s1").unwrap().body.contains("approve_plan 完成"));
    let mut wrong_session = decision.clone();
    wrong_session["sessionId"] = json!("other");
    assert_eq!(session.exchange(&wrong_session)["code"], "review_rejected");
    let plan_hash = preview["planHash"].as_str().unwrap();
    let path = jlcircuit_eda_bridge::plan::plan_path(&root, plan_hash);
    let mut plan: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    plan["createdFromPatchRationale"] = json!("tampered");
    std::fs::write(path, plan.to_string()).unwrap();
    assert_eq!(session.exchange(&decision)["code"], "review_rejected");
}

#[test]
fn preview_leaves_executor_readback_and_baseline_record_unchanged() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let before = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);
    let path = root
        .join("observations")
        .join(format!("{}.json", &before["snapshotHash"].as_str().unwrap()[7..]));
    let baseline_bytes = std::fs::read(&path).unwrap();
    let preview = session.exchange(&annotation_preview(&before["snapshotHash"]));
    assert_eq!(preview["type"], "preview_result");
    assert_eq!(std::fs::read(&path).unwrap(), baseline_bytes, "preview 不得改写观测基线");
    let after = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);
    for field in ["snapshotHash", "logicalHash", "physicalHash", "components", "nets"] {
        assert_eq!(before[field], after[field], "readback 的 {field} 不得变化");
    }
    let repeated = session.exchange(&annotation_preview(&after["snapshotHash"]));
    assert_eq!(preview["planHash"], repeated["planHash"]);
}

#[test]
fn incomplete_observation_and_stale_baseline_cannot_be_approved() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let baseline =
        complete_inspect(&mut session, &mut executor, &inspect_payload(), &["pins", "nets"]);
    assert!(baseline["inconclusive"].as_array().unwrap().contains(&json!("pins")));
    let request = annotation_preview(&baseline["snapshotHash"]);
    let preview = session.exchange(&request);
    assert_eq!(preview["type"], "preview_result");
    assert_eq!(preview["approvalAllowed"], false);
    let decision = json!({"type":"approve_plan", "sessionId":"s1", "planHash":preview["planHash"],
        "baselineSnapshotHash":baseline["snapshotHash"], "decision":"approved"});
    assert_eq!(session.exchange(&decision)["code"], "review_rejected");
    let mut moved = inspect_payload();
    moved["components"][0]["x"] = json!(100);
    complete_inspect(&mut session, &mut executor, &moved, &[]);
    assert_eq!(session.exchange(&request)["code"], "review_rejected");
    assert_eq!(session.exchange(&decision)["code"], "review_rejected");
    let mut invalid = request;
    invalid["baselineSnapshotHash"] = json!("sha256:../../outside");
    assert_eq!(session.exchange(&invalid)["code"], "review_rejected");
    assert_eq!(ProjectStorage::open(root).unwrap().list_change_sets().unwrap().len(), 1);
}

#[test]
fn repeated_content_keeps_identity_drift_history_and_prevents_unsafe_delete_approval() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");
    let first = complete_inspect(&mut session, &mut executor, &inspect_payload(), &[]);
    let mut replaced = inspect_payload();
    replaced["components"][0]["primitiveId"] = json!("replacement-r1");
    let second = complete_inspect(&mut session, &mut executor, &replaced, &[]);
    assert_eq!(first["snapshotHash"], second["snapshotHash"]);
    let observations = jlcircuit_eda_bridge::fusion::load_inspect_observations(&root).unwrap();
    assert_eq!(observations.len(), 2);
    let fusion = jlcircuit_eda_bridge::fusion::build_fusion_view(&observations);
    assert_eq!(
        fusion.mapping_for("component:R1").unwrap().status,
        jlcircuit_eda_bridge::fusion::MappingStatus::Drifted
    );
    let request = json!({"type":"preview","sessionId":"s1","baselineSnapshotHash":second["snapshotHash"],
        "patch":{"base_snapshot_hash":second["snapshotHash"],"rationale":"删除电阻", "operations":[{"remove_component":{"component_id":"component:R1"}}]}});
    let preview = session.exchange(&request);
    assert_eq!(preview["type"], "preview_result", "{preview}");
    // 身份漂移 → 不可批准；删除本身有库身份补偿，计划仍声明可回滚。
    assert_eq!(preview["approvalAllowed"], false);
    assert_eq!(preview["plan"]["rollbackable"], true);
}

// ---------------------------------------------------------------------------
// 执行器通道握手
// ---------------------------------------------------------------------------

#[test]
fn executor_handshake_ack_and_hello_timeout() {
    let bridge = RunningBridge::spawn();
    // 其他路径拒绝握手（非 101 应答）。
    WsClient::connect_rejected(&bridge.address, "/elsewhere");

    let _executor = connect_executor(&bridge.address);

    // hello 的 protocolVersion 不是 1 → 连接被断开。
    let mut bad = WsClient::connect(&bridge.address, "/eda-operations");
    bad.send(&json!({"type":"executor_hello","protocolVersion":2}));
    assert!(bad.recv_frame().map_or(true, |frame| frame.is_none()), "无效 hello 必须断开连接");

    // 握手后 5 秒内不发 hello → 连接被断开。
    let started = Instant::now();
    let mut idle = WsClient::connect(&bridge.address, "/eda-operations");
    let _ = idle.recv_frame(); // 期望 EOF / 超时关闭
    assert!(
        started.elapsed() >= Duration::from_secs(4),
        "the idle executor connection should be closed by the 5s hello timeout, closed after {:?}",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// 能力诚实化
// ---------------------------------------------------------------------------

#[test]
fn status_reports_honest_operations_and_executor_state() {
    let bridge = RunningBridge::spawn();
    let mut session = WsClient::connect(&bridge.address, "/bridge");

    let status = session.exchange(&json!({"type":"status"}));
    assert_eq!(status["type"], "status_ack");
    let mut capabilities: Vec<String> = status["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    capabilities.sort();
    assert_eq!(
        capabilities,
        vec![
            "apply",
            "bridge-chat",
            "bridge-context",
            "drc",
            "inspect",
            "preview",
            "readback",
            "rollback",
            "visual-capture"
        ]
    );
    for dishonest in ["apply", "readback", "rollback"] {
        assert!(
            capabilities.iter().any(|capability| capability == dishonest),
            "status must report `{dishonest}`"
        );
    }
    assert_eq!(
        status["operations"],
        json!([
            "inspect",
            "read_context",
            "drc",
            "visual_capture",
            "preview",
            "apply",
            "readback",
            "rollback"
        ])
    );
    assert_eq!(status["executorConnected"], false);

    // 执行器接入后实时变为 true。
    let _executor = connect_executor(&bridge.address);
    let status = session.exchange(&json!({"type":"status"}));
    assert_eq!(status["executorConnected"], true);
}

// ---------------------------------------------------------------------------
// inspect 端到端
// ---------------------------------------------------------------------------

#[test]
fn inspect_end_to_end_persists_stable_snapshot() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    // 执行器在后台线程里校验请求绑定并回复 progress + result。
    let mut responder_executor = executor;
    let responder = thread::spawn(move || {
        let request = responder_executor.recv();
        assert_eq!(request["type"], "eda_operation");
        assert_eq!(request["operation"], "inspect");
        assert_eq!(request["protocolVersion"], 1);
        assert_eq!(request["projectId"], "power-supply", "projectId 必须来自已选项目");
        assert_eq!(request["sessionId"], "s1", "sessionId 必须来自消息");
        assert_eq!(request["scope"], json!(["project.read"]));
        assert_eq!(request["baselineSnapshotHash"], Value::Null);
        assert_eq!(request["planHash"], Value::Null);
        assert_eq!(request["params"], json!({}));
        assert_eq!(request["document"], json!({"kind":"schematic"}));
        let request_id = request["requestId"].as_str().unwrap().to_owned();
        responder_executor.send(&json!({
            "type": "operation_progress",
            "requestId": request_id,
            "operation": "inspect",
            "phase": "枚举元件",
            "completed": 1,
            "total": 4,
        }));
        responder_executor.send(&operation_result(&request_id, "inspect", &inspect_payload()));
        (responder_executor, request_id)
    });

    session.send(&inspect_request());
    let started = session.recv();
    assert_eq!(started["type"], "operation_started");
    assert_eq!(started["operation"], "inspect");
    let progress = session.recv();
    assert_eq!(progress["type"], "operation_progress", "进度事件必须原样转发");
    assert_eq!(progress["phase"], "枚举元件");
    let result = session.recv();
    assert_eq!(result["type"], "inspect_result");
    assert_eq!(result["ok"], true);
    assert_eq!(result["components"], 2);
    assert_eq!(result["nets"], 1, "VCC 网络计数");
    assert_eq!(result["inconclusive"], json!([]));
    for key in ["snapshotHash", "logicalHash", "physicalHash"] {
        assert!(result[key].as_str().unwrap().starts_with("sha256:"), "{key} 必须是 sha256 摘要");
    }
    // 校验条目按协议 camelCase（constraintId/status）。
    let validation = result["validation"].as_array().unwrap();
    assert!(!validation.is_empty());
    assert!(
        validation
            .iter()
            .all(|entry| entry.get("constraintId").is_some() && entry.get("status").is_some()),
        "validation 条目必须是 camelCase：{validation:?}"
    );
    assert!(validation.iter().any(|entry| entry["constraintId"] == "schema.identity.topology"));
    let snapshot_hash = result["snapshotHash"].as_str().unwrap().to_owned();

    let (mut executor, first_request_id) = responder.join().unwrap();
    assert!(first_request_id.starts_with("op-"));

    // 观测记录落盘：observations/<snapshotHash 去前缀>.json，kind == inspect。
    let record_path = root
        .join("observations")
        .join(format!("{}.json", snapshot_hash.strip_prefix("sha256:").unwrap()));
    assert!(record_path.is_file(), "观测记录必须落盘：{record_path:?}");
    let record: Value = serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
    assert_eq!(record["kind"], "inspect");
    assert_eq!(record["projectId"], "power-supply");
    assert_eq!(record["sessionId"], "s1");
    assert_eq!(record["snapshot"]["snapshot_hash"], snapshot_hash);

    // 会话审计：inspect 完成后 EDA 类别会话被创建并追加 Note。
    let storage = ProjectStorage::open(&root).unwrap();
    let replay = storage.load_session("s1").expect("审计会话必须存在");
    assert_eq!(replay.metadata.category, circuitfabric_project::SessionCategory::Eda);
    assert!(
        replay.body.contains("inspect 完成") && replay.body.contains(&snapshot_hash),
        "会话审计必须记录快照哈希，got: {}",
        replay.body
    );

    // 同一载荷第二次 inspect → 相同 snapshotHash，parent 链指向第一次。
    session.send(&inspect_request());
    let started = session.recv();
    let second_request = executor.recv();
    assert_eq!(second_request["type"], "eda_operation");
    executor.send(&operation_result(
        second_request["requestId"].as_str().unwrap(),
        "inspect",
        &inspect_payload(),
    ));
    let second = session.recv();
    assert_eq!(second["type"], "inspect_result");
    assert_eq!(
        second["snapshotHash"], snapshot_hash,
        "同一原理图两次 inspect 必须得到相同 snapshotHash"
    );
    let record: Value = serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
    assert_eq!(
        record["snapshot"]["parent_snapshot_hash"], snapshot_hash,
        "第二次 inspect 的 parent 链必须指向第一次"
    );
    let _ = started;
}

// ---------------------------------------------------------------------------
// drc / visual_capture 端到端
// ---------------------------------------------------------------------------

#[test]
fn drc_end_to_end_persists_report() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    session.send(&json!({"type":"drc","sessionId":"s1","document":{"kind":"schematic"}}));
    let started = session.recv();
    assert_eq!(started["type"], "operation_started");
    let request = executor.recv();
    assert_eq!(request["operation"], "drc");
    let request_id = request["requestId"].as_str().unwrap().to_owned();
    executor.send(&operation_result(
        &request_id,
        "drc",
        &json!({
            "status": "failed",
            "errors": [
                { "type": "error", "rule": "IN-IN", "net": null,
                  "primitives": [ { "designator": "U1", "primitiveId": "prim-u1" } ] }
            ],
        }),
    ));
    let result = session.recv();
    assert_eq!(result["type"], "drc_result");
    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "failed");
    assert_eq!(result["errors"].as_array().unwrap().len(), 1);
    assert_eq!(result["errors"][0]["rule"], "IN-IN");
    assert_eq!(result["inconclusive"], json!([]));

    // DRC 报告落盘 observations/drc-<unix秒>.json。
    let report_path = observations(&root)
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
                name.starts_with("drc-")
                    && Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            })
        })
        .expect("drc report must be persisted");
    let report: Value = serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["kind"], "drc");
    assert_eq!(report["status"], "failed");
    assert_eq!(report["projectId"], "power-supply");
    assert_eq!(report["sessionId"], "s1");
}

#[test]
fn visual_capture_end_to_end_persists_png_evidence() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    let png = b"\x89PNG\r\n\x1a\nfake-image-bytes".to_vec();
    let encoded = base64_encode_test(&png);
    let expected_hash = format!("sha256:{:x}", Sha256::digest(&png));

    session
        .send(&json!({"type":"visual_capture","sessionId":"s1","document":{"kind":"schematic"}}));
    let started = session.recv();
    assert_eq!(started["type"], "operation_started");
    let request = executor.recv();
    assert_eq!(request["operation"], "visual_capture");
    let request_id = request["requestId"].as_str().unwrap().to_owned();
    executor.send(&operation_result(
        &request_id,
        "visual_capture",
        &json!({ "base64": encoded, "mime": "image/png" }),
    ));
    let result = session.recv();
    assert_eq!(result["type"], "visual_capture_result");
    assert_eq!(result["ok"], true);
    assert_eq!(result["evidence"]["contentHash"], expected_hash);
    assert_eq!(result["evidence"]["mime"], "image/png");
    assert_eq!(result["evidence"]["bytes"], png.len());
    assert_eq!(result["inconclusive"], json!([]));

    // PNG 与元数据落盘 observations/capture-<unix秒>.png/.json。
    let png_path = observations(&root)
        .into_iter()
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
        .expect("capture png must be persisted");
    assert_eq!(std::fs::read(&png_path).unwrap(), png);
    let meta: Value = serde_json::from_slice(
        &std::fs::read(png_path.with_extension("json")).expect("capture metadata"),
    )
    .unwrap();
    assert_eq!(meta["kind"], "visual_capture");
    assert_eq!(meta["contentHash"], expected_hash);
    assert_eq!(meta["mime"], "image/png");
    assert_eq!(meta["bytes"], png.len());
}

// ---------------------------------------------------------------------------
// 失败路径：无执行器、未实现能力、取消、执行器断开
// ---------------------------------------------------------------------------

#[test]
fn inspect_without_executor_reports_unavailable() {
    let bridge = RunningBridge::spawn();
    register_project(&bridge, "power-supply");
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    let reply = session.exchange(&inspect_request());
    assert_eq!(reply["type"], "operation_error");
    assert_eq!(reply["code"], "eda_executor_unavailable");
    assert!(reply["requestId"].as_str().unwrap().starts_with("op-"));
}

#[test]
fn apply_without_a_bound_plan_is_rejected() {
    let bridge = RunningBridge::spawn();
    register_project(&bridge, "power-supply");
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    let reply = session.exchange(&json!({
        "type": "apply", "sessionId": "s1", "document": { "kind": "schematic" },
    }));
    assert_eq!(reply["type"], "operation_error");
    assert_eq!(reply["operation"], "apply");
    assert_eq!(reply["code"], "materialization_rejected");
    assert!(reply["message"].as_str().unwrap().contains("planHash"));
}

#[test]
fn cancel_operation_drops_late_executor_result() {
    let bridge = RunningBridge::spawn();
    let root = register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    // 执行器延迟 800ms 才回结果，给取消留出窗口。
    let responder = thread::spawn(move || {
        let request = executor.recv();
        let request_id = request["requestId"].as_str().unwrap().to_owned();
        thread::sleep(Duration::from_millis(800));
        executor.send(&operation_result(&request_id, "inspect", &inspect_payload()));
        (executor, request_id)
    });

    session.send(&inspect_request());
    let started = session.recv();
    assert_eq!(started["type"], "operation_started");
    let request_id = started["requestId"].as_str().unwrap().to_owned();

    session.send(&json!({"type":"cancel_operation","requestId":request_id}));
    let reply = session.recv();
    assert_eq!(reply["type"], "operation_error", "unexpected reply: {reply}");
    assert_eq!(reply["code"], "cancelled");
    assert_eq!(reply["requestId"], request_id);
    assert!(
        ProjectStorage::open(root)
            .unwrap()
            .load_session("s1")
            .unwrap()
            .body
            .contains("inspect 取消")
    );

    let (executor, executor_request_id) = responder.join().unwrap();
    assert_eq!(executor_request_id, request_id);

    // 迟到的执行器结果必须被丢弃：会话不再收到该 requestId 的 inspect_result。
    session.expect_silence(Duration::from_secs(2));
    // 会话通道仍然可用（未被取消破坏）。
    let pong = session.exchange(&json!({"type":"ping"}));
    assert_eq!(pong["type"], "pong");
    drop(executor);
}

#[test]
fn executor_disconnect_fails_pending_operation() {
    let bridge = RunningBridge::spawn();
    register_project(&bridge, "power-supply");
    let mut executor = connect_executor(&bridge.address);
    let mut session = WsClient::connect(&bridge.address, "/bridge");
    select_project(&mut session, "power-supply");

    session.send(&inspect_request());
    let started = session.recv();
    let request_id = started["requestId"].as_str().unwrap().to_owned();
    // 执行器收到请求后中途断开。
    let request = executor.recv();
    assert_eq!(request["requestId"], request_id);
    drop(executor);

    let reply = session.recv();
    assert_eq!(reply["type"], "operation_error");
    assert_eq!(reply["code"], "eda_executor_unavailable");
    assert_eq!(reply["requestId"], request_id);
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

fn base64_encode_test(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 { ALPHABET[(value & 63) as usize] as char } else { '=' });
    }
    output
}
