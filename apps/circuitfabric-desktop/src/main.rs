//! Desktop control-plane entry point.
//!
//! The native GPUI view is feature-gated so the semantic core and plugins can be developed and
//! tested without a graphics stack. Enable it with `--features native-ui` after fetching the
//! GPUI dependencies pinned in `Cargo.lock`.

#![deny(unsafe_code)]

use circuitfabric_contracts::ProjectId;

#[cfg(all(feature = "native-ui", windows))]
#[allow(unsafe_code)]
mod windows_icon;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlPlaneScreen {
    Overview,
    Projects,
    Documents,
    Semantics,
    EdaServices,
    AgentsAndMcp,
    SessionsAndTasks,
    ChangesAndApprovals,
    BomAndExport,
    Plugins,
    Usage,
    Settings,
}

impl ControlPlaneScreen {
    #[cfg(feature = "native-ui")]
    const ALL: [Self; 12] = [
        Self::Overview,
        Self::Projects,
        Self::Documents,
        Self::Semantics,
        Self::EdaServices,
        Self::AgentsAndMcp,
        Self::SessionsAndTasks,
        Self::ChangesAndApprovals,
        Self::BomAndExport,
        Self::Plugins,
        Self::Usage,
        Self::Settings,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Projects => "Projects",
            Self::Documents => "Documents",
            Self::Semantics => "Circuit semantics",
            Self::EdaServices => "EDA services",
            Self::AgentsAndMcp => "Agents & tools",
            Self::SessionsAndTasks => "Sessions & tasks",
            Self::ChangesAndApprovals => "Changes & approvals",
            Self::BomAndExport => "BOM & export",
            Self::Plugins => "Plugins",
            Self::Usage => "Usage & audit",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub const fn group(self) -> &'static str {
        match self {
            Self::Overview | Self::Projects | Self::Documents | Self::Semantics => {
                "DESIGN EVIDENCE"
            }
            Self::EdaServices | Self::AgentsAndMcp | Self::SessionsAndTasks => "RUNTIME TOOLS",
            Self::ChangesAndApprovals | Self::BomAndExport => "MATERIALIZE & DELIVER",
            Self::Plugins | Self::Usage | Self::Settings => "GOVERNANCE",
        }
    }

    #[must_use]
    pub const fn is_todo(self) -> bool {
        !matches!(self, Self::Overview | Self::AgentsAndMcp)
    }
}

#[cfg(feature = "native-ui")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiLanguage {
    SimplifiedChinese,
    English,
}

#[cfg(feature = "native-ui")]
impl UiLanguage {
    const fn toggled(self) -> Self {
        match self {
            Self::SimplifiedChinese => Self::English,
            Self::English => Self::SimplifiedChinese,
        }
    }

    const fn toggle_label(self) -> &'static str {
        match self {
            Self::SimplifiedChinese => "EN",
            Self::English => "中文",
        }
    }

    const fn choose(self, chinese: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::SimplifiedChinese => chinese,
            Self::English => english,
        }
    }

    fn choose_owned(self, chinese: String, english: String) -> String {
        match self {
            Self::SimplifiedChinese => chinese,
            Self::English => english,
        }
    }

    const fn screen_label(self, screen: ControlPlaneScreen) -> &'static str {
        match self {
            Self::English => screen.label(),
            Self::SimplifiedChinese => match screen {
                ControlPlaneScreen::Overview => "总览",
                ControlPlaneScreen::Projects => "项目",
                ControlPlaneScreen::Documents => "文档",
                ControlPlaneScreen::Semantics => "电路语义",
                ControlPlaneScreen::EdaServices => "EDA 服务",
                ControlPlaneScreen::AgentsAndMcp => "智能体与工具",
                ControlPlaneScreen::SessionsAndTasks => "会话与任务",
                ControlPlaneScreen::ChangesAndApprovals => "变更与审批",
                ControlPlaneScreen::BomAndExport => "BOM 与导出",
                ControlPlaneScreen::Plugins => "插件",
                ControlPlaneScreen::Usage => "用量与审计",
                ControlPlaneScreen::Settings => "设置",
            },
        }
    }

    const fn group_label(self, screen: ControlPlaneScreen) -> &'static str {
        match self {
            Self::English => screen.group(),
            Self::SimplifiedChinese => match screen {
                ControlPlaneScreen::Overview
                | ControlPlaneScreen::Projects
                | ControlPlaneScreen::Documents
                | ControlPlaneScreen::Semantics => "设计证据",
                ControlPlaneScreen::EdaServices
                | ControlPlaneScreen::AgentsAndMcp
                | ControlPlaneScreen::SessionsAndTasks => "运行工具",
                ControlPlaneScreen::ChangesAndApprovals | ControlPlaneScreen::BomAndExport => {
                    "物化交付"
                }
                ControlPlaneScreen::Plugins
                | ControlPlaneScreen::Usage
                | ControlPlaneScreen::Settings => "治理",
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    const fn page_copy(
        self,
        screen: ControlPlaneScreen,
    ) -> (&'static str, &'static str, &'static str) {
        match (self, screen) {
            (Self::SimplifiedChinese, ControlPlaneScreen::Projects) => (
                "项目",
                "项目会把电路事实、文档、会话和配置收拢到同一个有作用域的工作区。",
                "TODO：创建并持久化项目记录，然后加入可搜索的列表和详情标签页。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::Documents) => (
                "文档",
                "已授权的设计证据会保留来源定位信息，供智能体进行可引用的检索。",
                "TODO：加入文档登记、安全扫描和证据感知检索。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::Semantics) => (
                "电路语义",
                "快照、拓扑、约束和验证事实会完整呈现，不会隐藏不确定性。",
                "TODO：加入快照历史、语义查询和拓扑可视化。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::EdaServices) => (
                "EDA 服务",
                "已连接 EDA 后端将展示能力、健康度、端点和回读状态。",
                "TODO：加入 bridge 发现、健康上报和能力协商。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::SessionsAndTasks) => (
                "会话与任务",
                "此只读回放界面将展示智能体轮次、工具调用、证据与任务进度。",
                "TODO：加入会话事件持久化和关联证据的回放时间线。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::ChangesAndApprovals) => (
                "变更与审批",
                "每个 ChangeSet 都会先与精确基线比对，之后才允许审批物化。",
                "TODO：加入审批抽屉、IR Diff、审计事件和回滚策略。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::BomAndExport) => (
                "BOM 与导出",
                "BOM、网表和仿真导出将持续关联到可追溯的语义快照。",
                "TODO：加入 BOM 生成以及 CSV、Excel、JSON、网表和 SPICE 导出。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::Plugins) => (
                "插件",
                "这里将统一治理插件 manifest、权限、签名、版本和健康状态。",
                "TODO：加入 manifest 发现、签名校验和权限控制。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::Usage) => (
                "用量与审计",
                "Token 用量和不可变工程审计事件将按项目和运行时聚合。",
                "TODO：加入用量汇总、筛选和审计导出。",
            ),
            (Self::SimplifiedChinese, ControlPlaneScreen::Settings) => (
                "设置",
                "外观、语言、数据目录、凭据提供方和日志偏好会在这里配置。",
                "TODO：加入主题选择和其余全局偏好。",
            ),
            (Self::English, ControlPlaneScreen::Projects) => (
                "Projects",
                "Project records will keep circuit facts, documents, sessions, and configuration in one scoped workspace.",
                "TODO: Create and persist project records; then add a searchable list and detail tabs.",
            ),
            (Self::English, ControlPlaneScreen::Documents) => (
                "Documents",
                "Authorized design evidence will be source-addressable and ready for citation by agents.",
                "TODO: Add document registration, safe scanning, and evidence-aware search.",
            ),
            (Self::English, ControlPlaneScreen::Semantics) => (
                "Circuit semantics",
                "Snapshots, topology, constraints, and verification facts will be presented without hiding uncertainty.",
                "TODO: Add snapshot history, semantic queries, and topology visualization.",
            ),
            (Self::English, ControlPlaneScreen::EdaServices) => (
                "EDA services",
                "Connected EDA backends will show capabilities, health, endpoint, and readback status.",
                "TODO: Add bridge discovery, health reporting, and capability negotiation.",
            ),
            (Self::English, ControlPlaneScreen::SessionsAndTasks) => (
                "Sessions & tasks",
                "This read-only replay surface will show agent turns, tool calls, evidence, and task progress.",
                "TODO: Add session event persistence and an evidence-linked replay timeline.",
            ),
            (Self::English, ControlPlaneScreen::ChangesAndApprovals) => (
                "Changes & approvals",
                "ChangeSets will be reviewed against their exact baseline before any materialization is approved.",
                "TODO: Add the approval drawer, IR diff, audit events, and rollback policy.",
            ),
            (Self::English, ControlPlaneScreen::BomAndExport) => (
                "BOM & export",
                "BOM, netlist, and simulation exports will remain traceable to a semantic snapshot.",
                "TODO: Add BOM generation plus CSV, Excel, JSON, netlist, and SPICE exporters.",
            ),
            (Self::English, ControlPlaneScreen::Plugins) => (
                "Plugins",
                "Plugin manifests, permissions, signatures, versions, and health will be governed here.",
                "TODO: Add manifest discovery, signature verification, and permission controls.",
            ),
            (Self::English, ControlPlaneScreen::Usage) => (
                "Usage & audit",
                "Token usage and immutable engineering audit events will be grouped by project and runtime.",
                "TODO: Add usage aggregation, filtering, and audit export.",
            ),
            (Self::English, ControlPlaneScreen::Settings) => (
                "Settings",
                "Appearance, language, data directory, credential provider, and log preferences will live here.",
                "TODO: Add theme selection and the remaining global preferences.",
            ),
            (_, ControlPlaneScreen::Overview | ControlPlaneScreen::AgentsAndMcp) => unreachable!(),
        }
    }
}

#[derive(Debug, Default)]
pub struct DesktopShell {
    selected_project: Option<ProjectId>,
}

impl DesktopShell {
    #[must_use]
    pub fn selected_project(&self) -> Option<&str> {
        self.selected_project.as_deref()
    }

    pub fn select_project(&mut self, project_id: ProjectId) {
        self.selected_project = Some(project_id);
    }
}

#[cfg(not(feature = "native-ui"))]
fn main() {
    println!("CircuitFabric desktop scaffold. Rebuild with --features native-ui to start GPUI.");
}

#[cfg(feature = "native-ui")]
#[allow(clippy::too_many_lines)]
fn main() {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use circuitfabric_codex_runtime::{
        CodexAppServerHandle, LlmProviderSettings, RuntimeSettings, ToolAuthorizationKind,
        ToolAuthorizationSettings,
    };
    use circuitfabric_contracts::Project;
    use circuitfabric_project::{
        DocumentCategory, ProjectDocument, ProjectRegistry, ProjectStorage, ProjectWorkspace,
        SessionListing, SessionReplay, is_text_extractable, rfc3339,
    };
    use gpui::{
        AppContext, Context, Entity, FontWeight, Image, ImageFormat, InteractiveElement,
        IntoElement, KeystrokeEvent, ParentElement, Render, StatefulInteractiveElement, Styled,
        Window, WindowOptions, div, img, prelude::FluentBuilder as _, px, rgb, rgba,
    };
    use gpui_base::{InputBase, input::InputEditorStyle};
    use gpui_component::{
        Disableable, Root, StyledExt,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        scroll::ScrollableElement as _,
    };
    const SIDEBAR_MARK: &[u8] =
        include_bytes!("../../../assets/branding/circuitfabric-sidebar-mark.png");

    // Design tokens: a dark-navy sidebar, a light content surface, and a cyan accent.
    const SIDEBAR_BG: u32 = 0x000f_172a;
    const SIDEBAR_DIVIDER: u32 = 0x001e_293b;
    const SIDEBAR_GROUP: u32 = 0x005f_7085;
    const SIDEBAR_TEXT: u32 = 0x009c_a7b8;
    const SIDEBAR_TEXT_ACTIVE: u32 = 0x00f1_f5f9;
    const SIDEBAR_ITEM_HOVER: u32 = 0x001a_2637;
    const SIDEBAR_ITEM_ACTIVE: u32 = 0x001e_2d46;
    const SIDEBAR_ITEM_PRESSED: u32 = 0x0026_3756;
    const ACCENT: u32 = 0x0022_d3ee;
    const ACCENT_SOFT: u32 = 0x0067_e8f9;

    const SURFACE_BG: u32 = 0x00f1_f5f9;
    const CARD_BG: u32 = 0x00ff_ffff;
    const BORDER: u32 = 0x00e2_e8f0;
    const TEXT_PRIMARY: u32 = 0x000f_172a;
    const TEXT_SECONDARY: u32 = 0x0047_5563;
    const TEXT_MUTED: u32 = 0x006b_7280;

    fn status_dot(color: u32) -> impl IntoElement {
        div().size(px(8.)).rounded_full().bg(rgb(color)).flex_none()
    }

    struct ProviderFields {
        id: Entity<InputState>,
        name: Entity<InputState>,
        base_url: Entity<InputState>,
        model: Entity<InputState>,
        api_key_environment_variable: Entity<InputState>,
        supports_vision: bool,
        vision_base_url: Entity<InputState>,
        vision_model: Entity<InputState>,
        vision_api_key_environment_variable: Entity<InputState>,
        enabled: bool,
    }

    /// One selectable runtime adapter on the Agents & tools page. Claude Code and DSH are
    /// deliberate placeholders: they show the planned surface without claiming to work.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum RuntimeAdapter {
        CodexAppServer,
        ClaudeCode,
        Dsh,
    }

    impl RuntimeAdapter {
        const fn label(self) -> &'static str {
            match self {
                Self::CodexAppServer => "Codex App Server",
                Self::ClaudeCode => "Claude Code",
                Self::Dsh => "DSH",
            }
        }

        const fn summary(self, language: UiLanguage) -> &'static str {
            match self {
                Self::CodexAppServer => language
                    .choose("本地子进程 · stdio JSON-RPC", "Local child process · stdio JSON-RPC"),
                Self::ClaudeCode => {
                    language.choose("本地任务 · JSON 输出", "Local task · JSON output")
                }
                Self::Dsh => {
                    language.choose("DeepSeek Harness · headless", "DeepSeek Harness · headless")
                }
            }
        }
    }

    /// What the Agents & tools detail pane currently shows.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum AgentsSelection {
        Runtime(RuntimeAdapter),
        Provider,
        SkillsAndMcp,
    }

    /// Observable state of the supervised Codex App Server process.
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum RuntimeLifecycleStatus {
        Starting,
        Running { pid: u32 },
        Stopped,
        Failed { reason: String },
    }

    impl RuntimeLifecycleStatus {
        const fn dot(&self) -> u32 {
            match self {
                Self::Starting => 0x00f5_9e0b,
                Self::Running { .. } => 0x0022_c55e,
                Self::Stopped => 0x0094_a3b8,
                Self::Failed { .. } => 0x00dc_2626,
            }
        }

        fn label(self, language: UiLanguage) -> String {
            match self {
                Self::Starting => language.choose("正在启动…", "Starting…").to_owned(),
                Self::Running { pid } => {
                    format!("{} · PID {pid}", language.choose("运行中", "Running"))
                }
                Self::Stopped => language.choose("已停止", "Stopped").to_owned(),
                Self::Failed { reason } => {
                    format!("{}：{reason}", language.choose("启动失败", "Start failed"))
                }
            }
        }
    }

    /// Where a skill or MCP-server authorization is stored.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ToolScope {
        Global,
        Project,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ProjectDetailTab {
        Overview,
        Documents,
        Sessions,
        AgentConfiguration,
        Usage,
    }

    impl ProjectDetailTab {
        const ALL: [Self; 5] = [
            Self::Overview,
            Self::Documents,
            Self::Sessions,
            Self::AgentConfiguration,
            Self::Usage,
        ];

        const fn label(self, language: UiLanguage) -> &'static str {
            match (self, language) {
                (Self::Overview, UiLanguage::SimplifiedChinese) => "概览",
                (Self::Documents, UiLanguage::SimplifiedChinese) => "文档",
                (Self::Sessions, UiLanguage::SimplifiedChinese) => "会话",
                (Self::AgentConfiguration, UiLanguage::SimplifiedChinese) => "智能体配置",
                (Self::Usage, UiLanguage::SimplifiedChinese) => "用量",
                (Self::Overview, UiLanguage::English) => "Overview",
                (Self::Documents, UiLanguage::English) => "Documents",
                (Self::Sessions, UiLanguage::English) => "Sessions",
                (Self::AgentConfiguration, UiLanguage::English) => "Agent configuration",
                (Self::Usage, UiLanguage::English) => "Usage",
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ProjectFilter {
        All,
        NeedsConfiguration,
    }

    /// One project's cached view of its persisted workspace state.
    #[derive(Default)]
    struct ProjectWorkspaceData {
        documents: Vec<ProjectDocument>,
        session_listing: SessionListing,
    }

    /// The read-only replay currently displayed in the Sessions tab.
    struct SessionReplaySelection {
        project_id: ProjectId,
        replay: SessionReplay,
    }

    struct ControlPlaneView {
        sidebar_mark: Arc<Image>,
        command: Entity<InputState>,
        working_directory: Entity<InputState>,
        bridge_address: Entity<InputState>,
        providers: Vec<ProviderFields>,
        default_provider_id: String,
        selected_provider: usize,
        screen: ControlPlaneScreen,
        language: UiLanguage,
        settings_path: std::path::PathBuf,
        status: String,
        command_palette_open: bool,
        command_search: Entity<InputState>,
        command_selected: usize,
        workspace: ProjectWorkspace,
        project_registry: ProjectRegistry,
        project_registry_path: PathBuf,
        project_storages: BTreeMap<ProjectId, ProjectStorage>,
        project_data: BTreeMap<ProjectId, ProjectWorkspaceData>,
        session_replay: Option<SessionReplaySelection>,
        evidence_query: Entity<InputState>,
        selected_project: Option<ProjectId>,
        project_search: Entity<InputState>,
        project_filter: ProjectFilter,
        project_tab: ProjectDetailTab,
        project_form_open: bool,
        new_project_id: Entity<InputState>,
        new_project_name: Entity<InputState>,
        new_project_description: Entity<InputState>,
        new_project_root: Entity<InputState>,
        agents_selection: AgentsSelection,
        codex_process: Option<CodexAppServerHandle>,
        codex_status: RuntimeLifecycleStatus,
        tool_authorizations: ToolAuthorizationSettings,
        new_tool_id: Entity<InputState>,
        new_tool_kind: ToolAuthorizationKind,
        new_tool_scope: ToolScope,
        catalog: circuitfabric_codex_runtime::tools::ToolCatalog,
        catalog_id: Entity<InputState>,
        catalog_source: Entity<InputState>,
        catalog_args: Entity<InputState>,
        catalog_env: Entity<InputState>,
        adapters: circuitfabric_codex_runtime::execution::AdapterSettings,
        adapter_command: Entity<InputState>,
        adapter_provider: Entity<InputState>,
        task_prompt: Entity<InputState>,
        task_image: Entity<InputState>,
        task_result: String,
        task_cancel: Option<circuitfabric_codex_runtime::execution::Cancellation>,
    }

    impl Drop for ControlPlaneView {
        fn drop(&mut self) {
            if let Some(cancel) = &self.task_cancel {
                cancel.cancel();
            }
        }
    }

    impl ControlPlaneView {
        fn project_registry_path(settings_path: &Path) -> PathBuf {
            settings_path.with_file_name("projects.json")
        }

        /// Loads one opened storage into the in-memory workspace and caches its listings.
        fn attach_project_storage(
            workspace: &mut ProjectWorkspace,
            storages: &mut BTreeMap<ProjectId, ProjectStorage>,
            data: &mut BTreeMap<ProjectId, ProjectWorkspaceData>,
            storage: ProjectStorage,
        ) -> Result<(), String> {
            let project_id = storage.manifest().project.id.clone();
            workspace
                .create_project(storage.manifest().project.clone())
                .map_err(|error| error.to_string())?;
            let configuration =
                storage.load_configuration().map_err(|error| format!("项目配置未恢复：{error}"))?;
            workspace
                .set_configuration(&project_id, configuration)
                .map_err(|error| error.to_string())?;
            if let Err(error) = workspace.hydrate_project_documents(&project_id, &storage) {
                return Err(format!("文档证据未恢复：{error}"));
            }
            let documents =
                storage.list_documents().map_err(|error| format!("文档索引未读取：{error}"))?;
            let session_listing =
                storage.list_sessions().map_err(|error| format!("会话记录未读取：{error}"))?;
            data.insert(project_id.clone(), ProjectWorkspaceData { documents, session_listing });
            storages.insert(project_id, storage);
            Ok(())
        }

        fn restore_project_workspace(
            registry_path: &Path,
        ) -> (
            ProjectRegistry,
            ProjectWorkspace,
            BTreeMap<ProjectId, ProjectStorage>,
            BTreeMap<ProjectId, ProjectWorkspaceData>,
            Vec<String>,
        ) {
            let registry = match ProjectRegistry::load_or_default(registry_path) {
                Ok(registry) => registry,
                Err(error) => {
                    return (
                        ProjectRegistry::default(),
                        ProjectWorkspace::default(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        vec![format!("项目注册表未加载：{error}")],
                    );
                }
            };
            let mut workspace = ProjectWorkspace::default();
            let mut storages = BTreeMap::new();
            let mut data = BTreeMap::new();
            let mut diagnostics = Vec::new();
            for entry in registry.entries() {
                match ProjectStorage::open(&entry.canonical_root_path) {
                    Ok(storage) if storage.manifest().project.id == entry.project_id => {
                        if let Err(error) = Self::attach_project_storage(
                            &mut workspace,
                            &mut storages,
                            &mut data,
                            storage,
                        ) {
                            diagnostics.push(format!("未恢复项目 `{}`：{error}", entry.project_id));
                        }
                    }
                    Ok(_) => diagnostics.push(format!(
                        "项目 `{}` 的根目录身份已变化，未自动打开。",
                        entry.project_id
                    )),
                    Err(error) => {
                        diagnostics.push(format!("项目 `{}` 无法打开：{error}", entry.project_id));
                    }
                }
            }
            (registry, workspace, storages, data, diagnostics)
        }

        fn input(
            window: &mut Window,
            value: String,
            placeholder: &'static str,
            cx: &mut Context<Self>,
        ) -> Entity<InputState> {
            cx.new(|cx| InputState::new(window, cx).default_value(value).placeholder(placeholder))
        }

        fn provider_fields(
            window: &mut Window,
            provider: LlmProviderSettings,
            cx: &mut Context<Self>,
        ) -> ProviderFields {
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
                supports_vision: provider.supports_vision,
                vision_base_url: Self::input(
                    window,
                    provider.vision_base_url.unwrap_or_default(),
                    "https://api.example.com/v1",
                    cx,
                ),
                vision_model: Self::input(
                    window,
                    provider.vision_model.unwrap_or_default(),
                    "vision-model-name",
                    cx,
                ),
                vision_api_key_environment_variable: Self::input(
                    window,
                    provider.vision_api_key_environment_variable.unwrap_or_default(),
                    "VISION_API_KEY_ENVIRONMENT_VARIABLE",
                    cx,
                ),
                enabled: provider.enabled,
            }
        }

        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let settings_path = RuntimeSettings::default_path();
            let loaded = RuntimeSettings::load_or_default(&settings_path);
            let load_error = loaded.as_ref().err().map(ToString::to_string);
            let settings = loaded.unwrap_or_default();
            let project_registry_path = Self::project_registry_path(&settings_path);
            let (
                project_registry,
                workspace,
                project_storages,
                project_data,
                project_restore_diagnostics,
            ) = Self::restore_project_workspace(&project_registry_path);
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
            let evidence_query = Self::input(window, String::new(), "capacitor", cx);
            let new_project_id = Self::input(window, String::new(), "power-supply", cx);
            let new_project_name = Self::input(window, String::new(), "Power supply", cx);
            let new_project_description =
                Self::input(window, String::new(), "Optional design workspace description", cx);
            let new_project_root =
                Self::input(window, String::new(), "Choose an existing empty folder", cx);
            let new_tool_id =
                Self::input(window, String::new(), "evidence-search, bom-export …", cx);
            let catalog_id = Self::input(window, String::new(), "server-id", cx);
            let catalog_source =
                Self::input(window, String::new(), "SKILL.md 路径 / MCP 可执行文件", cx);
            let catalog_args = Self::input(window, "[]".to_owned(), "参数 JSON 数组", cx);
            let catalog_env = Self::input(window, String::new(), "MCP_API_KEY", cx);
            let adapter_command =
                Self::input(window, settings.adapters.claude_command.clone(), "可执行文件", cx);
            let adapter_provider = Self::input(window, String::new(), "留空使用默认 Provider", cx);
            let task_prompt = Self::input(window, String::new(), "输入任务以验证真实模型调用", cx);
            let task_image =
                Self::input(window, String::new(), "可选图片路径；使用 Vision 服务", cx);
            for input in
                [&project_search, &evidence_query, &new_project_id, &new_project_root, &new_tool_id]
            {
                cx.subscribe(input, |_, _, event, cx| {
                    if let InputEvent::Change = event {
                        cx.notify();
                    }
                })
                .detach();
            }
            let status = if let Some(error) = load_error {
                format!("运行时配置读取失败，请修复配置后重新打开：{error}")
            } else if project_restore_diagnostics.is_empty() {
                "项目列表已恢复；全局运行时设置尚未修改。".to_owned()
            } else {
                format!("项目恢复提示：{}", project_restore_diagnostics.join("；"))
            };
            Self {
                sidebar_mark: Arc::new(Image::from_bytes(ImageFormat::Png, SIDEBAR_MARK.to_vec())),
                command: Self::input(window, settings.codex.command, "codex", cx),
                working_directory: Self::input(
                    window,
                    settings.codex.working_directory.display().to_string(),
                    "working directory",
                    cx,
                ),
                bridge_address: Self::input(
                    window,
                    settings.bridge.listen_address,
                    "127.0.0.1:49630",
                    cx,
                ),
                providers,
                default_provider_id: settings.default_provider_id,
                selected_provider: 0,
                screen: ControlPlaneScreen::Overview,
                language: UiLanguage::SimplifiedChinese,
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
                evidence_query,
                selected_project: None,
                project_search,
                project_filter: ProjectFilter::All,
                project_tab: ProjectDetailTab::Overview,
                project_form_open: false,
                new_project_id,
                new_project_name,
                new_project_description,
                new_project_root,
                agents_selection: AgentsSelection::Runtime(RuntimeAdapter::CodexAppServer),
                codex_process: None,
                codex_status: RuntimeLifecycleStatus::Stopped,
                tool_authorizations: settings.tools,
                new_tool_id,
                catalog: settings.catalog,
                catalog_id,
                catalog_source,
                catalog_args,
                catalog_env,
                adapters: settings.adapters,
                adapter_command,
                adapter_provider,
                task_prompt,
                task_image,
                task_result: String::new(),
                task_cancel: None,
                new_tool_kind: ToolAuthorizationKind::Skill,
                new_tool_scope: ToolScope::Global,
            }
        }

        fn provider_values(&self, cx: &Context<Self>) -> Vec<LlmProviderSettings> {
            self.providers
                .iter()
                .map(|provider| LlmProviderSettings {
                    id: provider.id.read(cx).value().to_string(),
                    name: provider.name.read(cx).value().to_string(),
                    base_url: provider.base_url.read(cx).value().to_string(),
                    model: provider.model.read(cx).value().to_string(),
                    api_key_environment_variable: provider
                        .api_key_environment_variable
                        .read(cx)
                        .value()
                        .to_string(),
                    enabled: provider.enabled,
                    supports_vision: provider.supports_vision,
                    vision_base_url: Self::optional_value(
                        provider.vision_base_url.read(cx).value().to_string(),
                    ),
                    vision_model: Self::optional_value(
                        provider.vision_model.read(cx).value().to_string(),
                    ),
                    vision_api_key_environment_variable: Self::optional_value(
                        provider.vision_api_key_environment_variable.read(cx).value().to_string(),
                    ),
                })
                .collect()
        }

        fn optional_value(value: String) -> Option<String> {
            (!value.trim().is_empty()).then_some(value)
        }

        fn add_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let mut number = self.providers.len() + 1;
            let id = loop {
                let candidate = format!("provider-{number}");
                if self.providers.iter().all(|provider| provider.id.read(cx).value() != candidate) {
                    break candidate;
                }
                number += 1;
            };
            let provider = LlmProviderSettings {
                id,
                name: format!("Provider {number}"),
                base_url: "https://api.openai.com/v1".to_owned(),
                model: "gpt-5.4".to_owned(),
                api_key_environment_variable: "OPENAI_API_KEY".to_owned(),
                enabled: true,
                supports_vision: false,
                vision_base_url: None,
                vision_model: None,
                vision_api_key_environment_variable: None,
            };
            self.providers.push(Self::provider_fields(window, provider, cx));
            self.selected_provider = self.providers.len() - 1;
            self.agents_selection = AgentsSelection::Provider;
            "已添加 Provider，请填写配置后保存。".clone_into(&mut self.status);
            cx.notify();
        }

        fn remove_provider(&mut self, cx: &mut Context<Self>) {
            if self.providers.len() <= 1 {
                "至少保留一个 Provider。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let selected = self.selected_provider.min(self.providers.len() - 1);
            let removed_id = self.providers[selected].id.read(cx).value().to_string();
            self.providers.remove(selected);
            self.selected_provider = selected.min(self.providers.len() - 1);
            if self.default_provider_id == removed_id {
                self.default_provider_id = self.providers[0].id.read(cx).value().to_string();
            }
            self.status = format!("已移除 Provider `{removed_id}`，保存后生效。");
            cx.notify();
        }

        fn set_default_provider(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.default_provider_id = self.providers[selected].id.read(cx).value().to_string();
            self.status =
                format!("默认 Provider 已设为 `{}`，保存后生效。", self.default_provider_id);
            cx.notify();
        }

        fn toggle_provider(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.providers[selected].enabled = !self.providers[selected].enabled;
            self.status = format!(
                "Provider `{}` 已{}。",
                self.providers[selected].id.read(cx).value(),
                if self.providers[selected].enabled { "启用" } else { "停用" }
            );
            cx.notify();
        }

        fn toggle_vision(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.providers[selected].supports_vision = !self.providers[selected].supports_vision;
            self.status = format!(
                "Provider `{}` 的 Vision 已{}。",
                self.providers[selected].id.read(cx).value(),
                if self.providers[selected].supports_vision { "启用" } else { "停用" }
            );
            cx.notify();
        }

        /// Collects every runtime form on the Agents & tools page into persistable settings.
        fn runtime_settings_from_form(&self, cx: &Context<Self>) -> RuntimeSettings {
            let providers = self.provider_values(cx);
            let default_provider_id =
                if providers.iter().any(|provider| provider.id == self.default_provider_id) {
                    self.default_provider_id.clone()
                } else {
                    providers.first().map_or_else(String::new, |provider| provider.id.clone())
                };
            let mut settings = RuntimeSettings::default();
            settings.codex.command = self.command.read(cx).value().to_string();
            settings.codex.working_directory =
                self.working_directory.read(cx).value().to_string().into();
            if let Some(provider) =
                providers.iter().find(|provider| provider.id == default_provider_id)
            {
                settings.codex.model = Some(provider.model.clone());
                settings
                    .codex
                    .api_key_environment_variable
                    .clone_from(&provider.api_key_environment_variable);
            }
            settings.default_provider_id.clone_from(&default_provider_id);
            settings.providers = providers;
            settings.bridge.listen_address = self.bridge_address.read(cx).value().to_string();
            settings.tools = self.tool_authorizations.clone();
            settings.catalog = self.catalog.clone();
            settings.adapters = self.adapters.clone();
            settings
        }

        fn save_settings(&mut self, cx: &mut Context<Self>) {
            let settings = self.runtime_settings_from_form(cx);
            if settings.providers.is_empty() {
                "未保存：至少需要一个 Provider。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            self.default_provider_id.clone_from(&settings.default_provider_id);
            self.status = match settings.save(&self.settings_path) {
                Ok(()) => format!(
                    "已保存到 {}。默认 Provider：{}。API Key 仅保存环境变量名。",
                    self.settings_path.display(),
                    settings.default_provider_id
                ),
                Err(error) => format!("未保存：{error}"),
            };
            cx.notify();
        }

        /// Starts the supervised Codex App Server child process.
        ///
        /// The launch persists the current form first, so what runs is exactly what was saved,
        /// and briefly watches the process so an immediate exit is reported as a failure.
        fn start_codex_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.codex_process.is_some()
                || matches!(self.codex_status, RuntimeLifecycleStatus::Starting)
            {
                "未启动：Codex App Server 正在运行或正在启动。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let settings = self.runtime_settings_from_form(cx);
            let Some(provider) = circuitfabric_codex_runtime::execution::selected_provider(
                &settings,
                circuitfabric_codex_runtime::execution::AgentKind::Codex,
            )
            .cloned() else {
                "未启动：请先配置默认 Provider。".clone_into(&mut self.status);
                cx.notify();
                return;
            };
            if let Err(error) = settings.save(&self.settings_path) {
                self.status = format!("未启动：设置未保存（{error}）。");
                cx.notify();
                return;
            }
            self.default_provider_id.clone_from(&settings.default_provider_id);
            self.codex_status = RuntimeLifecycleStatus::Starting;
            self.status = format!("Codex App Server: {} / {}", provider.id, provider.model);
            let launch = cx.background_spawn(async move {
                CodexAppServerHandle::launch(&settings.codex, &provider)
                    .map_err(|error| error.to_string())
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = launch.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        match result {
                            Ok(handle) => {
                                let pid = handle.pid();
                                view.codex_process = Some(handle);
                                view.codex_status = RuntimeLifecycleStatus::Running { pid };
                            }
                            Err(reason) => {
                                view.codex_status =
                                    RuntimeLifecycleStatus::Failed { reason: reason.clone() };
                                view.status = reason;
                            }
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
            cx.notify();
        }

        fn stop_codex_runtime(&mut self, cx: &mut Context<Self>) {
            if let Some(cancel) = &self.task_cancel {
                cancel.cancel();
            }
            if matches!(self.codex_status, RuntimeLifecycleStatus::Starting) {
                "未停止：Codex App Server 正在启动，请稍候。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let Some(mut process) = self.codex_process.take() else {
                "运行时当前未在运行。".clone_into(&mut self.status);
                cx.notify();
                return;
            };
            let pid = process.pid();
            match process.stop() {
                Ok(()) => {
                    self.codex_status = RuntimeLifecycleStatus::Stopped;
                    self.status = format!(
                        "已停止 Codex App Server（PID {pid}）。已保存的运行时设置保持不变。"
                    );
                }
                Err(error) => {
                    self.codex_status =
                        RuntimeLifecycleStatus::Failed { reason: error.to_string() };
                    self.status = format!("停止失败（PID {pid}）：{error}");
                }
            }
            cx.notify();
        }

        /// Reconciles the lifecycle chip with the real process state: a process that died on its
        /// own is reported as stopped or failed instead of staying green.
        fn refresh_codex_lifecycle(&mut self) {
            let exit = self.codex_process.as_mut().and_then(CodexAppServerHandle::try_exit);
            if let Some(exit) = exit {
                self.codex_process = None;
                if exit.success() {
                    self.codex_status = RuntimeLifecycleStatus::Stopped;
                } else {
                    self.codex_status = RuntimeLifecycleStatus::Failed {
                        reason: format!("进程已退出（{exit}）"),
                    };
                }
            }
        }

        /// Splits a tool-authorization input into IDs: commas (ASCII, full-width, and
        /// ideographic), semicolons, or whitespace separate items, and empty segments drop out.
        /// IDs themselves may never contain whitespace, so the split is unambiguous.
        fn parse_tool_ids(raw: &str) -> Vec<String> {
            raw.split(|character: char| {
                matches!(character, ',' | '，' | '、' | ';') || character.is_whitespace()
            })
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect()
        }

        /// Authorizes one or more skills / MCP servers in the selected scope, persisting
        /// immediately. Several IDs can be pasted at once; each becomes its own list row.
        fn authorize_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let requested = Self::parse_tool_ids(&self.new_tool_id.read(cx).value());
            if requested.is_empty() {
                "未授权：请输入至少一个 ID（多项之间用逗号或空格分隔）。"
                    .clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let kind = self.new_tool_kind;
            if requested.iter().any(|id| match kind {
                ToolAuthorizationKind::Skill => {
                    !self.catalog.skills.iter().any(|s| &s.id == id && s.enabled)
                }
                ToolAuthorizationKind::McpServer => {
                    !self.catalog.mcp_servers.iter().any(|s| &s.id == id && s.enabled)
                }
            }) {
                "未授权：请先导入技能或保存 MCP 定义，并启用所选条目。"
                    .clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let kind_label = Self::tool_kind_label(kind, self.language);
            match self.new_tool_scope {
                ToolScope::Global => {
                    let previous = self.tool_authorizations.clone();
                    let list = Self::global_tool_list_mut(&mut self.tool_authorizations, kind);
                    let mut authorized: Vec<String> = Vec::new();
                    let mut skipped = 0_usize;
                    for id in requested {
                        if list.contains(&id) {
                            skipped += 1;
                        } else {
                            list.push(id.clone());
                            authorized.push(id);
                        }
                    }
                    if authorized.is_empty() {
                        self.status = format!("未新增：所填{kind_label}均已授权（全局作用域）。");
                        cx.notify();
                        return;
                    }
                    list.sort();
                    list.dedup();
                    let skipped_note = if skipped == 0 {
                        String::new()
                    } else {
                        format!("（另跳过已授权的 {skipped} 项）")
                    };
                    let settings = self.runtime_settings_from_form(cx);
                    match settings.save(&self.settings_path) {
                        Ok(()) => {
                            self.default_provider_id.clone_from(&settings.default_provider_id);
                            self.new_tool_id
                                .update(cx, |state, cx| state.set_value("", window, cx));
                            self.status = format!(
                                "已授权 {} 项{kind_label}（全局作用域）：{}{skipped_note}。已写入 {}。",
                                authorized.len(),
                                authorized.join("、"),
                                self.settings_path.display()
                            );
                        }
                        Err(error) => {
                            self.tool_authorizations = previous;
                            self.status =
                                format!("未授权（全局授权会立即保存运行时设置）：{error}");
                        }
                    }
                }
                ToolScope::Project => {
                    let Some(project_id) = self.selected_project.clone() else {
                        "未授权：请先在「项目」页选择一个项目。".clone_into(&mut self.status);
                        cx.notify();
                        return;
                    };
                    let Some(storage) = self.project_storages.get(&project_id).cloned() else {
                        "未授权：项目根目录未打开。".clone_into(&mut self.status);
                        cx.notify();
                        return;
                    };
                    let mut configuration =
                        self.workspace.configuration(&project_id).cloned().unwrap_or_default();
                    let list = match kind {
                        ToolAuthorizationKind::Skill => &mut configuration.enabled_skill_ids,
                        ToolAuthorizationKind::McpServer => {
                            &mut configuration.enabled_mcp_server_ids
                        }
                    };
                    let mut authorized: Vec<String> = Vec::new();
                    let mut skipped = 0_usize;
                    for id in requested {
                        if list.contains(&id) {
                            skipped += 1;
                        } else {
                            list.push(id.clone());
                            authorized.push(id);
                        }
                    }
                    if authorized.is_empty() {
                        self.status = format!(
                            "未新增：所填{kind_label}均已授权（项目 `{project_id}` 作用域）。"
                        );
                        cx.notify();
                        return;
                    }
                    list.sort();
                    list.dedup();
                    let skipped_note = if skipped == 0 {
                        String::new()
                    } else {
                        format!("（另跳过已授权的 {skipped} 项）")
                    };
                    match storage.save_configuration(&configuration) {
                        Ok(()) => match self
                            .workspace
                            .set_configuration(&project_id, configuration.clone())
                        {
                            Ok(()) => {
                                self.new_tool_id
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                self.status = format!(
                                    "已授权 {} 项{kind_label}（项目 `{project_id}` 作用域）：{}{skipped_note}。已写入 {}。",
                                    authorized.len(),
                                    authorized.join("、"),
                                    storage.configuration_path().display()
                                );
                            }
                            Err(error) => {
                                self.status = format!(
                                    "文件已保存，但项目内存未刷新；请重新打开项目：{error}"
                                );
                            }
                        },
                        Err(error) => self.status = format!("未授权：{error}"),
                    }
                }
            }
            cx.notify();
        }

        /// Revokes one skill or MCP server authorization, persisting immediately.
        fn revoke_tool(
            &mut self,
            scope: ToolScope,
            kind: ToolAuthorizationKind,
            id: &str,
            cx: &mut Context<Self>,
        ) {
            if let Some(cancel) = &self.task_cancel {
                cancel.cancel();
            }
            let kind_label = Self::tool_kind_label(kind, self.language);
            match scope {
                ToolScope::Global => {
                    let previous = self.tool_authorizations.clone();
                    let list = Self::global_tool_list_mut(&mut self.tool_authorizations, kind);
                    list.retain(|existing| existing.as_str() != id);
                    let settings = self.runtime_settings_from_form(cx);
                    match settings.save(&self.settings_path) {
                        Ok(()) => {
                            self.default_provider_id.clone_from(&settings.default_provider_id);
                            self.status = format!(
                                "已撤销{kind_label} `{id}` 的全局授权，已写入 {}。",
                                self.settings_path.display()
                            );
                        }
                        Err(error) => {
                            self.tool_authorizations = previous;
                            self.status = format!("未撤销（撤销会立即保存运行时设置）：{error}");
                        }
                    }
                }
                ToolScope::Project => {
                    let Some(project_id) = self.selected_project.clone() else {
                        "未撤销：当前未选择项目。".clone_into(&mut self.status);
                        cx.notify();
                        return;
                    };
                    let Some(storage) = self.project_storages.get(&project_id).cloned() else {
                        "未撤销：项目根目录未打开。".clone_into(&mut self.status);
                        cx.notify();
                        return;
                    };
                    let mut configuration =
                        self.workspace.configuration(&project_id).cloned().unwrap_or_default();
                    let list = match kind {
                        ToolAuthorizationKind::Skill => &mut configuration.enabled_skill_ids,
                        ToolAuthorizationKind::McpServer => {
                            &mut configuration.enabled_mcp_server_ids
                        }
                    };
                    list.retain(|existing| existing.as_str() != id);
                    match storage.save_configuration(&configuration) {
                        Ok(()) => match self
                            .workspace
                            .set_configuration(&project_id, configuration.clone())
                        {
                            Ok(()) => {
                                self.status = format!(
                                    "已撤销{kind_label} `{id}` 在项目 `{project_id}` 中的授权，已写入 {}。",
                                    storage.configuration_path().display()
                                );
                            }
                            Err(error) => {
                                self.status = format!(
                                    "文件已保存，但项目内存未刷新；请重新打开项目：{error}"
                                );
                            }
                        },
                        Err(error) => self.status = format!("未撤销：{error}"),
                    }
                }
            }
            cx.notify();
        }

        const fn tool_kind_label(
            kind: ToolAuthorizationKind,
            language: UiLanguage,
        ) -> &'static str {
            match kind {
                ToolAuthorizationKind::Skill => language.choose("技能", "skill"),
                ToolAuthorizationKind::McpServer => language.choose("MCP 服务器", "MCP server"),
            }
        }

        fn global_tool_list_mut(
            tools: &mut ToolAuthorizationSettings,
            kind: ToolAuthorizationKind,
        ) -> &mut Vec<String> {
            match kind {
                ToolAuthorizationKind::Skill => &mut tools.authorized_skill_ids,
                ToolAuthorizationKind::McpServer => &mut tools.authorized_mcp_server_ids,
            }
        }

        fn open_project_form(&mut self, cx: &mut Context<Self>) {
            self.project_form_open = true;
            "选择一个已有文件夹，再确认创建受管理的项目目录。".clone_into(&mut self.status);
            cx.notify();
        }

        fn choose_project_root(&mut self, window: &Window, cx: &mut Context<Self>) {
            let dialog =
                rfd::AsyncFileDialog::new().set_title("选择项目根文件夹").set_parent(window);
            cx.spawn_in(window, async move |view, cx| {
                let Some(file_handle) = dialog.pick_folder().await else {
                    return;
                };
                let root = file_handle.path().to_path_buf();
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.new_project_root.update(cx, |state, cx| {
                            state.set_value(root.display().to_string(), window, cx);
                        });
                        view.status = format!(
                            "已选择项目根文件夹：{}。创建前不会修改该文件夹。",
                            root.display()
                        );
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
        }

        fn persist_project_registry(&self) -> Result<(), String> {
            self.project_registry
                .save(&self.project_registry_path)
                .map_err(|error| error.to_string())
        }

        fn select_project(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
            if let Some(cancel) = &self.task_cancel {
                cancel.cancel();
            }
            self.task_result.clear();
            if let Err(error) = self.project_registry.mark_opened(&project_id) {
                self.status = format!("项目已打开，但未能记录最近活动：{error}");
            } else if let Err(error) = self.persist_project_registry() {
                self.status = format!("项目已打开，但未能保存项目注册表：{error}");
            }
            self.selected_project = Some(project_id);
            self.project_tab = ProjectDetailTab::Overview;
            self.session_replay = None;
            cx.notify();
        }

        /// Reloads one project's cached document/session listings from its root.
        fn refresh_project_data(&mut self, project_id: &str) -> Result<(), String> {
            let storage = self
                .project_storages
                .get(project_id)
                .ok_or_else(|| "项目根目录未打开".to_owned())?;
            let documents =
                storage.list_documents().map_err(|error| format!("文档索引未读取：{error}"))?;
            let session_listing =
                storage.list_sessions().map_err(|error| format!("会话记录未读取：{error}"))?;
            self.project_data.entry(project_id.to_owned()).or_default().documents = documents;
            self.project_data.entry(project_id.to_owned()).or_default().session_listing =
                session_listing;
            Ok(())
        }

        fn import_project_document(
            &mut self,
            category: DocumentCategory,
            window: &Window,
            cx: &mut Context<Self>,
        ) {
            let Some(project_id) = self.selected_project.clone() else {
                return;
            };
            if !self.project_storages.contains_key(&project_id) {
                self.status = "未导入：项目根目录未打开。".to_owned();
                cx.notify();
                return;
            }
            let dialog_title = match category {
                DocumentCategory::Datasheet => "导入 Datasheet",
                DocumentCategory::ReferenceDesign => "导入参考设计",
            };
            let dialog = rfd::AsyncFileDialog::new().set_title(dialog_title).set_parent(window);
            cx.spawn_in(window, async move |view, cx| {
                let Some(file_handle) = dialog.pick_file().await else {
                    return;
                };
                let source = file_handle.path().to_path_buf();
                cx.update(|_window, cx| {
                    view.update(cx, |view, cx| {
                        let Some(storage) = view.project_storages.get(&project_id).cloned() else {
                            view.status = "未导入：项目根目录未打开。".to_owned();
                            cx.notify();
                            return;
                        };
                        match view.workspace.import_project_document(
                            &project_id,
                            &storage,
                            &source,
                            category,
                        ) {
                            Ok(imported) => {
                                let document = &imported.document;
                                if !imported.created {
                                    view.status = format!(
                                        "`{}` 已导入过（{}，类别 {}），未新增记录。",
                                        document.original_file_name,
                                        document.id,
                                        document.category.label(),
                                    );
                                } else {
                                    let searchable = is_text_extractable(&document.document_kind);
                                    if let Err(error) = view.refresh_project_data(&project_id) {
                                        view.status = format!("文档已导入，但列表未刷新：{error}");
                                    } else {
                                        view.status = format!(
                                            "已导入 `{}`（{}，{}…）{}。",
                                            document.original_file_name,
                                            document.id,
                                            &document.content_hash[..23],
                                            if searchable {
                                                "，文本可证据检索"
                                            } else {
                                                "，暂不参与文本检索"
                                            },
                                        );
                                    }
                                }
                            }
                            Err(error) => view.status = format!("未导入文档：{error}"),
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
        }

        fn open_session_replay(&mut self, session_id: String, cx: &mut Context<Self>) {
            let Some(project_id) = self.selected_project.clone() else {
                return;
            };
            let Some(storage) = self.project_storages.get(&project_id) else {
                self.status = "未打开会话：项目根目录未打开。".to_owned();
                cx.notify();
                return;
            };
            match storage.load_session(&session_id) {
                Ok(replay) => {
                    self.session_replay = Some(SessionReplaySelection { project_id, replay });
                }
                Err(error) => self.status = format!("未打开会话：{error}"),
            }
            cx.notify();
        }

        fn close_session_replay(&mut self, cx: &mut Context<Self>) {
            self.session_replay = None;
            cx.notify();
        }

        fn open_existing_project(&mut self, window: &Window, cx: &mut Context<Self>) {
            let dialog = rfd::AsyncFileDialog::new()
                .set_title("打开已有 CircuitFabric 项目")
                .set_parent(window);
            cx.spawn_in(window, async move |view, cx| {
                let Some(file_handle) = dialog.pick_folder().await else {
                    return;
                };
                let root = file_handle.path().to_path_buf();
                cx.update(|_window, cx| {
                    view.update(cx, |view, cx| {
                        let storage = match ProjectStorage::open(&root) {
                            Ok(storage) => storage,
                            Err(error) => {
                                view.status = format!("未打开项目：{error}");
                                cx.notify();
                                return;
                            }
                        };
                        let diagnostics = storage.diagnose_layout();
                        if !diagnostics.is_healthy() {
                            view.status = format!(
                                "项目未注册：目录布局不完整或不安全（缺失：{}；不安全：{}）。",
                                diagnostics
                                    .missing_entries
                                    .iter()
                                    .map(|entry| entry.display().to_string())
                                    .collect::<Vec<_>>()
                                    .join("、"),
                                diagnostics
                                    .unsafe_entries
                                    .iter()
                                    .map(|entry| entry.display().to_string())
                                    .collect::<Vec<_>>()
                                    .join("、"),
                            );
                            cx.notify();
                            return;
                        }

                        let project = storage.manifest().project.clone();
                        let opened_root = storage.root().display().to_string();
                        if let Some(registered_root) = view.project_registry.root_for(&project.id) {
                            if registered_root != storage.root() {
                                view.status = format!(
                                    "未打开项目：项目 ID `{}` 已绑定到 {}。",
                                    project.id,
                                    registered_root.display()
                                );
                                cx.notify();
                                return;
                            }
                        } else if let Err(error) = view.project_registry.register(&storage) {
                            view.status = format!("未注册已有项目：{error}");
                            cx.notify();
                            return;
                        }
                        if view.workspace.project(&project.id).is_none() {
                            if let Err(error) = Self::attach_project_storage(
                                &mut view.workspace,
                                &mut view.project_storages,
                                &mut view.project_data,
                                storage,
                            ) {
                                view.status = format!("未打开项目：{error}");
                                cx.notify();
                                return;
                            }
                        }
                        if let Err(error) = view.project_registry.mark_opened(&project.id) {
                            view.status = format!("项目已打开，但未能记录最近活动：{error}");
                        } else if let Err(error) = view.persist_project_registry() {
                            view.status = format!("项目已打开，但未能保存项目注册表：{error}");
                        } else {
                            view.status = format!("已打开项目 `{}`：{opened_root}。", project.id);
                        }
                        view.selected_project = Some(project.id);
                        view.project_tab = ProjectDetailTab::Overview;
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
        }

        fn create_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let id = self.new_project_id.read(cx).value().trim().to_owned();
            let name = self.new_project_name.read(cx).value().trim().to_owned();
            let description = self.new_project_description.read(cx).value().trim().to_owned();
            let root = self.new_project_root.read(cx).value().trim().to_owned();
            if root.is_empty() {
                "未创建项目：请先选择项目根文件夹。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            if self.workspace.project(&id).is_some() {
                self.status = format!("未创建项目：项目 ID `{id}` 已被使用。");
                cx.notify();
                return;
            }
            let project = Project {
                id: id.clone(),
                name,
                description: (!description.is_empty()).then_some(description),
            };

            match ProjectStorage::create(&root, project.clone()) {
                Ok(storage) => {
                    let created_root = storage.root().display().to_string();
                    match self.project_registry.register(&storage) {
                        Ok(()) => {
                            let attached = Self::attach_project_storage(
                                &mut self.workspace,
                                &mut self.project_storages,
                                &mut self.project_data,
                                storage,
                            );
                            match attached {
                                Ok(()) => {
                                    let persistence_error = self.persist_project_registry().err();
                                    self.selected_project = Some(id.clone());
                                    self.project_tab = ProjectDetailTab::Overview;
                                    self.project_form_open = false;
                                    self.new_project_id
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    self.new_project_name
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    self.new_project_description
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    self.new_project_root
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    self.status = match persistence_error {
                                        Some(error) => format!(
                                            "项目文件已创建于 {created_root}，但项目注册表未保存：{error}。"
                                        ),
                                        None => format!("已创建项目 `{id}`：{created_root}。"),
                                    };
                                }
                                Err(error) => {
                                    self.status = format!(
                                        "项目文件已创建于 {created_root}，但未能加入当前工作区：{error}。"
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            self.status = format!(
                                "项目文件已创建于 {created_root}，但未能注册：{error}。文件未被删除。"
                            );
                        }
                    }
                }
                Err(error) => self.status = format!("未创建项目：{error}"),
            }
            cx.notify();
        }

        fn project_id_feedback(&self, cx: &Context<Self>) -> (&'static str, u32) {
            let id = self.new_project_id.read(cx).value();
            if id.trim().is_empty() {
                ("请输入唯一项目 ID。", TEXT_MUTED)
            } else if self.workspace.project(id.trim()).is_some() {
                ("此项目 ID 已被使用。", 0x00dc_2626)
            } else {
                ("此项目 ID 可用。", 0x0016_a34a)
            }
        }

        fn project_matches(&self, project: &Project, query: &str) -> bool {
            let query_matches = query.is_empty()
                || project.id.to_lowercase().contains(query)
                || project.name.to_lowercase().contains(query)
                || project
                    .description
                    .as_deref()
                    .is_some_and(|description| description.to_lowercase().contains(query));
            let filter_matches = match self.project_filter {
                ProjectFilter::All => true,
                ProjectFilter::NeedsConfiguration => {
                    self.workspace.configuration(&project.id).is_none()
                }
            };
            query_matches && filter_matches
        }
    }

    impl ControlPlaneView {
        fn agents_group_label(label: &'static str) -> impl IntoElement {
            div()
                .pt_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_MUTED))
                .child(label)
        }

        /// The API-key boundary note shown wherever credentials are referenced.
        fn info_note(zh: &'static str, en: &'static str, language: UiLanguage) -> impl IntoElement {
            div()
                .flex()
                .items_start()
                .gap_2p5()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(ACCENT_SOFT))
                .bg(rgb(0x00f0_f9ff))
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .flex_none()
                        .rounded_sm()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x000e_7490))
                        .bg(rgb(0x00e0_f2fe))
                        .child(language.choose("密钥边界", "Key boundary")),
                )
                .child(
                    div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(language.choose(zh, en)),
                )
        }

        fn labeled_field(
            label: &'static str,
            id: &'static str,
            hint: Option<&'static str>,
            state: &Entity<InputState>,
        ) -> impl IntoElement {
            div()
                .v_flex()
                .gap_1()
                .flex_1()
                .min_w(px(240.))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                .child(
                    div()
                        .h(px(36.))
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            InputBase::new(id)
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .child(state.clone()),
                        ),
                )
                .when_some(hint, |this, hint| {
                    this.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(hint))
                })
        }

        fn tool_authorization_row(
            scope: ToolScope,
            kind: ToolAuthorizationKind,
            id: &str,
            language: UiLanguage,
            entity: &Entity<Self>,
        ) -> impl IntoElement {
            let revoker = entity.clone();
            let owned_id = id.to_owned();
            let scope_key = match scope {
                ToolScope::Global => "global",
                ToolScope::Project => "project",
            };
            let scope_label = match scope {
                ToolScope::Global => language.choose("全局", "Global"),
                ToolScope::Project => language.choose("项目", "Project"),
            };
            let (badge_background, badge_foreground) = match kind {
                ToolAuthorizationKind::Skill => (0x00e0_f2fe, 0x000e_7490),
                ToolAuthorizationKind::McpServer => (0x00f3_e8ff, 0x0076_2ba3),
            };
            div()
                .id(format!("tool-row-{scope_key}-{kind:?}-{owned_id}"))
                .flex()
                .items_center()
                .gap_2()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .text_xs()
                        .bg(rgb(badge_background))
                        .text_color(rgb(badge_foreground))
                        .child(Self::tool_kind_label(kind, language)),
                )
                .child(div().text_sm().child(owned_id.clone()))
                .child(
                    div()
                        .ml_auto()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .text_xs()
                        .bg(rgb(SURFACE_BG))
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(scope_label),
                )
                .child(
                    Button::new(format!("revoke-{scope_key}-{kind:?}-{owned_id}"))
                        .ghost()
                        .label(language.choose("撤销", "Revoke"))
                        .on_click(move |_, _, cx| {
                            revoker.update(cx, |view, cx| {
                                view.revoke_tool(scope, kind, &owned_id, cx);
                            });
                        }),
                )
        }

        #[allow(clippy::too_many_lines)]
        fn render_agents_page(
            &mut self,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;

            let mut runtime_cards = div().v_flex().gap_2();
            for adapter in
                [RuntimeAdapter::CodexAppServer, RuntimeAdapter::ClaudeCode, RuntimeAdapter::Dsh]
            {
                let selector = entity.clone();
                let selected = matches!(self.agents_selection, AgentsSelection::Runtime(chosen) if chosen == adapter);
                let is_codex = adapter == RuntimeAdapter::CodexAppServer;
                let status_label = if is_codex {
                    self.codex_status.clone().label(language)
                } else {
                    String::new()
                };
                runtime_cards = runtime_cards.child(
                    div()
                        .id(format!("runtime-card-{}", adapter.label()))
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if selected { ACCENT } else { BORDER }))
                        .bg(rgb(if selected { 0x00f0_f9ff } else { CARD_BG }))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                        .on_click(move |_, window, cx| {
                            selector.update(cx, |view, cx| {
                                view.agents_selection = AgentsSelection::Runtime(adapter);
                                let (command, provider) = match adapter {
                                    RuntimeAdapter::ClaudeCode => (
                                        &view.adapters.claude_command,
                                        &view.adapters.claude_provider_id,
                                    ),
                                    RuntimeAdapter::Dsh => {
                                        (&view.adapters.dsh_command, &view.adapters.dsh_provider_id)
                                    }
                                    RuntimeAdapter::CodexAppServer => (
                                        &view.adapters.claude_command,
                                        &view.adapters.codex_provider_id,
                                    ),
                                };
                                let command = command.clone();
                                let provider = provider.clone();
                                view.adapter_command
                                    .update(cx, |input, cx| input.set_value(command, window, cx));
                                view.adapter_provider
                                    .update(cx, |input, cx| input.set_value(provider, window, cx));
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when(is_codex, |row| {
                                    row.child(status_dot(self.codex_status.dot()))
                                })
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(if selected {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::MEDIUM
                                        })
                                        .child(adapter.label()),
                                )
                                .child(div().ml_auto().flex().items_center().gap_2().when(
                                    is_codex,
                                    |row| {
                                        row.child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(TEXT_MUTED))
                                                .child(status_label.clone()),
                                        )
                                    },
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(adapter.summary(language)),
                        ),
                );
            }

            let selected_provider = self.selected_provider.min(self.providers.len() - 1);
            let mut provider_cards = div().v_flex().gap_2();
            for (index, provider) in self.providers.iter().enumerate() {
                let selector = entity.clone();
                let selected = self.agents_selection == AgentsSelection::Provider
                    && index == selected_provider;
                let provider_id = provider.id.read(cx).value().to_string();
                let provider_name = provider.name.read(cx).value().to_string();
                let provider_model = provider.model.read(cx).value().to_string();
                let is_default = provider_id == self.default_provider_id;
                provider_cards = provider_cards.child(
                    div()
                        .id(format!("provider-card-{index}"))
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if selected { ACCENT } else { BORDER }))
                        .bg(rgb(if selected { 0x00f0_f9ff } else { CARD_BG }))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_provider = index;
                                view.agents_selection = AgentsSelection::Provider;
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(status_dot(if provider.enabled {
                                    0x0022_c55e
                                } else {
                                    0x0094_a3b8
                                }))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(if selected {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::MEDIUM
                                        })
                                        .truncate()
                                        .child(provider_name),
                                )
                                .when(is_default, |row| {
                                    row.child(
                                        div()
                                            .ml_auto()
                                            .text_xs()
                                            .text_color(rgb(0x00b4_5309))
                                            .child("★"),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(div().truncate().child(provider_id))
                                .child(div().truncate().child(provider_model)),
                        ),
                );
            }

            let tools_selected = self.agents_selection == AgentsSelection::SkillsAndMcp;
            let global_skill_count = self.tool_authorizations.authorized_skill_ids.len();
            let global_mcp_count = self.tool_authorizations.authorized_mcp_server_ids.len();
            let tools_project_line = match self
                .selected_project
                .as_deref()
                .and_then(|project_id| self.workspace.configuration(project_id))
            {
                Some(configuration) => language.choose_owned(
                    format!(
                        "当前项目：{} 技能 · {} MCP",
                        configuration.enabled_skill_ids.len(),
                        configuration.enabled_mcp_server_ids.len()
                    ),
                    format!(
                        "Project: {} skills · {} MCP",
                        configuration.enabled_skill_ids.len(),
                        configuration.enabled_mcp_server_ids.len()
                    ),
                ),
                None => language.choose("未选择项目", "No project selected").to_owned(),
            };
            let tools_selector = entity.clone();
            let tools_card = div()
                .id("tools-card")
                .v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(if tools_selected { ACCENT } else { BORDER }))
                .bg(rgb(if tools_selected { 0x00f0_f9ff } else { CARD_BG }))
                .cursor_pointer()
                .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                .on_click(move |_, _, cx| {
                    tools_selector.update(cx, |view, cx| {
                        view.agents_selection = AgentsSelection::SkillsAndMcp;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(if tools_selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .child(language.choose("技能与 MCP 授权", "Skills & MCP")),
                        )
                        .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose_owned(
                                format!("全局 {global_skill_count} · {global_mcp_count}"),
                                format!("{global_skill_count} · {global_mcp_count} global"),
                            ),
                        )),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(tools_project_line));

            let detail = match self.agents_selection {
                AgentsSelection::Runtime(RuntimeAdapter::CodexAppServer) => {
                    self.render_codex_runtime_detail(cx).into_any_element()
                }
                AgentsSelection::Runtime(adapter) => {
                    self.render_adapter_detail(adapter, cx).into_any_element()
                }
                AgentsSelection::Provider => self.render_provider_detail(cx).into_any_element(),
                AgentsSelection::SkillsAndMcp => self.render_skills_detail(cx).into_any_element(),
            };

            let save_runtime = entity.clone();
            let add_provider = entity.clone();
            div()
                .size_full()
                .min_w(px(880.))
                .relative()
                .v_flex()
                .gap_4()
                .p_6()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(language
                                            .choose("智能体与工具", "Agents & tools")),
                                )
                                .child(
                                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                        language.choose(
                                            "运行时端点、LLM Provider 与技能/MCP 授权统一在这里管理；API Key 始终只以环境变量名引用。",
                                            "Runtime endpoints, LLM providers, and skills/MCP authorizations in one place; API keys stay environment-variable names.",
                                        ),
                                    ),
                                ),
                        )
                        .child(
                            Button::new("save-runtime")
                                .primary()
                                .label(language.choose("保存设置", "Save settings"))
                                .on_click(move |_, _, cx| {
                                    save_runtime.update(cx, ControlPlaneView::save_settings);
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .flex()
                        .gap_4()
                        .child(
                            div()
                                .w(px(320.))
                                .flex_none()
                                .v_flex()
                                .gap_2()
                                .child(Self::agents_group_label(
                                    language.choose("运行时端点", "Runtime endpoints"),
                                ))
                                .child(runtime_cards)
                                .child(Self::agents_group_label(
                                    language.choose("LLM Provider", "LLM providers"),
                                ))
                                .child(provider_cards)
                                .child(
                                    Button::new("add-provider")
                                        .label(language.choose("添加 Provider", "Add provider"))
                                        .on_click(move |_, window, cx| {
                                            add_provider.update(cx, |view, cx| {
                                                view.add_provider(window, cx);
                                            });
                                        }),
                                )
                                .child(Self::agents_group_label(
                                    language.choose("技能与 MCP", "Skills & MCP"),
                                ))
                                .child(tools_card),
                        )
                        .child(detail),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
        }

        #[allow(clippy::too_many_lines)]
        fn render_codex_runtime_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let status = self.codex_status.clone();
            let is_running = self.codex_process.is_some();
            let is_starting = matches!(self.codex_status, RuntimeLifecycleStatus::Starting);
            let starter = entity.clone();
            let binding_saver = entity.clone();
            let stopper = entity;
            // The endpoint launches with the default provider — the same one
            // `runtime_settings_from_form` normalizes to — so surface that link here.
            let launch_provider = self
                .providers
                .iter()
                .find(|provider| {
                    provider.id.read(cx).value()
                        == *if self.adapters.codex_provider_id.is_empty() {
                            &self.default_provider_id
                        } else {
                            &self.adapters.codex_provider_id
                        }
                })
                .or_else(|| self.providers.first());
            let launch_provider_summary = match launch_provider {
                Some(provider) => format!(
                    "`{}`（{} · {}）",
                    provider.id.read(cx).value(),
                    provider.name.read(cx).value(),
                    provider.model.read(cx).value()
                ),
                None => language
                    .choose(
                        "尚未配置（请先添加 Provider）",
                        "none configured yet (add a provider first)",
                    )
                    .to_owned(),
            };
            div()
                .flex_1()
                .min_w(px(0.))
                .v_flex()
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Codex App Server"),
                                )
                                .child(
                                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                        language.choose(
                                            "本地 stdio JSON-RPC 端点；由 CircuitFabric 以子进程方式启动与停止。",
                                            "Local stdio JSON-RPC endpoint; started and stopped as a CircuitFabric child process.",
                                        ),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .bg(rgb(SURFACE_BG))
                                .child(status_dot(status.dot()))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(status.label(language)),
                                ),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("start-codex-runtime")
                                        .primary()
                                        .disabled(is_running || is_starting)
                                        .label(language.choose("启动", "Start"))
                                        .on_click(move |_, window, cx| {
                                            starter.update(cx, |view, cx| {
                                                view.start_codex_runtime(window, cx);
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("stop-codex-runtime")
                                        .disabled(!is_running && self.task_cancel.is_none())
                                        .label(language.choose("停止", "Stop"))
                                        .on_click(move |_, _, cx| {
                                            stopper.update(cx, ControlPlaneView::stop_codex_runtime);
                                        }),
                                ),
                        )
                        .child(
                            div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(format!(
                                "{}{}",
                                language.choose(
                                    "下次启动使用 Provider：",
                                    "Next launch provider: "
                                ),
                                launch_provider_summary,
                            )),
                        )
                        .child(
                            div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose(
                                    "启动会先保存当前设置，然后以子进程运行 Codex App Server；运行状态不持久化，退出 CircuitFabric 时进程会随之终止。停止只终止进程，不修改已保存的设置。切换 Provider：在 LLM Provider 详情中「设为默认」，再次启动即生效。",
                                    "Start saves the current settings first, then runs the Codex App Server as a child process; the running state is not persisted and ends with CircuitFabric. Stop terminates the process without changing saved settings. To switch providers, set a default in the LLM provider detail and start again.",
                                ),
                            ),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(Self::labeled_field(
                            language.choose("Codex 命令", "Codex command"),
                            "codex-command",
                            None,
                            &self.command,
                        ))
                        .child(Self::labeled_field(
                            language.choose("工作目录", "Working directory"),
                            "working-directory",
                            None,
                            &self.working_directory,
                        ))
                        .child(Self::labeled_field(
                            language.choose("JLC bridge 地址", "JLC bridge address"),
                            "bridge-address",
                            None,
                            &self.bridge_address,
                        )),
                )
                .child(Self::info_note(
                    "API Key 仅以环境变量名引用（在 LLM Provider 中配置）；CircuitFabric 不保存、不回显任何密钥值。",
                    "API keys are referenced by environment-variable name only (configured per LLM provider); CircuitFabric never stores or echoes a key value.",
                    language,
                ))
                .child(Self::labeled_field("Provider ID（留空使用默认项）", "codex-provider", None, &self.adapter_provider))
                .child(Button::new("save-codex-binding").label("保存关联").on_click(move |_, _, cx| {
                    binding_saver.update(cx, |view, cx| {
                        let before = view.adapters.codex_provider_id.clone();
                        view.adapters.codex_provider_id = view.adapter_provider.read(cx).value().to_string();
                        match view.runtime_settings_from_form(cx).save(&view.settings_path) {
                            Ok(()) => "已保存关联；新任务立即使用，连接检查进程须重启后生效".clone_into(&mut view.status),
                            Err(error) => { view.adapters.codex_provider_id = before; view.status = format!("未保存：{error}"); }
                        }
                        cx.notify();
                    });
                }))
                .child(self.render_task_controls(RuntimeAdapter::CodexAppServer, cx))
        }

        fn effective_grants(&self) -> ToolAuthorizationSettings {
            let mut grants = self.tool_authorizations.clone();
            if let Some(configuration) =
                self.selected_project.as_ref().and_then(|id| self.workspace.configuration(id))
            {
                grants.authorized_skill_ids.extend(configuration.enabled_skill_ids.clone());
                grants
                    .authorized_mcp_server_ids
                    .extend(configuration.enabled_mcp_server_ids.clone());
            }
            grants.authorized_skill_ids.sort();
            grants.authorized_skill_ids.dedup();
            grants.authorized_mcp_server_ids.sort();
            grants.authorized_mcp_server_ids.dedup();
            grants
        }

        fn run_agent_task(
            &mut self,
            adapter: RuntimeAdapter,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            use circuitfabric_codex_runtime::execution::{
                AgentKind, Cancellation, run_task_with_image,
            };
            if self.task_cancel.is_some() {
                return;
            }
            let settings = self.runtime_settings_from_form(cx);
            if let Err(error) = settings.save(&self.settings_path) {
                self.status = format!("未执行：{error}");
                cx.notify();
                return;
            }
            let grants = self.effective_grants();
            let mut prompt = self.task_prompt.read(cx).value().to_string();
            let image_text = self.task_image.read(cx).value().to_string();
            let image =
                if adapter == RuntimeAdapter::CodexAppServer && !image_text.trim().is_empty() {
                    Some(PathBuf::from(image_text))
                } else {
                    None
                };
            if let Some(instructions) = self
                .selected_project
                .as_ref()
                .and_then(|id| self.workspace.configuration(id))
                .and_then(|c| c.agent_instructions.as_ref())
            {
                prompt = format!("{instructions}\n\n{prompt}");
            }
            let kind = match adapter {
                RuntimeAdapter::CodexAppServer => AgentKind::Codex,
                RuntimeAdapter::ClaudeCode => AgentKind::Claude,
                RuntimeAdapter::Dsh => AgentKind::Dsh,
            };
            let cancel = Cancellation::default();
            self.task_cancel = Some(cancel.clone());
            let project = self.selected_project.clone();
            let provider =
                circuitfabric_codex_runtime::execution::selected_provider(&settings, kind);
            let snapshot = provider
                .map_or_else(String::new, |p| format!("{} / {} / {}", p.id, p.model, p.base_url));
            self.task_result = format!(
                "正在调用 {}：{snapshot}。使用已保存快照；修改配置在下次任务生效。",
                adapter.label()
            );
            let work = cx.background_spawn(async move {
                run_task_with_image(&settings, kind, &grants, &prompt, image.as_deref(), &cancel)
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = work.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.task_cancel = None;
                        if view.selected_project == project {
                            view.task_result = match result {
                                Ok(output) => format!("任务完成（{snapshot}）\n{output}"),
                                Err(error) => format!("任务未完成（{snapshot}）：{error}"),
                            };
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
            cx.notify();
        }

        fn render_task_controls(
            &mut self,
            adapter: RuntimeAdapter,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let runner = cx.entity().clone();
            let stopper = runner.clone();
            div()
                .v_flex()
                .gap_2()
                .child(Self::labeled_field("任务", "runtime-task", None, &self.task_prompt))
                .when(adapter == RuntimeAdapter::CodexAppServer, |panel| {
                    panel.child(Self::labeled_field(
                        "图片（可选）",
                        "runtime-image",
                        None,
                        &self.task_image,
                    ))
                })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("run-agent-task")
                                .label("执行任务")
                                .primary()
                                .disabled(self.task_cancel.is_some())
                                .on_click(move |_, window, cx| {
                                    runner.update(cx, |view, cx| {
                                        view.run_agent_task(adapter, window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("cancel-agent-task")
                                .label("取消任务")
                                .disabled(self.task_cancel.is_none())
                                .on_click(move |_, _, cx| {
                                    stopper.update(cx, |view, cx| {
                                        if let Some(cancel) = &view.task_cancel {
                                            cancel.cancel();
                                        }
                                        view.task_result = "正在取消并清理进程…".into();
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(div().text_sm().child(self.task_result.clone()))
        }

        fn render_adapter_detail(
            &mut self,
            adapter: RuntimeAdapter,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let saver = cx.entity().clone();
            div().flex_1().min_w(px(0.)).v_flex().gap_3().p_5().bg(rgb(CARD_BG)).rounded_xl()
                .child(div().text_xl().child(adapter.label()))
                .child("每次执行创建独立任务进程；完成、失败或取消后清理。Claude Code 使用 Anthropic Messages 协议。")
                .child(Self::labeled_field("运行时命令", "adapter-command", None, &self.adapter_command))
                .child(Self::labeled_field("Provider ID（留空使用默认项）", "adapter-provider", None, &self.adapter_provider))
                .child(Button::new("save-adapter").label("保存关联").on_click(move |_, _, cx| { saver.update(cx, |view, cx| {
                    let before = view.adapters.clone();
                    let command = view.adapter_command.read(cx).value().to_string();
                    let provider = view.adapter_provider.read(cx).value().to_string();
                    match adapter { RuntimeAdapter::ClaudeCode => { view.adapters.claude_command = command; view.adapters.claude_provider_id = provider; }, RuntimeAdapter::Dsh => { view.adapters.dsh_command = command; view.adapters.dsh_provider_id = provider; }, RuntimeAdapter::CodexAppServer => {} }
                    match view.runtime_settings_from_form(cx).save(&view.settings_path) { Ok(()) => view.status = "已保存运行时关联；下次任务生效".into(), Err(error) => { view.adapters = before; view.status = format!("未保存：{error}"); } }
                    cx.notify();
                }); }))
                .child(self.render_task_controls(adapter, cx))
        }

        fn render_provider_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let selected = self.selected_provider.min(self.providers.len() - 1);
            let provider = &self.providers[selected];
            let provider_id = provider.id.read(cx).value().to_string();
            let provider_name = provider.name.read(cx).value().to_string();
            let selected_is_default = provider_id == self.default_provider_id;
            let set_default = entity.clone();
            let toggle_provider = entity.clone();
            let remove_provider = entity.clone();
            let toggle_vision = entity;

            let default_badge = selected_is_default.then(|| {
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .text_xs()
                    .bg(rgb(0x00fe_f3c7))
                    .text_color(rgb(0x00b4_5309))
                    .child(language.choose("★ 默认", "★ Default"))
            });
            let enabled_badge = div()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .text_xs()
                .bg(rgb(if provider.enabled { 0x00dc_fce7 } else { SURFACE_BG }))
                .text_color(rgb(if provider.enabled { 0x0016_a34a } else { TEXT_MUTED }))
                .child(if provider.enabled {
                    language.choose("已启用", "Enabled")
                } else {
                    language.choose("已停用", "Disabled")
                });
            let vision_badge = div()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .text_xs()
                .bg(rgb(0x00e0_f2fe))
                .text_color(rgb(0x000e_7490))
                .child("Vision");

            div()
                .flex_1()
                .min_w(px(0.))
                .v_flex()
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(provider_name),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(TEXT_MUTED))
                                        .child(provider_id),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when_some(default_badge, ParentElement::child)
                                .child(enabled_badge)
                                .when(provider.supports_vision, |row| row.child(vision_badge)),
                        ),
                )
                .child(Self::info_note(
                    "API Key 只记录环境变量名：CircuitFabric 不保存、不回显密钥值；子进程启动时直接从环境读取。",
                    "API keys are stored as environment-variable names only: CircuitFabric never saves or echoes a key value; the child process reads it from the environment at launch.",
                    language,
                ))
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(Self::labeled_field(
                                    "Provider ID",
                                    "provider-id",
                                    None,
                                    &provider.id,
                                ))
                                .child(Self::labeled_field(
                                    language.choose("显示名称", "Display name"),
                                    "provider-name",
                                    None,
                                    &provider.name,
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(Self::labeled_field(
                                    "LLM Base URL",
                                    "provider-base-url",
                                    None,
                                    &provider.base_url,
                                ))
                                .child(Self::labeled_field(
                                    language.choose("LLM 模型", "LLM model"),
                                    "provider-model",
                                    None,
                                    &provider.model,
                                )),
                        )
                        .child(Self::labeled_field(
                            language.choose("LLM API Key 环境变量名", "LLM API key environment variable"),
                            "provider-api-key-env",
                            Some(language.choose(
                                "仅环境变量名，例如 OPENAI_API_KEY；密钥值不会出现在这里。",
                                "Environment-variable name only, e.g. OPENAI_API_KEY; the key value never appears here.",
                            )),
                            &provider.api_key_environment_variable,
                        )),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(language.choose("Vision 配置", "Vision configuration")),
                                )
                                .child(
                                    Button::new("toggle-vision")
                                        .label(if provider.supports_vision {
                                            language.choose("Vision：已启用", "Vision: enabled")
                                        } else {
                                            language.choose("Vision：已停用", "Vision: disabled")
                                        })
                                        .on_click(move |_, _, cx| {
                                            toggle_vision.update(cx, ControlPlaneView::toggle_vision);
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(Self::labeled_field(
                                    "Vision Base URL",
                                    "vision-base-url",
                                    None,
                                    &provider.vision_base_url,
                                ))
                                .child(Self::labeled_field(
                                    language.choose("Vision 模型", "Vision model"),
                                    "vision-model",
                                    None,
                                    &provider.vision_model,
                                )),
                        )
                        .child(Self::labeled_field(
                            language.choose(
                                "Vision API Key 环境变量名",
                                "Vision API key environment variable",
                            ),
                            "vision-api-key-env",
                            Some(language.choose(
                                "同样只保存环境变量名。",
                                "Also an environment-variable name only.",
                            )),
                            &provider.vision_api_key_environment_variable,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("set-default-provider")
                                .label(if selected_is_default {
                                    language.choose("当前为默认 Provider", "Current default provider")
                                } else {
                                    language.choose("设为默认 Provider", "Set as default provider")
                                })
                                .disabled(selected_is_default)
                                .on_click(move |_, _, cx| {
                                    set_default.update(cx, ControlPlaneView::set_default_provider);
                                }),
                        )
                        .child(
                            Button::new("toggle-provider")
                                .label(if provider.enabled {
                                    language.choose("停用", "Disable")
                                } else {
                                    language.choose("启用", "Enable")
                                })
                                .on_click(move |_, _, cx| {
                                    toggle_provider.update(cx, ControlPlaneView::toggle_provider);
                                }),
                        )
                        .child(
                            Button::new("remove-provider")
                                .danger()
                                .label(language.choose("删除", "Remove"))
                                .on_click(move |_, _, cx| {
                                    remove_provider.update(cx, ControlPlaneView::remove_provider);
                                }),
                        ),
                )
                .child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "「设为默认」后，Codex App Server 端点将使用此 Provider 启动（启动时读取，运行中的进程不受影响）。",
                            "After \"Set as default\", the Codex App Server endpoint launches with this provider (read at start; a running process is unaffected).",
                        ),
                    ),
                )
        }

        #[allow(clippy::too_many_lines)]
        fn render_catalog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let skill_importer = entity.clone();
            let mcp_saver = entity.clone();
            let mut rows = div().v_flex().gap_2();
            for skill in self.catalog.skills.clone() {
                let remover = entity.clone();
                let toggler = entity.clone();
                let id = skill.id.clone();
                let toggle_id = id.clone();
                let authorizer = entity.clone();
                let grant_id = id.clone();
                let previewer = entity.clone();
                let preview_id = id.clone();
                rows = rows.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(format!(
                            "技能 {} · {} · {}",
                            skill.id,
                            if skill.enabled { "启用" } else { "停用" },
                            skill.path.display()
                        ))
                        .child(
                            Button::new(format!("skill-grant-{id}"))
                                .label("授权到所选作用域")
                                .on_click(move |_, window, cx| {
                                    authorizer.update(cx, |view, cx| {
                                        view.new_tool_kind = ToolAuthorizationKind::Skill;
                                        view.new_tool_id.update(cx, |input, cx| {
                                            input.set_value(grant_id.clone(), window, cx)
                                        });
                                        view.authorize_tool(window, cx);
                                    });
                                }),
                        )
                        .child(Button::new(format!("skill-preview-{id}")).label("详情").on_click(
                            move |_, _, cx| {
                                previewer.update(cx, |view, cx| {
                                    view.status = match view.catalog.skill_instructions(
                                        &ToolAuthorizationSettings {
                                            authorized_skill_ids: vec![preview_id.clone()],
                                            authorized_mcp_server_ids: vec![],
                                        },
                                    ) {
                                        Ok(text) => text,
                                        Err(error) => format!("无法读取技能：{error}"),
                                    };
                                    cx.notify();
                                });
                            },
                        ))
                        .child(
                            Button::new(format!("skill-toggle-{id}")).label("启用/停用").on_click(
                                move |_, _, cx| {
                                    toggler.update(cx, |view, cx| {
                                        let previous = view.catalog.clone();
                                        if let Some(s) = view
                                            .catalog
                                            .skills
                                            .iter_mut()
                                            .find(|s| s.id == toggle_id)
                                        {
                                            s.enabled = !s.enabled;
                                        }
                                        view.persist_catalog(previous, cx);
                                    });
                                },
                            ),
                        )
                        .child(Button::new(format!("skill-delete-{id}")).label("删除").on_click(
                            move |_, _, cx| {
                                remover.update(cx, |view, cx| {
                                    let previous = view.catalog.clone();
                                    view.catalog.skills.retain(|s| s.id != id);
                                    view.persist_catalog(previous, cx);
                                });
                            },
                        )),
                );
            }
            for server in self.catalog.mcp_servers.clone() {
                let remover = entity.clone();
                let tester = entity.clone();
                let editor = entity.clone();
                let toggler = entity.clone();
                let id = server.id.clone();
                let test_id = id.clone();
                let toggle_id = id.clone();
                let authorizer = entity.clone();
                let grant_id = id.clone();
                rows = rows.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(format!(
                            "MCP {} · {} · {}",
                            server.id,
                            if server.enabled { "启用" } else { "停用" },
                            server.command
                        ))
                        .child(
                            Button::new(format!("mcp-grant-{id}"))
                                .label("授权到所选作用域")
                                .on_click(move |_, window, cx| {
                                    authorizer.update(cx, |view, cx| {
                                        view.new_tool_kind = ToolAuthorizationKind::McpServer;
                                        view.new_tool_id.update(cx, |input, cx| {
                                            input.set_value(grant_id.clone(), window, cx)
                                        });
                                        view.authorize_tool(window, cx);
                                    });
                                }),
                        )
                        .child(Button::new(format!("mcp-edit-{id}")).label("编辑").on_click(
                            move |_, window, cx| {
                                editor.update(cx, |view, cx| {
                                    view.catalog_id.update(cx, |input, cx| {
                                        input.set_value(server.id.clone(), window, cx);
                                    });
                                    view.catalog_source.update(cx, |input, cx| {
                                        input.set_value(server.command.clone(), window, cx);
                                    });
                                    view.catalog_args.update(cx, |input, cx| {
                                        input.set_value(
                                            serde_json::to_string(&server.args).unwrap_or_default(),
                                            window,
                                            cx,
                                        );
                                    });
                                    view.catalog_env.update(cx, |input, cx| {
                                        input.set_value(
                                            server.environment_variables.join(","),
                                            window,
                                            cx,
                                        );
                                    });
                                    cx.notify();
                                });
                            },
                        ))
                        .child(Button::new(format!("mcp-toggle-{id}")).label("启用/停用").on_click(
                            move |_, _, cx| {
                                toggler.update(cx, |view, cx| {
                                    let previous = view.catalog.clone();
                                    if let Some(s) = view
                                        .catalog
                                        .mcp_servers
                                        .iter_mut()
                                        .find(|s| s.id == toggle_id)
                                    {
                                        s.enabled = !s.enabled;
                                    }
                                    view.persist_catalog(previous, cx);
                                });
                            },
                        ))
                        .child(
                            Button::new(format!("mcp-test-{id}")).label("连接并发现工具").on_click(
                                move |_, window, cx| {
                                    tester.update(cx, |view, cx| {
                                        let catalog = view.catalog.clone();
                                        let grants = view.effective_grants();
                                        let id = test_id.clone();
                                        view.status = "正在连接 MCP…".into();
                                        let work = cx.background_spawn(async move {
                                            catalog
                                                .list_tools(&id, &grants)
                                                .map(|value| value.to_string())
                                                .map_err(|e| e.to_string())
                                        });
                                        cx.spawn_in(window, async move |view, cx| {
                                            let result = work.await;
                                            cx.update(|_, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.status = match result {
                                                        Ok(tools) => {
                                                            format!("MCP 已连接，工具：{tools}")
                                                        }
                                                        Err(error) => {
                                                            format!("MCP 连接失败：{error}")
                                                        }
                                                    };
                                                    cx.notify();
                                                })
                                                .ok();
                                            })
                                            .ok();
                                        })
                                        .detach();
                                        cx.notify();
                                    });
                                },
                            ),
                        )
                        .child(Button::new(format!("mcp-delete-{id}")).label("删除").on_click(
                            move |_, _, cx| {
                                remover.update(cx, |view, cx| {
                                    let previous = view.catalog.clone();
                                    view.catalog.mcp_servers.retain(|s| s.id != id);
                                    view.persist_catalog(previous, cx);
                                });
                            },
                        )),
                );
            }
            div()
                .v_flex()
                .gap_2()
                .child("技能与 MCP 定义（保存定义后，在下方选择作用域授权）")
                .child(Self::labeled_field("MCP 标识", "catalog-id", None, &self.catalog_id))
                .child(Self::labeled_field(
                    "技能文件/目录路径，或 MCP 启动程序",
                    "catalog-source",
                    None,
                    &self.catalog_source,
                ))
                .child(Self::labeled_field(
                    "MCP 参数（JSON 数组）",
                    "catalog-args",
                    None,
                    &self.catalog_args,
                ))
                .child(Self::labeled_field(
                    "MCP 环境变量名（逗号分隔）",
                    "catalog-env",
                    None,
                    &self.catalog_env,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Button::new("import-skill").label("导入技能").on_click(
                            move |_, _, cx| {
                                skill_importer.update(cx, |view, cx| {
                                    let previous = view.catalog.clone();
                                    let path = PathBuf::from(
                                        view.catalog_source.read(cx).value().to_string(),
                                    );
                                    match view.catalog.import_skill(&path) {
                                        Ok(_) => view.persist_catalog(previous, cx),
                                        Err(error) => {
                                            view.status = format!("导入失败：{error}");
                                            cx.notify();
                                        }
                                    }
                                });
                            },
                        ))
                        .child(Button::new("save-mcp").label("新增/保存 MCP").on_click(
                            move |_, _, cx| {
                                mcp_saver.update(cx, |view, cx| {
                                    let Ok(args) = serde_json::from_str::<Vec<String>>(
                                        &view.catalog_args.read(cx).value(),
                                    ) else {
                                        view.status = "参数必须是字符串 JSON 数组".into();
                                        cx.notify();
                                        return;
                                    };
                                    let previous = view.catalog.clone();
                                    let id = view.catalog_id.read(cx).value().to_string();
                                    view.catalog.mcp_servers.retain(|s| s.id != id);
                                    view.catalog.mcp_servers.push(
                                        circuitfabric_codex_runtime::tools::McpServerDefinition {
                                            id,
                                            command: view
                                                .catalog_source
                                                .read(cx)
                                                .value()
                                                .to_string(),
                                            args,
                                            environment_variables: Self::parse_tool_ids(
                                                &view.catalog_env.read(cx).value(),
                                            ),
                                            enabled: true,
                                        },
                                    );
                                    view.persist_catalog(previous, cx);
                                });
                            },
                        )),
                )
                .child(rows)
        }

        fn persist_catalog(
            &mut self,
            previous: circuitfabric_codex_runtime::tools::ToolCatalog,
            cx: &mut Context<Self>,
        ) {
            match self.runtime_settings_from_form(cx).save(&self.settings_path) {
                Ok(()) => {
                    if let Some(cancel) = &self.task_cancel {
                        cancel.cancel();
                    }
                    self.status = "已保存定义；当前任务已请求取消，下次任务使用新配置".into();
                }
                Err(error) => {
                    self.catalog = previous;
                    self.status = format!("未保存：{error}");
                }
            }
            cx.notify();
        }

        fn render_skills_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;

            let scope_note = match self.new_tool_scope {
                ToolScope::Global => format!(
                    "{} {}",
                    language.choose(
                        "全局授权会立即写入",
                        "Global authorizations are written immediately to"
                    ),
                    self.settings_path.display()
                ),
                ToolScope::Project => {
                    match self
                        .selected_project
                        .as_deref()
                        .and_then(|project_id| self.project_storages.get(project_id))
                    {
                        Some(storage) => format!(
                            "{} {}",
                            language.choose(
                                "项目授权会立即写入",
                                "Project authorizations are written immediately to",
                            ),
                            storage.configuration_path().display()
                        ),
                        None => language
                            .choose(
                                "请先在「项目」页选择并打开一个项目。",
                                "Select and open a project on the Projects page first.",
                            )
                            .to_owned(),
                    }
                }
            };

            let kind_skill = entity.clone();
            let kind_mcp = entity.clone();
            let scope_global = entity.clone();
            let scope_project = entity.clone();
            let authorizer = entity.clone();

            let mut global_rows = div().v_flex().gap_2();
            let mut global_count = 0_usize;
            for kind in [ToolAuthorizationKind::Skill, ToolAuthorizationKind::McpServer] {
                for id in self.tool_authorizations.ids_for_kind(kind).to_vec() {
                    global_rows = global_rows.child(Self::tool_authorization_row(
                        ToolScope::Global,
                        kind,
                        &id,
                        language,
                        &entity,
                    ));
                    global_count += 1;
                }
            }
            if global_count == 0 {
                global_rows = global_rows.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .text_sm()
                        .text_color(rgb(TEXT_MUTED))
                        .child(language.choose(
                            "尚无全局授权——在上方添加第一条技能或 MCP 服务器。",
                            "No global authorizations yet — add the first skill or MCP server above.",
                        )),
                );
            }

            let project_section = if let Some(project_id) = self.selected_project.clone() {
                let configuration =
                    self.workspace.configuration(&project_id).cloned().unwrap_or_default();
                let mut rows = div().v_flex().gap_2();
                let mut count = 0_usize;
                for kind in [ToolAuthorizationKind::Skill, ToolAuthorizationKind::McpServer] {
                    let ids = match kind {
                        ToolAuthorizationKind::Skill => configuration.enabled_skill_ids.clone(),
                        ToolAuthorizationKind::McpServer => {
                            configuration.enabled_mcp_server_ids.clone()
                        }
                    };
                    for id in ids {
                        rows = rows.child(Self::tool_authorization_row(
                            ToolScope::Project,
                            kind,
                            &id,
                            language,
                            &entity,
                        ));
                        count += 1;
                    }
                }
                if count == 0 {
                    rows = rows.child(
                        div()
                            .p_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(SURFACE_BG))
                            .text_sm()
                            .text_color(rgb(TEXT_MUTED))
                            .child(language.choose(
                                "此项目尚未授权任何技能或 MCP 服务器。",
                                "This project has no authorized skills or MCP servers yet.",
                            )),
                    );
                }
                div()
                    .v_flex()
                    .gap_2()
                    .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                        language.choose_owned(
                            format!("项目作用域 · {project_id}"),
                            format!("Project scope · {project_id}"),
                        ),
                    ))
                    .child(rows)
                    .into_any_element()
            } else {
                div()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG))
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose(
                        "在「项目」页选择一个项目后，可在这里管理它的项目级授权。",
                        "Select a project on the Projects page to manage its project-scoped authorizations here.",
                    ))
                    .into_any_element()
            };

            div()
                .flex_1()
                .min_w(px(0.))
                .v_flex()
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("技能与 MCP 授权", "Skills & MCP")),
                        )
                        .child(
                            div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                language.choose(
                                    "全局授权与当前项目授权取并集；从一个作用域撤销，不会删除另一作用域的授权。停用定义对所有作用域生效。",
                                    "Runtimes may only load authorized skills and MCP servers: the global scope applies to every project, the project scope writes to that project's own configuration file.",
                                ),
                            ),
                        ),
                )
                .child(self.render_catalog(cx))
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(220.))
                                        .h(px(36.))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(BORDER))
                                        .bg(rgb(CARD_BG))
                                        .child(
                                            InputBase::new("new-tool-id")
                                                .flex_1()
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .child(self.new_tool_id.clone()),
                                        ),
                                )
                                .child(
                                    Button::new("tool-kind-skill")
                                        .label(language.choose("技能", "Skill"))
                                        .when(
                                            self.new_tool_kind == ToolAuthorizationKind::Skill,
                                            ButtonVariants::primary,
                                        )
                                        .on_click(move |_, _, cx| {
                                            kind_skill.update(cx, |view, cx| {
                                                view.new_tool_kind =
                                                    ToolAuthorizationKind::Skill;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("tool-kind-mcp")
                                        .label(language.choose("MCP 服务器", "MCP server"))
                                        .when(
                                            self.new_tool_kind == ToolAuthorizationKind::McpServer,
                                            ButtonVariants::primary,
                                        )
                                        .on_click(move |_, _, cx| {
                                            kind_mcp.update(cx, |view, cx| {
                                                view.new_tool_kind =
                                                    ToolAuthorizationKind::McpServer;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    div()
                                        .w(px(1.))
                                        .h(px(24.))
                                        .flex_none()
                                        .bg(rgb(BORDER)),
                                )
                                .child(
                                    Button::new("tool-scope-global")
                                        .label(language.choose("全局", "Global"))
                                        .when(self.new_tool_scope == ToolScope::Global, |button| {
                                            button.primary()
                                        })
                                        .on_click(move |_, _, cx| {
                                            scope_global.update(cx, |view, cx| {
                                                view.new_tool_scope = ToolScope::Global;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("tool-scope-project")
                                        .label(language.choose("当前项目", "This project"))
                                        .when(self.new_tool_scope == ToolScope::Project, |button| {
                                            button.primary()
                                        })
                                        .on_click(move |_, _, cx| {
                                            scope_project.update(cx, |view, cx| {
                                                view.new_tool_scope = ToolScope::Project;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("authorize-tool")
                                        .primary()
                                        .label(language.choose("授权", "Authorize"))
                                        .on_click(move |_, window, cx| {
                                            authorizer.update(cx, |view, cx| {
                                                view.authorize_tool(window, cx);
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(language.choose(
                                    "可以一次粘贴多个 ID（逗号或空格分隔）：每一项都会成为清单中的一行，可单独撤销。",
                                    "Paste several IDs at once (comma- or space-separated): each becomes its own list row with an individual revoke.",
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(scope_note),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose(
                                    "全局作用域（所有项目）",
                                    "Global scope (all projects)",
                                )),
                        )
                        .child(global_rows),
                )
                .child(project_section)
        }
    }

    impl ControlPlaneView {
        fn render_project_form(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let (id_feedback, feedback_color) = self.project_id_feedback(cx);
            let field = |label: &'static str, id: &'static str, state: Entity<InputState>| {
                div()
                    .v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                    .child(
                        div()
                            .h(px(36.))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(CARD_BG))
                            .child(
                                InputBase::new(id)
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .child(state),
                            ),
                    )
                    .into_any_element()
            };
            let creator = entity.clone();
            let closer = entity.clone();
            let root_chooser = entity.clone();
            let selected_root = self.new_project_root.read(cx).value().trim().to_owned();
            let creation_preview = if selected_root.is_empty() {
                language
                    .choose(
                        "请选择一个已有文件夹；在确认创建前，不会写入任何文件。",
                        "Choose an existing folder; no files are written until confirmation.",
                    )
                    .to_owned()
            } else {
                format!(
                    "{}\n  .circuitfabric/、sessions/、documents/、logic/、schematics/",
                    language.choose("将在以下根目录创建：", "Will create under:"),
                ) + &format!("\n  {selected_root}")
            };

            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("project-form-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x000f_172a_b3))
                        .occlude()
                        .on_click(move |_, _, cx| {
                            closer.update(cx, |view, cx| {
                                view.project_form_open = false;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .relative()
                        .occlude()
                        .w(px(560.))
                        .v_flex()
                        .gap_4()
                        .p_5()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(ACCENT_SOFT))
                        .bg(rgb(SURFACE_BG))
                        .shadow_lg()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_PRIMARY))
                                        .child(language.choose("新建项目", "New project")),
                                )
                                .child(
                                    Button::new("close-project-form")
                                        .ghost()
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, _, cx| {
                                            entity.update(cx, |view, cx| {
                                                view.project_form_open = false;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(field("Project ID", "new-project-id", self.new_project_id.clone()))
                        .child(div().text_xs().text_color(rgb(feedback_color)).child(id_feedback))
                        .child(field("Name", "new-project-name", self.new_project_name.clone()))
                        .child(field(
                            "Description",
                            "new-project-description",
                            self.new_project_description.clone(),
                        ))
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div().text_sm().font_weight(FontWeight::MEDIUM).child(
                                        language.choose("项目根文件夹", "Project root folder"),
                                    ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            div()
                                                .h(px(36.))
                                                .px_2()
                                                .flex_1()
                                                .flex()
                                                .items_center()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(rgb(BORDER))
                                                .bg(rgb(CARD_BG))
                                                .child(
                                                    InputBase::new("new-project-root")
                                                        .flex_1()
                                                        .h_full()
                                                        .flex()
                                                        .items_center()
                                                        .child(self.new_project_root.clone()),
                                                ),
                                        )
                                        .child(
                                            Button::new("choose-project-root")
                                                .label(
                                                    language.choose("选择文件夹", "Choose folder"),
                                                )
                                                .on_click(move |_, window, cx| {
                                                    root_chooser.update(cx, |view, cx| {
                                                        view.choose_project_root(window, cx);
                                                    });
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .whitespace_normal()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(creation_preview),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                    language.choose(
                                        "项目级设置不会修改全局运行时设置。",
                                        "Project settings never modify global runtime settings.",
                                    ),
                                ))
                                .child(
                                    Button::new("create-project")
                                        .primary()
                                        .label(language.choose("确认并创建", "Confirm and create"))
                                        .on_click(move |_, window, cx| {
                                            creator.update(cx, |view, cx| {
                                                view.create_project(window, cx);
                                            });
                                        }),
                                ),
                        ),
                )
        }

        #[allow(clippy::too_many_lines)]
        fn render_projects_page(
            &mut self,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let project_form =
                self.project_form_open.then(|| self.render_project_form(cx).into_any_element());
            let query = self.project_search.read(cx).value().trim().to_lowercase();
            let projects = self
                .workspace
                .projects()
                .into_iter()
                .filter(|project| self.project_matches(project, &query))
                .cloned()
                .collect::<Vec<_>>();
            let selected_project =
                self.selected_project.as_deref().and_then(|id| self.workspace.project(id)).cloned();
            let mut cards = div().v_flex().gap_2();
            if projects.is_empty() {
                cards = cards.child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_5()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("没有匹配的项目", "No matching projects")),
                        )
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                            language.choose(
                                "清除搜索或筛选，或创建第一个项目。",
                                "Clear the search or filter, or create the first project.",
                            ),
                        )),
                );
            }
            for project in projects {
                let project_id = project.id.clone();
                let is_selected = self.selected_project.as_deref() == Some(project.id.as_str());
                let selector = entity.clone();
                let description = project.description.unwrap_or_else(|| {
                    language.choose("尚未添加项目描述", "No project description yet").to_owned()
                });
                let root = self.project_registry.root_for(&project.id).map_or_else(
                    || language.choose("根目录未注册", "Root not registered").to_owned(),
                    |path| path.display().to_string(),
                );
                let (document_count, session_count, last_activity) =
                    self.project_data.get(&project.id).map_or((0, 0, None), |data| {
                        let last_activity = data
                            .session_listing
                            .sessions
                            .first()
                            .map(|summary| rfc3339(summary.metadata.started_at_unix_seconds));
                        (data.documents.len(), data.session_listing.sessions.len(), last_activity)
                    });
                let document_label = language.choose_owned(
                    format!("{document_count} 份文档"),
                    format!("{document_count} documents"),
                );
                let session_label = language.choose_owned(
                    format!("{session_count} 个会话"),
                    format!("{session_count} sessions"),
                );
                let activity_label = last_activity
                    .unwrap_or_else(|| language.choose("尚无活动", "No activity yet").to_owned());
                cards = cards.child(
                    div()
                        .id(format!("project-card-{}", project.id))
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if is_selected { ACCENT } else { BORDER }))
                        .bg(rgb(if is_selected { 0x00f0_f9ff } else { CARD_BG }))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.select_project(project_id.clone(), cx);
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_PRIMARY))
                                        .child(project.name),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(SURFACE_BG))
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(project.id),
                                ),
                        )
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(description))
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(root))
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(document_label)
                                .child(session_label)
                                .child(activity_label),
                        ),
                );
            }

            let detail = if let Some(project) = selected_project {
                self.render_project_detail(project, cx).into_any_element()
            } else {
                let opener = entity.clone();
                div()
                    .flex_1()
                    .v_flex()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .p_8()
                    .bg(rgb(CARD_BG))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("选择一个项目", "Select a project")),
                    )
                    .child(
                        div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                            "项目详情、文档、会话和项目级配置会显示在这里。",
                            "Project details, documents, sessions, and project-scoped configuration appear here.",
                        )),
                    )
                    .child(
                        Button::new("open-project-form-empty")
                            .primary()
                            .label(language.choose("新建项目", "New project"))
                            .on_click(move |_, _, cx| {
                                opener.update(cx, ControlPlaneView::open_project_form);
                            }),
                    )
                    .into_any_element()
            };

            let open_form = entity.clone();
            let open_existing = entity.clone();
            let all_filter = entity.clone();
            let setup_filter = entity.clone();
            div()
                .size_full()
                .min_w(px(880.))
                .relative()
                .v_flex()
                .gap_4()
                .p_6()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(language.choose("项目工作区", "Project workspaces")),
                                )
                                .child(
                                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                        language.choose(
                                            "将项目资料、会话和授权配置限定在同一个设计工作区。",
                                            "Keep design evidence, sessions, and authorized configuration in one workspace.",
                                        ),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("open-existing-project")
                                        .label(language.choose("打开已有项目", "Open existing"))
                                        .on_click(move |_, window, cx| {
                                            open_existing.update(cx, |view, cx| {
                                                view.open_existing_project(window, cx);
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("open-project-form")
                                        .primary()
                                        .label(language.choose("新建项目", "New project"))
                                        .on_click(move |_, _, cx| {
                                            open_form.update(cx, ControlPlaneView::open_project_form);
                                        }),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .flex()
                        .gap_4()
                        .child(
                            div()
                                .w(px(330.))
                                .flex_none()
                                .v_flex()
                                .gap_3()
                                .child(
                                    div()
                                        .id("project-search")
                                        .w_full()
                                        .child(Input::new(&self.project_search)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            Button::new("project-filter-all")
                                                .label(language.choose("全部", "All"))
                                                .when(self.project_filter == ProjectFilter::All, |button| {
                                                    button.primary()
                                                })
                                                .on_click(move |_, _, cx| {
                                                    all_filter.update(cx, |view, cx| {
                                                        view.project_filter = ProjectFilter::All;
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            Button::new("project-filter-needs-configuration")
                                                .label(language.choose("待配置", "Needs setup"))
                                                .when(
                                                    self.project_filter == ProjectFilter::NeedsConfiguration,
                                                    |button| button.primary(),
                                                )
                                                .on_click(move |_, _, cx| {
                                                    setup_filter.update(cx, |view, cx| {
                                                        view.project_filter = ProjectFilter::NeedsConfiguration;
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                )
                                .child(cards),
                        )
                        .child(detail),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
                .when_some(project_form, ParentElement::child)
        }

        #[allow(clippy::too_many_lines)]
        fn render_project_detail(
            &mut self,
            project: Project,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let selected_tab = self.project_tab;
            let configuration = self.workspace.configuration(&project.id).cloned();
            let project_root = self.project_registry.root_for(&project.id).map_or_else(
                || language.choose("根目录未注册", "Root not registered").to_owned(),
                |path| path.display().to_string(),
            );
            let (document_count, session_count) = self
                .project_data
                .get(&project.id)
                .map_or((0, 0), |data| (data.documents.len(), data.session_listing.sessions.len()));
            let session_tokens = self
                .project_data
                .get(&project.id)
                .map(|data| {
                    data.session_listing.sessions.iter().fold(0_u64, |total, summary| {
                        total
                            + summary.metadata.usage.input_tokens
                            + summary.metadata.usage.output_tokens
                    })
                })
                .unwrap_or(0);
            let mut tabs = div().flex().gap_1().flex_wrap();
            for tab in ProjectDetailTab::ALL {
                let chooser = entity.clone();
                let active = tab == selected_tab;
                tabs = tabs.child(
                    Button::new(format!("project-tab-{}", tab.label(UiLanguage::English)))
                        .label(tab.label(language))
                        .when(active, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            chooser.update(cx, |view, cx| {
                                view.project_tab = tab;
                                cx.notify();
                            });
                        }),
                );
            }

            let content = match selected_tab {
                ProjectDetailTab::Overview => div()
                    .v_flex()
                    .gap_3()
                    .child(
                        div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                            project.description.clone().unwrap_or_else(|| {
                                language
                                    .choose("尚未添加描述。", "No description has been added.")
                                    .to_owned()
                            }),
                        ),
                    )
                    .child(
                        div()
                            .p_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(SURFACE_BG))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(language.choose("项目根目录", "Project root")),
                            )
                            .child(div().text_sm().text_color(rgb(TEXT_PRIMARY)).child(project_root)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(Self::project_metric(
                                document_count.to_string(),
                                language.choose("授权文档", "Authorized documents"),
                            ))
                            .child(Self::project_metric(
                                session_count.to_string(),
                                language.choose("会话记录", "Session records"),
                            ))
                            .child(Self::project_metric(
                                session_tokens.to_string(),
                                language.choose("累计 tokens", "Total tokens"),
                            )),
                    )
                    .into_any_element(),
                ProjectDetailTab::Documents => {
                    self.render_documents_tab(&project, cx).into_any_element()
                }
                ProjectDetailTab::Sessions => {
                    self.render_sessions_tab(&project, cx).into_any_element()
                }
                ProjectDetailTab::AgentConfiguration => {
                    let scope_summary = if let Some(configuration) = configuration {
                        format!(
                            "{} skills · {} MCP servers",
                            configuration.enabled_skill_ids.len(),
                            configuration.enabled_mcp_server_ids.len()
                        )
                    } else {
                        language
                            .choose("尚无项目级覆盖项", "No project-scoped overrides")
                            .to_owned()
                    };
                    div()
                        .v_flex()
                        .gap_3()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("项目级配置", "Project-scoped configuration")),
                        )
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(scope_summary))
                        .child(
                            div()
                                .p_3()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .bg(rgb(SURFACE_BG))
                                .text_sm()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(language.choose(
                                    "技能许可、MCP 许可和项目说明会保存在此项目作用域内。Codex 命令、Provider 和 bridge 地址是全局运行时设置，只能在“智能体与工具”中修改，不会被此项目覆盖。",
                                    "Skill permissions, MCP permissions, and project instructions belong to this project. The Codex command, providers, and bridge address are global runtime settings; they can only be changed in Agents & tools and are never overridden here.",
                                )),
                        )
                        .into_any_element()
                }
                ProjectDetailTab::Usage => Self::project_empty_state(
                    language.choose("尚无用量记录", "No usage recorded"),
                    language.choose(
                        "用量会按项目、Provider 和运行时聚合，且不会混入其他项目。",
                        "Usage will be grouped by project, provider, and runtime without mixing other projects.",
                    ),
                )
                .into_any_element(),
            };

            div()
                .flex_1()
                .min_w(px(0.))
                .v_flex()
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(project.name),
                                )
                                .child(
                                    div().text_sm().text_color(rgb(TEXT_MUTED)).child(project.id),
                                ),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .bg(rgb(0x00dc_fce7))
                                .text_xs()
                                .text_color(rgb(0x0016_a34a))
                                .child(language.choose("项目作用域", "Project scope")),
                        ),
                )
                .child(tabs)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .p_4()
                        .rounded_lg()
                        .bg(rgb(SURFACE_BG))
                        .child(content),
                )
        }

        fn project_metric(value: String, label: &'static str) -> impl IntoElement {
            div()
                .flex_1()
                .v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
        }

        #[allow(clippy::too_many_lines)]
        fn render_documents_tab(
            &mut self,
            project: &Project,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let documents = self
                .project_data
                .get(&project.id)
                .map(|data| data.documents.clone())
                .unwrap_or_default();

            let header = div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("授权文档", "Authorized documents")),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child({
                            let importer = entity.clone();
                            Button::new("import-datasheet")
                                .label(language.choose("导入 Datasheet", "Import datasheet"))
                                .on_click(move |_, window, cx| {
                                    importer.update(cx, |view, cx| {
                                        view.import_project_document(
                                            DocumentCategory::Datasheet,
                                            window,
                                            cx,
                                        );
                                    });
                                })
                        })
                        .child({
                            let importer = entity.clone();
                            Button::new("import-reference-design")
                                .primary()
                                .label(language.choose("导入参考设计", "Import reference design"))
                                .on_click(move |_, window, cx| {
                                    importer.update(cx, |view, cx| {
                                        view.import_project_document(
                                            DocumentCategory::ReferenceDesign,
                                            window,
                                            cx,
                                        );
                                    });
                                })
                        }),
                );

            if documents.is_empty() {
                return div()
                    .v_flex()
                    .gap_4()
                    .size_full()
                    .child(header)
                    .child(
                        Self::project_empty_state(
                            language.choose("还没有授权文档", "No authorized documents yet"),
                            language.choose(
                                "导入第一份 datasheet 或参考设计后，会记录内容哈希与来源，并可被证据检索引用。",
                                "Import the first datasheet or reference design; its content hash and source are recorded and citable.",
                            ),
                        ),
                    )
                    .into_any_element();
            }

            let mut list = div().v_flex().gap_2();
            for document in documents {
                let searchable = is_text_extractable(&document.document_kind);
                let category_style = match document.category {
                    DocumentCategory::Datasheet => (0x00e0_f2fe, 0x000e_7490),
                    DocumentCategory::ReferenceDesign => (0x00f3_e8ff, 0x0076_2b_a3),
                };
                list = list.child(
                    div()
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(document.original_file_name.clone()),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(category_style.0))
                                        .text_color(rgb(category_style.1))
                                        .child(document.category.label()),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(if searchable { 0x00dc_fce7 } else { SURFACE_BG }))
                                        .text_color(rgb(if searchable {
                                            0x0016_a34a
                                        } else {
                                            TEXT_MUTED
                                        }))
                                        .child(if searchable {
                                            language.choose("文本可检索", "Text searchable")
                                        } else {
                                            language.choose("仅哈希存档", "Hash-only archive")
                                        }),
                                )
                                .child(
                                    div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        format!(
                                            "{} · {} bytes",
                                            &document.content_hash[..19],
                                            document.byte_size
                                        ),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(format!("{} · {}", document.id, document.source_locator)),
                        ),
                );
            }

            let query = self.evidence_query.read(cx).value().trim().to_owned();
            let evidence = if query.is_empty() {
                None
            } else {
                self.workspace.retrieve_document_evidence(&project.id, &query).ok()
            };
            let mut search_panel = div()
                .v_flex()
                .gap_2()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE_BG))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(language.choose("项目内证据检索", "Project-scoped evidence search")),
                )
                .child(div().id("evidence-query").w_full().child(Input::new(&self.evidence_query)));
            match evidence {
                Some(package) if !package.fragments.is_empty() => {
                    search_panel = search_panel.child(
                        div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose_owned(
                            format!("命中 {} 个可引用片段：", package.fragments.len()),
                            format!("{} citation-ready fragments:", package.fragments.len()),
                        )),
                    );
                    for fragment in package.fragments.iter().take(12) {
                        search_panel = search_panel.child(
                            div()
                                .v_flex()
                                .gap_0p5()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(CARD_BG))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x000e_7490))
                                        .child(fragment.locator.clone()),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(TEXT_PRIMARY))
                                        .child(fragment.text.clone()),
                                ),
                        );
                    }
                }
                Some(_) => {
                    search_panel = search_panel.child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(language.choose(
                                "没有命中片段；换一个关键词，或导入更多文本类文档。",
                                "No matching fragments; try another keyword or import more text documents.",
                            )),
                    );
                }
                None => {}
            }

            div()
                .v_flex()
                .gap_4()
                .size_full()
                .child(header)
                .child(list)
                .child(search_panel)
                .into_any_element()
        }

        #[allow(clippy::too_many_lines)]
        fn render_sessions_tab(
            &mut self,
            project: &Project,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;

            if let Some(selection) = &self.session_replay
                && selection.project_id == project.id
            {
                let replay = &selection.replay;
                let metadata = &replay.metadata;
                let closer = entity.clone();
                let mut body = div().v_flex().gap_0p5();
                for line in replay.body.lines() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_PRIMARY))
                            .whitespace_normal()
                            .child(line.to_owned()),
                    );
                }
                return div()
                    .v_flex()
                    .gap_3()
                    .size_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(metadata.session_id.clone()),
                                    )
                                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        format!(
                                                "{} · {} · {} → {}",
                                                metadata
                                                    .backend_id
                                                    .clone()
                                                    .unwrap_or_else(|| "-".to_owned()),
                                                rfc3339(metadata.started_at_unix_seconds),
                                                metadata.status.as_str(),
                                                metadata
                                                    .completed_at_unix_seconds
                                                    .map(rfc3339)
                                                    .unwrap_or_else(|| "—".to_owned()),
                                            ),
                                    )),
                            )
                            .child(
                                Button::new("close-session-replay")
                                    .ghost()
                                    .label(language.choose("返回列表", "Back to list"))
                                    .on_click(move |_, _, cx| {
                                        closer.update(cx, ControlPlaneView::close_session_replay);
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(format!(
                                "{} input / {} output tokens",
                                metadata.usage.input_tokens, metadata.usage.output_tokens
                            ))
                            .when(!metadata.citations.is_empty(), |this| {
                                this.child(format!(" · 引用 {}", metadata.citations.join("、")))
                            }),
                    )
                    .child(
                        div()
                            .id("session-replay-body")
                            .v_flex()
                            .gap_0p5()
                            .p_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(CARD_BG))
                            .max_h(px(480.))
                            .overflow_y_scroll()
                            .child(body),
                    )
                    .into_any_element();
            }

            let listing = self
                .project_data
                .get(&project.id)
                .map(|data| data.session_listing.clone())
                .unwrap_or_default();

            if listing.sessions.is_empty() {
                return div()
                    .v_flex()
                    .gap_2()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .text_center()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("还没有会话", "No sessions yet")),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                        language.choose(
                            "会话由受支持的 EDA bridge 启动后，会以 Markdown 审计记录的形式出现在这里。",
                            "Sessions started from a supported EDA bridge appear here as Markdown audit records.",
                        ),
                    ))
                    .when(!listing.orphaned_temp_files.is_empty(), |this| {
                        this.child(div().text_xs().text_color(rgb(0x00b4_5309)).child(
                            language.choose_owned(
                                format!(
                                    "检测到 {} 个中断写入的临时文件，可在确认后手动删除。",
                                    listing.orphaned_temp_files.len()
                                ),
                                format!(
                                    "{} interrupted-write temporary files detected; review and remove them manually.",
                                    listing.orphaned_temp_files.len()
                                ),
                            ),
                        ))
                    })
                    .into_any_element();
            }

            let mut rows = div().v_flex().gap_2();
            for summary in &listing.sessions {
                let metadata = &summary.metadata;
                let session_id = metadata.session_id.clone();
                let opener = entity.clone();
                let (status_bg, status_fg) = match metadata.status.as_str() {
                    "completed" => (0x00dc_fce7, 0x0016_a34a),
                    "failed" => (0x00fe_e2e2, 0x00b4_2323),
                    _ => (0x00fe_f3c7, 0x00b4_5309),
                };
                rows = rows.child(
                    div()
                        .id(format!("session-row-{}", metadata.session_id))
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                        .on_click(move |_, _, cx| {
                            opener.update(cx, |view, cx| {
                                view.open_session_replay(session_id.clone(), cx);
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(metadata.session_id.clone()),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(status_bg))
                                        .text_color(rgb(status_fg))
                                        .child(metadata.status.as_str()),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(SURFACE_BG))
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(
                                            metadata
                                                .backend_id
                                                .clone()
                                                .unwrap_or_else(|| "-".to_owned()),
                                        ),
                                )
                                .child(
                                    div()
                                        .ml_auto()
                                        .text_xs()
                                        .text_color(rgb(TEXT_MUTED))
                                        .child(rfc3339(metadata.started_at_unix_seconds)),
                                ),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                            "{} in / {} out tokens · {}",
                            metadata.usage.input_tokens,
                            metadata.usage.output_tokens,
                            summary.file_name
                        ))),
                );
            }
            if !listing.orphaned_temp_files.is_empty() {
                rows = rows.child(
                    div().text_xs().text_color(rgb(0x00b4_5309)).child(language.choose_owned(
                        format!(
                            "检测到 {} 个中断写入的临时文件，可在确认后手动删除。",
                            listing.orphaned_temp_files.len()
                        ),
                        format!(
                            "{} interrupted-write temporary files detected; review and remove them manually.",
                            listing.orphaned_temp_files.len()
                        ),
                    )),
                );
            }

            div().v_flex().gap_3().size_full().child(rows).into_any_element()
        }

        fn project_empty_state(title: &'static str, description: &'static str) -> impl IntoElement {
            div()
                .v_flex()
                .gap_2()
                .items_center()
                .justify_center()
                .h_full()
                .text_center()
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(description))
        }

        fn section_page(language: UiLanguage, screen: ControlPlaneScreen) -> impl IntoElement {
            let (title, description, next_step) = language.page_copy(screen);
            div().size_full().min_w(px(720.)).v_flex().justify_center().items_center().p_8().child(
                div()
                    .w(px(680.))
                    .v_flex()
                    .gap_4()
                    .p_6()
                    .bg(rgb(CARD_BG))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .shadow_sm()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(4.)).h(px(22.)).rounded_full().bg(rgb(ACCENT)))
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(title),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(description))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2p5()
                            .p_3()
                            .bg(rgb(SURFACE_BG))
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x000e_7490))
                                    .bg(rgb(0x00e0_f2fe))
                                    .child("TODO"),
                            )
                            .child(
                                div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(next_step),
                            ),
                    ),
            )
        }

        #[allow(clippy::too_many_lines)]
        fn overview_page(language: UiLanguage) -> impl IntoElement {
            let metric = |value: &'static str, label: &'static str, color: u32| {
                div()
                    .flex_1()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .bg(rgb(CARD_BG))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .shadow_xs()
                    .child(
                        div().flex().items_center().gap_2().child(status_dot(color)).child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(value),
                        ),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
            };

            let panel = |title: &'static str, body: &'static str| {
                div()
                    .flex_1()
                    .v_flex()
                    .gap_3()
                    .p_5()
                    .bg(rgb(CARD_BG))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .shadow_xs()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_PRIMARY))
                            .child(title),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(body))
            };

            div()
                .size_full()
                .min_w(px(760.))
                .v_flex()
                .gap_5()
                .p_6()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(language.choose("总览", "Overview")),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(language.choose(
                                    "以证据为先、安全管理设计工作区。",
                                    "A safe, evidence-first view of your design workspace.",
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(metric("0", language.choose("项目", "Projects"), 0x003b_82f6))
                        .child(metric(
                            "0",
                            language.choose("已授权文档", "Authorized documents"),
                            0x008b_5cf6,
                        ))
                        .child(metric("0", language.choose("在线 bridge", "Online bridges"), 0x0022_c55e))
                        .child(metric("0", language.choose("待审批", "Pending approvals"), 0x00f5_9e0b)),
                )
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .child(panel(
                            language.choose("运行时健康度", "Runtime health"),
                            language.choose(
                                "尚未连接——请在「智能体与工具」中配置运行时和 EDA bridge。",
                                "Not connected — configure a runtime and EDA bridge in Agents & tools.",
                            ),
                        ))
                        .child(panel(
                            language.choose("最近会话", "Recent sessions"),
                            language.choose(
                                "还没有会话历史。会话回放将在此显示。",
                                "No session history yet. Session replay will appear here.",
                            ),
                        ))
                        .child(panel(
                            language.choose("需要关注", "Attention needed"),
                            language.choose(
                                "尚未记录验证结果。",
                                "No verification result has been recorded.",
                            ),
                        )),
                )
        }
    }

    impl ControlPlaneView {
        fn command_matches(&self, cx: &Context<Self>) -> Vec<ControlPlaneScreen> {
            let needle = self.command_search.read(cx).value().to_lowercase();
            ControlPlaneScreen::ALL
                .into_iter()
                .filter(|screen| {
                    needle.is_empty()
                        || self.language.screen_label(*screen).to_lowercase().contains(&needle)
                })
                .collect()
        }

        fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            self.command_palette_open = true;
            self.command_selected = 0;
            self.command_search.update(cx, |state, cx| {
                state.set_value("", window, cx);
                state.focus(window, cx);
            });
            cx.notify();
        }

        fn close_command_palette(&mut self, cx: &mut Context<Self>) {
            if self.command_palette_open {
                self.command_palette_open = false;
                cx.notify();
            }
        }

        fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.command_palette_open {
                self.close_command_palette(cx);
            } else {
                self.open_command_palette(window, cx);
            }
        }

        fn command_activate(&mut self, screen: ControlPlaneScreen, cx: &mut Context<Self>) {
            self.screen = screen;
            self.close_command_palette(cx);
        }

        fn handle_command_keystroke(
            &mut self,
            event: &KeystrokeEvent,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let keystroke = &event.keystroke;

            if keystroke.modifiers.secondary() && keystroke.key.eq_ignore_ascii_case("k") {
                self.toggle_command_palette(window, cx);
                return;
            }

            if !self.command_palette_open {
                return;
            }

            match keystroke.key.as_str() {
                "escape" => self.close_command_palette(cx),
                "enter" => {
                    let screen = self.command_matches(cx).get(self.command_selected).copied();
                    if let Some(screen) = screen {
                        self.command_activate(screen, cx);
                    }
                }
                "up" => {
                    let count = self.command_matches(cx).len();
                    if count > 0 {
                        self.command_selected = (self.command_selected + count - 1) % count;
                        cx.notify();
                    }
                }
                "down" => {
                    let count = self.command_matches(cx).len();
                    if count > 0 {
                        self.command_selected = (self.command_selected + 1) % count;
                        cx.notify();
                    }
                }
                _ => {}
            }
        }

        #[allow(clippy::too_many_lines)]
        fn render_command_palette(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let matches = self.command_matches(cx);
            let selected = self.command_selected.min(matches.len().saturating_sub(1));

            let mut list = div().v_flex().gap_0p5().p_2();
            for (index, screen) in matches.iter().copied().enumerate() {
                let is_selected = index == selected;
                let label = language.screen_label(screen);
                let group = language.group_label(screen);
                let navigator = entity.clone();
                list = list.child(
                    div()
                        .id(format!("command-item-{}", screen.label()))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .h(px(36.))
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_selected, |this| this.bg(rgb(ACCENT)).text_color(rgb(SIDEBAR_BG)))
                        .when(!is_selected, |this| this.text_color(rgb(SIDEBAR_TEXT)))
                        .when(!is_selected, |this| {
                            this.hover(|this| this.bg(rgb(SIDEBAR_ITEM_HOVER)))
                        })
                        .on_click(move |_, _, cx| {
                            navigator.update(cx, |view, cx| view.command_activate(screen, cx));
                        })
                        .child(
                            div()
                                .text_sm()
                                .font_weight(if is_selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .child(label),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(if is_selected {
                                    rgb(SIDEBAR_BG)
                                } else {
                                    rgb(SIDEBAR_GROUP)
                                })
                                .child(group),
                        ),
                );
            }

            let body = if matches.is_empty() {
                div()
                    .px_3()
                    .py_4()
                    .text_sm()
                    .text_color(rgb(SIDEBAR_TEXT))
                    .child(language.choose("无匹配模块", "No matching modules"))
            } else {
                list
            };

            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .pt_16()
                .child(
                    div()
                        .id("command-palette-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x0000_0080))
                        .occlude()
                        .on_click(move |_, _, cx| {
                            entity.update(cx, ControlPlaneView::close_command_palette);
                        }),
                )
                .child(
                    div()
                        .relative()
                        .occlude()
                        .w(px(560.))
                        .v_flex()
                        .overflow_hidden()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(SIDEBAR_DIVIDER))
                        .bg(rgb(SIDEBAR_BG))
                        .shadow_lg()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .h(px(44.))
                                .border_b_1()
                                .border_color(rgb(SIDEBAR_DIVIDER))
                                .child(
                                    InputBase::new("command-palette-search")
                                        .flex_1()
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .text_sm()
                                        .text_color(rgb(SIDEBAR_TEXT_ACTIVE))
                                        .child(self.command_search.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(SIDEBAR_GROUP))
                                        .child(language.choose("ESC 关闭", "ESC to close")),
                                ),
                        )
                        .child(body),
                )
        }
    }

    impl Render for ControlPlaneView {
        #[allow(clippy::too_many_lines)]
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.refresh_codex_lifecycle();
            let command_palette = if self.command_palette_open {
                Some(self.render_command_palette(cx).into_any_element())
            } else {
                None
            };
            let entity = cx.entity().clone();
            let active_screen = self.screen;
            let language = self.language;
            let selected_project_label = self
                .selected_project
                .as_deref()
                .and_then(|id| self.workspace.project(id))
                .map(|project| format!("{} · {}", project.name, project.id))
                .unwrap_or_else(|| language.choose("未选择项目", "No project selected").to_owned());
            let mut navigation = div().v_flex().gap_0p5();
            let mut current_group = "";

            for screen in ControlPlaneScreen::ALL {
                if screen.group() != current_group {
                    current_group = screen.group();
                    navigation = navigation.child(
                        div()
                            .pt_4()
                            .pb_1()
                            .px_3()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(SIDEBAR_GROUP))
                            .child(language.group_label(screen)),
                    );
                }

                let selector = entity.clone();
                let active = screen == active_screen;
                let label = language.screen_label(screen);
                let is_todo = screen.is_todo();
                navigation = navigation.child(
                    div()
                        .id(format!("nav-{}", screen.label()))
                        .w_full()
                        .h(px(36.))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(active, |this| this.bg(rgb(SIDEBAR_ITEM_ACTIVE)))
                        .hover(|this| this.bg(rgb(SIDEBAR_ITEM_HOVER)))
                        .active(|this| this.bg(rgb(SIDEBAR_ITEM_PRESSED)))
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.screen = screen;
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .w(px(3.))
                                .h(px(18.))
                                .rounded_full()
                                .flex_none()
                                .when(active, |this| this.bg(rgb(ACCENT))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_sm()
                                .font_weight(if active {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_color(if active {
                                    rgb(SIDEBAR_TEXT_ACTIVE)
                                } else {
                                    rgb(SIDEBAR_TEXT)
                                })
                                .child(label),
                        )
                        .when(is_todo, |this| {
                            this.child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if active {
                                        rgb(ACCENT_SOFT)
                                    } else {
                                        rgb(SIDEBAR_GROUP)
                                    })
                                    .child("TODO"),
                            )
                        }),
                );
            }

            let page = match active_screen {
                ControlPlaneScreen::Overview => Self::overview_page(language).into_any_element(),
                ControlPlaneScreen::Projects => {
                    self.render_projects_page(window, cx).into_any_element()
                }
                ControlPlaneScreen::AgentsAndMcp => {
                    self.render_agents_page(window, cx).into_any_element()
                }
                screen => Self::section_page(language, screen).into_any_element(),
            };

            div()
                .size_full()
                .flex()
                .bg(rgb(SURFACE_BG))
                .text_color(rgb(TEXT_PRIMARY))
                .child(
                    // Sidebar
                    div()
                        .w(px(248.))
                        .flex_none()
                        .h_full()
                        .v_flex()
                        .p_3()
                        .bg(rgb(SIDEBAR_BG))
                        .text_color(rgb(SIDEBAR_TEXT))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_1()
                                .pb_4()
                                .mb_1()
                                .border_b_1()
                                .border_color(rgb(SIDEBAR_DIVIDER))
                                // This compact mark intentionally omits the logo's outer
                                // frame: the tile itself supplies the only frame at this size.
                                .child(
                                    div()
                                        .size(px(40.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_md()
                                        .bg(rgb(SIDEBAR_ITEM_ACTIVE))
                                        .border_1()
                                        .border_color(rgb(ACCENT_SOFT))
                                        .child(img(self.sidebar_mark.clone()).size(px(32.))),
                                )
                                .child(
                                    div()
                                        .v_flex()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_base()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(SIDEBAR_TEXT_ACTIVE))
                                                .child("CircuitFabric"),
                                        )
                                        .child(
                                            div().text_xs().text_color(rgb(SIDEBAR_GROUP)).child(
                                                language.choose(
                                                    "电路设计控制面",
                                                    "Circuit control plane",
                                                ),
                                            ),
                                        ),
                                ),
                        )
                        .child(navigation)
                        .child(div().flex_1())
                        .child(
                            div()
                                .px_1()
                                .pt_3()
                                .v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(status_dot(self.codex_status.dot()))
                                        .child(
                                            div().text_xs().text_color(rgb(SIDEBAR_TEXT)).child(
                                                match &self.codex_status {
                                                    RuntimeLifecycleStatus::Starting => language
                                                        .choose("Codex 启动中…", "Codex starting…"),
                                                    RuntimeLifecycleStatus::Running { .. } => {
                                                        language
                                                            .choose("Codex 运行中", "Codex running")
                                                    }
                                                    RuntimeLifecycleStatus::Stopped => language
                                                        .choose("运行时离线", "Runtime offline"),
                                                    RuntimeLifecycleStatus::Failed { .. } => {
                                                        language.choose(
                                                            "Codex 启动失败",
                                                            "Codex failed to start",
                                                        )
                                                    }
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    div().text_xs().text_color(rgb(SIDEBAR_GROUP)).child(
                                        language.choose(
                                            "v0.1 · 本地控制面",
                                            "v0.1 · local control plane",
                                        ),
                                    ),
                                ),
                        ),
                )
                .child(
                    // Main column
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .v_flex()
                        .bg(rgb(CARD_BG))
                        .relative()
                        .child(
                            // Top bar
                            div()
                                .h(px(56.))
                                .px_5()
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(rgb(BORDER))
                                .child(
                                    div()
                                        .v_flex()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_base()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(TEXT_PRIMARY))
                                                .child(language.screen_label(active_screen)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(TEXT_MUTED))
                                                .child(selected_project_label),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .id("command-palette-trigger")
                                                .flex()
                                                .items_center()
                                                .px_2()
                                                .h(px(28.))
                                                .rounded_md()
                                                .border_1()
                                                .border_color(rgb(BORDER))
                                                .bg(rgb(SURFACE_BG))
                                                .cursor_pointer()
                                                .hover(|this| this.bg(rgb(BORDER)))
                                                .on_click({
                                                    let opener = entity.clone();
                                                    move |_, window, cx| {
                                                        opener.update(cx, |view, cx| {
                                                            view.open_command_palette(window, cx);
                                                        });
                                                    }
                                                })
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(rgb(TEXT_MUTED))
                                                        .child(if cfg!(target_os = "macos") {
                                                            "⌘K"
                                                        } else {
                                                            "Ctrl K"
                                                        }),
                                                ),
                                        )
                                        .child(
                                            Button::new("toggle-language")
                                                .ghost()
                                                .label(language.toggle_label())
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |view, cx| {
                                                        view.language = view.language.toggled();
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            // Scrollable content
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_scrollbar()
                                .id("main-content-scroll")
                                .bg(rgb(SURFACE_BG))
                                .child(page),
                        )
                        .child(
                            // Status bar
                            div()
                                .h(px(28.))
                                .px_5()
                                .flex()
                                .items_center()
                                .justify_between()
                                .bg(rgb(CARD_BG))
                                .border_t_1()
                                .border_color(rgb(BORDER))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(status_dot(0x0094_a3b8))
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                            language.choose(
                                                "Bridge 未连接 · 验证未运行",
                                                "Bridge not connected · Verification not run",
                                            ),
                                        )),
                                )
                                .child(
                                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose(
                                            "CircuitFabric 桌面端",
                                            "CircuitFabric desktop",
                                        ),
                                    ),
                                ),
                        )
                        .when_some(command_palette, ParentElement::child),
                )
        }
    }

    gpui_platform::application().run(move |cx| {
        gpui_component::init(cx);
        cx.set_app_identity("io.circuitfabric.desktop", "CircuitFabric");
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    app_id: Some("io.circuitfabric.desktop".to_owned()),
                    ..Default::default()
                },
                |window, cx| {
                    // `WindowOptions::default` leaves the native title unset on
                    // Windows, which produces an otherwise normal caption bar with
                    // an empty text region.
                    window.set_window_title("CircuitFabric");

                    #[cfg(windows)]
                    windows_icon::apply(window)
                        .expect("failed to apply the CircuitFabric Windows window icon");

                    let view = cx.new(|cx| ControlPlaneView::new(window, cx));
                    cx.observe_keystrokes({
                        let view = view.clone();
                        move |event, window, cx| {
                            view.update(cx, |view, cx| {
                                view.handle_command_keystroke(event, window, cx);
                            });
                        }
                    })
                    .detach();
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open CircuitFabric desktop window");
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_project_is_shell_state_not_circuit_state() {
        let mut shell = DesktopShell::default();
        shell.select_project("project-1".to_owned());

        assert_eq!(shell.selected_project(), Some("project-1"));
    }
}
