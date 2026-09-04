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
        path::{Path, PathBuf},
        sync::Arc,
    };

    use circuitfabric_codex_runtime::{LlmProviderSettings, RuntimeSettings};
    use circuitfabric_contracts::Project;
    use circuitfabric_project::{ProjectRegistry, ProjectStorage, ProjectWorkspace};
    use gpui::{
        AppContext, Context, Entity, FontWeight, Image, ImageFormat, InteractiveElement,
        IntoElement, KeystrokeEvent, ParentElement, Render, StatefulInteractiveElement, Styled,
        Window, WindowOptions, div, img, prelude::FluentBuilder as _, px, rgb, rgba,
    };
    use gpui_base::{InputBase, input::InputEditorStyle};
    use gpui_component::{
        Root, StyledExt,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        scroll::ScrollableElement as _,
    };
    use rfd::FileDialog;

    const APP_LOGO: &[u8] =
        include_bytes!("../../../assets/branding/circuitfabric-logo-v3-framed-transparent.png");
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

    struct ControlPlaneView {
        logo: Arc<Image>,
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
        selected_project: Option<ProjectId>,
        project_search: Entity<InputState>,
        project_filter: ProjectFilter,
        project_tab: ProjectDetailTab,
        project_form_open: bool,
        new_project_id: Entity<InputState>,
        new_project_name: Entity<InputState>,
        new_project_description: Entity<InputState>,
        new_project_root: Entity<InputState>,
    }

    impl ControlPlaneView {
        fn project_registry_path(settings_path: &Path) -> PathBuf {
            settings_path.with_file_name("projects.json")
        }

        fn restore_project_workspace(
            registry_path: &Path,
        ) -> (ProjectRegistry, ProjectWorkspace, Vec<String>) {
            let registry = match ProjectRegistry::load_or_default(registry_path) {
                Ok(registry) => registry,
                Err(error) => {
                    return (
                        ProjectRegistry::default(),
                        ProjectWorkspace::default(),
                        vec![format!("项目注册表未加载：{error}")],
                    );
                }
            };
            let mut workspace = ProjectWorkspace::default();
            let mut diagnostics = Vec::new();
            for entry in registry.entries() {
                match ProjectStorage::open(&entry.canonical_root_path) {
                    Ok(storage) if storage.manifest().project.id == entry.project_id => {
                        if let Err(error) =
                            workspace.create_project(storage.manifest().project.clone())
                        {
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
            (registry, workspace, diagnostics)
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
            let settings = RuntimeSettings::load_or_default(&settings_path).unwrap_or_default();
            let project_registry_path = Self::project_registry_path(&settings_path);
            let (project_registry, workspace, project_restore_diagnostics) =
                Self::restore_project_workspace(&project_registry_path);
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
            let new_project_id = Self::input(window, String::new(), "power-supply", cx);
            let new_project_name = Self::input(window, String::new(), "Power supply", cx);
            let new_project_description =
                Self::input(window, String::new(), "Optional design workspace description", cx);
            let new_project_root =
                Self::input(window, String::new(), "Choose an existing empty folder", cx);
            for input in [&project_search, &new_project_id, &new_project_root] {
                cx.subscribe(input, |_, _, event, cx| {
                    if let InputEvent::Change = event {
                        cx.notify();
                    }
                })
                .detach();
            }
            let status = if project_restore_diagnostics.is_empty() {
                "项目列表已恢复；全局运行时设置尚未修改。".to_owned()
            } else {
                format!("项目恢复提示：{}", project_restore_diagnostics.join("；"))
            };
            Self {
                logo: Arc::new(Image::from_bytes(ImageFormat::Png, APP_LOGO.to_vec())),
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
                selected_project: None,
                project_search,
                project_filter: ProjectFilter::All,
                project_tab: ProjectDetailTab::Overview,
                project_form_open: false,
                new_project_id,
                new_project_name,
                new_project_description,
                new_project_root,
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

        fn save_settings(&mut self, cx: &mut Context<Self>) {
            let providers = self.provider_values(cx);
            if providers.is_empty() {
                "未保存：至少需要一个 Provider。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let default_provider_id =
                if providers.iter().any(|provider| provider.id == self.default_provider_id) {
                    self.default_provider_id.clone()
                } else {
                    providers[0].id.clone()
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
            self.status = match settings.save(&self.settings_path) {
                Ok(()) => format!(
                    "已保存到 {}。默认 Provider：{}。现在可启动 circuitfabric-jlc-bridge。",
                    self.settings_path.display(),
                    default_provider_id
                ),
                Err(error) => format!("未保存：{error}"),
            };
            self.default_provider_id = default_provider_id;
            cx.notify();
        }

        fn field(
            label: &'static str,
            id: &'static str,
            state: &Entity<InputState>,
        ) -> impl IntoElement {
            div().v_flex().gap_1().child(div().text_sm().child(label)).child(
                div()
                    .id(id)
                    .w_full()
                    .h_8()
                    .px_2()
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(rgb(0x00cb_d5e1))
                    .child(Input::new(state)),
            )
        }

        fn open_project_form(&mut self, cx: &mut Context<Self>) {
            self.project_form_open = true;
            "选择一个已有文件夹，再确认创建受管理的项目目录。".clone_into(&mut self.status);
            cx.notify();
        }

        fn choose_project_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if let Some(root) = FileDialog::new().set_title("选择项目根文件夹").pick_folder()
            {
                self.new_project_root.update(cx, |state, cx| {
                    state.set_value(root.display().to_string(), window, cx);
                });
                self.status =
                    format!("已选择项目根文件夹：{}。创建前不会修改该文件夹。", root.display());
            }
            cx.notify();
        }

        fn persist_project_registry(&self) -> Result<(), String> {
            self.project_registry
                .save(&self.project_registry_path)
                .map_err(|error| error.to_string())
        }

        fn select_project(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
            if let Err(error) = self.project_registry.mark_opened(&project_id) {
                self.status = format!("项目已打开，但未能记录最近活动：{error}");
            } else if let Err(error) = self.persist_project_registry() {
                self.status = format!("项目已打开，但未能保存项目注册表：{error}");
            }
            self.selected_project = Some(project_id);
            self.project_tab = ProjectDetailTab::Overview;
            cx.notify();
        }

        fn open_existing_project(&mut self, cx: &mut Context<Self>) {
            let Some(root) =
                FileDialog::new().set_title("打开已有 CircuitFabric 项目").pick_folder()
            else {
                return;
            };
            let storage = match ProjectStorage::open(&root) {
                Ok(storage) => storage,
                Err(error) => {
                    self.status = format!("未打开项目：{error}");
                    cx.notify();
                    return;
                }
            };
            let diagnostics = storage.diagnose_layout();
            if !diagnostics.is_healthy() {
                self.status = format!(
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
            if let Some(registered_root) = self.project_registry.root_for(&project.id) {
                if registered_root != storage.root() {
                    self.status = format!(
                        "未打开项目：项目 ID `{}` 已绑定到 {}。",
                        project.id,
                        registered_root.display()
                    );
                    cx.notify();
                    return;
                }
            } else if let Err(error) = self.project_registry.register(&storage) {
                self.status = format!("未注册已有项目：{error}");
                cx.notify();
                return;
            }
            if self.workspace.project(&project.id).is_none()
                && let Err(error) = self.workspace.create_project(project.clone())
            {
                self.status = format!("未打开项目：{error}");
                cx.notify();
                return;
            }
            if let Err(error) = self.project_registry.mark_opened(&project.id) {
                self.status = format!("项目已打开，但未能记录最近活动：{error}");
            } else if let Err(error) = self.persist_project_registry() {
                self.status = format!("项目已打开，但未能保存项目注册表：{error}");
            } else {
                self.status =
                    format!("已打开项目 `{}`：{}。", project.id, storage.root().display());
            }
            self.selected_project = Some(project.id);
            self.project_tab = ProjectDetailTab::Overview;
            cx.notify();
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
                Ok(storage) => match self.project_registry.register(&storage) {
                    Ok(()) => match self.workspace.create_project(project) {
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
                                    "项目文件已创建于 {}，但项目注册表未保存：{error}。",
                                    storage.root().display()
                                ),
                                None => {
                                    format!("已创建项目 `{id}`：{}。", storage.root().display())
                                }
                            };
                        }
                        Err(error) => {
                            self.status = format!(
                                "项目文件已创建于 {}，但未能加入当前工作区：{error}。",
                                storage.root().display()
                            );
                        }
                    },
                    Err(error) => {
                        self.status = format!(
                            "项目文件已创建于 {}，但未能注册：{error}。文件未被删除。",
                            storage.root().display()
                        );
                    }
                },
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
        #[allow(clippy::too_many_lines)]
        fn render_legacy_provider_form(
            &mut self,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let selected = self.selected_provider.min(self.providers.len() - 1);
            let provider = &self.providers[selected];
            let selected_provider_id = provider.id.read(cx).value().to_string();
            let selected_is_default = selected_provider_id == self.default_provider_id;
            let mut provider_list = div().flex().flex_wrap().gap_2();
            for (index, provider) in self.providers.iter().enumerate() {
                let provider_id = provider.id.read(cx).value().to_string();
                let is_selected = index == selected;
                let label = if provider_id == self.default_provider_id {
                    format!("★ {provider_id}")
                } else {
                    provider_id.clone()
                };
                let selector = entity.clone();
                provider_list = provider_list.child(
                    div()
                        .id(format!("select-provider-{index}"))
                        .px_3()
                        .h(px(32.))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .cursor_pointer()
                        .text_sm()
                        .font_weight(if is_selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .border_1()
                        .when(is_selected, |this| {
                            this.bg(rgb(0x000e_7490))
                                .border_color(rgb(0x000e_7490))
                                .text_color(rgb(CARD_BG))
                        })
                        .when(!is_selected, |this| {
                            this.bg(rgb(CARD_BG))
                                .border_color(rgb(BORDER))
                                .text_color(rgb(TEXT_SECONDARY))
                        })
                        .hover(|this| this.bg(rgb(SURFACE_BG)))
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_provider = index;
                                cx.notify();
                            });
                        })
                        .child(label),
                );
            }
            let add_provider = entity.clone();
            let remove_provider = entity.clone();
            let set_default = entity.clone();
            let toggle_provider = entity.clone();
            let toggle_vision = entity.clone();
            div().v_flex().size_full().min_w(px(760.)).items_center().justify_center().bg(rgb(0x00f4_f7ff)).child(
                div()
                    .v_flex()
                    .items_center()
                    .gap_3()
                    .p_8()
                    .bg(rgb(0x00ff_ffff))
                    .rounded_xl()
                    .shadow_lg()
                    .child(img(self.logo.clone()).size(px(176.)))
                    .child(div().text_xl().child("CircuitFabric"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x004b_5563))
                            .child("Codex App Server + JLC EDA local bridge"),
                    )
                    .child(
                        div()
                            .w(px(720.))
                            .v_flex()
                            .gap_3()
                            .child(Self::field(language.choose("Codex 命令", "Codex command"), "codex-command", &self.command))
                            .child(Self::field(
                                language.choose("工作目录", "Working directory"),
                                "working-directory",
                                &self.working_directory,
                            ))
                            .child(Self::field(
                                language.choose("JLC bridge 地址", "JLC bridge address"),
                                "bridge-address",
                                &self.bridge_address,
                            ))
                            .child(div().text_lg().child(language.choose("LLM Provider 管理", "LLM Provider management")))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x004b_5563))
                                    .child("可添加多个 OpenAI-compatible Provider；API Key 只填写环境变量名，不在此界面保存密钥值。"),
                            )
                            .child(provider_list)
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("add-provider")
                                            .label("添加 Provider")
                                            .on_click(move |_, window, cx| {
                                                add_provider.update(cx, |view, cx| {
                                                    view.add_provider(window, cx);
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new("remove-provider")
                                            .label("删除当前")
                                            .on_click(move |_, _, cx| {
                                                remove_provider.update(cx, ControlPlaneView::remove_provider);
                                            }),
                                    ),
                            )
                            .child(div().text_sm().child(format!("当前编辑：{selected_provider_id}")))
                            .child(Self::field("Provider ID", "provider-id", &provider.id))
                            .child(Self::field("显示名称", "provider-name", &provider.name))
                            .child(Self::field("LLM Base URL", "provider-base-url", &provider.base_url))
                            .child(Self::field("LLM Model", "provider-model", &provider.model))
                            .child(Self::field(
                                "LLM API Key 环境变量名",
                                "provider-api-key-env",
                                &provider.api_key_environment_variable,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("toggle-provider")
                                            .label(if provider.enabled { "当前：已启用" } else { "当前：已停用" })
                                            .on_click(move |_, _, cx| {
                                                toggle_provider.update(cx, ControlPlaneView::toggle_provider);
                                            }),
                                    )
                                    .child(
                                        Button::new("set-default-provider")
                                            .label(if selected_is_default { "当前为默认 Provider" } else { "设为默认 Provider" })
                                            .on_click(move |_, _, cx| {
                                                set_default.update(cx, ControlPlaneView::set_default_provider);
                                            }),
                                    ),
                            )
                            .child(div().text_lg().child("Vision 配置"))
                            .child(Self::field(
                                "Vision Base URL",
                                "vision-base-url",
                                &provider.vision_base_url,
                            ))
                            .child(Self::field("Vision LLM Model", "vision-model", &provider.vision_model))
                            .child(Self::field(
                                "Vision API Key 环境变量名",
                                "vision-api-key-env",
                                &provider.vision_api_key_environment_variable,
                            ))
                            .child(
                                Button::new("toggle-vision")
                                    .label(if provider.supports_vision {
                                        "Vision：已启用"
                                    } else {
                                        "Vision：已停用"
                                    })
                                    .on_click(move |_, _, cx| {
                                        toggle_vision.update(cx, ControlPlaneView::toggle_vision);
                                    }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x004b_5563))
                                    .child(self.status.clone()),
                            )
                            .child(
                                Button::new("save-runtime")
                                    .primary()
                                    .label(language.choose("保存 App Server 设置", "Save App Server settings"))
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, ControlPlaneView::save_settings);
                                    }),
                            ),
                    ),
            )
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
                                .child(language.choose("0 份文档", "0 documents"))
                                .child(language.choose("0 个会话", "0 sessions"))
                                .child(language.choose("尚无活动", "No activity yet")),
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
                                        .on_click(move |_, _, cx| {
                                            open_existing.update(cx, |view, cx| {
                                                view.open_existing_project(cx);
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
                                        .h(px(34.))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .border_1()
                                        .border_color(rgb(BORDER))
                                        .rounded_md()
                                        .bg(rgb(CARD_BG))
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
                            .child(Self::project_empty_metric(language.choose("文档", "Documents")))
                            .child(Self::project_empty_metric(language.choose("会话", "Sessions")))
                            .child(Self::project_empty_metric(language.choose("用量", "Usage"))),
                    )
                    .into_any_element(),
                ProjectDetailTab::Documents => Self::project_empty_state(
                    language.choose("还没有授权文档", "No authorized documents yet"),
                    language.choose(
                        "文档登记后会显示其来源、内容哈希和可引用片段。",
                        "Registered documents will show their source, content hash, and citation-ready fragments.",
                    ),
                )
                .into_any_element(),
                ProjectDetailTab::Sessions => Self::project_empty_state(
                    language.choose("还没有会话", "No sessions yet"),
                    language.choose(
                        "此项目的智能体会话和工具调用记录将仅显示在这里。",
                        "Agent sessions and tool calls scoped to this project will appear only here.",
                    ),
                )
                .into_any_element(),
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

        fn project_empty_metric(label: &'static str) -> impl IntoElement {
            div()
                .flex_1()
                .v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("0"))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
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
                    self.render_legacy_provider_form(window, cx).into_any_element()
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
                                        .child(status_dot(0x0094_a3b8))
                                        .child(
                                            div().text_xs().text_color(rgb(SIDEBAR_TEXT)).child(
                                                language.choose("运行时离线", "Runtime offline"),
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
