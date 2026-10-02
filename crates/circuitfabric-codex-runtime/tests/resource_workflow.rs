//! Real local files and stdio processes; no model/remote service is substituted
//! for the runtime acceptance test in native_smoke.py.
use circuitfabric_codex_runtime::{
    RuntimeSettings, ToolAuthorizationSettings,
    execution::Cancellation,
    tools::{BUNDLED_JEV_SERVER_ID, McpServerDefinition, ToolCatalog},
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cf-resource-flow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn catalog(&self) -> ToolCatalog {
        let mut catalog = ToolCatalog::default();
        for id in ["first", "second"] {
            let folder = self.0.join(id);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("SKILL.md"), format!("---\nname: {id}\ndescription: >\n  Actual skill {id}\n  description\n---\nCF_SKILL_{id}\n")).unwrap();
            catalog.import_skill(&folder).unwrap();
            catalog.mcp_servers.push(McpServerDefinition {
                id: id.into(),
                display_name: format!("Local {id}"),
                command: "python".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/mcp_fixture.py")
                        .display()
                        .to_string(),
                    self.0.join(format!("{id}.log")).display().to_string(),
                ],
                environment_variables: vec![],
                enabled: true,
            });
        }
        catalog
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn both() -> ToolAuthorizationSettings {
    ToolAuthorizationSettings {
        authorized_skill_ids: vec!["first".into(), "second".into()],
        authorized_mcp_server_ids: vec!["first".into(), "second".into()],
    }
}

#[test]
fn two_resources_load_discover_call_reload_and_revoke_independently() {
    let fixture = Fixture::new();
    let mut settings =
        RuntimeSettings { catalog: fixture.catalog(), tools: both(), ..Default::default() };
    let path = fixture.0.join("runtime.json");
    settings.save(&path).unwrap();
    settings = RuntimeSettings::load_or_default(&path).unwrap();
    let catalog = &mut settings.catalog;
    let instructions = catalog.skill_instructions(&both()).unwrap();
    assert!(instructions.contains("CF_SKILL_first") && instructions.contains("CF_SKILL_second"));
    assert_eq!(catalog.skills[0].preview().unwrap().description, "Actual skill first description");
    for id in ["first", "second"] {
        let tools = catalog.list_tools(id, &both()).unwrap();
        assert_eq!(tools["tools"][0]["name"], "echo");
        let value = catalog.call_tool(id, &both(), "echo", &json!({"text":id})).unwrap();
        assert_eq!(value["content"][0]["text"], id);
        assert!(fs::read_to_string(fixture.0.join(format!("{id}.log"))).unwrap().contains(id));
    }
    let project_a = ToolAuthorizationSettings {
        authorized_skill_ids: vec!["first".into()],
        authorized_mcp_server_ids: vec!["first".into()],
    };
    let project_b = ToolAuthorizationSettings {
        authorized_skill_ids: vec!["second".into()],
        authorized_mcp_server_ids: vec!["second".into()],
    };
    let a = catalog.effective_grants(&both(), Some(&project_a));
    let b = catalog.effective_grants(&both(), Some(&project_b));
    assert_eq!(a, project_a);
    assert_eq!(b, project_b);
    assert!(catalog.list_tools("second", &a).is_err());
    assert!(catalog.list_tools("first", &b).is_err());
    assert!(catalog.list_tools("first", &ToolAuthorizationSettings::default()).is_err());
    assert_eq!(
        catalog.effective_grants(&ToolAuthorizationSettings::default(), Some(&both())),
        ToolAuthorizationSettings::default()
    );
    catalog.skills[0].enabled = false;
    catalog.mcp_servers[0].enabled = false;
    assert!(catalog.list_tools("first", &both()).is_err());
    assert!(catalog.skill_instructions(&project_a).is_err());
    let effective = catalog.effective_grants(&both(), None);
    assert_eq!(effective, project_b);
    assert_eq!(
        catalog.call_tool("second", &effective, "echo", &json!({"text":"still-working"})).unwrap()
            ["content"][0]["text"],
        "still-working"
    );
    catalog.mcp_servers[1].command = "cf-missing-executable-490".into();
    assert!(catalog.list_tools("second", &effective).is_err());
}

#[test]
fn revoked_operation_stops_a_live_server_before_it_can_report_success() {
    let fixture = Fixture::new();
    let script = fixture.0.join("slow.py");
    let marker = fixture.0.join("started");
    fs::write(
        &script,
        format!(
            "import pathlib,time\npathlib.Path({:?}).write_text('started')\ntime.sleep(20)\n",
            marker.display().to_string()
        ),
    )
    .unwrap();
    let catalog = ToolCatalog {
        mcp_servers: vec![McpServerDefinition {
            id: "first".into(),
            display_name: "Slow".into(),
            command: "python".into(),
            args: vec![script.display().to_string()],
            environment_variables: vec![],
            enabled: true,
        }],
        ..Default::default()
    };
    let cancel = Cancellation::default();
    let worker_cancel = cancel.clone();
    let worker = std::thread::spawn(move || {
        catalog.request_with_cancellation("first", &both(), None, None, &worker_cancel)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(marker.exists(), "server really launched");
    let start = std::time::Instant::now();
    cancel.cancel();
    assert!(worker.join().unwrap().unwrap_err().to_string().contains("已取消"));
    assert!(start.elapsed() < Duration::from_secs(3), "must not wait for old server timeout");
}

#[test]
fn invalid_import_duplicate_ids_credentials_and_bundled_deletion_are_persistent() {
    let fixture = Fixture::new();
    let mut catalog = fixture.catalog();
    let old = catalog.clone();
    fs::write(fixture.0.join("wrong.md"), "instructions").unwrap();
    assert!(catalog.import_skill(&fixture.0.join("wrong.md")).is_err());
    assert_eq!(catalog, old);
    fs::write(fixture.0.join("first/SKILL.md"), "").unwrap();
    assert!(catalog.skill_instructions(&both()).is_err());
    let folder = fixture.0.join("duplicate");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("SKILL.md"), "---\nname: first\n---\nother").unwrap();
    assert!(catalog.import_skill(&folder).is_err());
    assert_eq!(catalog, old);
    catalog.mcp_servers[0].args = vec!["--token=literal-secret".into()];
    assert!(catalog.validate().is_err());
    catalog = old;
    catalog.mcp_servers[0].environment_variables = vec!["TOKEN=value".into()];
    assert!(catalog.validate().is_err());
    let mut saved = RuntimeSettings::default();
    saved.catalog.removed_bundled_servers.push(BUNDLED_JEV_SERVER_ID.into());
    saved.catalog.mcp_servers.retain(|s| s.id != BUNDLED_JEV_SERVER_ID);
    let path = fixture.0.join("runtime.json");
    saved.save(&path).unwrap();
    let restored = RuntimeSettings::load_or_default(&path).unwrap();
    assert!(!restored.catalog.mcp_servers.iter().any(|s| s.id == BUNDLED_JEV_SERVER_ID));
}

#[test]
fn a_real_server_returning_no_call_result_is_a_failure() {
    let fixture = Fixture::new();
    let script = fixture.0.join("malformed.py");
    fs::write(&script, r#"import json,sys
for line in sys.stdin:
    request=json.loads(line)
    if 'id' not in request: continue
    result={'protocolVersion':'2024-11-05','capabilities':{},'serverInfo':{'name':'malformed','version':'1'}} if request['method']=='initialize' else None
    print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}),flush=True)
"#).unwrap();
    let catalog = ToolCatalog {
        mcp_servers: vec![McpServerDefinition {
            id: "first".into(),
            display_name: "Malformed".into(),
            command: "python".into(),
            args: vec![script.display().to_string()],
            environment_variables: vec![],
            enabled: true,
        }],
        ..Default::default()
    };
    assert!(
        catalog
            .call_tool("first", &both(), "echo", &json!({"text":"must-not-succeed"}))
            .unwrap_err()
            .to_string()
            .contains("有效的调用结果")
    );
}
