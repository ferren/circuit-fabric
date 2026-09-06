//! Foreground integration probe; credentials are supplied only through the environment.
use circuitfabric_codex_runtime::{
    RuntimeSettings,
    execution::{AgentKind, Cancellation, run_task_with_image},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let settings = RuntimeSettings::load_or_default(std::path::Path::new(&args[1]))?;
    if args[2] == "mcp" {
        for id in &settings.tools.authorized_mcp_server_ids {
            let tools = settings.catalog.list_tools(id, &settings.tools)?;
            assert_eq!(tools["tools"][0]["name"], "echo");
            let result = settings.catalog.call_tool(
                id,
                &settings.tools,
                "echo",
                &serde_json::json!({"text":"CF_MCP_OK"}),
            )?;
            assert_eq!(result["content"][0]["text"], "CF_MCP_OK");
            assert!(
                settings
                    .catalog
                    .call_tool(
                        id,
                        &circuitfabric_codex_runtime::ToolAuthorizationSettings::default(),
                        "echo",
                        &serde_json::json!({})
                    )
                    .is_err()
            );
        }
        println!("CF_OK");
        return Ok(());
    }
    let kind = match args[2].as_str() {
        "codex" => AgentKind::Codex,
        "claude" => AgentKind::Claude,
        "dsh" => AgentKind::Dsh,
        _ => return Err("unknown runtime".into()),
    };
    let cancel = Cancellation::default();
    let timer = args.get(4).filter(|v| v.as_str() == "cancel").map(|_| {
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            flag.cancel();
        })
    });
    let image = args.get(4).filter(|v| v.as_str() != "cancel").map(std::path::Path::new);
    let result = run_task_with_image(&settings, kind, &settings.tools, &args[3], image, &cancel);
    if let Some(timer) = timer {
        timer.join().expect("cancellation timer");
        assert!(result.is_err(), "cancelled task must not complete successfully");
        println!("CF_CANCEL_OK");
    } else {
        println!("{}", result?);
    }
    Ok(())
}
