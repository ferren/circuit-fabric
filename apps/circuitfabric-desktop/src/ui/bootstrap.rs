//! Bootstrap presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn input(
        window: &mut Window,
        value: String,
        placeholder: &'static str,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let input =
            cx.new(|cx| InputState::new(window, cx).default_value(value).placeholder(placeholder));
        cx.subscribe(&input, |_, _, event, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        })
        .detach();
        input
    }

    /// Password-style input: no echo, and the value stays out of the clipboard.
    pub(super) fn masked_input(
        window: &mut Window,
        placeholder: &'static str,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(placeholder))
    }

    pub(super) fn provider_fields(
        window: &mut Window,
        provider: LlmProviderSettings,
        cx: &mut Context<Self>,
    ) -> ProviderFields {
        let vision_base_url = Self::input(
            window,
            provider.vision_base_url.unwrap_or_default(),
            "https://api.example.com/v1",
            cx,
        );
        let vision_model =
            Self::input(window, provider.vision_model.unwrap_or_default(), "vision-model-name", cx);
        let vision_api_key_environment_variable = Self::input(
            window,
            provider.vision_api_key_environment_variable.unwrap_or_default(),
            "VISION_API_KEY_ENVIRONMENT_VARIABLE",
            cx,
        );
        // While the LLM itself handles images, the separate vision fields gray
        // out; their stored values stay visible and are restored as editable
        // drafts once native vision is turned off.
        if provider.native_vision {
            for state in [&vision_base_url, &vision_model, &vision_api_key_environment_variable] {
                state.update(cx, |state, cx| state.set_disabled(true, cx));
            }
        }
        ProviderFields {
            id: Self::input(window, provider.id, "provider-id", cx),
            name: Self::input(window, provider.name, "Provider name", cx),
            base_url: Self::input(window, provider.base_url, "https://api.example.com/v1", cx),
            model: Self::input(window, provider.model, "model-name", cx),
            api_key_environment_variable: Self::input(
                window,
                provider.api_key_environment_variable,
                "API_KEY_ENVIRONMENT_VARIABLE",
                cx,
            ),
            native_vision: provider.native_vision,
            vision_base_url,
            vision_model,
            vision_api_key_environment_variable,
            enabled: provider.enabled,
        }
    }

    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_paths(
            RuntimeSettings::default_path(),
            UnlockedVault::default_path(),
            window,
            cx,
        )
    }

    /// Explicit profile paths also let UI regressions run without reading the user's profile.
    pub(super) fn new_with_paths(
        settings_path: PathBuf,
        vault_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let loaded = RuntimeSettings::load_or_default(&settings_path);
        let load_error = loaded.as_ref().err().map(ToString::to_string);
        let settings = loaded.unwrap_or_default();
        DARK_MODE.store(Self::theme_is_dark(settings.global_preferences.theme), Ordering::Relaxed);
        let project_registry_path = project_data::project_registry_path(&settings_path);
        let (
            project_registry,
            workspace,
            project_storages,
            project_data,
            project_restore_diagnostics,
        ) = project_data::restore_project_workspace(&project_registry_path);
        let providers = settings
            .providers
            .iter()
            .cloned()
            .map(|provider| Self::provider_fields(window, provider, cx))
            .collect();
        let command_search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索模块…"));
        command_search.update(cx, |state, _| {
            state.set_editor_style(InputEditorStyle {
                foreground: rgb(SIDEBAR_TEXT_ACTIVE).into(),
                muted_foreground: rgb(SIDEBAR_TEXT).into(),
                background: rgb(SIDEBAR_BG).into(),
                border: rgb(SIDEBAR_DIVIDER).into(),
                selection: rgba(0x0022_d36e).into(),
                caret: rgb(SIDEBAR_TEXT_ACTIVE).into(),
                ..InputEditorStyle::default()
            });
        });
        cx.subscribe(&command_search, |view, _, event, cx| {
            if let InputEvent::Change = event {
                view.command_selected = 0;
                cx.notify();
            }
        })
        .detach();
        let project_search = Self::input(window, String::new(), "Search projects", cx);
        let data_directory = Self::input(
            window,
            settings.global_preferences.data_directory.display().to_string(),
            "CircuitFabric data directory",
            cx,
        );
        let evidence_query = Self::input(
            window,
            String::new(),
            "VIN 输入电压 / \"input voltage\"（多个词同时命中，引号保持短语）",
            cx,
        );
        cx.subscribe(&evidence_query, |view, _, event, cx| match event {
            InputEvent::Change => view.schedule_evidence_search(EVIDENCE_SEARCH_DEBOUNCE, cx),
            InputEvent::PressEnter { .. } => {
                view.schedule_evidence_search(Duration::ZERO, cx);
            }
            _ => {}
        })
        .detach();
        let semantic_query =
            Self::input(window, String::new(), "reference, pin, net, constraint, document…", cx);
        let usage_filter = Self::input(window, String::new(), "project, provider, or runtime", cx);
        let audit_filter =
            Self::input(window, String::new(), "project, runtime, session, or event", cx);
        let approval_note = Self::input(window, String::new(), "Approval or rejection reason", cx);
        let new_project_id = Self::input(window, String::new(), "power-supply", cx);
        let new_project_name = Self::input(window, String::new(), "Power supply", cx);
        let new_project_description =
            Self::input(window, String::new(), "Optional design workspace description", cx);
        let new_project_root =
            Self::input(window, String::new(), "Choose an existing empty folder", cx);
        let new_tool_id = Self::input(window, String::new(), "evidence-search, bom-export …", cx);
        let catalog_id = Self::input(window, String::new(), "server-id", cx);
        let catalog_source =
            Self::input(window, String::new(), "SKILL.md 路径 / MCP 可执行文件", cx);
        let catalog_args = Self::input(window, "[]".to_owned(), "参数 JSON 数组", cx);
        let catalog_env = Self::input(window, String::new(), "MCP_API_KEY", cx);
        let claude_command =
            Self::input(window, settings.adapters.claude_command.clone(), "可执行文件", cx);
        let claude_provider = Self::input(
            window,
            settings.adapters.claude_provider_id.clone(),
            "留空使用默认 Provider",
            cx,
        );
        let dsh_command =
            Self::input(window, settings.adapters.dsh_command.clone(), "可执行文件", cx);
        let dsh_provider = Self::input(
            window,
            settings.adapters.dsh_provider_id.clone(),
            "留空使用默认 Provider",
            cx,
        );
        let codex_provider = Self::input(
            window,
            settings.adapters.codex_provider_id.clone(),
            "留空使用默认 Provider",
            cx,
        );
        let task_prompt = Self::input(window, String::new(), "输入任务以验证真实模型调用", cx);
        let task_image = Self::input(window, String::new(), "可选图片路径；使用 Vision 服务", cx);
        let plugin_directory = Self::input(
            window,
            PathBuf::from("plugins").display().to_string(),
            "Plugin manifest directory",
            cx,
        );
        let vault_password = Self::masked_input(window, "保险库密码", cx);
        let vault_password_confirm = Self::masked_input(window, "再次输入密码", cx);
        let secret_name = Self::input(window, String::new(), "OPENAI_API_KEY", cx);
        let secret_value = Self::masked_input(window, "粘贴密钥值，保存后不再回显", cx);
        let jev_api_key =
            Self::masked_input(window, "粘贴当前判断后端的 API Key，保存后不再回显", cx);
        let jev_llm_settings = settings
            .catalog
            .mcp_servers
            .iter()
            .find(|s| s.id == circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID)
            .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
            .or_else(|| settings.catalog.llm_judge.clone())
            .unwrap_or_default();
        let jev_llm_draft = settings
            .catalog
            .mcp_servers
            .iter()
            .find(|s| s.id == circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID)
            .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
            .is_some();
        let jev_base_url = Self::input(
            window,
            jev_llm_settings.base_url.clone(),
            "兼容 Chat Completions 的 Base URL",
            cx,
        );
        let jev_model = Self::input(window, jev_llm_settings.model.clone(), "模型 ID", cx);
        let jev_key_env = Self::input(
            window,
            jev_llm_settings.api_key_environment_variable.clone(),
            "密钥环境变量名",
            cx,
        );
        let jev_timeout =
            Self::input(window, jev_llm_settings.timeout_seconds.to_string(), "5～180 秒", cx);
        let jev_retries =
            Self::input(window, jev_llm_settings.malformed_retries.to_string(), "0～3 次", cx);
        for input in [
            &project_search,
            &semantic_query,
            &usage_filter,
            &audit_filter,
            &approval_note,
            &new_project_id,
            &new_project_root,
            &new_tool_id,
            &secret_name,
            &plugin_directory,
            &jev_base_url,
            &jev_model,
            &jev_key_env,
            &jev_timeout,
            &jev_retries,
        ] {
            cx.subscribe(input, |_, _, event, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            })
            .detach();
        }
        let restored_projects: Vec<ProjectId> = project_storages.keys().cloned().collect();
        cx.spawn(async move |view, cx| {
            view.update(cx, |view, cx| {
                for project_id in &restored_projects {
                    view.schedule_pdf_indexing(project_id, cx);
                }
            })
            .ok();
        })
        .detach();
        let status = if let Some(error) = load_error {
            format!("运行时配置读取失败，请修复配置后重新打开：{error}")
        } else if project_restore_diagnostics.is_empty() {
            "项目列表已恢复；全局运行时设置尚未修改。".to_owned()
        } else {
            format!("项目恢复提示：{}", project_restore_diagnostics.join("；"))
        };
        let plugin_governance_path = settings_path.with_file_name("plugin-governance.json");
        let plugin_governance =
            match PluginGovernanceStore::load_or_default(&plugin_governance_path) {
                Ok(store) => store,
                Err(error) => {
                    // Keep the existing store intact on disk and surface the problem in
                    // the status bar; an empty in-memory inventory is safer than
                    // overwriting an unreadable audit trail.
                    eprintln!("Plugin governance store was not loaded: {error}");
                    PluginGovernanceStore::default()
                }
            };
        let vault_file_exists = UnlockedVault::exists(&vault_path);
        let vault_index = if vault_file_exists {
            UnlockedVault::variable_names(&vault_path).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            sidebar_mark: Arc::new(Image::from_bytes(ImageFormat::Png, SIDEBAR_MARK.to_vec())),
            saved_settings: settings.clone(),
            command: Self::input(window, settings.codex.command, "codex", cx),
            bridge_address: Self::input(
                window,
                settings.bridge.listen_address,
                "127.0.0.1:49630",
                cx,
            ),
            providers,
            default_provider_id: settings.default_provider_id,
            selected_provider: 0,
            navigation: DesktopShell::default(),
            language: UiLanguage::from_preference(settings.global_preferences.language),
            global_theme: settings.global_preferences.theme,
            data_directory,
            secret_storage_provider: settings.global_preferences.secret_storage_provider,
            log_level: settings.global_preferences.log_level,
            settings_path,
            status,
            command_palette_open: false,
            command_search,
            command_selected: 0,
            workspace,
            project_registry,
            project_registry_path,
            project_storages,
            project_data,
            session_replay: None,
            overview_cached: Default::default(),
            project_read_errors: BTreeMap::new(),
            overview_restore_diagnostics: project_restore_diagnostics,
            session_project_filter: None,
            document_preview: None,
            document_preview_width: DOCUMENT_PREVIEW_WIDTH,
            preview_show_data: false,
            datasheet_extracting: false,
            datasheet_feedback: None,
            datasheet_stream: None,
            datasheet_extract_started: None,
            datasheet_cancel: None,
            datasheet_checkpoint: None,
            pdf_index_state: BTreeMap::new(),
            datasheet_source_indexes: BTreeMap::new(),
            datasheet_rows_visible: 40,
            pre_preview_window_size: None,
            usage_period: UsagePeriod::All,
            usage_filter,
            audit_kind_filter: AuditKindFilter::All,
            audit_filter,
            audit_page: 0,
            audit_page_filter: String::new(),
            usage_audit_cached: None,
            evidence_query,
            evidence_scope: EvidenceScope::All,
            evidence_generation: 0,
            evidence_searching: false,
            evidence_result: None,
            evidence_visible: EVIDENCE_PAGE_SIZE,
            preview_focus: None,
            preview_scroll: gpui::ScrollHandle::new(),
            preview_text_focus: cx.focus_handle(),
            selection_pressed: std::rc::Rc::new(std::cell::Cell::new(false)),
            preview_pdf_zoom: crate::pdf_zoom::ZoomMotion::default(),
            preview_zoom_tick: None,
            preview_pdf_pan: None,
            preview_zoom_anchor: None,
            preview_row_anchor: None,
            preview_focus_generation: 0,
            preview_scroll_pending: std::cell::Cell::new(false),
            semantic_query,
            semantic_query_scope: SemanticQueryScope::Components,
            selected_semantic_snapshot: None,
            selected_change_set: None,
            approval_drawer_open: false,
            approval_note,
            bom_export_format: BomExportFormat::Csv,
            window_title: String::new(),
            project_search,
            project_filter: ProjectFilter::All,
            project_tab: ProjectDetailTab::Overview,
            project_form_open: false,
            new_project_id,
            new_project_name,
            new_project_description,
            new_project_root,
            agents_selection: AgentsSelection::Runtime(RuntimeAdapter::CodexAppServer),
            adapter_settings_open: None,
            provider_editor_open: false,
            dialog_error: None,
            codex_process: None,
            codex_status: RuntimeLifecycleStatus::Stopped,
            codex_active_provider: None,
            codex_project_context: None,
            bridge_process: None,
            bridge_status: RuntimeLifecycleStatus::Stopped,
            bridge_active_address: None,
            bridge_health: BridgeHealth::Unknown,
            bridge_probed_at: None,
            bridge_probe_pending: false,
            bridge_test_pending: false,
            bridge_test: None,
            eda_services_selection: EdaServiceSelection::JlcircuitBridge,
            tool_authorizations: settings.tools,
            new_tool_id,
            catalog: settings.catalog,
            catalog_id,
            catalog_source,
            catalog_args,
            catalog_env,
            catalog_name: Self::input(window, String::new(), "显示名称", cx),
            catalog_search: Self::input(window, String::new(), "搜索名称、标识或来源", cx),
            selected_skill: None,
            selected_mcp: None,
            catalog_editor: None,
            catalog_error: None,
            tool_feedback: "请选择技能或 MCP server。".into(),
            tool_reports: BTreeMap::new(),
            tool_revision: 0,
            tool_call_name: Self::input(window, String::new(), "工具名称", cx),
            tool_call_args: Self::input(window, "{}".into(), "调用参数 JSON 对象", cx),
            jev_api_key,
            jev_base_url,
            jev_model,
            jev_key_env,
            jev_timeout,
            jev_retries,
            jev_llm_settings,
            jev_llm_draft,
            jev_backend_modal_open: false,
            jev_key_modal_open: false,
            adapters: settings.adapters,
            claude_command,
            claude_provider,
            dsh_command,
            dsh_provider,
            codex_provider,
            task_prompt,
            task_image,
            task_result: String::new(),
            task_cancel: None,
            task_identity: None,
            project_agent_adapter: RuntimeAdapter::CodexAppServer,
            plugin_directory,
            plugin_governance_path,
            plugin_governance,
            vault_path,
            vault: None,
            vault_file_exists,
            vault_index,
            vault_prompt_open: vault_file_exists,
            vault_quick_unlock_open: false,
            vault_busy: false,
            vault_message: None,
            vault_password,
            vault_password_confirm,
            secret_name,
            secret_value,
            selected_secret: None,
            new_tool_kind: ToolAuthorizationKind::Skill,
            new_tool_scope: ToolScope::Global,
        }
    }
}
