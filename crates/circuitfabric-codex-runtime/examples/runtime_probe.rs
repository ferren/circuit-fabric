//! Foreground integration probe; credentials are supplied only through the environment.
use circuitfabric_codex_runtime::{
    LlmProviderSettings, ProviderConnection, RuntimeSettings,
    execution::{AgentKind, Cancellation, run_task_observed},
    models,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let settings = RuntimeSettings::load_or_default(std::path::Path::new(&args[1]))?;
    if args[2] == "models" {
        return probe_models(&settings, &args[3]);
    }
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
        "kimi" => AgentKind::Kimi,
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
    let mut usage = None;
    let result = run_task_observed(
        &settings,
        kind,
        &settings.tools,
        &args[3],
        image,
        None,
        None,
        &cancel,
        &mut |_, _| {},
        &mut |reported| usage = Some(reported),
    );
    if let Some(timer) = timer {
        timer.join().expect("cancellation timer");
        assert!(result.is_err(), "cancelled task must not complete successfully");
        println!("CF_CANCEL_OK");
    } else {
        println!("{}", result?);
        if std::env::var_os("CF_SMOKE_REQUIRE_USAGE").is_some() {
            if matches!(kind, AgentKind::Dsh | AgentKind::Kimi) {
                assert!(usage.is_none(), "Dsh/Kimi output must not fabricate usage");
            } else {
                let usage = usage.expect("real adapter must report fixture model counters");
                assert!(
                    usage.input_tokens > 0 && usage.output_tokens > 0,
                    "fixture counters: {usage:?}"
                );
            }
            println!("CF_USAGE_OK {usage:?}");
        }
    }
    Ok(())
}

/// Prints the catalog one model per line and asserts it is non-empty.
fn probe_models(settings: &RuntimeSettings, source: &str) -> Result<(), Box<dyn std::error::Error>> {
    let choices = match source {
        "codex-account" => {
            let provider = LlmProviderSettings {
                id: "probe-codex".into(),
                name: "probe".into(),
                connection: ProviderConnection::CodexAccount,
                ..LlmProviderSettings::default()
            };
            models::list_codex_models(&settings.codex, &provider, None)?
        }
        "kimi-account" => models::list_kimi_models(&settings.adapters.kimi_command)?,
        source => {
            let provider = settings
                .providers
                .iter()
                .find(|provider| provider.id == source)
                .ok_or("provider not in settings; use codex-account, kimi-account, or a provider id")?;
            models::list_endpoint_models(provider, None)?
        }
    };
    assert!(!choices.is_empty(), "model catalog must not be empty");
    for choice in &choices {
        println!(
            "CF_MODEL {}{}{}",
            choice.id,
            choice.display_name.as_deref().map(|name| format!(" ({name})")).unwrap_or_default(),
            if choice.supports_vision == Some(true) { " [vision]" } else { "" }
        );
    }
    println!("CF_MODELS_OK {}", choices.len());
    Ok(())
}
