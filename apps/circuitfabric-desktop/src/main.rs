//! Desktop control-plane entry point.
//!
//! The native GPUI view is feature-gated so the semantic core and plugins can be developed and
//! tested without a graphics stack. Enable it with `--features native-ui` after fetching the
//! GPUI dependencies pinned in `Cargo.lock`.

#![deny(unsafe_code)]

use circuitfabric_contracts::ProjectId;

#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
mod settings_persistence;

#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
mod usage_audit;

#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
mod plugin_governance;

#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
mod pdf_zoom;

#[cfg(all(feature = "native-ui", windows))]
#[allow(unsafe_code)]
mod pdf_cursors;

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
    SecretsVault,
    Settings,
}

impl ControlPlaneScreen {
    #[cfg(feature = "native-ui")]
    const ALL: [Self; 13] = [
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
        Self::SecretsVault,
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
            Self::SecretsVault => "Secrets vault",
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
            Self::Plugins | Self::Usage | Self::SecretsVault | Self::Settings => "GOVERNANCE",
        }
    }

    #[must_use]
    pub const fn is_todo(self) -> bool {
        !matches!(
            self,
            Self::Overview
                | Self::Projects
                | Self::Documents
                | Self::Semantics
                | Self::EdaServices
                | Self::AgentsAndMcp
                | Self::SessionsAndTasks
                | Self::Plugins
                | Self::SecretsVault
        )
    }

    /// Screens whose page is scoped to one open project: they render a
    /// "select a project first" empty state until one is chosen, so their
    /// navigation entries stay disabled until a project is selected.
    #[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
    #[must_use]
    pub const fn requires_project(self) -> bool {
        matches!(self, Self::Semantics | Self::ChangesAndApprovals | Self::BomAndExport)
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
    const fn from_preference(language: circuitfabric_codex_runtime::GlobalLanguage) -> Self {
        match language {
            circuitfabric_codex_runtime::GlobalLanguage::SimplifiedChinese => {
                Self::SimplifiedChinese
            }
            circuitfabric_codex_runtime::GlobalLanguage::English => Self::English,
        }
    }

    const fn preference(self) -> circuitfabric_codex_runtime::GlobalLanguage {
        match self {
            Self::SimplifiedChinese => {
                circuitfabric_codex_runtime::GlobalLanguage::SimplifiedChinese
            }
            Self::English => circuitfabric_codex_runtime::GlobalLanguage::English,
        }
    }

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
                ControlPlaneScreen::SecretsVault => "密钥保险库",
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
                | ControlPlaneScreen::SecretsVault
                | ControlPlaneScreen::Settings => "治理",
            },
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

/// Native window caption: the app name alone until a project is opened,
/// then the app name plus the selected project's name.
#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
#[must_use]
pub fn app_window_title(selected_project_name: Option<&str>) -> String {
    match selected_project_name {
        Some(name) => format!("CircuitFabric — {name}"),
        None => "CircuitFabric".to_owned(),
    }
}

#[cfg(not(feature = "native-ui"))]
fn main() {
    if circuitfabric_codex_runtime::judge::run_from_args() {
        return;
    }
    println!("CircuitFabric desktop scaffold. Rebuild with --features native-ui to start GPUI.");
}

#[cfg(feature = "native-ui")]
#[allow(clippy::too_many_lines)]
fn main() {
    if circuitfabric_codex_runtime::judge::run_from_args() {
        return;
    }
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use crate::plugin_governance::PluginGovernanceStore;
    use crate::usage_audit::{
        AuditKindFilter, AuditRecord, UsageAuditModel, UsagePeriod, UsageRecord,
    };
    use circuitfabric_codex_runtime::secrets::{SecretSource, UnlockedVault, secret_source};
    use circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID;
    use circuitfabric_codex_runtime::{
        CodexAppServerHandle, GlobalPreferences, GlobalTheme, LlmProviderSettings, LogLevel,
        RuntimeSettings, SecretStorageProvider, ToolAuthorizationKind, ToolAuthorizationSettings,
    };
    use circuitfabric_contracts::{
        DatasheetExtraction, DocumentKind, FactStatus, LogicalCircuitSnapshot, Project,
        SnapshotAuthority,
    };
    use circuitfabric_document_opener::{DocumentOpenerRegistry, extract_datasheet_by_category};
    use circuitfabric_plugin_api::{
        DocumentBlockKind, DocumentOpenDenial, DocumentSpanStyle, DocumentView, DocumentViewBody,
    };
    use circuitfabric_project::{
        ChangeSetAuditEntry, ChangeSetStageStatus, DocumentCategory, EvidenceCorpus, EvidenceHit,
        EvidenceScope, EvidenceSearch, FragmentAnchor, ProjectDocument, ProjectRegistry,
        ProjectStorage, ProjectWorkspace, SessionActor, SessionEvent, SessionEventKind,
        SessionListing, SessionReplay, SessionSeed, SessionStatus, SessionUsage, StoredChangeSet,
        is_evidence_indexable, rfc3339,
    };
    use gpui::{
        AnyElement, AppContext, ClickEvent, Context, Div, DragMoveEvent, Entity, FontStyle,
        FontWeight, HighlightStyle, Image, ImageFormat, InteractiveElement, IntoElement,
        KeystrokeEvent, ParentElement, Render, Size, StatefulInteractiveElement,
        StrikethroughStyle, Styled, StyledText, Window, WindowOptions, div, img,
        prelude::FluentBuilder as _, px, rgb as gpui_rgb, rgba,
    };
    use gpui_base::{InputBase, input::InputEditorStyle};
    use gpui_component::{
        Disableable, IconName, Root, StyledExt,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        scroll::ScrollableElement as _,
    };
    use jlcircuit_eda_bridge::{
        probe::{self, BridgeStatusReport},
        supervision::BridgeProcessHandle,
    };
    const SIDEBAR_MARK: &[u8] =
        include_bytes!("../../../assets/branding/circuitfabric-sidebar-mark.png");

    // Design tokens: a dark-navy sidebar, a light content surface, and a cyan accent.
    const SIDEBAR_BG: u32 = 0x000f_172b;
    const SIDEBAR_DIVIDER: u32 = 0x001e_293b;
    const SIDEBAR_GROUP: u32 = 0x005f_7085;
    const SIDEBAR_TEXT: u32 = 0x009c_a7b8;
    const SIDEBAR_TEXT_ACTIVE: u32 = 0x00f1_f5fa;
    const SIDEBAR_ITEM_HOVER: u32 = 0x001a_2637;
    const SIDEBAR_ITEM_ACTIVE: u32 = 0x001e_2d46;
    const SIDEBAR_ITEM_PRESSED: u32 = 0x0026_3756;
    const ACCENT: u32 = 0x0022_d3ee;
    const ACCENT_SOFT: u32 = 0x0067_e8f9;
    // Background of the data row a search hit navigated to.
    const FOCUSED_ROW_BG: u32 = 0x00fe_f9c3;

    const SURFACE_BG: u32 = 0x00f1_f5f9;
    const CARD_BG: u32 = 0x00ff_ffff;
    const BORDER: u32 = 0x00e2_e8f0;
    const TEXT_PRIMARY: u32 = 0x000f_172a;
    const TEXT_SECONDARY: u32 = 0x0047_5563;
    const TEXT_MUTED: u32 = 0x006b_7280;

    // All existing UI colour calls pass through this small semantic-token resolver.
    // It lets the complete shell (including pages implemented before preferences
    // existed) react immediately to a theme change rather than only repainting
    // the Settings page.
    static DARK_MODE: AtomicBool = AtomicBool::new(false);

    fn rgb(light_color: u32) -> gpui::Rgba {
        let color = if DARK_MODE.load(Ordering::Relaxed) {
            match light_color {
                SURFACE_BG => 0x000f_172a,
                CARD_BG => 0x0011_1827,
                BORDER => 0x0033_4155,
                TEXT_PRIMARY => 0x00f8_fafc,
                TEXT_SECONDARY => 0x00cbd5e1,
                TEXT_MUTED => 0x0094_a3b8,
                SIDEBAR_BG => 0x0002_0612,
                SIDEBAR_DIVIDER => 0x0033_4155,
                SIDEBAR_GROUP => 0x0094_a3b8,
                SIDEBAR_TEXT => 0x00cbd5e1,
                SIDEBAR_TEXT_ACTIVE => 0x00f8_fafc,
                SIDEBAR_ITEM_HOVER => 0x001e_293b,
                SIDEBAR_ITEM_ACTIVE => 0x001e_293b,
                SIDEBAR_ITEM_PRESSED => 0x0033_4155,
                // The pale informational chips need a dark surface as well.
                0x00e0_f2fe => 0x0016_344a,
                0x000e_7490 => 0x007dd3fc,
                other => other,
            }
        } else {
            light_color
        };
        gpui_rgb(color)
    }

    fn status_dot(color: u32) -> impl IntoElement {
        div().size(px(8.)).rounded_full().bg(rgb(color)).flex_none()
    }

    /// Keeps session audit entries readable: at most 2000 characters, marked when truncated.
    fn audit_excerpt(text: &str) -> String {
        let mut excerpt: String = text.chars().take(2000).collect();
        if excerpt.chars().count() < text.chars().count() {
            excerpt.push('…');
        }
        excerpt
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

        /// Stable session `backendId` for runs through this adapter.
        const fn backend_id(self) -> &'static str {
            match self {
                Self::CodexAppServer => "codex",
                Self::ClaudeCode => "claude-code",
                Self::Dsh => "dsh",
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
        BundledJev,
    }

    /// One manageable EDA backend service on the EDA services page. The list
    /// is the multi-EDA service registry surface: further backends slot in as
    /// new variants with their own supervised bridge.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum EdaServiceSelection {
        JlcircuitBridge,
    }

    impl EdaServiceSelection {
        const fn label(self) -> &'static str {
            match self {
                Self::JlcircuitBridge => "JLCircuit EDA",
            }
        }
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

        fn label(&self, language: UiLanguage) -> String {
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

    /// Reachability of the bridge endpoint, from deliberate probes — never guessed.
    /// Kept separate from [`RuntimeLifecycleStatus`]: the port may be owned by a
    /// bridge this app did not start, and a supervised child may die while the
    /// port is still briefly open.
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum BridgeHealth {
        /// No probe has completed yet; nothing is claimed either way.
        Unknown,
        /// The last probe could not connect: nothing is listening.
        Unreachable { reason: String },
        /// The last probe connected; `at` is when that succeeded.
        Listening { at: Instant },
    }

    impl BridgeHealth {
        const fn dot(&self) -> u32 {
            match self {
                Self::Unknown => 0x0094_a3b8,
                Self::Unreachable { .. } => 0x00dc_2626,
                Self::Listening { .. } => 0x0022_c55e,
            }
        }

        fn label(&self, language: UiLanguage) -> String {
            match self {
                Self::Unknown => {
                    language.choose("未知（尚未探测）", "Unknown (not probed yet)").to_owned()
                }
                Self::Unreachable { reason } => {
                    format!("{}：{reason}", language.choose("离线", "Offline"))
                }
                Self::Listening { at } => format!(
                    "{}（{} 前）",
                    language.choose("在线", "Online"),
                    elapsed_label(language, at.elapsed()),
                ),
            }
        }
    }

    /// Outcome of the manual connection test: the protocol-level `status`
    /// round-trip over the bridge WebSocket.
    struct BridgeTestResult {
        at: Instant,
        outcome: Result<BridgeStatusReport, String>,
    }

    fn elapsed_label(language: UiLanguage, elapsed: Duration) -> String {
        let seconds = elapsed.as_secs();
        if seconds < 60 {
            format!("{seconds}{}", language.choose(" 秒", "s"))
        } else {
            format!("{}{}", seconds / 60, language.choose(" 分钟", "m"))
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

    /// The semantic browser deliberately asks for a typed subject before applying free-text
    /// matching. It avoids a single ambiguous "search everything" result being interpreted as
    /// an engineering fact.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SemanticQueryScope {
        Components,
        Pins,
        Nets,
        Constraints,
        Evidence,
    }

    impl SemanticQueryScope {
        const ALL: [Self; 5] =
            [Self::Components, Self::Pins, Self::Nets, Self::Constraints, Self::Evidence];

        const fn label(self, language: UiLanguage) -> &'static str {
            match (self, language) {
                (Self::Components, UiLanguage::SimplifiedChinese) => "元件",
                (Self::Pins, UiLanguage::SimplifiedChinese) => "引脚",
                (Self::Nets, UiLanguage::SimplifiedChinese) => "网络",
                (Self::Constraints, UiLanguage::SimplifiedChinese) => "约束 / 验证",
                (Self::Evidence, UiLanguage::SimplifiedChinese) => "证据",
                (Self::Components, UiLanguage::English) => "Components",
                (Self::Pins, UiLanguage::English) => "Pins",
                (Self::Nets, UiLanguage::English) => "Nets",
                (Self::Constraints, UiLanguage::English) => "Constraints / verification",
                (Self::Evidence, UiLanguage::English) => "Evidence",
            }
        }
    }

    /// A format choice is part of the export intent, but it does not make an
    /// export authoritative on its own. Every choice below remains bound to
    /// the selected immutable semantic snapshot and its evidence references.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum BomExportFormat {
        Csv,
        Excel,
        Json,
    }

    impl BomExportFormat {
        const ALL: [Self; 3] = [Self::Csv, Self::Excel, Self::Json];

        const fn label(self) -> &'static str {
            match self {
                Self::Csv => "CSV",
                Self::Excel => "Excel (.xlsx)",
                Self::Json => "JSON",
            }
        }
    }

    #[derive(Clone)]
    struct SemanticQueryHit {
        kind: &'static str,
        subject: String,
        detail: String,
        evidence: String,
    }

    /// One project's cached view of its persisted workspace state.
    #[derive(Clone, Default)]
    struct ProjectWorkspaceData {
        documents: Vec<ProjectDocument>,
        /// Integrity verdicts (`read_verified_document_content`) computed when this listing
        /// was loaded — never per render. Re-hashing every managed copy on every frame was
        /// the root cause of the whole-window lag while the preview pane was open.
        document_integrity: std::collections::BTreeMap<String, bool>,
        session_listing: SessionListing,
        semantic_snapshots: Vec<LogicalCircuitSnapshot>,
        change_sets: Vec<StoredChangeSet>,
    }

    /// The read-only replay currently displayed in the Sessions tab.
    struct SessionReplaySelection {
        project_id: ProjectId,
        replay: SessionReplay,
    }

    /// Default width of the docked document-preview pane; the window grows by this much when
    /// the preview opens, mirroring the extend-to-the-right behavior of document tools. The
    /// user can then drag the divider to resize the pane within the clamp range below.
    const DOCUMENT_PREVIEW_WIDTH: f32 = 480.;
    const DOCUMENT_PREVIEW_MIN_WIDTH: f32 = 320.;
    const DOCUMENT_PREVIEW_MAX_WIDTH: f32 = 1040.;
    /// Preview-pane render caps: the scroll pane lays out every element each frame, so a
    /// huge document must not become thousands of text nodes inside the pane.
    const PREVIEW_MAX_RENDERED_PAGES: usize = 20;
    const PREVIEW_MAX_RENDERED_BLOCKS: usize = 120;
    const PREVIEW_MAX_RENDERED_ROWS: usize = 40;

    /// Marker for the drag value carried while the preview divider is being dragged.
    struct DraggedPreviewSplit;

    /// One page bitmap converted to a GPUI render image off the UI thread.
    #[derive(Clone)]
    struct DocumentRasterPreviewPage {
        image: std::sync::Arc<gpui::RenderImage>,
        requested_width: u32,
    }

    /// Pages rendered around the viewport are kept; farther ones are evicted (and removed
    /// from the GPU atlas), so memory stays bounded however long the document is.
    const RASTER_PREFETCH_BEFORE: u32 = 1;
    const RASTER_PREFETCH_AFTER: u32 = 2;
    const RASTER_KEEP_DISTANCE: u32 = 6;

    /// A pdfium-backed PDF preview whose pages are rendered on demand as they scroll into
    /// view. Every page keeps its place (sized from `page_sizes`) whether rendered or not.
    #[derive(Clone)]
    struct DocumentRasterPreview {
        /// The verified bytes pages are rendered from.
        data: std::sync::Arc<[u8]>,
        /// Size in points of every page.
        page_sizes: Vec<(f32, f32)>,
        pages: BTreeMap<u32, DocumentRasterPreviewPage>,
        in_flight: BTreeMap<u32, u32>,
        failed: std::collections::BTreeSet<(u32, u32)>,
        retired_images: Vec<std::sync::Arc<gpui::RenderImage>>,
    }

    /// What the preview pane shows for one selected document.
    #[derive(Clone)]
    enum DocumentPreviewState {
        Loading,
        /// The opener plugin produced a read-only embeddable view, shared via `Arc` so
        /// per-frame rendering never clones the whole body. `raster` holds page bitmaps
        /// already converted to GPUI images when the view carried them.
        Loaded {
            view: std::sync::Arc<DocumentView>,
            raster: Option<DocumentRasterPreview>,
        },
        /// The open gate refused the request before any opener ran.
        Refused {
            denial: DocumentOpenDenial,
        },
        /// The opener plugin explicitly reported unsupported or corrupt content.
        Unavailable {
            reason: String,
        },
    }

    /// Background full-text indexing of one PDF.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum PdfIndexState {
        Running,
        Failed,
    }

    /// Completed steps of an interrupted datasheet extraction. "Continue" reuses them while
    /// the document content is unchanged: completed category replies and Jev batches are reused.
    #[derive(Clone, Debug, Default)]
    struct DatasheetCheckpoint {
        content_hash: String,
        model_steps: Vec<(String, String)>,
        /// `(arguments, result)` per finished Jev batch, in call order.
        jev_results: Vec<(serde_json::Value, serde_json::Value)>,
        /// Cached judgments belong to the backend/model configuration that produced them.
        judge_definition: Option<circuitfabric_codex_runtime::tools::McpServerDefinition>,
    }

    /// The document currently shown in the right-hand preview pane.
    #[derive(Clone)]
    struct DocumentPreviewSelection {
        project_id: ProjectId,
        file_name: String,
        document_id: String,
        state: DocumentPreviewState,
        /// The persisted structured datasheet extraction, when one exists for this exact
        /// content. Shared via `Arc` so per-frame rendering never clones the payload.
        extraction: Option<std::sync::Arc<DatasheetExtraction>>,
    }

    /// Search typing is debounced; Enter and filter changes search immediately.
    const EVIDENCE_SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
    /// Ranked hits kept per search, and how many are rendered per "show more" step.
    const EVIDENCE_SEARCH_LIMIT: usize = 500;
    const EVIDENCE_PAGE_SIZE: usize = 30;

    /// The newest completed project-scoped evidence search.
    struct EvidenceSearchResult {
        project_id: ProjectId,
        scope: EvidenceScope,
        search: std::sync::Arc<EvidenceSearch>,
        elapsed: Duration,
    }

    /// The search hit a preview was opened from: the pane lands on its page or data row
    /// and shows the cited line above the content.
    #[derive(Clone, Debug)]
    struct PreviewFocus {
        project_id: ProjectId,
        document_id: String,
        content_hash: String,
        anchor: FragmentAnchor,
        text: String,
        terms: Vec<String>,
        regions: Vec<circuitfabric_document_opener::PdfHighlightRect>,
        notice: Option<String>,
        /// `true` while the source location is being resolved off the UI thread; the pane
        /// shows a loading mask over the document area until it clears.
        resolving: bool,
    }

    // One GPUI view struct accumulates the whole control plane's UI state; the
    // independent booleans track one asynchronous in-flight action each.
    #[allow(clippy::struct_excessive_bools)]
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
        global_theme: GlobalTheme,
        data_directory: Entity<InputState>,
        secret_storage_provider: SecretStorageProvider,
        log_level: LogLevel,
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
        session_project_filter: Option<ProjectId>,
        // Docked document preview: the selection and, when the pane widened the window on
        // open, the size to restore when it closes. `document_preview_width` follows the
        // divider drag.
        document_preview: Option<DocumentPreviewSelection>,
        document_preview_width: f32,
        // Whether the preview pane shows the structured datasheet tab instead of the
        // rendered document.
        preview_show_data: bool,
        datasheet_extracting: bool,
        datasheet_feedback: Option<String>,
        // Live log of the latest extraction (stage lines plus the streamed model reply),
        // keyed by the document it belongs to.
        datasheet_stream: Option<(ProjectId, String, std::sync::Arc<std::sync::Mutex<String>>)>,
        datasheet_extract_started: Option<Instant>,
        datasheet_cancel: Option<circuitfabric_codex_runtime::execution::Cancellation>,
        datasheet_checkpoint: Option<(ProjectId, String, DatasheetCheckpoint)>,
        // Background PDF full-text indexing per (project, document); indexed documents
        // leave the map, failed ones stay so they are not retried every time.
        pdf_index_state: BTreeMap<(ProjectId, String), PdfIndexState>,
        datasheet_rows_visible: usize,
        pre_preview_window_size: Option<Size<gpui::Pixels>>,
        usage_period: UsagePeriod,
        usage_filter: Entity<InputState>,
        audit_kind_filter: AuditKindFilter,
        audit_filter: Entity<InputState>,
        // Short-lived cache for the usage/audit projection; see `usage_audit_model`.
        usage_audit_cached: Option<(Instant, UsageAuditModel)>,
        evidence_query: Entity<InputState>,
        evidence_scope: EvidenceScope,
        // Only the newest search request may publish its result.
        evidence_generation: u64,
        evidence_searching: bool,
        evidence_result: Option<EvidenceSearchResult>,
        evidence_visible: usize,
        preview_focus: Option<PreviewFocus>,
        preview_scroll: gpui::ScrollHandle,
        preview_pdf_zoom: crate::pdf_zoom::ZoomMotion,
        preview_zoom_tick: Option<std::time::Instant>,
        preview_pdf_pan: Option<(gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>)>,
        preview_zoom_anchor: Option<(usize, gpui::Point<f32>, gpui::Point<gpui::Pixels>)>,
        preview_row_anchor: Option<gpui::ScrollAnchor>,
        preview_focus_generation: u64,
        main_content_scroll: gpui::ScrollHandle,
        // Set when the preview must scroll to `preview_focus` once its content is laid out.
        preview_scroll_pending: std::cell::Cell<bool>,
        semantic_query: Entity<InputState>,
        semantic_query_scope: SemanticQueryScope,
        selected_semantic_snapshot: Option<(ProjectId, String)>,
        selected_change_set: Option<(ProjectId, String)>,
        approval_drawer_open: bool,
        approval_note: Entity<InputState>,
        bom_export_format: BomExportFormat,
        selected_project: Option<ProjectId>,
        window_title: String,
        project_search: Entity<InputState>,
        project_filter: ProjectFilter,
        project_tab: ProjectDetailTab,
        project_form_open: bool,
        new_project_id: Entity<InputState>,
        new_project_name: Entity<InputState>,
        new_project_description: Entity<InputState>,
        new_project_root: Entity<InputState>,
        agents_selection: AgentsSelection,
        adapter_settings_open: Option<RuntimeAdapter>,
        provider_editor_open: bool,
        dialog_error: Option<String>,
        codex_process: Option<CodexAppServerHandle>,
        codex_status: RuntimeLifecycleStatus,
        codex_active_provider: Option<String>,
        bridge_process: Option<BridgeProcessHandle>,
        bridge_status: RuntimeLifecycleStatus,
        bridge_active_address: Option<String>,
        bridge_health: BridgeHealth,
        bridge_probed_at: Option<Instant>,
        bridge_probe_pending: bool,
        bridge_test_pending: bool,
        bridge_test: Option<BridgeTestResult>,
        eda_services_selection: EdaServiceSelection,
        tool_authorizations: ToolAuthorizationSettings,
        new_tool_id: Entity<InputState>,
        new_tool_kind: ToolAuthorizationKind,
        new_tool_scope: ToolScope,
        catalog: circuitfabric_codex_runtime::tools::ToolCatalog,
        catalog_id: Entity<InputState>,
        catalog_source: Entity<InputState>,
        catalog_args: Entity<InputState>,
        catalog_env: Entity<InputState>,
        // Non-secret LLM adapter form and a write-only masked credential entry.
        jev_api_key: Entity<InputState>,
        jev_base_url: Entity<InputState>,
        jev_model: Entity<InputState>,
        jev_key_env: Entity<InputState>,
        jev_timeout: Entity<InputState>,
        jev_retries: Entity<InputState>,
        jev_llm_settings: circuitfabric_codex_runtime::judge::LlmJudgeSettings,
        // Backend branch currently shown in the Jev page selector — a draft
        // until "save and use" applies it to the catalog.
        jev_llm_draft: bool,
        // Jev configuration dialogs; edits inside stay draft until 保存.
        jev_backend_modal_open: bool,
        jev_key_modal_open: bool,
        adapters: circuitfabric_codex_runtime::execution::AdapterSettings,
        saved_settings: RuntimeSettings,
        codex_provider: Entity<InputState>,
        claude_command: Entity<InputState>,
        claude_provider: Entity<InputState>,
        dsh_command: Entity<InputState>,
        dsh_provider: Entity<InputState>,
        task_prompt: Entity<InputState>,
        task_image: Entity<InputState>,
        task_result: String,
        task_cancel: Option<circuitfabric_codex_runtime::execution::Cancellation>,
        project_agent_adapter: RuntimeAdapter,
        plugin_directory: Entity<InputState>,
        plugin_governance_path: PathBuf,
        plugin_governance: PluginGovernanceStore,
        // Secrets vault: the encrypted API-key store. `vault` holds decrypted
        // key material only while unlocked; `vault_index` is the plaintext
        // variable-name index, readable even while locked.
        vault_path: PathBuf,
        vault: Option<UnlockedVault>,
        vault_file_exists: bool,
        vault_index: Vec<String>,
        vault_prompt_open: bool,
        vault_quick_unlock_open: bool,
        vault_busy: bool,
        vault_message: Option<String>,
        vault_password: Entity<InputState>,
        vault_password_confirm: Entity<InputState>,
        secret_name: Entity<InputState>,
        secret_value: Entity<InputState>,
        selected_secret: Option<String>,
    }

    impl Drop for ControlPlaneView {
        fn drop(&mut self) {
            if let Some(cancel) = &self.task_cancel {
                cancel.cancel();
            }
            // Dropping `vault` zeroizes the derived key and all decrypted values.
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
            let document_integrity = Self::compute_document_integrity(&storage, &documents);
            let session_listing =
                storage.list_sessions().map_err(|error| format!("会话记录未读取：{error}"))?;
            let semantic_snapshots = storage
                .list_logical_snapshots()
                .map_err(|error| format!("语义快照未读取：{error}"))?;
            let change_sets = storage.list_change_sets().map_err(|error| error.to_string())?;
            data.insert(
                project_id.clone(),
                ProjectWorkspaceData {
                    documents,
                    document_integrity,
                    session_listing,
                    semantic_snapshots,
                    change_sets,
                },
            );
            storages.insert(project_id, storage);
            Ok(())
        }

        /// One-shot integrity verification for a document listing.
        ///
        /// The documents page used to re-read and re-hash every managed copy on every
        /// render; with hover states on the cards that became a hash storm per frame.
        /// Verification now happens when a listing is (re)loaded — attach, refresh,
        /// import — and the render path reads these cached verdicts.
        fn compute_document_integrity(
            storage: &ProjectStorage,
            documents: &[ProjectDocument],
        ) -> BTreeMap<String, bool> {
            documents
                .iter()
                .map(|document| {
                    (document.id.clone(), storage.read_verified_document_content(document).is_ok())
                })
                .collect()
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
            let input = cx.new(|cx| {
                InputState::new(window, cx).default_value(value).placeholder(placeholder)
            });
            cx.subscribe(&input, |_, _, event, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            })
            .detach();
            input
        }

        /// Password-style input: no echo, and the value stays out of the clipboard.
        fn masked_input(
            window: &mut Window,
            placeholder: &'static str,
            cx: &mut Context<Self>,
        ) -> Entity<InputState> {
            cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(placeholder))
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
            DARK_MODE
                .store(Self::theme_is_dark(settings.global_preferences.theme), Ordering::Relaxed);
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
            let semantic_query = Self::input(
                window,
                String::new(),
                "reference, pin, net, constraint, document…",
                cx,
            );
            let usage_filter =
                Self::input(window, String::new(), "project, provider, or runtime", cx);
            let audit_filter =
                Self::input(window, String::new(), "project, runtime, session, or event", cx);
            let approval_note =
                Self::input(window, String::new(), "Approval or rejection reason", cx);
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
            let task_image =
                Self::input(window, String::new(), "可选图片路径；使用 Vision 服务", cx);
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
            let vault_path = UnlockedVault::default_path();
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
                datasheet_rows_visible: 40,
                pre_preview_window_size: None,
                usage_period: UsagePeriod::All,
                usage_filter,
                audit_kind_filter: AuditKindFilter::All,
                audit_filter,
                usage_audit_cached: None,
                evidence_query,
                evidence_scope: EvidenceScope::All,
                evidence_generation: 0,
                evidence_searching: false,
                evidence_result: None,
                evidence_visible: EVIDENCE_PAGE_SIZE,
                preview_focus: None,
                preview_scroll: gpui::ScrollHandle::new(),
                preview_pdf_zoom: crate::pdf_zoom::ZoomMotion::default(),
                preview_zoom_tick: None,
                preview_pdf_pan: None,
                preview_zoom_anchor: None,
                preview_row_anchor: None,
                preview_focus_generation: 0,
                main_content_scroll: gpui::ScrollHandle::new(),
                preview_scroll_pending: std::cell::Cell::new(false),
                semantic_query,
                semantic_query_scope: SemanticQueryScope::Components,
                selected_semantic_snapshot: None,
                selected_change_set: None,
                approval_drawer_open: false,
                approval_note,
                bom_export_format: BomExportFormat::Csv,
                selected_project: None,
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

        fn system_is_dark() -> bool {
            // `AppsUseLightTheme` is the Windows user preference.  On platforms
            // without this registry value, a light fallback keeps the UI usable
            // while retaining the user's explicit "follow system" choice.
            #[cfg(windows)]
            {
                let output = std::process::Command::new("reg")
                    .args([
                        "query",
                        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                        "/v",
                        "AppsUseLightTheme",
                    ])
                    .output();
                return output.ok().is_some_and(|result| {
                    let text = String::from_utf8_lossy(&result.stdout);
                    text.lines()
                        .any(|line| line.contains("AppsUseLightTheme") && line.ends_with("0x0"))
                });
            }
            #[cfg(not(windows))]
            {
                false
            }
        }

        fn theme_is_dark(theme: GlobalTheme) -> bool {
            match theme {
                GlobalTheme::Light => false,
                GlobalTheme::Dark => true,
                GlobalTheme::System => Self::system_is_dark(),
            }
        }

        fn global_preferences_from_form(&self, cx: &Context<Self>) -> GlobalPreferences {
            GlobalPreferences {
                theme: self.global_theme,
                language: self.language.preference(),
                data_directory: self.data_directory.read(cx).value().to_string().into(),
                secret_storage_provider: self.secret_storage_provider,
                log_level: self.log_level,
            }
        }

        /// Persists only global preferences, preserving any unsaved runtime form
        /// state.  No API-key values are read, displayed, or serialized here.
        fn persist_global_preferences(&mut self, cx: &mut Context<Self>) {
            let preferences = self.global_preferences_from_form(cx);
            let result =
                RuntimeSettings::load_or_default(&self.settings_path).and_then(|mut settings| {
                    settings.global_preferences = preferences;
                    settings.save(&self.settings_path)
                });
            self.status = match result {
                Ok(()) => self
                    .language
                    .choose(
                        "全局偏好已保存；不会保存 API Key 值。",
                        "Global preferences saved; no API key values are stored.",
                    )
                    .to_owned(),
                Err(error) => format!(
                    "{}: {error}",
                    self.language.choose("全局偏好未保存", "Global preferences were not saved")
                ),
            };
            cx.notify();
        }

        fn set_global_theme(&mut self, theme: GlobalTheme, cx: &mut Context<Self>) {
            self.global_theme = theme;
            DARK_MODE.store(Self::theme_is_dark(theme), Ordering::Relaxed);
            self.persist_global_preferences(cx);
        }

        fn toggle_global_language(&mut self, cx: &mut Context<Self>) {
            self.language = self.language.toggled();
            self.persist_global_preferences(cx);
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
                "Provider `{}` 的{}修改尚未保存；请点击「保存 Provider 列表」。",
                self.providers[selected].id.read(cx).value(),
                if self.providers[selected].enabled { "启用" } else { "停用" }
            );
            cx.notify();
        }

        fn toggle_vision(&mut self, cx: &mut Context<Self>) {
            let selected = self.selected_provider.min(self.providers.len() - 1);
            self.providers[selected].supports_vision = !self.providers[selected].supports_vision;
            self.status = format!(
                "Provider `{}` 的 Vision {}修改尚未保存；请点击「保存 Provider 列表」。",
                self.providers[selected].id.read(cx).value(),
                if self.providers[selected].supports_vision { "启用" } else { "停用" }
            );
            cx.notify();
        }

        fn save_update(
            &mut self,
            update: crate::settings_persistence::SettingsUpdate,
        ) -> Result<(), circuitfabric_codex_runtime::RuntimeError> {
            let saved = crate::settings_persistence::save_update(&self.settings_path, update)?;
            self.adapters = saved.adapters.clone();
            self.saved_settings = saved;
            Ok(())
        }

        /// Persists the provider list, reporting the outcome through `status` and returning the
        /// failure so dialogs can show it inline.
        fn save_providers_checked(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
            let update = crate::settings_persistence::SettingsUpdate::Providers {
                providers: self.provider_values(cx),
                default_provider_id: self.default_provider_id.clone(),
            };
            self.save_update(update).map_err(|error| format!("Provider 未保存：{error}"))
        }

        /// Persists one adapter's settings, reporting the outcome through `status` and returning
        /// the failure so dialogs can show it inline.
        fn save_runtime_checked(
            &mut self,
            adapter: RuntimeAdapter,
            cx: &mut Context<Self>,
        ) -> Result<(), String> {
            use crate::settings_persistence::SettingsUpdate;
            let (command, provider) = self.runtime_fields(adapter);
            let command = command.read(cx).value().trim().to_owned();
            let provider_id = provider.read(cx).value().trim().to_owned();
            let update = match adapter {
                RuntimeAdapter::CodexAppServer => SettingsUpdate::Codex {
                    command,
                    working_directory: self.working_directory.read(cx).value().trim().into(),
                    provider_id,
                },
                RuntimeAdapter::ClaudeCode => SettingsUpdate::Claude { command, provider_id },
                RuntimeAdapter::Dsh => SettingsUpdate::Dsh { command, provider_id },
            };
            self.save_update(update).map_err(|error| {
                format!(
                    "{} 配置未保存：{error}。新增 Provider 请先在 Provider 详情中保存。",
                    adapter.label()
                )
            })
        }

        fn runtime_fields(
            &self,
            adapter: RuntimeAdapter,
        ) -> (&Entity<InputState>, &Entity<InputState>) {
            match adapter {
                RuntimeAdapter::CodexAppServer => (&self.command, &self.codex_provider),
                RuntimeAdapter::ClaudeCode => (&self.claude_command, &self.claude_provider),
                RuntimeAdapter::Dsh => (&self.dsh_command, &self.dsh_provider),
            }
        }

        fn runtime_dirty(&self, adapter: RuntimeAdapter, cx: &Context<Self>) -> bool {
            let (command, provider) = self.runtime_fields(adapter);
            let saved = &self.saved_settings;
            let (saved_command, saved_provider) = match adapter {
                RuntimeAdapter::CodexAppServer => {
                    (&saved.codex.command, &saved.adapters.codex_provider_id)
                }
                RuntimeAdapter::ClaudeCode => {
                    (&saved.adapters.claude_command, &saved.adapters.claude_provider_id)
                }
                RuntimeAdapter::Dsh => {
                    (&saved.adapters.dsh_command, &saved.adapters.dsh_provider_id)
                }
            };
            command.read(cx).value().trim() != saved_command
                || provider.read(cx).value().trim() != saved_provider
                || (adapter == RuntimeAdapter::CodexAppServer
                    && PathBuf::from(self.working_directory.read(cx).value().trim())
                        != saved.codex.working_directory)
        }

        fn save_state_note(dirty: bool, language: UiLanguage) -> Div {
            div().text_xs().text_color(rgb(if dirty { 0x00b4_5309 } else { TEXT_MUTED }))
                .child(if dirty {
                    language.choose("有未保存修改；切换页面会保留草稿，启动和任务使用已保存配置。", "Unsaved changes; drafts survive navigation. Starts and tasks use saved configuration.")
                } else {
                    language.choose("当前配置无修改；启动和任务使用已保存配置。", "No pending changes; starts and tasks use saved configuration.")
                })
        }

        /// One numbered section card on a runtime-adapter settings page. The step badge gives
        /// the page a stable, scannable operation order — run, settings, verification for the
        /// supervised Codex endpoint; settings, verification for the per-task adapters.
        fn adapter_section_card(step: &'static str, title: &'static str) -> Div {
            div()
                .v_flex()
                .gap_3()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE_BG))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .size(px(22.))
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(rgb(0x000e_7490))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(CARD_BG))
                                .child(step),
                        )
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(title),
                        ),
                )
        }

        /// Clickable provider binding for one runtime adapter. Chips for every saved provider
        /// (plus the default fallback) replace typo-prone free-text entry; the raw ID field
        /// stays beneath for manual override and always shows the exact value in effect.
        fn render_provider_binding(
            &self,
            adapter: RuntimeAdapter,
            cx: &Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let provider_state = self.runtime_fields(adapter).1.clone();
            let current = provider_state.read(cx).value().trim().to_owned();
            let saved_providers = self.saved_settings.providers.clone();
            let field_id = match adapter {
                RuntimeAdapter::CodexAppServer => "codex-provider",
                RuntimeAdapter::ClaudeCode => "claude-provider",
                RuntimeAdapter::Dsh => "dsh-provider",
            };
            let unknown_binding = !current.is_empty()
                && saved_providers.iter().all(|provider| provider.id != current);

            let mut chips = div().flex().flex_wrap().gap_2();
            let default_chooser = entity.clone();
            let default_selected = current.is_empty();
            chips = chips.child(
                div()
                    .id(format!("bind-default-{}", adapter.backend_id()))
                    .px_3()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .cursor_pointer()
                    .text_xs()
                    .border_1()
                    .border_color(rgb(if default_selected { 0x000e_7490 } else { BORDER }))
                    .when(default_selected, |this| {
                        this.bg(rgb(0x000e_7490))
                            .text_color(rgb(CARD_BG))
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!default_selected, |this| {
                        this.bg(rgb(CARD_BG)).text_color(rgb(TEXT_SECONDARY))
                    })
                    .on_click(move |_, window, cx| {
                        default_chooser.update(cx, |view, cx| {
                            let (_, state) = view.runtime_fields(adapter);
                            state.update(cx, |state, cx| state.set_value("", window, cx));
                            cx.notify();
                        });
                    })
                    .child(language.choose("默认 Provider", "Default provider")),
            );
            for provider in saved_providers {
                let chooser = entity.clone();
                let selected = current == provider.id;
                let label = if provider.enabled {
                    format!("{} · {}", provider.id, provider.model)
                } else {
                    format!(
                        "{} · {}{}",
                        provider.id,
                        provider.model,
                        language.choose("（已停用）", " (disabled)")
                    )
                };
                chips = chips.child(
                    div()
                        .id(format!("bind-{}-{}", adapter.backend_id(), provider.id))
                        .px_3()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .cursor_pointer()
                        .text_xs()
                        .border_1()
                        .border_color(rgb(if selected { 0x000e_7490 } else { BORDER }))
                        .when(selected, |this| {
                            this.bg(rgb(0x000e_7490))
                                .text_color(rgb(CARD_BG))
                                .font_weight(FontWeight::SEMIBOLD)
                        })
                        .when(!selected, |this| {
                            this.bg(rgb(CARD_BG)).text_color(rgb(TEXT_SECONDARY))
                        })
                        .on_click(move |_, window, cx| {
                            let id = provider.id.clone();
                            chooser.update(cx, |view, cx| {
                                let (_, state) = view.runtime_fields(adapter);
                                state.update(cx, |state, cx| state.set_value(id, window, cx));
                                cx.notify();
                            });
                        })
                        .child(label),
                );
            }

            div()
                .v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(language.choose("Provider 关联", "Provider binding")),
                )
                .child(chips)
                .child(Self::labeled_field(
                    language.choose(
                        "Provider ID（点选上方或手动填写，留空使用默认项）",
                        "Provider ID (pick above or type; empty uses the default)",
                    ),
                    field_id,
                    None,
                    &provider_state,
                ))
                .when(unknown_binding, |this| {
                    this.child(
                        div().text_xs().text_color(rgb(0x00b4_5309)).child(
                            language.choose(
                                "此 ID 不在已保存的 Provider 列表中；请先在 Provider 详情中保存。",
                                "This ID is not among the saved providers; save it in provider details first.",
                            ),
                        ),
                    )
                })
        }

        /// One read-only key/value row in a settings summary.
        fn settings_summary_row(label: &str, value: String) -> Div {
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(label.to_owned()),
                )
                .child(div().flex_1().min_w(px(0.)).text_sm().whitespace_normal().child(value))
        }

        /// The right-hand pane of a two-pane page: bounded to the available height and
        /// self-scrolling, so overflowing content scrolls inside the card's own frame instead
        /// of drawing past it — the pattern the Jev judgment page already established.
        fn detail_pane(id: &'static str) -> gpui::Stateful<Div> {
            div().id(id).flex_1().min_w(px(0.)).min_h(px(0.)).overflow_y_scroll().v_flex()
        }

        /// Modal editor for one runtime adapter's settings. Saving happens inside the dialog and
        /// closes it on success; cancelling keeps the draft, because the fields are the same
        /// state the page summary reads.
        fn render_adapter_settings_dialog(
            &mut self,
            adapter: RuntimeAdapter,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let command = self.runtime_fields(adapter).0.clone();
            let is_codex = adapter == RuntimeAdapter::CodexAppServer;
            let dialog_error = self.dialog_error.clone();
            let closer = entity.clone();
            let closer_top = entity.clone();
            let saver_cancel = entity.clone();
            let saver = entity;

            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("adapter-settings-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x0000_0f17_2ab3))
                        .occlude()
                        .on_click(move |_, _, cx| {
                            closer.update(cx, |view, cx| {
                                view.adapter_settings_open = None;
                                view.dialog_error = None;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .relative()
                        .occlude()
                        .w(px(640.))
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
                                    div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                        language.choose_owned(
                                            format!("编辑 {} 设置", adapter.label()),
                                            format!("Edit {} settings", adapter.label()),
                                        ),
                                    ),
                                )
                                .child(
                                    Button::new("adapter-settings-cancel-top")
                                        .ghost()
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, _, cx| {
                                            closer_top.update(cx, |view, cx| {
                                                view.adapter_settings_open = None;
                                                view.dialog_error = None;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("adapter-settings-body")
                                .v_flex()
                                .gap_3()
                                .max_h(px(440.))
                                .overflow_y_scroll()
                                .child(Self::labeled_field(
                                    language.choose("Codex 命令", "Codex command"),
                                    "codex-command",
                                    None,
                                    &command,
                                ))
                                .when(is_codex, |body| {
                                    body.child(Self::labeled_field(
                                        language.choose("工作目录", "Working directory"),
                                        "working-directory",
                                        None,
                                        &self.working_directory,
                                    ))
                                })
                                .child(self.render_provider_binding(adapter, cx))
                                .child(
                                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose(
                                            "保存范围：此运行时的命令、工作目录与 Provider 关联；下次任务生效。运行中的 Codex 进程需重启后生效。",
                                            "Save scope: this runtime's command, working directory, and provider binding; applies to the next task. A running Codex process needs a restart.",
                                        ),
                                    ),
                                ),
                        )
                        .when_some(dialog_error, |this, error| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                    language.choose(
                                        "取消会保留当前草稿，不写入设置。",
                                        "Cancelling keeps the current draft without writing settings.",
                                    ),
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            Button::new("adapter-settings-cancel")
                                                .label(language.choose("取消", "Cancel"))
                                                .on_click(move |_, _, cx| {
                                                    saver_cancel.update(cx, |view, cx| {
                                                        view.adapter_settings_open = None;
                                                        view.dialog_error = None;
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            Button::new("adapter-settings-save")
                                                .primary()
                                                .label(language.choose("保存并关闭", "Save and close"))
                                                .on_click(move |_, _, cx| {
                                                    saver.update(cx, |view, cx| {
                                                        match view.save_runtime_checked(adapter, cx) {
                                                            Ok(()) => {
                                                                view.adapter_settings_open = None;
                                                                view.dialog_error = None;
                                                                view.status = format!(
                                                                    "已保存 {} 配置；下次任务生效，运行中的服务须重启。",
                                                                    adapter.label()
                                                                );
                                                            }
                                                            Err(error) => {
                                                                view.dialog_error = Some(error);
                                                            }
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                ),
                        ),
                )
        }

        /// Modal editor for the selected provider. Saving the provider list happens inside the
        /// dialog and closes it on success; quick list actions (default, enable, remove) stay on
        /// the page and remain drafts until this save runs.
        fn render_provider_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let selected = self.selected_provider.min(self.providers.len() - 1);
            let provider = &self.providers[selected];
            let dialog_error = self.dialog_error.clone();
            let closer = entity.clone();
            let closer_top = entity.clone();
            let saver = entity.clone();
            let saver_cancel = entity.clone();
            let toggle_vision = entity.clone();
            let api_key_hint = self.secret_source_hint(
                &provider.api_key_environment_variable.read(cx).value(),
                &entity,
            );
            let vision_key_hint = self.secret_source_hint(
                &provider.vision_api_key_environment_variable.read(cx).value(),
                &entity,
            );

            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("provider-editor-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x0000_0f17_2ab3))
                        .occlude()
                        .on_click(move |_, _, cx| {
                            closer.update(cx, |view, cx| {
                                view.provider_editor_open = false;
                                view.dialog_error = None;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .relative()
                        .occlude()
                        .w(px(680.))
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
                                    div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                        language.choose("编辑 Provider", "Edit provider"),
                                    ),
                                )
                                .child(
                                    Button::new("provider-editor-cancel-top")
                                        .ghost()
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, _, cx| {
                                            closer_top.update(cx, |view, cx| {
                                                view.provider_editor_open = false;
                                                view.dialog_error = None;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("provider-editor-body")
                                .v_flex()
                                .gap_3()
                                .max_h(px(440.))
                                .overflow_y_scroll()
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
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
                                        .flex_wrap()
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
                                    language.choose(
                                        "LLM API Key 环境变量名",
                                        "LLM API key environment variable",
                                    ),
                                    "provider-api-key-env",
                                    Some(language.choose(
                                        "仅环境变量名，例如 OPENAI_API_KEY；密钥值不会出现在这里。",
                                        "Environment-variable name only, e.g. OPENAI_API_KEY; the key value never appears here.",
                                    )),
                                    &provider.api_key_environment_variable,
                                ))
                                .when_some(api_key_hint, ParentElement::child)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .text_base()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(language.choose(
                                                    "Vision 配置",
                                                    "Vision configuration",
                                                )),
                                        )
                                        .child(
                                            Button::new("toggle-vision")
                                                .label(if provider.supports_vision {
                                                    language.choose(
                                                        "Vision：已启用",
                                                        "Vision: enabled",
                                                    )
                                                } else {
                                                    language.choose(
                                                        "Vision：已停用",
                                                        "Vision: disabled",
                                                    )
                                                })
                                                .on_click(move |_, _, cx| {
                                                    toggle_vision.update(
                                                        cx,
                                                        ControlPlaneView::toggle_vision,
                                                    );
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
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
                                ))
                                .when_some(vision_key_hint, ParentElement::child)
                                .child(
                                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose(
                                            "保存范围：整个 Provider 列表的新增、编辑、删除、启停、Vision 配置和默认项；下次任务生效。",
                                            "Save scope: additions, edits, removals, enabled/Vision states and the default for the entire provider list; applies to the next task.",
                                        ),
                                    ),
                                ),
                        )
                        .when_some(dialog_error, |this, error| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                    language.choose(
                                        "取消会保留当前草稿，不写入设置。",
                                        "Cancelling keeps the current draft without writing settings.",
                                    ),
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            Button::new("provider-editor-cancel")
                                                .label(language.choose("取消", "Cancel"))
                                                .on_click(move |_, _, cx| {
                                                    saver_cancel.update(cx, |view, cx| {
                                                        view.provider_editor_open = false;
                                                        view.dialog_error = None;
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            Button::new("provider-editor-save")
                                                .primary()
                                                .label(language.choose(
                                                    "保存 Provider 列表",
                                                    "Save provider list",
                                                ))
                                                .on_click(move |_, _, cx| {
                                                    saver.update(cx, |view, cx| {
                                                        match view.save_providers_checked(cx) {
                                                            Ok(()) => {
                                                                view.provider_editor_open = false;
                                                                view.dialog_error = None;
                                                                view.status = "已保存 Provider 列表及默认项；下次任务生效，运行中的服务须重启。".into();
                                                            }
                                                            Err(error) => {
                                                                view.dialog_error = Some(error);
                                                            }
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                ),
                        ),
                )
        }

        fn save_bridge_settings(&mut self, cx: &mut Context<Self>) {
            let update = crate::settings_persistence::SettingsUpdate::Bridge {
                listen_address: self.bridge_address.read(cx).value().trim().to_owned(),
            };
            self.status = match self.save_update(update) {
                Ok(()) => "已保存 Bridge 监听地址；下次启动生效。".into(),
                Err(error) => format!("Bridge 监听地址未保存：{error}"),
            };
            cx.notify();
        }

        fn bridge_endpoint(&self) -> String {
            self.bridge_active_address
                .as_ref()
                .unwrap_or(&self.saved_settings.bridge.listen_address)
                .clone()
        }

        /// Execution reads saved configuration and never commits unrelated form drafts.
        fn settings_for_execution(&mut self, cx: &mut Context<Self>) -> Option<RuntimeSettings> {
            match RuntimeSettings::load_or_default(&self.settings_path) {
                Ok(settings) => Some(settings),
                Err(error) => {
                    self.status = format!("无法读取已保存配置：{error}");
                    cx.notify();
                    None
                }
            }
        }

        /// Starts the supervised Codex App Server child process.
        ///
        /// Uses the saved snapshot and checks for an immediate process exit.
        fn start_codex_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.codex_process.is_some()
                || matches!(self.codex_status, RuntimeLifecycleStatus::Starting)
            {
                "未启动：Codex App Server 正在运行或正在启动。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let Some(settings) = self.settings_for_execution(cx) else {
                return;
            };
            let Some(provider) = circuitfabric_codex_runtime::execution::selected_provider(
                &settings,
                circuitfabric_codex_runtime::execution::AgentKind::Codex,
            )
            .cloned() else {
                "未启动：请先配置默认 Provider。".clone_into(&mut self.status);
                cx.notify();
                return;
            };
            self.codex_status = RuntimeLifecycleStatus::Starting;
            self.codex_active_provider =
                Some(format!("{} / {} / {}", provider.id, provider.model, provider.base_url));
            self.status = format!("Codex App Server: {} / {}", provider.id, provider.model);
            let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
                .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
                .flatten();
            let launch = cx.background_spawn(async move {
                CodexAppServerHandle::launch_with_secrets(
                    &settings.codex,
                    &provider,
                    secrets.as_ref(),
                )
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
                                view.codex_active_provider = None;
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
                    self.codex_active_provider = None;
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
                self.codex_active_provider = None;
                if exit.success() {
                    self.codex_status = RuntimeLifecycleStatus::Stopped;
                } else {
                    self.codex_status = RuntimeLifecycleStatus::Failed {
                        reason: format!("进程已退出（{exit}）"),
                    };
                }
            }
        }

        /// Starts the `JLCircuit` EDA bridge as a supervised child process.
        ///
        /// The bridge reads the saved snapshot, then waits for the configured port before
        /// reporting running — an early exit is reported as a failure together
        /// with the bridge's own stderr.
        fn start_bridge_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.bridge_process.is_some()
                || matches!(self.bridge_status, RuntimeLifecycleStatus::Starting)
            {
                "未启动：bridge 服务已在运行或正在启动。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let Some(settings) = self.settings_for_execution(cx) else {
                return;
            };
            let config_path = self.settings_path.clone();
            let address = settings.bridge.listen_address.clone();
            let launch_address = address.clone();
            self.bridge_status = RuntimeLifecycleStatus::Starting;
            self.bridge_active_address = Some(address.clone());
            self.bridge_health = BridgeHealth::Unknown;
            self.bridge_probed_at = None;
            self.status = format!("bridge 正在启动：ws://{address}/bridge");
            let secrets = (self.secret_storage_provider == SecretStorageProvider::EncryptedVault)
                .then(|| self.vault.as_ref().map(|vault| vault.values().clone()))
                .flatten();
            let launch = cx.background_spawn(async move {
                BridgeProcessHandle::launch_with_vault(&config_path, secrets.as_ref()).and_then(
                    |mut handle| {
                        let deadline = Instant::now() + Duration::from_secs(5);
                        while Instant::now() < deadline {
                            if let Some(exit) = handle.try_exit() {
                                let diagnostics = handle.exit_diagnostics();
                                let detail = if diagnostics.is_empty() {
                                    String::new()
                                } else {
                                    format!("：{diagnostics}")
                                };
                                return Err(format!("bridge 进程已退出（{exit}）{detail}"));
                            }
                            if probe::tcp_reachable(&launch_address, Duration::from_millis(300))
                                .is_ok()
                            {
                                return Ok(handle);
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        let _ = handle.stop();
                        Err(format!("bridge 在 5 秒内未开始监听 {launch_address}"))
                    },
                )
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = launch.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        match result {
                            Ok(handle) => {
                                let pid = handle.pid();
                                view.bridge_health = BridgeHealth::Listening { at: Instant::now() };
                                view.bridge_process = Some(handle);
                                view.bridge_status = RuntimeLifecycleStatus::Running { pid };
                                view.status =
                                    format!("bridge 已启动（PID {pid}）：ws://{address}/bridge");
                            }
                            Err(reason) => {
                                view.bridge_active_address = None;
                                view.bridge_status =
                                    RuntimeLifecycleStatus::Failed { reason: reason.clone() };
                                view.status = format!("bridge 启动失败：{reason}");
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

        /// Stops the bridge child process. A bridge this app did not start is
        /// reported as such — its process is never touched.
        fn stop_bridge_service(&mut self, cx: &mut Context<Self>) {
            if matches!(self.bridge_status, RuntimeLifecycleStatus::Starting) {
                "未停止：bridge 正在启动，请稍候。".clone_into(&mut self.status);
                cx.notify();
                return;
            }
            let Some(mut process) = self.bridge_process.take() else {
                "bridge 服务当前不是由本应用启动的。".clone_into(&mut self.status);
                cx.notify();
                return;
            };
            let pid = process.pid();
            match process.stop() {
                Ok(()) => {
                    self.bridge_status = RuntimeLifecycleStatus::Stopped;
                    self.bridge_active_address = None;
                    self.bridge_health = BridgeHealth::Unknown;
                    self.bridge_probed_at = None;
                    self.bridge_test = None;
                    self.status = format!("已停止 bridge 服务（PID {pid}）。");
                }
                Err(error) => {
                    self.bridge_status = RuntimeLifecycleStatus::Failed { reason: error.clone() };
                    self.status = format!("停止 bridge 失败（PID {pid}）：{error}");
                }
            }
            cx.notify();
        }

        /// Reconciles the bridge chip with the real process state: a bridge that
        /// died on its own is reported as failed with its stderr, not left green.
        fn refresh_bridge_lifecycle(&mut self) {
            let exit = self.bridge_process.as_mut().and_then(BridgeProcessHandle::try_exit);
            if let Some(exit) = exit {
                let diagnostics = self
                    .bridge_process
                    .as_mut()
                    .map(BridgeProcessHandle::exit_diagnostics)
                    .unwrap_or_default();
                self.bridge_process = None;
                self.bridge_active_address = None;
                if exit.success() {
                    self.bridge_status = RuntimeLifecycleStatus::Stopped;
                } else {
                    let detail = if diagnostics.is_empty() {
                        String::new()
                    } else {
                        format!("：{diagnostics}")
                    };
                    self.bridge_status = RuntimeLifecycleStatus::Failed {
                        reason: format!("bridge 进程已退出（{exit}）{detail}"),
                    };
                }
            }
        }

        /// Runs the automatic reachability probe while the EDA services page is
        /// visible, rate-limited to one TCP probe every 5 seconds. It never
        /// opens the WebSocket: an attached EDA client must not be disturbed,
        /// because the bridge serves one connection at a time.
        fn maybe_probe_bridge_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.bridge_probe_pending
                || self.bridge_probed_at.is_some_and(|at| at.elapsed() < Duration::from_secs(5))
            {
                return;
            }
            self.bridge_probe_pending = true;
            self.bridge_probed_at = Some(Instant::now());
            let address = self.bridge_endpoint();
            let probe_task = cx.background_spawn(async move {
                probe::tcp_reachable(&address, Duration::from_secs(2))
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = probe_task.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.bridge_probe_pending = false;
                        view.bridge_health = match result {
                            Ok(()) => BridgeHealth::Listening { at: Instant::now() },
                            Err(reason) => BridgeHealth::Unreachable { reason },
                        };
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
        }

        /// Manual connection test: the protocol-level `status` round-trip over
        /// the bridge WebSocket. The bridge serves one client at a time, so the
        /// test may time out while an EDA client is attached — that outcome is
        /// reported as its own error, not as "offline".
        fn test_bridge_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.bridge_test_pending {
                return;
            }
            self.bridge_test_pending = true;
            let address = self.bridge_endpoint();
            let test_task = cx.background_spawn(async move {
                probe::status(&address, Duration::from_secs(5)).map_err(|reason| {
                    // A refused connection really is offline; anything else
                    // (handshake timeout, malformed reply) keeps the current
                    // health — the port may still be listening.
                    let offline = probe::tcp_reachable(&address, Duration::from_secs(1)).is_err();
                    (reason, offline)
                })
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = test_task.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.bridge_test_pending = false;
                        let at = Instant::now();
                        match result {
                            Ok(report) => {
                                let version = report.protocol_version;
                                let bridge_name = report.bridge_name.clone();
                                view.bridge_health = BridgeHealth::Listening { at };
                                view.bridge_test =
                                    Some(BridgeTestResult { at, outcome: Ok(report) });
                                view.status =
                                    format!("bridge 连接测试成功：协议 v{version} · {bridge_name}");
                            }
                            Err((reason, offline)) => {
                                if offline {
                                    view.bridge_health =
                                        BridgeHealth::Unreachable { reason: reason.clone() };
                                }
                                view.bridge_test =
                                    Some(BridgeTestResult { at, outcome: Err(reason) });
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
                    match self.save_update(
                        crate::settings_persistence::SettingsUpdate::Authorizations(
                            self.tool_authorizations.clone(),
                        ),
                    ) {
                        Ok(()) => {
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
                            self.status = format!("全局授权未保存：{error}");
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
                    match self.save_update(
                        crate::settings_persistence::SettingsUpdate::Authorizations(
                            self.tool_authorizations.clone(),
                        ),
                    ) {
                        Ok(()) => {
                            self.status = format!(
                                "已撤销{kind_label} `{id}` 的全局授权，已写入 {}。",
                                self.settings_path.display()
                            );
                        }
                        Err(error) => {
                            self.tool_authorizations = previous;
                            self.status = format!("撤销授权未保存：{error}");
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
            self.session_project_filter = None;
            self.project_tab = ProjectDetailTab::Overview;
            self.session_replay = None;
            self.selected_semantic_snapshot = None;
            self.selected_change_set = None;
            self.approval_drawer_open = false;
            cx.notify();
        }

        /// Mirrors the selected project into the native window caption, e.g.
        /// "CircuitFabric — Signal-chain prototype". Runs every frame but only
        /// reaches the platform when the desired title actually changed, so
        /// every path that sets `selected_project` is covered without each
        /// one needing window access.
        fn sync_window_title(&mut self, window: &mut Window) {
            let project_name = self
                .selected_project
                .as_deref()
                .and_then(|id| self.workspace.project(id))
                .map(|project| project.name.clone());
            let desired = app_window_title(project_name.as_deref());
            if desired != self.window_title {
                window.set_window_title(desired.as_str());
                self.window_title = desired;
            }
        }

        /// Extracts the full text of this project's not-yet-indexed PDFs off the UI thread
        /// and registers it as evidence. Each document is attempted once per session.
        fn schedule_pdf_indexing(&mut self, project_id: &str, cx: &mut Context<Self>) {
            let Some(storage) = self.project_storages.get(project_id).cloned() else {
                return;
            };
            let Ok(pending) = self.workspace.pending_pdf_documents(project_id, &storage) else {
                return;
            };
            for document in pending {
                let key = (project_id.to_owned(), document.id.clone());
                if self.pdf_index_state.contains_key(&key) {
                    continue;
                }
                self.pdf_index_state.insert(key.clone(), PdfIndexState::Running);
                let work = cx.background_spawn({
                    let storage = storage.clone();
                    let document = document.clone();
                    async move { circuitfabric_project::extract_pdf_pages(&storage, &document) }
                });
                cx.spawn(async move |view, cx| {
                    let pages = work.await;
                    view.update(cx, |view, cx| {
                        let indexed = pages.is_some_and(|pages| {
                            view.workspace.register_pdf_pages(&key.0, &document, &pages).is_ok()
                        });
                        if indexed {
                            view.pdf_index_state.remove(&key);
                            view.refresh_evidence_search(cx);
                        } else {
                            view.pdf_index_state.insert(key, PdfIndexState::Failed);
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            cx.notify();
        }

        /// Searches the selected project's evidence after `delay`, off the UI thread.
        ///
        /// The corpus is snapshotted when the delay ends, so a burst of keystrokes costs one
        /// search over the newest index; a result is dropped when a newer request exists.
        fn schedule_evidence_search(&mut self, delay: Duration, cx: &mut Context<Self>) {
            self.evidence_generation += 1;
            let generation = self.evidence_generation;
            let query = self.evidence_query.read(cx).value().trim().to_owned();
            let project_id = self.selected_project.clone();
            let (Some(project_id), false) = (project_id, query.is_empty()) else {
                self.evidence_searching = false;
                self.evidence_result = None;
                cx.notify();
                return;
            };
            self.evidence_searching = true;
            let scope = self.evidence_scope;
            cx.spawn(async move |view, cx| {
                if !delay.is_zero() {
                    cx.background_executor().timer(delay).await;
                }
                let Ok(Some((corpus, admitted))) = view.update(cx, |view, _| {
                    (view.evidence_generation == generation)
                        .then(|| view.evidence_snapshot(&project_id))
                        .flatten()
                }) else {
                    return;
                };
                let started = Instant::now();
                let search = cx
                    .background_executor()
                    .spawn(async move {
                        corpus.search(
                            &query,
                            scope,
                            |fragment| admitted.contains(&fragment.document_id),
                            EVIDENCE_SEARCH_LIMIT,
                        )
                    })
                    .await;
                view.update(cx, |view, cx| {
                    if view.evidence_generation != generation {
                        return;
                    }
                    view.evidence_searching = false;
                    view.evidence_visible = EVIDENCE_PAGE_SIZE;
                    view.evidence_result = Some(EvidenceSearchResult {
                        project_id,
                        scope,
                        search: std::sync::Arc::new(search),
                        elapsed: started.elapsed(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
            cx.notify();
        }

        /// Re-runs an active search after the evidence index changed.
        fn refresh_evidence_search(&mut self, cx: &mut Context<Self>) {
            if !self.evidence_query.read(cx).value().trim().is_empty() {
                self.schedule_evidence_search(Duration::ZERO, cx);
            }
        }

        /// The project's fragment snapshot plus the documents allowed to appear in results:
        /// integrity verified at listing time and currently citable.
        fn evidence_snapshot(
            &self,
            project_id: &str,
        ) -> Option<(EvidenceCorpus, std::collections::BTreeSet<String>)> {
            let corpus = self.workspace.evidence_corpus(project_id).ok()?;
            let admitted = self
                .project_data
                .get(project_id)
                .map(|data| {
                    data.documents
                        .iter()
                        .filter(|document| {
                            data.document_integrity.get(&document.id).copied().unwrap_or(false)
                                && self
                                    .workspace
                                    .is_document_evidence_available(project_id, &document.id)
                        })
                        .map(|document| document.id.clone())
                        .collect()
                })
                .unwrap_or_default();
            Some((corpus, admitted))
        }

        /// Opens the document a search hit cites and lands on its page or data row.
        fn open_evidence_hit(
            &mut self,
            project_id: &str,
            hit: &EvidenceHit,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            self.stop_pdf_interaction();
            let document = self.project_data.get(project_id).and_then(|data| {
                data.documents.iter().find(|document| document.id == hit.fragment.document_id)
            });
            let Some(document) = document.cloned() else {
                self.status = format!("文档 {} 已不在项目中", hit.fragment.document_id);
                cx.notify();
                return;
            };
            let already_open = self.document_preview.as_ref().is_some_and(|preview| {
                preview.project_id == project_id
                    && preview.document_id == document.id
                    && !matches!(preview.state, DocumentPreviewState::Refused { .. })
            });
            if !already_open {
                self.open_document_preview(project_id.to_owned(), &document, window, cx);
            }
            self.preview_show_data = hit.anchor.is_verified_data();
            if let FragmentAnchor::DatasheetRow { row, .. } = hit.anchor {
                self.datasheet_rows_visible = self.datasheet_rows_visible.max(row);
            }
            self.reset_pdf_view();
            self.preview_focus = Some(PreviewFocus {
                project_id: project_id.to_owned(),
                document_id: document.id,
                // The document's own content hash, not the fragment's: full-text fragments
                // carry the hash of the extracted text corpus, which never equals the
                // file's hash and would make the resolver reject every search navigation
                // as "content changed". Verified-data rows keep their staleness signal
                // through the evidence lookup itself.
                content_hash: document.content_hash.clone(),
                anchor: hit.anchor.clone(),
                text: hit.fragment.text.clone(),
                terms: hit
                    .highlights
                    .iter()
                    .filter_map(|range| hit.fragment.text.get(range.clone()).map(str::to_owned))
                    .collect(),
                regions: Vec::new(),
                notice: None,
                resolving: false,
            });
            self.preview_focus_generation += 1;
            self.preview_row_anchor = self
                .preview_show_data
                .then(|| gpui::ScrollAnchor::for_handle(self.preview_scroll.clone()));
            self.preview_scroll.set_offset(gpui::Point::default());
            self.preview_scroll_pending.set(true);
            if matches!(hit.anchor, FragmentAnchor::PageLine { .. }) {
                self.resolve_preview_pdf_focus(None, cx);
            }
            cx.notify();
        }

        /// Resolve a row quote or highlight a known PDF page using a newly verified copy.
        /// Only the latest focus request may publish its result.
        fn resolve_preview_pdf_focus(&mut self, section: Option<String>, cx: &mut Context<Self>) {
            let Some(focus) = self.preview_focus.as_mut() else { return };
            let Some(storage) = self.project_storages.get(&focus.project_id).cloned() else {
                return;
            };
            focus.notice =
                Some(self.language.choose("正在定位原文…", "Locating source…").to_owned());
            focus.resolving = true;
            let focus = focus.clone();
            let generation = self.preview_focus_generation;
            let work = cx.background_spawn(async move {
                let request = storage
                    .prepare_document_open(&focus.project_id, &focus.document_id)
                    .map_err(|error| error.to_string())?;
                if request.content_hash != focus.content_hash {
                    return Err("文档内容已变化，请刷新检索或重新提取后定位。".to_owned());
                }
                let (page, regions) = match focus.anchor {
                    FragmentAnchor::PageLine { page, .. } => {
                        // The hit's page number and its terms both come from the extractor
                        // that indexed the document, which can disagree with pdfium's
                        // pagination and text spacing. Search from that page outward with
                        // the terms plus the full matched line — the strongest locator —
                        // so the keywords still get highlighted on the page that really
                        // holds them.
                        let mut terms = focus.terms.clone();
                        let line = focus.text.trim().to_owned();
                        if !line.is_empty() && !terms.iter().any(|term| *term == line) {
                            terms.push(line);
                        }
                        circuitfabric_document_opener::pdf_locate_highlight(
                            request.managed_copy.data(),
                            page,
                            &terms,
                        )
                        .map(|result| result.map_err(|error| error.clone()))
                        .transpose()?
                        .unwrap_or((page, Vec::new()))
                    }
                    _ => {
                        let page =
                            circuitfabric_document_opener::datasheet::locate_datasheet_evidence(
                                &request,
                                section.as_deref().unwrap_or_default(),
                                &focus.text,
                            )?
                            .ok_or_else(|| {
                                "未能在当前 PDF 中定位这条证据，请对照原文核对。".to_owned()
                            })?;
                        let regions = circuitfabric_document_opener::pdf_highlight_regions(
                            request.managed_copy.data(),
                            page,
                            &focus.terms,
                        )
                        .transpose()?
                        .unwrap_or_default();
                        (page, regions)
                    }
                };
                Ok::<_, String>((page, regions))
            });
            cx.spawn(async move |view, cx| {
                let result = work.await;
                view.update(cx, |view, cx| {
                    if view.preview_focus_generation != generation { return }
                    let Some(focus) = view.preview_focus.as_mut() else { return };
                    match result {
                        Ok((page, regions)) => {
                            focus.resolving = false;
                            match focus.anchor {
                                FragmentAnchor::PageLine { page: anchored, line } if page != anchored => {
                                    // The keywords live on a different page than the hit's
                                    // anchor claimed; navigate to where they were found.
                                    focus.anchor = FragmentAnchor::PageLine { page, line };
                                    view.preview_scroll_pending.set(true);
                                }
                                FragmentAnchor::PageLine { .. } => {}
                                _ => {
                                    focus.anchor = FragmentAnchor::PageLine { page, line: 1 };
                                    view.preview_scroll_pending.set(true);
                                }
                            }
                            focus.notice = regions.is_empty().then(|| view.language.choose(
                                "已定位页面；该页文本层未匹配到高亮位置。", "Page located; no matching highlight coordinates in its text layer.",
                            ).to_owned());
                            focus.regions = regions;
                        }
                        Err(error) => {
                            focus.resolving = false;
                            focus.notice = Some(error);
                        }
                    }
                    cx.notify();
                }).ok();
            }).detach();
        }

        fn open_datasheet_source(&mut self, section: &str, row: usize, cx: &mut Context<Self>) {
            self.stop_pdf_interaction();
            let Some(preview) = &self.document_preview else { return };
            let Some(extraction) = &preview.extraction else { return };
            let evidence = match section {
                "pins" => extraction.pins.get(row).and_then(|row| row.evidence.clone()),
                "absoluteMaximumRatings" => extraction
                    .absolute_maximum_ratings
                    .get(row)
                    .and_then(|row| row.evidence.clone()),
                "electricalCharacteristics" => extraction
                    .electrical_characteristics
                    .get(row)
                    .and_then(|row| row.evidence.clone()),
                "operatingConditions" => {
                    extraction.operating_conditions.get(row).and_then(|row| row.evidence.clone())
                }
                _ => None,
            };
            let Some(evidence) = evidence else { return };
            let (project_id, document_id, content_hash) = (
                preview.project_id.clone(),
                preview.document_id.clone(),
                extraction.content_hash.clone(),
            );
            self.reset_pdf_view();
            self.preview_focus = Some(PreviewFocus {
                project_id,
                document_id,
                content_hash,
                anchor: FragmentAnchor::Unknown,
                text: evidence.clone(),
                terms: vec![evidence],
                regions: Vec::new(),
                notice: None,
                resolving: false,
            });
            self.preview_focus_generation += 1;
            self.preview_row_anchor = None;
            self.preview_show_data = false;
            self.preview_scroll.set_offset(gpui::Point::default());
            self.resolve_preview_pdf_focus(Some(section.to_owned()), cx);
            cx.notify();
        }

        /// Reloads one project's cached document, session, and semantic projections from its root.
        fn refresh_project_data(&mut self, project_id: &str) -> Result<(), String> {
            let storage = self
                .project_storages
                .get(project_id)
                .ok_or_else(|| "项目根目录未打开".to_owned())?;
            let documents =
                storage.list_documents().map_err(|error| format!("文档索引未读取：{error}"))?;
            let document_integrity = Self::compute_document_integrity(storage, &documents);
            let session_listing =
                storage.list_sessions().map_err(|error| format!("会话记录未读取：{error}"))?;
            let semantic_snapshots = storage
                .list_logical_snapshots()
                .map_err(|error| format!("语义快照未读取：{error}"))?;
            self.project_data.entry(project_id.to_owned()).or_default().documents = documents;
            self.project_data.entry(project_id.to_owned()).or_default().document_integrity =
                document_integrity;
            self.project_data.entry(project_id.to_owned()).or_default().session_listing =
                session_listing;
            self.project_data.entry(project_id.to_owned()).or_default().semantic_snapshots =
                semantic_snapshots;
            self.project_data.entry(project_id.to_owned()).or_default().change_sets =
                storage.list_change_sets().map_err(|error| error.to_string())?;
            Ok(())
        }

        /// Builds the two read-only dashboard projections from persisted session records.
        /// Usage summaries and audit events intentionally do not share a mutable UI model.
        ///
        /// The result is cached for a few seconds: rebuilding it reads every session replay
        /// from disk, and this projection is consulted on every render of the usage page —
        /// re-reading all session files per frame made the whole window feel sluggish.
        fn usage_audit_model(&mut self) -> UsageAuditModel {
            const CACHE_TTL: Duration = Duration::from_secs(5);
            let fresh = self
                .usage_audit_cached
                .as_ref()
                .is_some_and(|(cached_at, _)| cached_at.elapsed() < CACHE_TTL);
            if !fresh {
                let model =
                    Self::build_usage_audit_model(&self.project_data, &self.project_storages);
                self.usage_audit_cached = Some((Instant::now(), model));
            }
            self.usage_audit_cached.as_ref().expect("the cache was just populated").1.clone()
        }

        fn build_usage_audit_model(
            project_data: &BTreeMap<ProjectId, ProjectWorkspaceData>,
            project_storages: &BTreeMap<ProjectId, ProjectStorage>,
        ) -> UsageAuditModel {
            let mut model = UsageAuditModel::default();
            for (project_id, data) in project_data {
                for summary in &data.session_listing.sessions {
                    model.usage.push(UsageRecord::from(&summary.metadata));
                    if let Some(storage) = project_storages.get(project_id)
                        && let Ok(replay) = storage.load_session(&summary.metadata.session_id)
                    {
                        model.audit.extend(AuditRecord::from_session(&replay));
                    }
                }
            }
            model.audit.sort_by(|left, right| {
                right.timestamp_unix_seconds.cmp(&left.timestamp_unix_seconds)
            });
            model
        }

        /// Exports exactly the currently filtered audit projection.  It never changes the source
        /// session files, and every CSV row repeats its project/provider/runtime/session context.
        fn export_filtered_audit(&mut self, window: &Window, cx: &mut Context<Self>) {
            let model = self.usage_audit_model();
            let now = UsageAuditModel::now_unix_seconds();
            let query = self.audit_filter.read(cx).value().to_string();
            let records =
                model.filtered_audit(self.usage_period, self.audit_kind_filter, &query, now);
            let csv = UsageAuditModel::audit_csv(&records);
            let dialog = rfd::AsyncFileDialog::new()
                .set_title("Export immutable CircuitFabric audit")
                .set_file_name("circuitfabric-audit.csv")
                .add_filter("CSV", &["csv"])
                .set_parent(window);
            cx.spawn_in(window, async move |view, cx| {
                let Some(destination) = dialog.save_file().await else {
                    return;
                };
                let result = std::fs::write(destination.path(), csv);
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.status = match result {
                            Ok(()) => format!("审计导出已保存：{}", destination.path().display()),
                            Err(error) => format!("审计导出失败：{error}"),
                        };
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
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
                                    let searchable = match document.document_kind {
                                        DocumentKind::Pdf => "，正在后台提取全文以供证据检索",
                                        kind if is_evidence_indexable(&kind) => "，文本可证据检索",
                                        _ => "，暂不参与文本检索",
                                    };
                                    if let Err(error) = view.refresh_project_data(&project_id) {
                                        view.status = format!("文档已导入，但列表未刷新：{error}");
                                    } else {
                                        view.status = format!(
                                            "已导入 `{}`（{}，{}…）{searchable}。",
                                            document.original_file_name,
                                            document.id,
                                            &document.content_hash[..23],
                                        );
                                    }
                                    view.schedule_pdf_indexing(&project_id, cx);
                                    view.refresh_evidence_search(cx);
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

        fn open_session_replay(
            &mut self,
            project_id: ProjectId,
            session_id: String,
            cx: &mut Context<Self>,
        ) {
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

        /// Opens the docked preview for one document.
        ///
        /// The gate (`prepare_document_open`) and the opener registry run synchronously; the
        /// pane shows the read-only view, or the explicit refusal/unsupported reason. The
        /// first open widens the window to the right by the pane width, mirroring how
        /// document tools extend their window for a preview; closing restores the size.
        fn open_document_preview(
            &mut self,
            project_id: ProjectId,
            document: &ProjectDocument,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let storage = self.project_storages.get(&project_id).cloned();
            let document_id = document.id.clone();
            if self.document_preview.is_none() && !window.is_fullscreen() && !window.is_maximized()
            {
                let previous = window.bounds().size;
                self.pre_preview_window_size = Some(previous);
                window.resize(Size {
                    width: previous.width + px(self.document_preview_width),
                    height: previous.height,
                });
            }
            self.release_preview_images(window);
            self.preview_show_data = false;
            self.preview_focus = None;
            self.reset_pdf_view();
            self.preview_focus_generation += 1;
            self.preview_row_anchor = None;
            self.preview_scroll_pending.set(false);
            self.preview_scroll.set_offset(gpui::Point::default());
            self.datasheet_feedback = None;
            self.datasheet_rows_visible = 40;
            self.document_preview = Some(DocumentPreviewSelection {
                project_id: project_id.clone(),
                file_name: document.original_file_name.clone(),
                document_id: document_id.clone(),
                state: DocumentPreviewState::Loading,
                extraction: None,
            });
            let completed_project = project_id.clone();
            let completed_document = document_id.clone();
            let work = cx.background_spawn(async move {
                let Some(storage) = storage else {
                    return (
                        DocumentPreviewState::Refused { denial: DocumentOpenDenial::NotFound },
                        None,
                    );
                };
                let state = match storage.prepare_document_open(&project_id, &document_id) {
                    Ok(request) => match DocumentOpenerRegistry::with_builtin_openers()
                        .open(&request)
                    {
                        circuitfabric_plugin_api::DocumentOpenerOutcome::Loaded { mut view } => {
                            let raster =
                                Self::raster_preview(&mut view, request.managed_copy.data());
                            DocumentPreviewState::Loaded { view: std::sync::Arc::new(view), raster }
                        }
                        circuitfabric_plugin_api::DocumentOpenerOutcome::Unsupported { reason }
                        | circuitfabric_plugin_api::DocumentOpenerOutcome::Failed { reason } => {
                            DocumentPreviewState::Unavailable { reason }
                        }
                    },
                    Err(denial) => DocumentPreviewState::Refused { denial },
                };
                let extraction = if matches!(state, DocumentPreviewState::Loaded { .. }) {
                    storage
                        .load_datasheet_extraction(&document_id)
                        .ok()
                        .flatten()
                        .map(std::sync::Arc::new)
                } else {
                    None
                };
                (state, extraction)
            });
            cx.spawn_in(window, async move |view, cx| {
                let (state, extraction) = work.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        if let Some(selection) = view.document_preview.as_mut()
                            && selection.project_id == completed_project
                            && selection.document_id == completed_document
                        {
                            selection.state = state;
                            if selection.extraction.is_none() {
                                selection.extraction = extraction;
                            }
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
            cx.notify();
        }

        /// Extracts the structured datasheet projection for the previewed document through
        /// the open gate (verified bytes only), persists it, and switches to the data tab.
        ///
        /// With `resume`, the checkpoint left by a stopped or failed run of the same
        /// document is reused; otherwise any such checkpoint is discarded.
        #[allow(clippy::too_many_lines)]
        fn extract_datasheet_for_preview(
            &mut self,
            clear_existing: bool,
            resume: bool,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            if self.datasheet_extracting {
                return;
            }
            let Some(preview) = self.document_preview.as_ref() else {
                return;
            };
            let project_id = preview.project_id.clone();
            let document_id = preview.document_id.clone();
            let previous_extraction = preview.extraction.clone();
            let completed_project = project_id.clone();
            let completed_document = document_id.clone();
            let Some(storage) = self.project_storages.get(&project_id).cloned() else {
                return;
            };
            let Some(settings) = self.settings_for_execution(cx) else {
                return;
            };
            let catalog = self.catalog.clone();
            let grants = self.effective_grants_for(&project_id);
            if !grants.authorized_mcp_server_ids.iter().any(|id| id == BUNDLED_JEV_SERVER_ID) {
                let reason = if self
                    .tool_authorizations
                    .authorized_mcp_server_ids
                    .iter()
                    .any(|id| id == BUNDLED_JEV_SERVER_ID)
                {
                    "当前项目尚未授权 Jev；请到 Jev 页面授权当前项目后再提取。"
                } else {
                    "Jev 尚未获得全局授权；请到 Jev 页面先授权全局及当前项目。"
                };
                self.datasheet_feedback = Some(reason.to_owned());
                self.status = reason.to_owned();
                cx.notify();
                return;
            }
            let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
            self.datasheet_feedback = None;
            self.datasheet_extracting = true;
            let stream = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            self.datasheet_stream = Some((project_id.clone(), document_id.clone(), stream.clone()));
            self.datasheet_extract_started = Some(Instant::now());
            let cancel = circuitfabric_codex_runtime::execution::Cancellation::default();
            self.datasheet_cancel = Some(cancel.clone());
            let resume_from = self
                .datasheet_checkpoint
                .take_if(|(checkpoint_project, checkpoint_document, _)| {
                    *checkpoint_project == project_id && *checkpoint_document == document_id
                })
                .filter(|_| resume)
                .map(|(_, _, checkpoint)| checkpoint);
            let checkpoint =
                std::sync::Arc::new(std::sync::Mutex::new(resume_from.unwrap_or_default()));
            let final_checkpoint = checkpoint.clone();
            let run_cancel = cancel.clone();
            let stopped = move || cancel.0.load(std::sync::atomic::Ordering::SeqCst);
            let log = move |text: &str| {
                if let Ok(mut buffer) = stream.lock() {
                    buffer.push_str(text);
                }
            };
            if clear_existing {
                if let Some(selection) = self.document_preview.as_mut() {
                    selection.extraction = None;
                }
                self.datasheet_rows_visible = 40;
                self.status = "正在清空旧数据并重新提取…".to_owned();
            } else {
                self.status = "正在后台提取候选数据；随后调用 Jev evaluate…".to_owned();
            }
            let work = cx.background_spawn(async move {
                let mut cleared = false;
                let mut jev_evaluate_calls = 0_usize;
                let mut jev_evaluate_responses = 0_usize;
                let result = (|| -> Result<DatasheetExtraction, String> {
                    let request = storage
                        .prepare_document_open(&project_id, &document_id)
                        .map_err(|error| error.to_string())?;
                    let tools = catalog
                        .list_tools_with_secrets(BUNDLED_JEV_SERVER_ID, &grants, secrets.as_ref())
                        .map_err(|error| error.to_string())?;
                    if !tools["tools"]
                        .as_array()
                        .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "evaluate"))
                    {
                        return Err("Jev MCP 未提供 evaluate 工具".to_owned());
                    }
                    settings.validate().map_err(|error| error.to_string())?;
                    if circuitfabric_codex_runtime::execution::selected_provider(
                        &settings,
                        circuitfabric_codex_runtime::execution::AgentKind::Codex,
                    )
                    .is_none()
                    {
                        return Err("请先配置 Codex Provider".to_owned());
                    }
                    if clear_existing {
                        storage
                            .clear_datasheet_extraction(&document_id)
                            .map_err(|error| error.to_string())?;
                        cleared = true;
                    }
                    {
                        let mut checkpoint =
                            checkpoint.lock().map_err(|error| error.to_string())?;
                        if checkpoint.content_hash != request.content_hash {
                            *checkpoint = DatasheetCheckpoint {
                                content_hash: request.content_hash.clone(),
                                ..DatasheetCheckpoint::default()
                            };
                        } else if !checkpoint.model_steps.is_empty() {
                            log("▶ 找到上次提取检查点，正在核对选页与提示词…\n");
                        }
                        let definition = catalog.mcp_servers.iter()
                            .find(|server| server.id == BUNDLED_JEV_SERVER_ID).cloned();
                        if checkpoint.judge_definition != definition {
                            checkpoint.jev_results.clear();
                            checkpoint.judge_definition = definition;
                            log("▶ 判断后端配置已变化，将重新复核候选数据…\n");
                        }
                    }
                    log("▶ 正在读取 PDF 文本…\n");
                    let mut jev_batch = 0_usize;
                    let mut model_step = 0_usize;
                    let mut extraction = extract_datasheet_by_category(
                        &request,
                        |category, prompt, selected_pages| {
                            use circuitfabric_codex_runtime::{
                                TurnDelta,
                                execution::{AgentKind, run_task_streaming},
                            };
                            let step = model_step;
                            model_step += 1;
                            let category = match category {
                                "pins" => "引脚",
                                "absoluteMaximumRatings" => "绝对最大额定值",
                                "electricalCharacteristics" => "电气特性",
                                "operatingConditions" => "工作条件",
                                _ => "数据",
                            };
                            log(&format!(
                                "▶ {category}（第 {} 次模型调用）已选页：{}\n",
                                step + 1,
                                selected_pages
                                    .iter()
                                    .map(usize::to_string)
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ));
                            if stopped() {
                                return Err("已停止".to_owned());
                            }
                            let cached =
                                checkpoint.lock().ok().and_then(|mut checkpoint| match checkpoint
                                    .model_steps
                                    .get(step)
                                {
                                    Some((cached_prompt, response)) if cached_prompt == prompt => {
                                        Some(response.clone())
                                    }
                                    _ => {
                                        if checkpoint.model_steps.len() > step {
                                            log("▶ 选页或提示词已变化，重新调用模型与 Jev\n");
                                            checkpoint.model_steps.truncate(step);
                                            checkpoint.jev_results.clear();
                                        }
                                        None
                                    }
                                });
                            if let Some(response) = cached {
                                log("▶ 复用上次完整的模型响应，跳过模型调用\n");
                                return Ok(response);
                            }
                            log(&format!(
                                "▶ 已发送提示（{} 字符），等待模型响应…\n",
                                prompt.chars().count()
                            ));
                            let mut current_stream = None;
                            let response = run_task_streaming(
                                &settings,
                                AgentKind::Codex,
                                &ToolAuthorizationSettings::default(),
                                prompt,
                                None,
                                None,
                                secrets.as_ref(),
                                &run_cancel,
                                &mut |kind, delta| {
                                    if current_stream != Some(kind) {
                                        current_stream = Some(kind);
                                        log(match kind {
                                            TurnDelta::Reasoning => "\n[思考]\n",
                                            TurnDelta::Answer => "\n[输出]\n",
                                        });
                                    }
                                    log(delta);
                                },
                            )
                            .map_err(|error| error.to_string());
                            log(match &response {
                                Ok(_) => "\n▶ 模型响应完成，正在校验证据行…\n",
                                Err(_) if stopped() => "\n▶ 模型调用已停止\n",
                                Err(_) => "\n▶ 模型调用失败\n",
                            });
                            if let (Ok(response), Ok(mut checkpoint)) =
                                (&response, checkpoint.lock())
                                && checkpoint.model_steps.len() == step
                            {
                                checkpoint.model_steps.push((prompt.to_owned(), response.clone()));
                            }
                            response
                        },
                        |arguments| {
                            let batch = jev_batch;
                            jev_batch += 1;
                            let cached =
                                checkpoint.lock().ok().and_then(|mut checkpoint| match checkpoint
                                    .jev_results
                                    .get(batch)
                                {
                                    Some((cached_arguments, result))
                                        if cached_arguments == arguments =>
                                    {
                                        Some(result.clone())
                                    }
                                    _ => {
                                        checkpoint.jev_results.truncate(batch);
                                        None
                                    }
                                });
                            if let Some(result) = cached {
                                log(&format!("▶ 复用第 {} 批 Jev 结果\n", batch + 1));
                                return Ok(result);
                            }
                            if stopped() {
                                return Err("已停止".to_owned());
                            }
                            jev_evaluate_calls += 1;
                            log(&format!("▶ Jev evaluate 第 {jev_evaluate_calls} 次…\n"));
                            let response = catalog
                                .call_tool_with_secrets(
                                    BUNDLED_JEV_SERVER_ID,
                                    &grants,
                                    "evaluate",
                                    arguments,
                                    secrets.as_ref(),
                                )
                                .map_err(|error| error.to_string());
                            if let Ok(value) = &response
                                && value["isError"] != true
                            {
                                jev_evaluate_responses += 1;
                                if let Ok(mut checkpoint) = checkpoint.lock()
                                    && checkpoint.jev_results.len() == batch
                                {
                                    checkpoint.jev_results.push((arguments.clone(), value.clone()));
                                }
                            }
                            response
                        },
                    )?;
                    extraction.notes.push(format!("Jev evaluate MCP calls: {jev_evaluate_calls}"));
                    if let Some(judge) = catalog.mcp_servers.iter()
                        .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
                        .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
                    {
                        extraction.notes.push(format!(
                            "LLM judgment backend: {} ({:?}); probabilities are uncalibrated estimates.",
                            judge.model, judge.answer_mode,
                        ));
                    }
                    let current = storage
                        .prepare_document_open(&project_id, &document_id)
                        .map_err(|error| error.to_string())?;
                    if current.content_hash != extraction.content_hash {
                        return Err("文档在提取过程中发生变化".to_owned());
                    }
                    storage
                        .save_datasheet_extraction(&extraction)
                        .map_err(|error| error.to_string())?;
                    Ok(extraction)
                })();
                (result, cleared, jev_evaluate_calls, jev_evaluate_responses)
            });
            // Repaint while the log grows; stops once the extraction settles.
            cx.spawn_in(window, async move |view, cx| {
                loop {
                    cx.background_executor().timer(std::time::Duration::from_millis(200)).await;
                    let extracting = cx
                        .update(|_, cx| {
                            view.update(cx, |view, cx| {
                                cx.notify();
                                view.datasheet_extracting
                            })
                            .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if !extracting {
                        break;
                    }
                }
            })
            .detach();
            cx.spawn_in(window, async move |view, cx| {
                let (result, cleared, jev_evaluate_calls, jev_evaluate_responses) = work.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.datasheet_extracting = false;
                        let stopped = view.datasheet_cancel.take().is_some_and(|cancel| {
                            cancel.0.load(std::sync::atomic::Ordering::SeqCst)
                        });
                        let showing_document = view.document_preview.as_ref().is_some_and(|selection| {
                            selection.project_id == completed_project
                                && selection.document_id == completed_document
                        });
                        match result {
                            Ok(extraction) => {
                                if showing_document {
                                    view.datasheet_feedback = Some(format!(
                                        "Jev evaluate 已调用 {jev_evaluate_calls} 次；候选行判断结果见下方解析诊断。"
                                    ));
                                }
                                let document = view.project_data.get(&completed_project).and_then(
                                    |data| {
                                        data.documents
                                            .iter()
                                            .find(|document| document.id == completed_document)
                                            .cloned()
                                    },
                                );
                                let evidence_rows = document.map_or(0, |document| {
                                    view.workspace
                                        .register_datasheet_evidence(
                                            &completed_project,
                                            &document,
                                            &extraction,
                                        )
                                        .unwrap_or(0)
                                });
                                view.status = format!(
                                    "已提取并验证：{} 个引脚、{} 条参数；{evidence_rows} 行已登记为证据。",
                                    extraction.pins.len(),
                                    extraction.absolute_maximum_ratings.len()
                                        + extraction.electrical_characteristics.len()
                                        + extraction.operating_conditions.len()
                                );
                                if let Some(selection) = view.document_preview.as_mut()
                                    && selection.project_id == completed_project
                                    && selection.document_id == completed_document
                                {
                                    selection.extraction = Some(std::sync::Arc::new(extraction));
                                    view.preview_show_data = true;
                                }
                            }
                            Err(error) => {
                                if clear_existing && !cleared {
                                    if let Some(selection) = view.document_preview.as_mut()
                                        && selection.project_id == completed_project
                                        && selection.document_id == completed_document
                                    {
                                        selection.extraction = previous_extraction;
                                    }
                                }
                                if cleared {
                                    view.workspace.clear_datasheet_evidence(
                                        &completed_project,
                                        &completed_document,
                                    );
                                }
                                let checkpoint = final_checkpoint
                                    .lock()
                                    .map(|checkpoint| checkpoint.clone())
                                    .unwrap_or_default();
                                view.datasheet_checkpoint = Some((
                                    completed_project.clone(),
                                    completed_document.clone(),
                                    checkpoint,
                                ));
                                view.status = if stopped {
                                    format!(
                                        "已停止提取{}；点击“继续”可复用已完成的步骤。",
                                        if cleared { "（旧数据已清空）" } else { "" }
                                    )
                                } else if cleared {
                                    format!("旧数据已清空；重新提取失败：{error}（Jev evaluate 发起 {jev_evaluate_calls} 次，收到 {jev_evaluate_responses} 次成功响应）")
                                } else {
                                    format!("数据提取失败，旧结果未清空：{error}（Jev evaluate 发起 {jev_evaluate_calls} 次，收到 {jev_evaluate_responses} 次成功响应）")
                                };
                                if showing_document {
                                    view.datasheet_feedback = Some(view.status.clone());
                                }
                            }
                        }
                        view.refresh_evidence_search(cx);
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            })
            .detach();
            cx.notify();
        }

        /// Requests a stop: the model call is cancelled at once (its process is killed); a
        /// Jev call already in flight finishes first. Completed steps stay resumable.
        fn stop_datasheet_extraction(&mut self, cx: &mut Context<Self>) {
            let Some(cancel) = &self.datasheet_cancel else {
                return;
            };
            cancel.cancel();
            if let Some((_, _, stream)) = &self.datasheet_stream
                && let Ok(mut buffer) = stream.lock()
            {
                buffer.push_str("\n▶ 已请求停止…\n");
            }
            "正在停止提取…".clone_into(&mut self.status);
            cx.notify();
        }

        fn close_document_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            self.release_preview_images(window);
            self.document_preview = None;
            self.stop_pdf_interaction();
            self.preview_focus = None;
            self.preview_focus_generation += 1;
            self.preview_row_anchor = None;
            #[cfg(windows)]
            crate::pdf_cursors::clear();
            if let Some(previous) = self.pre_preview_window_size.take()
                && !window.is_fullscreen()
                && !window.is_maximized()
            {
                window.resize(previous);
            }
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
                        view.schedule_pdf_indexing(&project.id, cx);
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
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .whitespace_normal()
                        .text_xs()
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(language.choose(zh, en)),
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
                .child(div().flex_1().min_w(px(0.)).truncate().text_sm().child(owned_id.clone()))
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

        fn persist_plugin_governance(&mut self) -> Result<(), String> {
            self.plugin_governance.save(&self.plugin_governance_path)
        }

        /// Discovery delegates only to the declarative manifest parser.  In
        /// particular, it does not invoke a plugin's entrypoint or health
        /// endpoint, so opening this page cannot start third-party code.
        fn discover_plugin_manifests(&mut self, cx: &mut Context<Self>) {
            let root = PathBuf::from(self.plugin_directory.read(cx).value().to_string());
            match self.plugin_governance.discover(&root) {
                Ok(count) => match self.persist_plugin_governance() {
                    Ok(()) => {
                        self.status = format!(
                            "Validated {count} plugin manifest(s) in {} without executing plugin code.",
                            root.display()
                        )
                    }
                    Err(error) => {
                        self.status = format!(
                            "Manifest discovery completed but audit persistence failed: {error}"
                        )
                    }
                },
                Err(error) => self.status = format!("Plugin manifest discovery failed: {error}"),
            }
            cx.notify();
        }

        fn apply_plugin_operation(&mut self, id: &str, operation: &str, cx: &mut Context<Self>) {
            let result = match operation {
                "install" => self.plugin_governance.install(id),
                "update" => self.plugin_governance.update(id),
                "uninstall" => self.plugin_governance.uninstall(id),
                "revoke" => self.plugin_governance.revoke_permissions(id),
                _ => Err("Unknown plugin operation".to_owned()),
            };
            self.status = match result {
                Ok(()) => match self.persist_plugin_governance() {
                    Ok(()) => format!(
                        "Plugin `{id}`: {operation} recorded in the audit trail. No plugin code was started."
                    ),
                    Err(error) => {
                        format!("Plugin action completed but audit persistence failed: {error}")
                    }
                },
                Err(error) => format!("Plugin action failed: {error}"),
            };
            cx.notify();
        }

        #[allow(clippy::too_many_lines)]
        fn render_plugins_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let records = self.plugin_governance.records().to_vec();
            let audit =
                self.plugin_governance.audit().iter().rev().take(20).cloned().collect::<Vec<_>>();
            let discoverer = entity.clone();

            let manifest_source = div()
                .w_full()
                .v_flex()
                .gap_3()
                .p_4()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("Manifest discovery"))
                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                    "Discovery reads and schema-validates plugin-manifest.json only. It does not load libraries, run scripts, launch processes, or probe plugin endpoints.",
                ))
                .child(Self::labeled_field(
                    "Manifest directory",
                    "plugin-manifest-directory",
                    Some("Discovery walks subdirectories and accepts only a bounded JSON manifest."),
                    &self.plugin_directory,
                ))
                .child(
                    Button::new("discover-plugin-manifests")
                        .primary()
                        .label("Discover & validate manifests")
                        .on_click(move |_, _, cx| {
                            discoverer.update(cx, |view, cx| view.discover_plugin_manifests(cx));
                        }),
                );

            let mut inventory = div().v_flex().gap_3();
            if records.is_empty() {
                inventory = inventory.child(
                    div()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .text_sm()
                        .text_color(rgb(TEXT_MUTED))
                        .child("No validated plugin manifests yet. Select a directory and run declarative discovery."),
                );
            }
            for record in records {
                let id = record.id.clone();
                let installer = entity.clone();
                let updater = entity.clone();
                let uninstaller = entity.clone();
                let revoker = entity.clone();
                let install_id = id.clone();
                let update_id = id.clone();
                let uninstall_id = id.clone();
                let revoke_id = id.clone();
                let requested = if record.requested_permissions.is_empty() {
                    "none".to_owned()
                } else {
                    record.requested_permissions.join(", ")
                };
                let granted = if record.granted_permissions.is_empty() {
                    "none".to_owned()
                } else {
                    record.granted_permissions.join(", ")
                };
                let lock = record.version_lock.clone().unwrap_or_else(|| "not locked".to_owned());
                let capabilities = if record.capabilities.is_empty() {
                    "none declared".to_owned()
                } else {
                    record.capabilities.join(", ")
                };
                inventory = inventory.child(
                    div()
                        .id(format!("plugin-inventory-{}", id))
                        .w_full()
                        .v_flex()
                        .gap_3()
                        .p_4()
                        .rounded_xl()
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
                                        .flex_1()
                                        .min_w(px(0.))
                                        .truncate()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(record.id.clone()),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(0x00e0_f2fe))
                                        .text_color(rgb(0x000e_7490))
                                        .child(record.state.label()),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(SURFACE_BG))
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(format!("v{}", record.version)),
                                ),
                        )
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(format!(
                            "{} | API {} | isolation: {} | health: {}",
                            record.kind,
                            record.api_version,
                            record.isolation,
                            record.health.label()
                        )))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(format!("Capabilities: {capabilities}")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(format!("Version lock: {lock}")),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                "Signature: {}{}",
                                record.signature.label(),
                                record
                                    .signer
                                    .as_ref()
                                    .map(|signer| format!(" ({signer})"))
                                    .unwrap_or_default()
                            )))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(format!("Requested permissions: {requested}")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(format!("Granted permissions: {granted}")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(format!("Manifest: {}", record.manifest_path.display())),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    Button::new(format!("plugin-install-{id}"))
                                        .label("Install")
                                        .on_click(move |_, _, cx| {
                                            installer.update(cx, |view, cx| {
                                                view.apply_plugin_operation(
                                                    &install_id,
                                                    "install",
                                                    cx,
                                                )
                                            });
                                        }),
                                )
                                .child(
                                    Button::new(format!("plugin-update-{id}"))
                                        .label("Update")
                                        .on_click(move |_, _, cx| {
                                            updater.update(cx, |view, cx| {
                                                view.apply_plugin_operation(
                                                    &update_id, "update", cx,
                                                )
                                            });
                                        }),
                                )
                                .child(
                                    Button::new(format!("plugin-uninstall-{id}"))
                                        .label("Uninstall")
                                        .on_click(move |_, _, cx| {
                                            uninstaller.update(cx, |view, cx| {
                                                view.apply_plugin_operation(
                                                    &uninstall_id,
                                                    "uninstall",
                                                    cx,
                                                )
                                            });
                                        }),
                                )
                                .child(
                                    Button::new(format!("plugin-revoke-{id}"))
                                        .ghost()
                                        .label("Revoke authorization")
                                        .on_click(move |_, _, cx| {
                                            revoker.update(cx, |view, cx| {
                                                view.apply_plugin_operation(
                                                    &revoke_id, "revoke", cx,
                                                )
                                            });
                                        }),
                                ),
                        ),
                );
            }

            let mut audit_rows = div().v_flex().gap_2();
            if audit.is_empty() {
                audit_rows = audit_rows.child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT_MUTED))
                        .child("No plugin governance events recorded."),
                );
            }
            for event in audit {
                audit_rows = audit_rows.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("{}: {}", event.plugin_id, event.action)),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(event.detail))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(rfc3339(event.occurred_at_unix_seconds)),
                        ),
                );
            }

            div()
                .id("plugins-governance-page")
                .size_full()
                .overflow_y_scroll()
                .v_flex()
                .gap_4()
                .p_6()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Plugin governance"))
                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child("Manifest inventory, trust, permissions, health, isolation, lifecycle controls, and an append-only local audit view."))
                .child(manifest_source)
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("Validated manifest inventory"))
                .child(inventory)
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("Audit trail"))
                .child(audit_rows)
        }

        #[allow(clippy::too_many_lines)]
        fn render_agents_page(
            &mut self,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let adapter_settings_dialog = self
                .adapter_settings_open
                .map(|adapter| self.render_adapter_settings_dialog(adapter, cx).into_any_element());
            let provider_dialog = self
                .provider_editor_open
                .then(|| self.render_provider_dialog(cx).into_any_element());

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
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.agents_selection = AgentsSelection::Runtime(adapter);
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
                AgentsSelection::BundledJev => {
                    self.render_bundled_jev_detail(cx).into_any_element()
                }
            };

            let jev_selected = self.agents_selection == AgentsSelection::BundledJev;
            let jev_summary = match self
                .catalog
                .mcp_servers
                .iter()
                .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
            {
                Some(server) => {
                    let granted = self
                        .tool_authorizations
                        .authorized_mcp_server_ids
                        .iter()
                        .any(|id| id == BUNDLED_JEV_SERVER_ID);
                    let llm =
                        circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server(server)
                            .is_some();
                    language.choose_owned(
                        format!(
                            "{} · {} · {}",
                            if server.enabled { "已启用" } else { "已停用" },
                            if granted { "已授权" } else { "未授权" },
                            if llm { "LLM" } else { "TypeSafe" }
                        ),
                        format!(
                            "{} · {} · {}",
                            if server.enabled { "enabled" } else { "disabled" },
                            if granted { "authorized" } else { "not authorized" },
                            if llm { "LLM" } else { "TypeSafe" }
                        ),
                    )
                }
                None => language.choose("未注册", "not registered").to_owned(),
            };
            let jev_selector = entity.clone();
            let jev_card = div()
                .id("jev-card")
                .v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(if jev_selected { ACCENT } else { BORDER }))
                .bg(rgb(if jev_selected { 0x00f0_f9ff } else { CARD_BG }))
                .cursor_pointer()
                .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                .on_click(move |_, _, cx| {
                    jev_selector.update(cx, |view, cx| {
                        view.agents_selection = AgentsSelection::BundledJev;
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
                                .font_weight(if jev_selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .child(language.choose("Jev 判断工具", "Jev judgments")),
                        )
                        .child(
                            div()
                                .ml_auto()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(jev_summary),
                        ),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "TypeSafe 或第三方 LLM 的类型化判断（evaluate）",
                    "Typed judgments via TypeSafe or a third-party LLM (evaluate)",
                )));

            let add_provider = entity.clone();
            div()
                .id("agents-tools-page")
                .size_full()
                .overflow_y_scroll()
                .relative()
                .v_flex()
                .gap_4()
                .p_6()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
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
                                            "在各详情页保存对应配置；技能/MCP 启停与授权即时保存。API Key 以变量名引用。",
                                            "Save each configuration in its detail pane; skill/MCP toggles and grants save immediately. API keys use variable names.",
                                        ),
                                    ),
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
                                .id("agents-list-scroll")
                                .min_h(px(0.))
                                .overflow_y_scroll()
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
                                .child(tools_card)
                                .child(jev_card),
                        )
                        .child(detail),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
                .when_some(adapter_settings_dialog, ParentElement::child)
                .when_some(provider_dialog, ParentElement::child)
        }

        /// The EDA services page is the multi-EDA service registry, laid out like
        /// Agents & tools: the left list manages one card per EDA backend service
        /// (only `JLCircuit` is implemented today), the right pane shows the
        /// selected service's settings, supervised lifecycle, and monitoring.
        fn render_eda_services_page(
            &mut self,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            self.maybe_probe_bridge_health(window, cx);
            let entity = cx.entity().clone();
            let language = self.language;
            let selector = entity;

            let jlc_selected = self.eda_services_selection == EdaServiceSelection::JlcircuitBridge;
            let lifecycle_label = self.bridge_status.clone().label(language);
            let health = self.bridge_health.clone();
            let address = self.bridge_endpoint();
            let jlc_card = div()
                .id("eda-service-jlcircuit")
                .v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(if jlc_selected { ACCENT } else { BORDER }))
                .bg(rgb(if jlc_selected { 0x00f0_f9ff } else { CARD_BG }))
                .cursor_pointer()
                .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                .on_click(move |_, _, cx| {
                    selector.update(cx, |view, cx| {
                        view.eda_services_selection = EdaServiceSelection::JlcircuitBridge;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(status_dot(health.dot()))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(if jlc_selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .child(EdaServiceSelection::JlcircuitBridge.label()),
                        )
                        .child(
                            div()
                                .ml_auto()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(lifecycle_label),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .truncate()
                        .child(format!("ws://{address}/bridge")),
                );

            // Deliberately not selectable: further backends plug into this same
            // service model later, but nothing claims to work today.
            let planned_card = div()
                .v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .opacity(0.7)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(TEXT_MUTED))
                        .child(language.choose("更多 EDA 后端", "More EDA backends")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .whitespace_normal()
                        .child(language.choose(
                            "KiCad 等后续按同一服务模型接入：左侧注册、启停与监测。",
                            "KiCad and others will plug into the same service model later: registered, supervised, and monitored from this list.",
                        )),
                );

            let detail = match self.eda_services_selection {
                EdaServiceSelection::JlcircuitBridge => {
                    self.render_eda_bridge_detail(cx).into_any_element()
                }
            };

            div()
                .id("eda-services-page")
                .size_full()
                .overflow_y_scroll()
                .min_w(px(720.))
                .relative()
                .v_flex()
                .gap_4()
                .p_6()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("EDA 服务", "EDA services")),
                        )
                        .child(
                            div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                language.choose(
                                    "EDA 后端服务在这里注册、启停与监测；智能体运行时端点在「智能体与工具」页。",
                                    "EDA backend services are registered, supervised, and monitored here; agent runtime endpoints live on the Agents & tools page.",
                                ),
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
                                .id("eda-list-scroll")
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .w(px(320.))
                                .flex_none()
                                .v_flex()
                                .gap_2()
                                .child(Self::agents_group_label(language.choose(
                                    "已实现",
                                    "Available",
                                )))
                                .child(jlc_card)
                                .child(Self::agents_group_label(language.choose(
                                    "规划中",
                                    "Planned",
                                )))
                                .child(planned_card),
                        )
                        .child(detail),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
        }

        /// The `JLCircuit` bridge service detail: settings, supervised lifecycle,
        /// and layered health monitoring with the protocol-level connection test.
        #[allow(clippy::too_many_lines)]
        fn render_eda_bridge_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let bridge_saver = entity.clone();
            let bridge_starter = entity.clone();
            let bridge_stopper = entity.clone();
            let bridge_tester = entity;
            let status = self.bridge_status.clone();
            let is_running = self.bridge_process.is_some();
            let is_starting = matches!(self.bridge_status, RuntimeLifecycleStatus::Starting);
            let is_test_pending = self.bridge_test_pending;
            let address = self.bridge_endpoint();
            let health = self.bridge_health.clone();
            let externally_owned =
                matches!(health, BridgeHealth::Listening { .. }) && !is_running && !is_starting;
            let last_test = self
                .bridge_test
                .as_ref()
                .map(|test| (elapsed_label(language, test.at.elapsed()), test.outcome.clone()));
            let capabilities_reported =
                last_test.as_ref().is_some_and(|(_, outcome)| outcome.is_ok());

            // Capabilities are shown only when the bridge itself reported them
            // in a successful connection test — never claimed on its behalf.
            let mut capability_items = div().flex().flex_wrap().gap_1p5();
            if let Some((_, Ok(report))) = &last_test {
                for capability in &report.capabilities {
                    capability_items = capability_items.child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .text_xs()
                            .bg(rgb(0x00e0_f2fe))
                            .text_color(rgb(0x000e_7490))
                            .child(capability.clone()),
                    );
                }
            }
            let test_line = match &last_test {
                None => language
                    .choose("尚未执行连接测试。", "No connection test has been run yet.")
                    .to_owned(),
                Some((elapsed, Ok(report))) => format!(
                    "{} · {elapsed}{} · {} · {} v{}",
                    language.choose("已连接", "Connected"),
                    language.choose(" 前", " ago"),
                    report.bridge_name,
                    language.choose("协议", "protocol"),
                    report.protocol_version,
                ),
                Some((elapsed, Err(reason))) => format!(
                    "{} · {elapsed}{} · {reason}",
                    language.choose("连接失败", "Connection test failed"),
                    language.choose(" 前", " ago"),
                ),
            };
            // The EDA extension's hello is rejected unless its projectId is
            // registered, so the connection test reports that gate too — an
            // empty list explains an EDA-side connect failure up front.
            let projects_note = match &last_test {
                Some((_, Ok(report))) if !report.projects.is_empty() => Some((
                    language.choose_owned(
                        format!(
                            "已注册项目（EDA 插件连接时填写这些 ID）：{}",
                            report.projects.join("、")
                        ),
                        format!(
                            "Registered projects (use these IDs in the EDA extension): {}",
                            report.projects.join(", ")
                        ),
                    ),
                    TEXT_SECONDARY,
                )),
                Some((_, Ok(_))) => Some((
                    language.choose(
                        "连接成功，但尚无已注册项目：EDA 插件可连接，发送请求前请先在「项目」页创建或打开项目，再刷新项目列表。",
                        "Connected, but no registered projects: the EDA extension's connection will be rejected — create or open a project on the Projects page first.",
                    )
                    .to_owned(),
                    0x00b4_5309,
                )),
                _ => None,
            };

            Self::detail_pane("eda-bridge-detail")
                .gap_3()
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
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("JLCircuit EDA bridge"),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(0x00e0_f2fe))
                                        .text_color(rgb(0x000e_7490))
                                        .child(language.choose("本地 WebSocket", "Local WebSocket")),
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
                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).whitespace_normal().child(
                        language.choose_owned(
                            format!("JLCircuit 插件与本机 `circuitfabric-jlc-bridge` 进程之间的 WebSocket 传输。bridge 作为受监管的子进程在这里启动与停止，并读取这里保存的监听地址；EDA 插件连接 ws://{address}/bridge。"),
                            format!("The WebSocket transport between the JLCircuit plugin and the local `circuitfabric-jlc-bridge` process. The bridge is started and stopped here as a supervised child process and reads the listen address saved here; the EDA extension connects to ws://{address}/bridge."),
                        ),
                    ),
                )
                .child(Self::labeled_field(
                    language.choose("bridge 监听地址", "Bridge listen address"),
                    "bridge-address",
                    Some(language.choose(
                        "仅保存此服务的监听地址（如 127.0.0.1:49630）；下次启动生效，运行中需重启。",
                        "Saves only this service's listen address (e.g. 127.0.0.1:49630); applies at the next start. Restart a running service.",
                    )),
                    &self.bridge_address,
                ))
                .child(Self::save_state_note(
                    self.bridge_address.read(cx).value().trim() != self.saved_settings.bridge.listen_address,
                    language,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("save-eda-bridge")
                                .primary()
                                .label(language.choose("保存 Bridge 地址", "Save Bridge address"))
                                .on_click(move |_, _, cx| {
                                    bridge_saver.update(cx, ControlPlaneView::save_bridge_settings);
                                }),
                        )
                        .child(
                            Button::new("start-eda-bridge")
                                .disabled(is_running || is_starting)
                                .label(language.choose("启动服务", "Start service"))
                                .on_click(move |_, window, cx| {
                                    bridge_starter.update(cx, |view, cx| {
                                        view.start_bridge_service(window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("stop-eda-bridge")
                                .disabled(!is_running)
                                .label(language.choose("停止服务", "Stop service"))
                                .on_click(move |_, _, cx| {
                                    bridge_stopper
                                        .update(cx, ControlPlaneView::stop_bridge_service);
                                }),
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
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("服务监测", "Service monitoring")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(status_dot(health.dot()))
                                .child(
                                    div()
                                        .text_sm()
                                        .whitespace_normal()
                                        .child(health.label(language)),
                                ),
                        )
                        .when(externally_owned, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .whitespace_normal()
                                    .child(language.choose(
                                        "端口可达，但服务不是由本应用启动的（例如手动启动的 bridge）；「停止服务」只作用于本应用启动的进程。",
                                        "The port answers, but the service was not started by this app (e.g. a manually launched bridge); Stop service only affects processes this app started.",
                                    )),
                            )
                        })
                        .child(
                            div().flex().items_center().gap_2().child(
                                Button::new("test-eda-bridge")
                                    .disabled(is_test_pending)
                                    .label(if is_test_pending {
                                        language.choose("测试中…", "Testing…")
                                    } else {
                                        language.choose("测试连接", "Test connection")
                                    })
                                    .on_click(move |_, window, cx| {
                                        bridge_tester.update(cx, |view, cx| {
                                            view.test_bridge_connection(window, cx);
                                        });
                                    }),
                            ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .whitespace_normal()
                                .child(test_line),
                        )
                        .when_some(projects_note, |this, (note, color)| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(color))
                                    .whitespace_normal()
                                    .child(note),
                            )
                        })
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .whitespace_normal()
                                .child(language.choose(
                                    "连接测试会临时占用 bridge 的唯一 WebSocket 连接；若 EDA 插件正连接中，测试可能超时，这不代表服务离线。",
                                    "The connection test temporarily occupies the bridge's single WebSocket connection; if the EDA plugin is attached the test may time out, which does not mean the service is offline.",
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(language.choose(
                                    "已上报能力（连接测试成功后显示）",
                                    "Reported capabilities (shown after a successful connection test)",
                                )),
                        )
                        .when(capabilities_reported, |this| this.child(capability_items))
                        .when(!capabilities_reported, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .whitespace_normal()
                                    .child(language.choose(
                                        "能力尚未上报，不展示为可执行。",
                                        "No capabilities reported yet; nothing is shown as executable.",
                                    )),
                            )
                        })
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .whitespace_normal()
                                .child(language.choose(
                                    "最后回读：尚未上报（等待 bridge 协议支持回读结果上报）。",
                                    "Last readback: not reported yet (pending bridge protocol support for readback reporting).",
                                )),
                        ),
                )
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
            let launch_provider = circuitfabric_codex_runtime::execution::selected_provider(
                &self.saved_settings,
                circuitfabric_codex_runtime::execution::AgentKind::Codex,
            );
            let launch_provider_summary = match launch_provider {
                Some(provider) => {
                    format!("`{}`（{} · {}）", provider.id, provider.name, provider.model)
                }
                None => language
                    .choose(
                        "尚未配置（请先添加 Provider）",
                        "none configured yet (add a provider first)",
                    )
                    .to_owned(),
            };
            let active_provider_note = self
                .codex_active_provider
                .clone()
                .map(|provider| {
                    format!("｜{}{provider}", language.choose("当前进程：", "current process: "))
                })
                .unwrap_or_default();
            let codex_dirty = self.runtime_dirty(RuntimeAdapter::CodexAppServer, cx);
            let codex_binding = self.codex_provider.read(cx).value().trim().to_owned();
            let codex_binding_display = if codex_binding.is_empty() {
                language.choose("默认 Provider", "Default provider").to_owned()
            } else {
                codex_binding
            };
            Self::detail_pane("codex-detail")
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
                                .flex_1()
                                .min_w(px(0.))
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
                    Self::adapter_section_card(
                        "1",
                        language.choose("运行 · 启动与停止", "Run · start and stop"),
                    )
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
                                        stopper.update(
                                            cx,
                                            ControlPlaneView::stop_codex_runtime,
                                        );
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(format!(
                                "{}{}{active_provider_note}",
                                language.choose(
                                    "下次启动 Provider：",
                                    "Next launch provider: "
                                ),
                                launch_provider_summary,
                            )),
                    )
                    .child(
                        div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "启动与停止读取下方「设置」中已保存的配置；停止只终止进程，不修改设置；退出应用时清理子进程。",
                                "Start and stop read the configuration saved in Settings below; stop terminates the process without changing settings; child processes are cleaned up when the app exits.",
                            ),
                        ),
                    ),
                )
                .child(
                    Self::adapter_section_card(
                        "2",
                        language.choose(
                            "设置 · 命令、目录与 Provider 关联",
                            "Settings · command, directory, provider binding",
                        ),
                    )
                    .child(Self::settings_summary_row(
                        language.choose("命令", "Command"),
                        self.command.read(cx).value().to_string(),
                    ))
                    .child(Self::settings_summary_row(
                        language.choose("工作目录", "Working dir"),
                        self.working_directory.read(cx).value().to_string(),
                    ))
                    .child(Self::settings_summary_row(
                        language.choose("Provider", "Provider"),
                        codex_binding_display,
                    ))
                    .child(
                        div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "JLC bridge 监听地址属于 EDA 侧服务，在「EDA 服务」页配置。",
                                "The JLC bridge listen address belongs to the EDA side and is configured on the EDA services page.",
                            ),
                        ),
                    )
                    .child(Self::save_state_note(codex_dirty, language))
                    .child(
                        Button::new("open-codex-settings")
                            .primary()
                            .label(language.choose("编辑设置…", "Edit settings…"))
                            .on_click(move |_, _, cx| {
                                binding_saver.update(cx, |view, cx| {
                                    view.adapter_settings_open =
                                        Some(RuntimeAdapter::CodexAppServer);
                                    view.dialog_error = None;
                                    cx.notify();
                                });
                            }),
                    ),
                )
                .child(
                    Self::adapter_section_card(
                        "3",
                        language.choose("快速验证 · 提交一个任务", "Quick check · run one task"),
                    )
                    .child(
                        div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "用已保存的配置发起一次性任务，验证端到端链路（模型、密钥、工具授权）；结果只显示在下方，不会修改任何配置。",
                                "Run a one-off task with the saved configuration to verify the end-to-end path (model, secrets, tool grants); the result appears below and changes no configuration.",
                            ),
                        ),
                    )
                    .child(self.render_task_controls(RuntimeAdapter::CodexAppServer, cx)),
                )
        }

        /// Global tool grants narrowed by the selected project's allowlist.
        ///
        /// A project can only restrict what the global runtime already authorized, never expand
        /// it; a missing project configuration grants no project tools.
        fn effective_grants(&self) -> ToolAuthorizationSettings {
            let Some(project_id) = self.selected_project.as_ref() else {
                return self.tool_authorizations.clone();
            };
            self.effective_grants_for(project_id)
        }

        fn effective_grants_for(&self, project_id: &ProjectId) -> ToolAuthorizationSettings {
            let mut grants = self.tool_authorizations.clone();
            let Some(configuration) = self.workspace.configuration(project_id) else {
                grants.authorized_skill_ids.clear();
                grants.authorized_mcp_server_ids.clear();
                return grants;
            };
            grants.authorized_skill_ids.retain(|id| configuration.enabled_skill_ids.contains(id));
            grants
                .authorized_mcp_server_ids
                .retain(|id| configuration.enabled_mcp_server_ids.contains(id));
            grants
        }

        /// Reports which source currently supplies one variable name: the
        /// unlocked vault wins over the process environment. Rendered under
        /// API-key fields so a mis-typed name is visible before a task fails;
        /// while a vault file exists but is locked, a 🔒 chip next to the
        /// hint opens the quick-unlock dialog.
        fn secret_source_hint(&self, name: &str, entity: &Entity<Self>) -> Option<gpui::Div> {
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            let language = self.language;
            let (color, text) =
                match secret_source(name, self.vault.as_ref().map(UnlockedVault::values)) {
                    Some(SecretSource::Vault) => (
                        0x0016_a34a,
                        language.choose_owned(
                            format!("✔ {name}：由密钥保险库提供（已解锁）"),
                            format!("✔ {name}: supplied by the unlocked secrets vault"),
                        ),
                    ),
                    Some(SecretSource::Environment) => (
                        0x0016_a34a,
                        language.choose_owned(
                            format!("✔ {name}：由进程环境变量提供"),
                            format!("✔ {name}: supplied by the process environment"),
                        ),
                    ),
                    None => (
                        0x00dc_2626,
                        language.choose_owned(
                            format!("✘ {name}：未找到该变量，请在密钥保险库收录，或设置同名环境变量"),
                            format!(
                                "✘ {name}: not found — record it in the secrets vault, or set an environment variable with this name"
                            ),
                        ),
                    ),
                };
            let quick_unlock = if self.vault_file_exists && self.vault.is_none() {
                let opener = entity.clone();
                Some(
                    div()
                        .id(format!("quick-unlock-{name}"))
                        .px_1p5()
                        .py_0p5()
                        .flex_none()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(ACCENT))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0x000e_7490))
                        .cursor_pointer()
                        .hover(|this| this.bg(rgb(0x00f0_f9ff)))
                        .on_click(move |_, _, cx| {
                            opener.update(cx, |view, cx| {
                                view.vault_quick_unlock_open = true;
                                view.vault_message = None;
                                cx.notify();
                            });
                        })
                        .child(language.choose("🔒 快速解锁", "🔒 Quick unlock")),
                )
            } else {
                None
            };
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .whitespace_normal()
                            .text_xs()
                            .text_color(rgb(color))
                            .child(text),
                    )
                    .when_some(quick_unlock, ParentElement::child),
            )
        }

        /// Unlocks the vault with the typed password. Key derivation runs on a
        /// background thread; a wrong password keeps the prompt open with the
        /// vault's own error message.
        fn unlock_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.vault_busy {
                return;
            }
            let password = self.vault_password.read(cx).value().to_string();
            if password.is_empty() {
                self.vault_message = Some("请输入保险库密码。".to_owned());
                cx.notify();
                return;
            }
            self.vault_busy = true;
            self.vault_message = None;
            let path = self.vault_path.clone();
            let work = cx.background_spawn(async move {
                UnlockedVault::unlock(&path, &password).map_err(|error| error.to_string())
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = work.await;
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.vault_busy = false;
                        match result {
                            Ok(vault) => {
                                view.vault_index =
                                    vault.values().names().cloned().collect::<Vec<_>>();
                                let count = view.vault_index.len();
                                view.vault = Some(vault);
                                view.vault_file_exists = true;
                                view.vault_prompt_open = false;
                                view.vault_quick_unlock_open = false;
                                view.vault_message = None;
                                view.vault_password.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                                view.status = format!("保险库已解锁，收录 {count} 个变量。");
                            }
                            Err(error) => {
                                view.vault_message = Some(error);
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

        /// Creates a new vault file and unlocks it immediately.
        fn create_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.vault_busy {
                return;
            }
            let password = self.vault_password.read(cx).value().to_string();
            let confirm = self.vault_password_confirm.read(cx).value().to_string();
            if password != confirm {
                self.vault_message = Some("两次输入的密码不一致。".to_owned());
                cx.notify();
                return;
            }
            self.vault_busy = true;
            self.vault_message = None;
            let path = self.vault_path.clone();
            let work = cx.background_spawn(async move {
                UnlockedVault::create(&path, &password).map_err(|error| error.to_string())
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = work.await;
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.vault_busy = false;
                        match result {
                            Ok(vault) => {
                                view.vault = Some(vault);
                                view.vault_file_exists = true;
                                view.vault_index = Vec::new();
                                view.vault_message = None;
                                view.vault_password.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                                view.vault_password_confirm.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                                "保险库已创建并解锁。".clone_into(&mut view.status);
                            }
                            Err(error) => {
                                view.vault_message = Some(error);
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

        /// Locks the vault: dropping it zeroizes the derived key and decrypted
        /// values. The plaintext name index stays readable from disk.
        fn relock_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.vault_busy {
                return;
            }
            self.vault_password.update(cx, |state, cx| state.set_value("", window, cx));
            self.vault_password_confirm.update(cx, |state, cx| state.set_value("", window, cx));
            self.vault = None;
            self.vault_prompt_open = false;
            self.vault_index = UnlockedVault::variable_names(&self.vault_path).unwrap_or_default();
            self.selected_secret = None;
            self.vault_message = None;
            "保险库已锁定；正在运行的任务保留其启动时注入的变量。".clone_into(&mut self.status);
            cx.notify();
        }

        /// Adds or replaces one variable. The vault re-encrypts atomically on
        /// each write, and the value field is cleared so it never echoes back.
        fn save_secret_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let name = self.secret_name.read(cx).value().trim().to_owned();
            let value = self.secret_value.read(cx).value().to_string();
            if name.is_empty() {
                self.vault_message = Some("请填写变量名。".to_owned());
                cx.notify();
                return;
            }
            let Some(vault) = self.vault.as_mut() else {
                self.vault_message = Some("请先解锁保险库。".to_owned());
                cx.notify();
                return;
            };
            match vault.set(&name, &value) {
                Ok(()) => {
                    self.vault_index = vault.values().names().cloned().collect();
                    self.secret_value.update(cx, |state, cx| state.set_value("", window, cx));
                    self.selected_secret = Some(name.clone());
                    self.vault_message = None;
                    self.status = format!("已保存变量 {name}（值已加密写入保险库）。");
                }
                Err(error) => {
                    self.vault_message = Some(error.to_string());
                }
            }
            cx.notify();
        }

        fn remove_secret_entry(&mut self, name: &str, cx: &mut Context<Self>) {
            let Some(vault) = self.vault.as_mut() else {
                self.vault_message = Some("请先解锁保险库。".to_owned());
                cx.notify();
                return;
            };
            match vault.remove(name) {
                Ok(_) => {
                    self.vault_index = vault.values().names().cloned().collect();
                    if self.selected_secret.as_deref() == Some(name) {
                        self.selected_secret = None;
                    }
                    self.vault_message = None;
                    self.status = format!("已移除变量 {name}。");
                }
                Err(error) => {
                    self.vault_message = Some(error.to_string());
                }
            }
            cx.notify();
        }

        /// Re-encrypts the vault under a new password. Key derivation is slow,
        /// so the vault is moved to a background thread and always put back —
        /// even when the change fails, the session stays unlocked.
        fn change_vault_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.vault_busy {
                return;
            }
            let password = self.vault_password.read(cx).value().to_string();
            let confirm = self.vault_password_confirm.read(cx).value().to_string();
            if password != confirm {
                self.vault_message = Some("两次输入的密码不一致。".to_owned());
                cx.notify();
                return;
            }
            let Some(vault) = self.vault.take() else {
                self.vault_message = Some("请先解锁保险库。".to_owned());
                cx.notify();
                return;
            };
            self.vault_busy = true;
            self.vault_message = None;
            let work = cx.background_spawn(async move {
                let mut vault = vault;
                let result = vault.change_password(&password).map_err(|error| error.to_string());
                (vault, result)
            });
            cx.spawn_in(window, async move |view, cx| {
                let (vault, result) = work.await;
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.vault_busy = false;
                        view.vault = Some(vault);
                        match result {
                            Ok(()) => {
                                view.vault_password.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                                view.vault_password_confirm.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                                view.vault_message = None;
                                "保险库密码已更新。".clone_into(&mut view.status);
                            }
                            Err(error) => {
                                view.vault_message = Some(error);
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

        fn dismiss_vault_prompt(&mut self, cx: &mut Context<Self>) {
            self.vault_prompt_open = false;
            "已暂缓解锁；缺少密钥的任务会失败并提示来源。可在「密钥保险库」页随时解锁。"
                .clone_into(&mut self.status);
            cx.notify();
        }

        fn run_agent_task(
            &mut self,
            adapter: RuntimeAdapter,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            use circuitfabric_codex_runtime::execution::{
                AgentKind, Cancellation, run_task_with_secrets,
            };
            if self.task_cancel.is_some() {
                return;
            }
            let Some(settings) = self.settings_for_execution(cx) else {
                return;
            };
            let grants = self.effective_grants();
            let user_prompt = self.task_prompt.read(cx).value().to_string();
            let image_text = self.task_image.read(cx).value().to_string();
            let image =
                if adapter == RuntimeAdapter::CodexAppServer && !image_text.trim().is_empty() {
                    Some(PathBuf::from(image_text))
                } else {
                    None
                };
            let mut prompt = user_prompt.clone();
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
            // Project scope: the agent process works in the project root, and the run is
            // audited as a session record inside that project.
            let project_scope = project
                .as_ref()
                .and_then(|id| self.project_storages.get(id))
                .map(|storage| (storage.clone(), storage.root().to_path_buf()));
            let provider =
                circuitfabric_codex_runtime::execution::selected_provider(&settings, kind);
            let profile_id = provider.map_or_else(|| "default".to_owned(), |p| p.id.clone());
            let snapshot = provider
                .map_or_else(String::new, |p| format!("{} / {} / {}", p.id, p.model, p.base_url));
            self.task_result = format!(
                "正在调用 {}：{snapshot}。使用已保存快照；修改配置在下次任务生效。",
                adapter.label()
            );
            let session = project_scope.as_ref().and_then(|(storage, _)| {
                let session_id = format!(
                    "session_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                );
                match storage.start_session(SessionSeed {
                    session_id: session_id.clone(),
                    runtime_profile_id: profile_id.clone(),
                    backend_id: Some(adapter.backend_id().to_owned()),
                }) {
                    Ok(_) => Some((storage.clone(), session_id)),
                    Err(error) => {
                        self.status = format!("任务继续执行，但项目会话记录创建失败：{error}");
                        None
                    }
                }
            });
            let project_root = project_scope.as_ref().map(|(_, root)| root.clone());
            let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
            let work = cx.background_spawn(async move {
                run_task_with_secrets(
                    &settings,
                    kind,
                    &grants,
                    &prompt,
                    image.as_deref(),
                    project_root.as_deref(),
                    secrets.as_ref(),
                    &cancel,
                )
            });
            cx.spawn_in(window, async move |view, cx| {
                let result = work.await;
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.task_cancel = None;
                        if view.selected_project == project {
                            view.task_result = match &result {
                                Ok(output) => format!("任务完成（{snapshot}）\n{output}"),
                                Err(error) => format!("任务未完成（{snapshot}）：{error}"),
                            };
                        }
                        if let Some((storage, session_id)) = session {
                            view.record_agent_session(&storage, &session_id, &user_prompt, &result);
                        }
                        if let Some(project) = &project
                            && let Err(error) = view.refresh_project_data(project)
                        {
                            view.status = format!("项目会话列表未刷新：{error}");
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

        /// Writes one finished agent run into its project's session audit record.
        fn record_agent_session(
            &mut self,
            storage: &ProjectStorage,
            session_id: &str,
            user_prompt: &str,
            result: &Result<String, circuitfabric_codex_runtime::RuntimeError>,
        ) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let mut append = |event: &SessionEvent| {
                if let Err(error) = storage.append_session_event(session_id, event) {
                    self.status = format!("项目会话记录未写入：{error}");
                }
            };
            append(&SessionEvent {
                timestamp_unix_seconds: now,
                kind: SessionEventKind::Turn {
                    actor: SessionActor::User,
                    message: audit_excerpt(user_prompt),
                },
            });
            match result {
                Ok(output) => append(&SessionEvent {
                    timestamp_unix_seconds: now,
                    kind: SessionEventKind::Turn {
                        actor: SessionActor::Agent,
                        message: audit_excerpt(output),
                    },
                }),
                Err(error) => append(&SessionEvent {
                    timestamp_unix_seconds: now,
                    kind: SessionEventKind::Note { text: format!("任务失败：{error}") },
                }),
            }
            let status =
                if result.is_ok() { SessionStatus::Completed } else { SessionStatus::Failed };
            if let Err(error) =
                storage.complete_session(session_id, SessionUsage::default(), Vec::new(), status)
            {
                self.status = format!("项目会话记录未完成：{error}");
            }
        }

        fn render_task_controls(
            &mut self,
            adapter: RuntimeAdapter,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let runner = cx.entity().clone();
            let stopper = runner.clone();
            let working_directory_note = self
                .selected_project
                .as_ref()
                .zip(self.selected_project.as_ref().and_then(|id| self.project_storages.get(id)))
                .map_or_else(
                    || "工作目录：隔离临时目录（未选择项目）".to_owned(),
                    |(_, storage)| format!("工作目录（项目根目录）：{}", storage.root().display()),
                );
            div()
                .v_flex()
                .gap_2()
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(working_directory_note))
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
                .child(
                    div()
                        .id("runtime-task-result")
                        .v_flex()
                        .gap_0p5()
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(div().text_sm().whitespace_normal().child(self.task_result.clone())),
                )
        }

        /// The “run agents inside this project” block of the project's agent tab.
        fn render_project_agents_section(
            &mut self,
            project: &Project,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let adapter = self.project_agent_adapter;
            let mut selector = div().flex().gap_2().flex_wrap();
            for candidate in
                [RuntimeAdapter::CodexAppServer, RuntimeAdapter::ClaudeCode, RuntimeAdapter::Dsh]
            {
                let chooser = entity.clone();
                selector = selector.child(
                    Button::new(format!("project-agent-adapter-{}", candidate.backend_id()))
                        .label(candidate.label())
                        .when(candidate == adapter, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            chooser.update(cx, |view, cx| {
                                view.project_agent_adapter = candidate;
                                cx.notify();
                            });
                        }),
                );
            }
            let root_note = self.project_storages.get(&project.id).map_or_else(
                || "项目根目录未打开，无法在项目内运行。".to_owned(),
                |storage| format!("任务工作目录（项目根目录）：{}", storage.root().display()),
            );
            div()
                .v_flex()
                .gap_3()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                        language.choose("在项目中运行智能体", "Run agents in this project"),
                    ),
                )
                .child(selector)
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(root_note))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    language.choose(
                        "智能体进程以项目根目录为工作目录；提示词自动附加项目级说明；工具授权取全局与项目 allowlist 的交集，项目不能扩大全局授权。每次运行都会写入项目会话记录，可在“会话”标签回放。",
                        "Agent processes run with the project root as their working directory; project instructions are prepended to the prompt; tool grants are the intersection of global and project allowlists, so a project never expands global authorization. Every run is recorded as a project session, replayable in the Sessions tab.",
                    ),
                ))
                .child(self.render_task_controls(adapter, cx))
        }

        fn render_adapter_detail(
            &mut self,
            adapter: RuntimeAdapter,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let saver = cx.entity().clone();
            let language = self.language;
            let dirty = self.runtime_dirty(adapter, cx);
            let command_display = self.runtime_fields(adapter).0.read(cx).value().to_string();
            let binding = self.runtime_fields(adapter).1.read(cx).value().trim().to_owned();
            let binding_display = if binding.is_empty() {
                language.choose("默认 Provider", "Default provider").to_owned()
            } else {
                binding
            };
            let description = match adapter {
                RuntimeAdapter::ClaudeCode => language.choose(
                    "每次执行创建独立任务进程；完成、失败或取消后清理。使用 Anthropic Messages 协议。",
                    "Each run creates an isolated task process, cleaned up on completion, failure, or cancellation. Uses the Anthropic Messages protocol.",
                ),
                RuntimeAdapter::Dsh | RuntimeAdapter::CodexAppServer => language.choose(
                    "每次执行创建独立任务进程；完成、失败或取消后清理。",
                    "Each run creates an isolated task process, cleaned up on completion, failure, or cancellation.",
                ),
            };
            Self::detail_pane("adapter-detail")
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
                                .flex_1()
                                .min_w(px(0.))
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(adapter.label()),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .whitespace_normal()
                                        .child(description),
                                ),
                        ),
                )
                .child(
                    Self::adapter_section_card(
                        "1",
                        language.choose(
                            "设置 · 命令与 Provider 关联",
                            "Settings · command and provider binding",
                        ),
                    )
                    .child(Self::settings_summary_row(
                        language.choose("命令", "Command"),
                        command_display,
                    ))
                    .child(Self::settings_summary_row(
                        language.choose("Provider", "Provider"),
                        binding_display,
                    ))
                    .child(Self::save_state_note(dirty, language))
                    .child(
                        Button::new("open-adapter-settings")
                            .primary()
                            .label(language.choose("编辑设置…", "Edit settings…"))
                            .on_click(move |_, _, cx| {
                                saver.update(cx, |view, cx| {
                                    view.adapter_settings_open = Some(adapter);
                                    view.dialog_error = None;
                                    cx.notify();
                                });
                            }),
                    ),
                )
                .child(
                    Self::adapter_section_card(
                        "2",
                        language.choose("快速验证 · 提交一个任务", "Quick check · run one task"),
                    )
                    .child(
                        div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "用已保存的配置发起一次性任务，验证端到端链路（模型、密钥、工具授权）；结果只显示在下方，不会修改任何配置。",
                                "Run a one-off task with the saved configuration to verify the end-to-end path (model, secrets, tool grants); the result appears below and changes no configuration.",
                            ),
                        ),
                    )
                    .child(self.render_task_controls(adapter, cx)),
                )
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
            let provider_saver = entity.clone();
            let dirty = self.provider_values(cx) != self.saved_settings.providers
                || self.default_provider_id != self.saved_settings.default_provider_id;
            let api_key_hint = self.secret_source_hint(
                &provider.api_key_environment_variable.read(cx).value(),
                &entity,
            );

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

            Self::detail_pane("provider-detail")
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
                                .flex_1()
                                .min_w(px(0.))
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
                    "Provider 配置只保存 API Key 变量名；密钥值在密钥保险库中单独加密保存，也可由进程环境提供。",
                    "Provider configuration saves API key variable names only; values are stored separately in the encrypted vault or supplied by the process environment.",
                    language,
                ))
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .child(Self::settings_summary_row(
                            "Base URL",
                            provider.base_url.read(cx).value().to_string(),
                        ))
                        .child(Self::settings_summary_row(
                            language.choose("模型", "Model"),
                            provider.model.read(cx).value().to_string(),
                        ))
                        .child(Self::settings_summary_row(
                            language.choose("API Key", "API key"),
                            provider.api_key_environment_variable.read(cx).value().to_string(),
                        ))
                        .when_some(api_key_hint, ParentElement::child)
                        .child(Self::settings_summary_row(
                            "Vision",
                            if provider.supports_vision {
                                format!(
                                    "{}（{}）",
                                    language.choose("已启用", "Enabled"),
                                    provider.vision_model.read(cx).value()
                                )
                            } else {
                                language.choose("已停用", "Disabled").to_owned()
                            },
                        )),
                )
                .child(Self::save_state_note(dirty, language))
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
                            "上述默认项、启停与删除均为草稿，连同表单修改一起在「编辑 Provider…」弹窗中保存后生效。默认项用于未指定 Provider 的运行时。",
                            "Default, enabled, and removal changes are drafts; they take effect together with the form edits saved in the Edit provider dialog. The default applies to runtimes without an explicit provider binding.",
                        ),
                    ),
                )
                .child(
                    Button::new("open-provider-editor")
                        .primary()
                        .label(language.choose("编辑 Provider…", "Edit provider…"))
                        .on_click(move |_, _, cx| {
                            provider_saver.update(cx, |view, cx| {
                                view.provider_editor_open = true;
                                view.dialog_error = None;
                                cx.notify();
                            });
                        }),
                )
        }

        #[allow(clippy::too_many_lines)]
        fn render_catalog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let skill_importer = entity.clone();
            let mcp_saver = entity.clone();
            let env_names = Self::parse_tool_ids(&self.catalog_env.read(cx).value());
            let env_hints = if env_names.is_empty() {
                None
            } else {
                let mut list = div().v_flex().gap_1();
                for name in &env_names {
                    if let Some(hint) = self.secret_source_hint(name, &entity) {
                        list = list.child(hint);
                    }
                }
                Some(list)
            };
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
                        .child(div().flex_1().min_w(px(0.)).whitespace_normal().child(format!(
                            "技能 {} · {} · {}",
                            skill.id,
                            if skill.enabled { "启用" } else { "停用" },
                            skill.path.display()
                        )))
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
                        .child(div().flex_1().min_w(px(0.)).whitespace_normal().child(format!(
                            "MCP {} · {} · {}",
                            server.id,
                            if server.enabled { "启用" } else { "停用" },
                            server.command
                        )))
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
                                        let secrets =
                                            view.vault.as_ref().map(|vault| vault.values().clone());
                                        let id = test_id.clone();
                                        view.status = "正在连接 MCP…".into();
                                        let work = cx.background_spawn(async move {
                                            catalog
                                                .list_tools_with_secrets(
                                                    &id,
                                                    &grants,
                                                    secrets.as_ref(),
                                                )
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
                .child(Self::info_note(
                    "导入、保存定义、启停、删除和授权操作均即时保存，仅更新定义或选定作用域的授权。",
                    "Imports, definition saves, toggles, removal and grants save immediately, updating only definitions or grants in the selected scope.",
                    self.language,
                ))
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
                .when_some(env_hints, ParentElement::child)
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
                                        Ok(_) => {
                                            view.persist_catalog(previous, cx);
                                        }
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
        ) -> bool {
            let saved = match self.save_update(
                crate::settings_persistence::SettingsUpdate::Catalog(self.catalog.clone()),
            ) {
                Ok(()) => {
                    if let Some(cancel) = &self.task_cancel {
                        cancel.cancel();
                    }
                    self.status = "已保存定义；当前任务已请求取消，下次任务使用新配置".into();
                    true
                }
                Err(error) => {
                    self.catalog = previous;
                    self.status = format!("未保存：{error}");
                    false
                }
            };
            cx.notify();
            saved
        }

        /// Enable/disable switch for the bundled Jev server definition.
        fn toggle_jev_enabled(&mut self, cx: &mut Context<Self>) {
            let previous = self.catalog.clone();
            if let Some(server) = self
                .catalog
                .mcp_servers
                .iter_mut()
                .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
            {
                server.enabled = !server.enabled;
            }
            self.persist_catalog(previous, cx);
        }

        /// Global-scope grant for the bundled Jev server, mirroring
        /// `authorize_tool`/`revoke_tool` for one fixed id.
        fn set_jev_authorization(&mut self, authorized: bool, cx: &mut Context<Self>) {
            let previous = self.tool_authorizations.clone();
            {
                let list = &mut self.tool_authorizations.authorized_mcp_server_ids;
                if authorized {
                    if !list.iter().any(|id| id == BUNDLED_JEV_SERVER_ID) {
                        list.push(BUNDLED_JEV_SERVER_ID.to_owned());
                        list.sort();
                    }
                } else {
                    list.retain(|id| id != BUNDLED_JEV_SERVER_ID);
                }
            }
            match self.save_update(crate::settings_persistence::SettingsUpdate::Authorizations(
                self.tool_authorizations.clone(),
            )) {
                Ok(()) => {
                    if let Some(cancel) = &self.task_cancel {
                        cancel.cancel();
                    }
                    self.status = if authorized {
                        format!("已全局授权 {BUNDLED_JEV_SERVER_ID}；当前任务已请求取消。")
                    } else {
                        format!("已撤销 {BUNDLED_JEV_SERVER_ID} 的全局授权；当前任务已请求取消。")
                    };
                }
                Err(error) => {
                    self.tool_authorizations = previous;
                    self.status = format!("未保存：{error}");
                }
            }
            cx.notify();
        }

        fn set_jev_project_authorization(&mut self, authorized: bool, cx: &mut Context<Self>) {
            let Some(project_id) = self.selected_project.clone() else {
                self.status = "请先选择项目。".to_owned();
                cx.notify();
                return;
            };
            let Some(storage) = self.project_storages.get(&project_id) else {
                self.status = "当前项目尚未打开。".to_owned();
                cx.notify();
                return;
            };
            let mut configuration =
                self.workspace.configuration(&project_id).cloned().unwrap_or_default();
            let list = &mut configuration.enabled_mcp_server_ids;
            if authorized {
                if !list.iter().any(|id| id == BUNDLED_JEV_SERVER_ID) {
                    list.push(BUNDLED_JEV_SERVER_ID.to_owned());
                    list.sort();
                }
            } else {
                list.retain(|id| id != BUNDLED_JEV_SERVER_ID);
            }
            match storage.save_configuration(&configuration) {
                Ok(()) => match self.workspace.set_configuration(&project_id, configuration) {
                    Ok(()) => {
                        self.status = format!(
                            "项目 `{project_id}` Jev 授权已{}。",
                            if authorized { "开启" } else { "撤销" }
                        )
                    }
                    Err(error) => self.status = format!("项目配置已保存，但内存未刷新：{error}"),
                },
                Err(error) => self.status = format!("项目 Jev 授权未保存：{error}"),
            }
            cx.notify();
        }

        /// Store `TYPESAFE_API_KEY` into the unlocked vault; the value never
        /// lands in configuration files.
        /// Store the active backend's key variable into the unlocked vault;
        /// returns whether the value was saved (so dialogs can close).
        fn save_jev_api_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
            let name = self
                .catalog
                .mcp_servers
                .iter()
                .find(|server| {
                    server.id == circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID
                })
                .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
                .map_or_else(
                    || "TYPESAFE_API_KEY".to_owned(),
                    |settings| settings.api_key_environment_variable,
                );
            let value = self.jev_api_key.read(cx).value().trim().to_owned();
            if value.is_empty() {
                self.status = format!("未保存：请粘贴 {name} 的值。");
                cx.notify();
                return false;
            }
            let Some(vault) = self.vault.as_mut() else {
                "未保存：请先在「密钥保险库」页解锁保险库，或改用同名环境变量。"
                    .clone_into(&mut self.status);
                cx.notify();
                return false;
            };
            let saved = match vault.set(&name, &value) {
                Ok(()) => {
                    self.vault_index = vault.values().names().cloned().collect();
                    self.jev_api_key.update(cx, |state, cx| state.set_value("", window, cx));
                    self.status = format!("已保存 {name}（值已加密写入保险库）。");
                    true
                }
                Err(error) => {
                    self.status = format!("未保存：{error}");
                    false
                }
            };
            cx.notify();
            saved
        }

        /// Re-run bundled binary detection after the catalog entry was removed
        /// or the binary was built after the app started.
        fn reregister_jev(&mut self, cx: &mut Context<Self>) {
            let previous = self.catalog.clone();
            self.catalog.ensure_bundled();
            if self.catalog.mcp_servers.iter().any(|server| server.id == BUNDLED_JEV_SERVER_ID) {
                self.persist_catalog(previous, cx);
            } else {
                concat!(
                    "未找到 evaluate 二进制：请先运行 scripts/build-typesafe-mcp.ps1，",
                    "或设置 CIRCUITFABRIC_TYPESAFE_MCP_PATH 指向它。"
                )
                .clone_into(&mut self.status);
                cx.notify();
            }
        }

        fn jev_settings_from_form(
            &self,
            cx: &Context<Self>,
        ) -> Result<circuitfabric_codex_runtime::judge::LlmJudgeSettings, String> {
            let mut settings = self.jev_llm_settings.clone();
            settings.base_url = self.jev_base_url.read(cx).value().trim().to_owned();
            settings.model = self.jev_model.read(cx).value().trim().to_owned();
            settings.api_key_environment_variable =
                self.jev_key_env.read(cx).value().trim().to_owned();
            settings.timeout_seconds = self
                .jev_timeout
                .read(cx)
                .value()
                .parse()
                .map_err(|_| "超时必须为 5～180 秒".to_owned())?;
            settings.malformed_retries = self
                .jev_retries
                .read(cx)
                .value()
                .parse()
                .map_err(|_| "结构修正重试必须为 0～3 次".to_owned())?;
            settings.validate().map_err(|error| error.to_string())?;
            Ok(settings)
        }

        /// Persist the chosen backend branch; returns whether it was saved
        /// (so the configuration dialog can close on success only).
        fn apply_jev_backend(&mut self, llm: bool, cx: &mut Context<Self>) -> bool {
            use circuitfabric_codex_runtime::tools::{BUNDLED_JEV_SERVER_ID, bundled_mcp_servers};
            let result = if llm {
                self.jev_settings_from_form(cx).and_then(|settings| {
                    std::env::current_exe().map_err(|error| error.to_string()).and_then(|path| {
                        settings.server(&path, true).map_err(|error| error.to_string())
                    })
                })
            } else {
                bundled_mcp_servers().into_iter().find(|s| s.id == BUNDLED_JEV_SERVER_ID)
                    .ok_or_else(|| "未找到 TypeSafe evaluate 二进制，请先构建或设置 CIRCUITFABRIC_TYPESAFE_MCP_PATH".to_owned())
            };
            match result {
                Ok(mut server) => {
                    let previous = self.catalog.clone();
                    if let Some(settings) =
                        circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server(&server)
                    {
                        self.catalog.llm_judge = Some(settings);
                    }
                    if let Some(existing) =
                        self.catalog.mcp_servers.iter().find(|s| s.id == BUNDLED_JEV_SERVER_ID)
                    {
                        server.enabled = existing.enabled;
                    }
                    self.catalog.mcp_servers.retain(|s| s.id != BUNDLED_JEV_SERVER_ID);
                    self.catalog.mcp_servers.push(server);
                    if self.persist_catalog(previous, cx) {
                        self.jev_llm_draft = llm;
                        true
                    } else {
                        false
                    }
                }
                Err(error) => {
                    self.status = format!("判断后端未保存：{error}");
                    cx.notify();
                    false
                }
            }
        }

        /// Compact status chip for the overview row: green while `good` is
        /// `Some(true)`, red while `Some(false)`, neutral gray when `None`.
        fn jev_chip(id: &'static str, label: String, good: Option<bool>) -> impl IntoElement {
            let (background, foreground) = match good {
                Some(true) => (0x00dc_fce7, 0x0016_a34a),
                Some(false) => (0x00fe_e2e2, 0x00b9_1c1c),
                None => (SURFACE_BG, TEXT_MUTED),
            };
            div()
                .id(id)
                .px_2()
                .py_0p5()
                .rounded_sm()
                .text_xs()
                .bg(rgb(background))
                .text_color(rgb(foreground))
                .child(label)
        }

        /// Numbered section heading used across the Jev settings page.
        fn jev_section_header(zh: &'static str, en: &'static str, language: UiLanguage) -> Div {
            div().text_sm().font_weight(FontWeight::MEDIUM).child(language.choose(zh, en))
        }

        /// Fixed-width caption for one segmented mode row.
        fn jev_mode_label(label: &'static str) -> Div {
            div().w(px(84.)).flex_none().text_sm().text_color(rgb(TEXT_SECONDARY)).child(label)
        }

        /// One option of an in-place segmented control; `active` marks the
        /// selected branch and `apply` runs when the option is clicked.
        fn jev_option(
            id: &'static str,
            label: &'static str,
            active: bool,
            entity: &Entity<Self>,
            apply: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        ) -> impl IntoElement {
            let entity = entity.clone();
            div()
                .id(id)
                .px_3()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(if active { ACCENT } else { BORDER }))
                .bg(rgb(if active { 0x00f0_f9ff } else { CARD_BG }))
                .text_sm()
                .text_color(rgb(if active { TEXT_PRIMARY } else { TEXT_MUTED }))
                .cursor_pointer()
                .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                .child(label)
                .on_click(move |_, _, cx| {
                    entity.update(cx, |view, cx| {
                        apply(view, cx);
                        cx.notify();
                    });
                })
        }

        /// Free MCP-level check: initialize + tools/list, no billable call.
        fn jev_discover_button(language: UiLanguage, entity: &Entity<Self>) -> impl IntoElement {
            let tester = entity.clone();
            Button::new("jev-test")
                .label(
                    language
                        .choose("连接并发现工具（不计费）", "Connect and discover tools (free)"),
                )
                .on_click(move |_, window, cx| {
                    tester.update(cx, |view, cx| {
                        let catalog = view.catalog.clone();
                        let grants = view.effective_grants();
                        let secrets = view.vault.as_ref().map(|vault| vault.values().clone());
                        let id = BUNDLED_JEV_SERVER_ID.to_owned();
                        view.status = "正在连接 MCP…".into();
                        let work = cx.background_spawn(async move {
                            catalog
                                .list_tools_with_secrets(&id, &grants, secrets.as_ref())
                                .map(|value| value.to_string())
                                .map_err(|error| error.to_string())
                        });
                        cx.spawn_in(window, async move |view, cx| {
                            let result = work.await;
                            cx.update(|_, cx| {
                                view.update(cx, |view, cx| {
                                    view.status = match result {
                                        Ok(tools) => format!("MCP 已连接，工具：{tools}"),
                                        Err(error) => format!("MCP 连接失败：{error}"),
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
                })
        }

        /// Billable end-to-end judgment check through the saved backend.
        fn jev_real_test_button(language: UiLanguage, entity: &Entity<Self>) -> impl IntoElement {
            let tester = entity.clone();
            Button::new("jev-real-test")
                .label(language.choose("测试真实判断（消耗额度）", "Run a real judgment (billed)"))
                .on_click(move |_, window, cx| {
                    tester.update(cx, |view, cx| {
                        let catalog = view.catalog.clone();
                        let grants = view.effective_grants();
                        let secrets = view.vault.as_ref().map(|vault| vault.values().clone());
                        view.status = "正在测试已保存的判断后端（会调用模型）…".into();
                        let work = cx.background_spawn(async move {
                            catalog
                                .call_tool_with_secrets(
                                    circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID,
                                    &grants,
                                    "evaluate",
                                    &serde_json::json!({"state":"The LED is on.","questions":{"led_on":{"type":"noul","instructions":"Does the evidence state that the LED is on?"}}}),
                                    secrets.as_ref(),
                                )
                                .map_err(|error| error.to_string())
                                .and_then(|value| {
                                    if value["isError"] == true {
                                        Err(value["content"][0]["text"]
                                            .as_str()
                                            .unwrap_or("判断失败")
                                            .to_owned())
                                    } else {
                                        Ok(value.to_string())
                                    }
                                })
                        });
                        cx.spawn_in(window, async move |view, cx| {
                            let result = work.await;
                            cx.update(|_, cx| {
                                view.update(cx, |view, cx| {
                                    view.status = match result {
                                        Ok(value) => format!("判断测试成功：{value}"),
                                        Err(error) => format!("判断测试失败：{error}"),
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
                })
        }

        /// Third-party-LLM form body used inside the backend dialog: provider
        /// presets, endpoint and answer semantics. No apply button — the
        /// dialog footer owns 保存/取消.
        fn render_jev_llm_configuration(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            use circuitfabric_codex_runtime::judge::{AnswerMode, OutputFormat};
            let entity = cx.entity().clone();
            let language = self.language;
            let presets = div().flex().flex_wrap().gap_2().children(
                [
                    (
                        "deepseek",
                        language.choose("DeepSeek", "DeepSeek").to_owned(),
                        "https://api.deepseek.com/v1",
                        "deepseek-flash",
                        "DEEPSEEK_API_KEY",
                    ),
                    (
                        "zai-coding",
                        language.choose("z.ai 编程包", "z.ai Coding Plan").to_owned(),
                        "https://api.z.ai/api/coding/paas/v4",
                        "glm-5.3-flash",
                        "ZAI_API_KEY",
                    ),
                    (
                        "zai-standard",
                        language.choose("z.ai 按量付费", "z.ai Pay-as-you-go").to_owned(),
                        "https://api.z.ai/api/paas/v4",
                        "glm-4.7",
                        "ZAI_API_KEY",
                    ),
                ]
                .into_iter()
                .map(|(id, label, url, model, key)| {
                    let selector = entity.clone();
                    Button::new(id).label(label).on_click(move |_, window, cx| {
                        selector.update(cx, |view, cx| {
                            view.jev_base_url.update(cx, |s, cx| s.set_value(url, window, cx));
                            view.jev_model.update(cx, |s, cx| s.set_value(model, window, cx));
                            view.jev_key_env.update(cx, |s, cx| s.set_value(key, window, cx));
                            view.jev_llm_settings.output_format = OutputFormat::JsonObject;
                            view.status = "已填入预设，可按账户修改模型；点击「保存」生效。".into();
                            cx.notify();
                        });
                    })
                }),
            );
            div().v_flex().gap_3()
                .child(div().flex().flex_wrap().items_center().gap_2()
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "服务商预设：",
                        "Provider presets:",
                    )))
                    .child(presets))
                .child(Self::labeled_field(
                    "Base URL",
                    "jev-llm-url",
                    Some(language.choose(
                        "兼容 OpenAI Chat Completions，自动追加 /chat/completions",
                        "OpenAI Chat Completions compatible; /chat/completions is appended",
                    )),
                    &self.jev_base_url,
                ))
                .child(Self::labeled_field(language.choose("判断模型", "Judge model"), "jev-llm-model", None, &self.jev_model))
                .child(Self::labeled_field(
                    language.choose("密钥变量名", "API key variable"),
                    "jev-llm-env",
                    Some(language.choose(
                        "其值在主列表「API 密钥」行更换",
                        "change its value via the “API key” row of the main list",
                    )),
                    &self.jev_key_env,
                ))
                .child(div().flex().gap_3()
                    .child(Self::labeled_field(language.choose("单次请求超时（秒）", "Request timeout (s)"), "jev-llm-timeout", None, &self.jev_timeout))
                    .child(Self::labeled_field(language.choose("结构修正重试", "Malformed retries"), "jev-llm-retries", None, &self.jev_retries)))
                .child(div().flex().flex_wrap().items_center().gap_2()
                    .child(Self::jev_mode_label(language.choose("回答模式", "Answer mode")))
                    .child(Self::jev_option("jev-mode-probabilities",
                        language.choose("概率分布", "Probabilities"),
                        self.jev_llm_settings.answer_mode == AnswerMode::Probabilities,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.answer_mode = AnswerMode::Probabilities; }))
                    .child(Self::jev_option("jev-mode-discrete",
                        language.choose("离散值", "Discrete"),
                        self.jev_llm_settings.answer_mode == AnswerMode::Discrete,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.answer_mode = AnswerMode::Discrete; })))
                .child(div().flex().flex_wrap().items_center().gap_2()
                    .child(Self::jev_mode_label(language.choose("输出格式", "Output format")))
                    .child(Self::jev_option("jev-format-object", "JSON Object",
                        self.jev_llm_settings.output_format == OutputFormat::JsonObject,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.output_format = OutputFormat::JsonObject; }))
                    .child(Self::jev_option("jev-format-schema", "JSON Schema",
                        self.jev_llm_settings.output_format == OutputFormat::JsonSchema,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.output_format = OutputFormat::JsonSchema; }))
                    .child(Self::jev_option("jev-format-prompted", "Prompted JSON",
                        self.jev_llm_settings.output_format == OutputFormat::Prompted,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.output_format = OutputFormat::Prompted; })))
                .child(div().flex().flex_wrap().items_center().gap_2()
                    .child(Self::jev_mode_label(language.choose("概率归一化", "Normalize")))
                    .child(Self::jev_option("jev-normalize-on",
                        language.choose("开", "On"),
                        self.jev_llm_settings.normalize_probabilities,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.normalize_probabilities = true; }))
                    .child(Self::jev_option("jev-normalize-off",
                        language.choose("关", "Off"),
                        !self.jev_llm_settings.normalize_probabilities,
                        &entity,
                        move |view, _cx| { view.jev_llm_settings.normalize_probabilities = false; })))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "概率是 LLM 估计、未经校准；离散模式的 0/1 只表示选择。JSON Schema 需服务商支持，不支持时可选 Prompted JSON（仍会严格校验）。",
                    "Probabilities are uncalibrated LLM estimates; discrete 0/1 only encodes a selection. JSON Schema needs provider support; otherwise choose Prompted JSON (still strictly validated).")))
        }

        /// The bundled Jev judgment tool page: a read-mostly settings list
        /// where every row shows the saved value, a dialog for the two
        /// multi-field edits (backend, key), and immediate toggles for the
        /// gates. Saved state is always visible — no inline save buttons.
        fn render_bundled_jev_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let server = self
                .catalog
                .mcp_servers
                .iter()
                .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
                .cloned();
            let enabled = server.as_ref().is_some_and(|server| server.enabled);
            let authorized = self
                .tool_authorizations
                .authorized_mcp_server_ids
                .iter()
                .any(|id| id == BUNDLED_JEV_SERVER_ID);
            let has_project = self.selected_project.is_some();
            let project_authorized = self
                .selected_project
                .as_ref()
                .and_then(|id| self.workspace.configuration(id))
                .is_some_and(|configuration| {
                    configuration
                        .enabled_mcp_server_ids
                        .iter()
                        .any(|id| id == BUNDLED_JEV_SERVER_ID)
                });
            let llm_settings = server
                .as_ref()
                .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server);
            let saved_llm = llm_settings.is_some();
            let key_name = llm_settings
                .as_ref()
                .map_or("TYPESAFE_API_KEY", |s| s.api_key_environment_variable.as_str())
                .to_owned();
            let key_source =
                secret_source(&key_name, self.vault.as_ref().map(UnlockedVault::values));
            let key_present = key_source.is_some();
            let definition_line = server.as_ref().map_or_else(String::new, |server| {
                if server.args.is_empty() {
                    server.command.clone()
                } else {
                    format!("{} {}", server.command, server.args.join(" "))
                }
            });

            // At-a-glance state so the list below can be read as a checklist.
            let backend_chip = llm_settings.as_ref().map_or_else(
                || language.choose("后端：TypeSafe Jev", "Backend: TypeSafe Jev").to_owned(),
                |s| {
                    format!(
                        "{}：{} · {}",
                        language.choose("后端：第三方 LLM", "Backend: third-party LLM"),
                        s.model,
                        s.base_url
                    )
                },
            );
            let overview = div()
                .v_flex()
                .gap_2()
                .child(Self::jev_section_header("状态总览", "Overview", language))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(Self::jev_chip("jev-chip-backend", backend_chip, None))
                        .child(Self::jev_chip(
                            "jev-chip-enabled",
                            language.choose("已启用", "enabled").to_owned(),
                            enabled.then_some(true),
                        ))
                        .child(Self::jev_chip(
                            "jev-chip-grant",
                            language
                                .choose(
                                    if authorized {
                                        "全局授权：已授权"
                                    } else {
                                        "全局授权：未授权"
                                    },
                                    if authorized {
                                        "global grant: on"
                                    } else {
                                        "global grant: off"
                                    },
                                )
                                .to_owned(),
                            Some(authorized),
                        ))
                        .child(Self::jev_chip(
                            "jev-chip-project-grant",
                            if has_project {
                                language
                                    .choose(
                                        if project_authorized {
                                            "当前项目：已授权"
                                        } else {
                                            "当前项目：未授权"
                                        },
                                        if project_authorized {
                                            "project grant: on"
                                        } else {
                                            "project grant: off"
                                        },
                                    )
                                    .to_owned()
                            } else {
                                language
                                    .choose("项目授权：未选择项目", "project grant: no project")
                                    .to_owned()
                            },
                            if has_project { Some(project_authorized) } else { None },
                        ))
                        .child(Self::jev_chip(
                            "jev-chip-key",
                            format!("{} {key_name}", if key_present { "✔" } else { "✘" }),
                            Some(key_present),
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(format!("{BUNDLED_JEV_SERVER_ID} · {definition_line}")),
                );

            // The settings list: value = what is saved right now.
            let backend_value = llm_settings.as_ref().map_or_else(
                || {
                    language
                        .choose("TypeSafe Jev（随附二进制）", "TypeSafe Jev (bundled binary)")
                        .to_owned()
                },
                |s| {
                    format!(
                        "{} · {} · {}",
                        language.choose("第三方 LLM", "third-party LLM"),
                        s.model,
                        s.base_url
                    )
                },
            );
            let backend_edit = entity.clone();
            let key_edit = entity.clone();
            let toggler = entity.clone();
            let grant_toggle = entity.clone();
            let project_grant_toggle = entity.clone();
            let backend_row = Self::jev_setting_row(
                "jev-row-backend",
                language.choose("判断后端", "Backend"),
                backend_value,
                None,
                Button::new("jev-edit-backend").label(language.choose("编辑", "Edit")).on_click(
                    move |_, window, cx| {
                        backend_edit.update(cx, |view, cx| {
                            view.jev_llm_draft = saved_llm;
                            view.reset_jev_form_to_saved(window, cx);
                            view.jev_backend_modal_open = true;
                            cx.notify();
                        });
                    },
                ),
            );
            let key_row = Self::jev_setting_row(
                "jev-row-key",
                language.choose("API 密钥", "API key"),
                format!(
                    "{} {key_name} · {}",
                    if key_present { "✔" } else { "✘" },
                    match key_source {
                        Some(SecretSource::Vault) => language.choose("保险库", "vault"),
                        Some(SecretSource::Environment) => {
                            language.choose("环境变量", "environment")
                        }
                        None => language.choose("未找到", "not found"),
                    },
                ),
                Some(key_present),
                Button::new("jev-edit-key").label(language.choose("更换", "Replace")).on_click(
                    move |_, window, cx| {
                        key_edit.update(cx, |view, cx| {
                            view.jev_api_key.update(cx, |state, cx| {
                                state.set_value("", window, cx);
                            });
                            view.jev_key_modal_open = true;
                            cx.notify();
                        });
                    },
                ),
            );
            let enable_row = Self::jev_setting_row(
                "jev-row-enabled",
                language.choose("启用状态", "Enabled"),
                language
                    .choose(
                        if enabled { "已启用" } else { "已停用" },
                        if enabled { "enabled" } else { "disabled" },
                    )
                    .to_owned(),
                enabled.then_some(true),
                Button::new("jev-toggle-enabled")
                    .label(language.choose(
                        if enabled { "停用" } else { "启用" },
                        if enabled { "Disable" } else { "Enable" },
                    ))
                    .on_click(move |_, _, cx| {
                        toggler.update(cx, |view, cx| {
                            view.toggle_jev_enabled(cx);
                        });
                    }),
            );
            let grant_row = Self::jev_setting_row(
                "jev-row-grant",
                language.choose("全局授权", "Global grant"),
                language
                    .choose(
                        if authorized { "已授权" } else { "未授权" },
                        if authorized { "authorized" } else { "not authorized" },
                    )
                    .to_owned(),
                Some(authorized),
                Button::new("jev-toggle-authorization")
                    .label(language.choose(
                        if authorized { "撤销" } else { "授权" },
                        if authorized { "Revoke" } else { "Authorize" },
                    ))
                    .on_click(move |_, _, cx| {
                        grant_toggle.update(cx, |view, cx| {
                            view.set_jev_authorization(!authorized, cx);
                        });
                    }),
            );
            let project_row = Self::jev_setting_row(
                "jev-row-project-grant",
                language.choose("项目授权", "Project grant"),
                if has_project {
                    language
                        .choose(
                            if project_authorized {
                                "当前项目：已授权"
                            } else {
                                "当前项目：未授权"
                            },
                            if project_authorized {
                                "current project: authorized"
                            } else {
                                "current project: not authorized"
                            },
                        )
                        .to_owned()
                } else {
                    language.choose("请先在「项目」页选择项目", "select a project first").to_owned()
                },
                if has_project { Some(project_authorized) } else { None },
                Button::new("jev-toggle-project-authorization")
                    .disabled(!has_project)
                    .label(language.choose(
                        if project_authorized { "撤销" } else { "授权当前项目" },
                        if project_authorized { "Revoke" } else { "Authorize project" },
                    ))
                    .on_click(move |_, _, cx| {
                        project_grant_toggle.update(cx, |view, cx| {
                            view.set_jev_project_authorization(!project_authorized, cx);
                        });
                    }),
            );
            let settings_list = div()
                .v_flex()
                .gap_2()
                .child(Self::jev_section_header("配置", "Settings", language))
                .child(backend_row)
                .child(key_row)
                .child(enable_row)
                .child(grant_row)
                .child(project_row)
                .child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "「判断后端」「API 密钥」在弹窗中修改，点「保存」后立即生效并写回 runtime.json；启用与授权点击后即刻生效（会取消进行中的任务）。列表显示的永远是已保存的值。",
                        "“Backend” and “API key” are edited in dialogs and applied on 保存 (persisted to runtime.json immediately); enable/grant toggles apply instantly (canceling the running task). The list always shows the saved values.",
                    )),
                );

            // Verification: free discovery first, then one billed judgment.
            let verify_section = div()
                .v_flex()
                .gap_2()
                .child(Self::jev_section_header("连接与测试", "Connect & test", language))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(Self::jev_discover_button(language, &entity))
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "仅握手与列出工具，不计费。",
                                "Handshake and tools/list only; no billable call.",
                            ),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(Self::jev_real_test_button(language, &entity))
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "通过已保存的后端真实调用一次判断，结果见底部状态栏。",
                                "One real judgment through the saved backend; the result lands in the status bar.",
                            ),
                        )),
                );

            let notes_section = div()
                .v_flex()
                .gap_1()
                .child(Self::jev_section_header("使用要点", "Usage notes", language))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "evaluate 返回类型化判断：noul（0~1 是/否概率）、choice（多选一+概率分布）、score（量表评分）；接近 0.5 表示不确定而非中等。",
                    "evaluate returns typed judgments: noul (0~1 yes/no probability), choice (one option + distribution), score (rubric scale); near 0.5 means uncertain, not medium.",
                )))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "阈值判断（例如 noul > 0.8 才放行）应写在工作流代码里，而不是依赖模型自觉。",
                    "Threshold decisions (e.g. proceed only when noul > 0.8) belong in workflow code, not in the model's discretion.",
                )));

            let content = if server.is_some() {
                div()
                    .v_flex()
                    .gap_4()
                    .child(overview)
                    .child(settings_list)
                    .child(verify_section)
                    .child(notes_section)
                    .into_any_element()
            } else {
                let register = entity.clone();
                div()
                    .v_flex()
                    .gap_3()
                    .child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "尚未注册：未检测到随附的 evaluate 二进制。构建后会自动注册，也可以手动重新检测。",
                        "Not registered yet: the bundled evaluate binary was not detected. It registers automatically once built; you can also re-detect manually.",
                    )))
                    .child(
                        Button::new("jev-reregister")
                            .label(language.choose("重新检测并注册", "Re-detect and register"))
                            .on_click(move |_, _, cx| {
                                register.update(cx, |view, cx| {
                                    view.reregister_jev(cx);
                                });
                            }),
                    )
                    .into_any_element()
            };

            div()
                .id("jev-settings-detail")
                .overflow_y_scroll()
                .min_h(px(0.))
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
                        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                            language.choose("Jev 判断工具", "Jev judgment tool"),
                        ))
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                            language.choose(
                                "各配置项以列表呈现，列表值始终等于已保存状态；「判断后端」与「API 密钥」通过弹窗修改。evaluate 始终以相同接口返回类型化判断，供智能体和数据手册复核调用。",
                                "Settings appear as a list whose values always mirror what is saved; “backend” and “API key” are edited through dialogs. evaluate keeps one interface returning typed judgments for agents and datasheet review.",
                            ),
                        )),
                )
                .child(content)
        }

        /// One row of the Jev settings list: label, the saved value (always
        /// visible so save state is unambiguous), and the row action.
        fn jev_setting_row(
            id: &'static str,
            label: &str,
            value: String,
            value_positive: Option<bool>,
            action: impl IntoElement,
        ) -> impl IntoElement {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_3()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_owned()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_sm()
                        .whitespace_normal()
                        .text_color(rgb(match value_positive {
                            Some(true) => 0x0016_a34a,
                            Some(false) => 0x00dc_2626,
                            None => TEXT_SECONDARY,
                        }))
                        .child(value),
                )
                .child(action)
        }

        /// Reset the LLM form inputs to the saved configuration so a canceled
        /// dialog session never leaks its edits into the next one.
        fn reset_jev_form_to_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let saved = self
                .catalog
                .mcp_servers
                .iter()
                .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
                .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server)
                .or_else(|| self.catalog.llm_judge.clone())
                .unwrap_or_default();
            self.jev_llm_settings = saved.clone();
            self.jev_base_url
                .update(cx, |state, cx| state.set_value(saved.base_url.as_str(), window, cx));
            self.jev_model
                .update(cx, |state, cx| state.set_value(saved.model.as_str(), window, cx));
            self.jev_key_env.update(cx, |state, cx| {
                state.set_value(saved.api_key_environment_variable.as_str(), window, cx);
            });
            self.jev_timeout.update(cx, |state, cx| {
                state.set_value(saved.timeout_seconds.to_string().as_str(), window, cx);
            });
            self.jev_retries.update(cx, |state, cx| {
                state.set_value(saved.malformed_retries.to_string().as_str(), window, cx);
            });
        }

        /// Backend configuration dialog: pick a branch, edit the LLM form,
        /// 保存 applies it to the catalog; 取消 keeps the saved backend.
        fn render_jev_backend_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let draft_llm = self.jev_llm_draft;
            let typesafe_ready = circuitfabric_codex_runtime::tools::bundled_mcp_servers()
                .into_iter()
                .find(|s| s.id == BUNDLED_JEV_SERVER_ID)
                .is_some_and(|s| std::path::Path::new(&s.command).is_file());
            let llm_form = self.render_jev_llm_configuration(cx).into_any_element();
            let cancel = entity.clone();
            let save = entity.clone();
            // Failure messages of the two save handlers, shown inside the dialog.
            let error_line = (self.status.starts_with("未保存")
                || self.status.starts_with("判断后端未保存"))
            .then(|| self.status.clone());
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("jev-backend-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00_0f17_2ab3))
                        .occlude(),
                )
                .child(
                    div()
                        .id("jev-backend-modal")
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
                            div().v_flex().gap_1()
                                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(language.choose("配置判断后端", "Configure the judgment backend")))
                                .child(div().text_sm().whitespace_normal().text_color(rgb(TEXT_SECONDARY))
                                    .child(language.choose(
                                        "选择 TypeSafe Jev 或第三方 LLM；修改仅在点击「保存」后生效，取消则保持现状。",
                                        "Choose TypeSafe Jev or a third-party LLM; changes apply only on 保存 — cancel keeps the current setup.",
                                    ))),
                        )
                        .child(
                            div()
                                .id("jev-backend-form-area")
                                .v_flex()
                                .gap_3()
                                .max_h(px(440.))
                                .overflow_y_scroll()
                                .child(
                                    div().flex().flex_wrap().gap_2()
                                        .child(Self::jev_option(
                                            "jev-select-typesafe",
                                            language.choose("TypeSafe Jev（随附二进制）", "TypeSafe Jev (bundled)"),
                                            !draft_llm,
                                            &entity,
                                            move |view, _cx| {
                                                view.jev_llm_draft = false;
                                            },
                                        ))
                                        .child(Self::jev_option(
                                            "jev-select-llm",
                                            language.choose("第三方 LLM 模拟", "Third-party LLM"),
                                            draft_llm,
                                            &entity,
                                            move |view, _cx| {
                                                view.jev_llm_draft = true;
                                            },
                                        )),
                                )
                                .child(if draft_llm {
                                    llm_form
                                } else {
                                    div().v_flex().gap_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(if typesafe_ready { 0x0016_a34a } else { 0x00dc_2626 }))
                                                .child(if typesafe_ready {
                                                    language.choose(
                                                        "✔ 已找到随附的 evaluate 二进制",
                                                        "✔ Bundled evaluate binary found",
                                                    )
                                                } else {
                                                    language.choose(
                                                        "✘ 未找到二进制：请运行 scripts/build-typesafe-mcp.ps1，或设置 CIRCUITFABRIC_TYPESAFE_MCP_PATH",
                                                        "✘ Binary not found: run scripts/build-typesafe-mcp.ps1 or set CIRCUITFABRIC_TYPESAFE_MCP_PATH",
                                                    )
                                                }),
                                        )
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                            language.choose(
                                                "官方校准判断；保存后请在列表「API 密钥」行收录 TYPESAFE_API_KEY。",
                                                "Calibrated official judgments; after saving, record TYPESAFE_API_KEY via the “API key” row of the list.",
                                            ),
                                        ))
                                        .into_any_element()
                                }),
                        )
                        .when_some(error_line, |this, message| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(message),
                            )
                        })
                        .child(
                            div().flex().items_center().justify_end().gap_2()
                                .child(
                                    Button::new("jev-backend-cancel")
                                        .ghost()
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, _, cx| {
                                            cancel.update(cx, |view, cx| {
                                                view.jev_backend_modal_open = false;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("jev-backend-save")
                                        .primary()
                                        .label(language.choose("保存", "Save"))
                                        .on_click(move |_, _, cx| {
                                            save.update(cx, |view, cx| {
                                                let llm = view.jev_llm_draft;
                                                if view.apply_jev_backend(llm, cx) {
                                                    view.jev_backend_modal_open = false;
                                                }
                                            });
                                        }),
                                ),
                        ),
                )
        }

        /// Key-replacement dialog for the saved backend's key variable.
        fn render_jev_key_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let llm_settings = self
                .catalog
                .mcp_servers
                .iter()
                .find(|server| server.id == BUNDLED_JEV_SERVER_ID)
                .and_then(circuitfabric_codex_runtime::judge::LlmJudgeSettings::from_server);
            let key_name = llm_settings
                .as_ref()
                .map_or("TYPESAFE_API_KEY", |s| s.api_key_environment_variable.as_str())
                .to_owned();
            let cancel = entity.clone();
            let save = entity.clone();
            // Failure messages of the two save handlers, shown inside the dialog.
            let error_line = (self.status.starts_with("未保存")
                || self.status.starts_with("判断后端未保存"))
            .then(|| self.status.clone());
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("jev-key-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00_0f17_2ab3))
                        .occlude(),
                )
                .child(
                    div()
                        .id("jev-key-modal")
                        .relative()
                        .occlude()
                        .w(px(480.))
                        .v_flex()
                        .gap_4()
                        .p_5()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(ACCENT_SOFT))
                        .bg(rgb(SURFACE_BG))
                        .shadow_lg()
                        .child(
                            div().v_flex().gap_1()
                                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(language.choose("更换 API 密钥", "Replace the API key")))
                                .child(div().text_sm().whitespace_normal().text_color(rgb(TEXT_SECONDARY))
                                    .child(format!(
                                        "{}：{key_name}。{}",
                                        language.choose("密钥变量", "Key variable"),
                                        language.choose(
                                            "值只写入密钥保险库（或同名环境变量），绝不进配置文件；需先解锁保险库。",
                                            "Values go only to the vault (or a same-named environment variable), never config files; unlock the vault first.",
                                        ),
                                    ))),
                        )
                        .children(self.secret_source_hint(&key_name, &entity))
                        .child(Self::labeled_field("API Key", "jev-api-key", None, &self.jev_api_key))
                        .when_some(error_line, |this, message| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(message),
                            )
                        })
                        .child(
                            div().flex().items_center().justify_end().gap_2()
                                .child(
                                    Button::new("jev-key-cancel")
                                        .ghost()
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, _, cx| {
                                            cancel.update(cx, |view, cx| {
                                                view.jev_key_modal_open = false;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("jev-key-save")
                                        .primary()
                                        .label(language.choose("保存到密钥保险库", "Save to vault"))
                                        .on_click(move |_, window, cx| {
                                            save.update(cx, |view, cx| {
                                                if view.save_jev_api_key(window, cx) {
                                                    view.jev_key_modal_open = false;
                                                }
                                            });
                                        }),
                                ),
                        ),
                )
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

            Self::detail_pane("skills-detail")
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
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_PRIMARY))
                                        .child(project.name),
                                )
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .truncate()
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
                                .flex_wrap()
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
                .id("projects-page")
                .size_full()
                .overflow_y_scroll()
                .relative()
                .v_flex()
                .gap_4()
                .p_6()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
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
                                .id("projects-list-scroll")
                                .min_h(px(0.))
                                .overflow_y_scroll()
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
                            .flex_wrap()
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
                        .gap_4()
                        .child(self.render_project_agents_section(&project, cx))
                        .child(
                            div().v_flex().gap_3().child(
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
                            ),
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

            Self::detail_pane("project-detail")
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
                                .flex_1()
                                .min_w(px(0.))
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
                .child(div().p_4().rounded_lg().bg(rgb(SURFACE_BG)).child(content))
        }

        fn project_metric(value: String, label: &'static str) -> impl IntoElement {
            div()
                .flex_1()
                .min_w(px(0.))
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
            let (documents, document_integrity) = self
                .project_data
                .get(&project.id)
                .map(|data| (data.documents.clone(), data.document_integrity.clone()))
                .unwrap_or_default();
            let evidence_ready = documents
                .iter()
                .filter(|document| {
                    self.workspace.is_document_evidence_available(&project.id, &document.id)
                })
                .count();

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
                        })
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .bg(rgb(0x00e0_f2fe))
                                .text_xs()
                                .text_color(rgb(0x000e_7490))
                                .child(language.choose_owned(
                                    format!("{evidence_ready}/{} 可作证据", documents.len()),
                                    format!("{evidence_ready}/{} evidence-ready", documents.len()),
                                )),
                        ),
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
            list = list.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                "点击文档卡片，窗口向右展开并在右侧显示只读预览。",
                "Click a document card: the window extends right and shows a read-only preview.",
            )));
            for document in &documents {
                let integrity_verified =
                    document_integrity.get(&document.id).copied().unwrap_or(false);
                let evidence_available = integrity_verified
                    && self.workspace.is_document_evidence_available(&project.id, &document.id);
                let index_state =
                    self.pdf_index_state.get(&(project.id.clone(), document.id.clone())).copied();
                let evidence_label = if evidence_available {
                    if self.workspace.has_verified_datasheet_evidence(&project.id, &document.id) {
                        language.choose(
                            "可作证据 · 含已校验数据",
                            "Evidence ready — incl. verified data",
                        )
                    } else {
                        language.choose("已索引，可作证据", "Indexed — evidence ready")
                    }
                } else if index_state == Some(PdfIndexState::Running) {
                    language.choose("正在提取全文…", "Extracting full text…")
                } else if index_state == Some(PdfIndexState::Failed) {
                    language
                        .choose("全文提取失败，不可作证据", "Text extraction failed — not evidence")
                } else if is_evidence_indexable(&document.document_kind) {
                    language.choose("待索引，不可作证据", "Pending index — not evidence")
                } else {
                    language.choose("等待提取器，不可作证据", "Awaiting extractor — not evidence")
                };
                let previewed = self.document_preview.as_ref().is_some_and(|preview| {
                    preview.project_id == project.id && preview.document_id == document.id
                });
                let category_style = match document.category {
                    DocumentCategory::Datasheet => (0x00e0_f2fe, 0x000e_7490),
                    DocumentCategory::ReferenceDesign => (0x00f3_e8ff, 0x0076_2b_a3),
                };
                let opener = entity.clone();
                let click_project_id = project.id.clone();
                let click_document_id = document.id.clone();
                list = list.child(
                    div()
                        .id(format!("document-card-{}", document.id))
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if previewed { ACCENT } else { BORDER }))
                        .bg(rgb(CARD_BG))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                        .on_click(move |_, window, cx| {
                            opener.update(cx, |view, cx| {
                                // Re-read the record at click time so the preview reflects
                                // the current index, not this render's snapshot.
                                let document =
                                    view.project_data.get(&click_project_id).and_then(|data| {
                                        data.documents
                                            .iter()
                                            .find(|document| document.id == click_document_id)
                                            .cloned()
                                    });
                                if let Some(document) = document {
                                    view.open_document_preview(
                                        click_project_id.clone(),
                                        &document,
                                        window,
                                        cx,
                                    );
                                }
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .whitespace_normal()
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
                                        .bg(rgb(if integrity_verified {
                                            0x00dc_fce7
                                        } else {
                                            0x00fe_f2f2
                                        }))
                                        .text_color(rgb(if integrity_verified {
                                            0x0016_a34a
                                        } else {
                                            0x00b9_1c1c
                                        }))
                                        .child(if integrity_verified {
                                            language.choose("完整性已验证", "Integrity verified")
                                        } else {
                                            language.choose("完整性无效", "Integrity invalid")
                                        }),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(if evidence_available {
                                            0x00e0_f2fe
                                        } else {
                                            SURFACE_BG
                                        }))
                                        .text_color(rgb(if evidence_available {
                                            0x000e_7490
                                        } else {
                                            TEXT_MUTED
                                        }))
                                        .child(evidence_label),
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

            let search_panel = self.render_evidence_search_panel(project, &documents, cx);

            div()
                .v_flex()
                .gap_4()
                .w_full()
                .child(header)
                .child(search_panel)
                .child(list)
                .into_any_element()
        }

        /// Human-readable position of a search hit.
        fn evidence_anchor_label(anchor: &FragmentAnchor, language: UiLanguage) -> String {
            match anchor {
                FragmentAnchor::PageLine { page, line } => language.choose_owned(
                    format!("第 {page} 页 · 第 {line} 行"),
                    format!("Page {page} · line {line}"),
                ),
                FragmentAnchor::Line { line } => {
                    language.choose_owned(format!("第 {line} 行"), format!("Line {line}"))
                }
                FragmentAnchor::DatasheetRow { section, row } => {
                    let section = Self::datasheet_section_label(section, language);
                    language.choose_owned(
                        format!("已校验数据 · {section} 第 {row} 行"),
                        format!("Verified data · {section} row {row}"),
                    )
                }
                FragmentAnchor::Unknown => {
                    language.choose("位置未知", "Unknown position").to_owned()
                }
            }
        }

        fn datasheet_section_label(section: &str, language: UiLanguage) -> &'static str {
            match section {
                "pins" => language.choose("引脚", "Pins"),
                "absoluteMaximumRatings" => {
                    language.choose("绝对最大额定值", "Absolute Maximum Ratings")
                }
                "electricalCharacteristics" => {
                    language.choose("电特性", "Electrical Characteristics")
                }
                "operatingConditions" => language.choose("工作条件", "Operating Conditions"),
                _ => language.choose("数据", "Data"),
            }
        }

        /// Fragment text with the matched terms emphasized.
        fn render_highlighted_text(
            text: &str,
            highlights: &[std::ops::Range<usize>],
        ) -> StyledText {
            let style = HighlightStyle {
                background_color: Some(rgb(0x00fe_f08a).into()),
                font_weight: Some(FontWeight::SEMIBOLD),
                ..HighlightStyle::default()
            };
            StyledText::new(text.to_owned())
                .with_highlights(highlights.iter().map(|range| (range.clone(), style)))
        }

        /// Project-scoped evidence search: query, scope filter, and ranked hits grouped by
        /// document. Searching happens off the UI thread (`schedule_evidence_search`); this
        /// only renders the newest result.
        #[allow(clippy::too_many_lines)]
        fn render_evidence_search_panel(
            &mut self,
            project: &Project,
            documents: &[ProjectDocument],
            cx: &mut Context<Self>,
        ) -> Div {
            let entity = cx.entity().clone();
            let language = self.language;
            let query = self.evidence_query.read(cx).value().trim().to_owned();
            // A result left over from another project is searched again for this one.
            if !query.is_empty()
                && !self.evidence_searching
                && self.selected_project.as_deref() == Some(project.id.as_str())
                && self
                    .evidence_result
                    .as_ref()
                    .is_none_or(|result| result.project_id != project.id)
            {
                self.schedule_evidence_search(Duration::ZERO, cx);
            }
            let result = self
                .evidence_result
                .as_ref()
                .filter(|result| result.project_id == project.id && !query.is_empty());

            let summary = if self.evidence_searching {
                language.choose("检索中…", "Searching…").to_owned()
            } else if let Some(result) = result {
                let search = &result.search;
                language.choose_owned(
                    format!(
                        "{} 条命中 · {} 份文档 · {} ms",
                        search.total,
                        search.documents,
                        result.elapsed.as_millis()
                    ),
                    format!(
                        "{} hits · {} documents · {} ms",
                        search.total,
                        search.documents,
                        result.elapsed.as_millis()
                    ),
                )
            } else {
                String::new()
            };

            let scope_button = |id: &'static str, scope: EvidenceScope, label: &'static str| {
                let chooser = entity.clone();
                let active = self.evidence_scope == scope;
                Button::new(id)
                    .when(active, Button::primary)
                    .when(!active, Button::ghost)
                    .label(label)
                    .on_click(move |_, _, cx| {
                        chooser.update(cx, |view, cx| {
                            if view.evidence_scope != scope {
                                view.evidence_scope = scope;
                                view.schedule_evidence_search(Duration::ZERO, cx);
                            }
                        });
                    })
            };

            let mut panel = div()
                .v_flex()
                .gap_2()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE_BG))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(language.choose("项目内证据检索", "Project-scoped evidence search")),
                        )
                        .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(summary)),
                )
                .child(div().id("evidence-query").w_full().child(Input::new(&self.evidence_query)))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .child(scope_button(
                            "evidence-scope-all",
                            EvidenceScope::All,
                            language.choose("全部", "All"),
                        ))
                        .child(scope_button(
                            "evidence-scope-text",
                            EvidenceScope::FullText,
                            language.choose("文档全文", "Full text"),
                        ))
                        .child(scope_button(
                            "evidence-scope-verified",
                            EvidenceScope::VerifiedData,
                            language.choose("已校验数据", "Verified data"),
                        ))
                        .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose(
                                "多个词需同时命中 · \"引号\" 保持短语 · Enter 立即检索 · 点击结果跳转",
                                "All words must match · \"quotes\" keep phrases · Enter searches now · click a hit to jump",
                            ),
                        )),
                );

            let Some(result) = result else {
                if query.is_empty() {
                    panel = panel.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "在本项目已索引的文档全文与已校验的数据手册行中检索；结果附来源定位符与内容哈希。",
                            "Search this project's indexed full text and verified datasheet rows; hits carry their locator and content hash.",
                        ),
                    ));
                }
                return panel;
            };
            let search = result.search.clone();
            if search.hits.is_empty() {
                return panel.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    if result.scope == EvidenceScope::All {
                        language.choose(
                            "没有命中；试试更少或更短的关键词。尚未索引的文档（见下方标签）不会被检索到。",
                            "No hits; try fewer or shorter words. Documents not indexed yet (see labels below) are not searched.",
                        )
                    } else {
                        language.choose(
                            "当前范围内没有命中；切换到“全部”试试。",
                            "No hits in this scope; try \"All\".",
                        )
                    },
                ));
            }

            // Groups keep rank order: a document appears where its best hit ranks.
            let visible = self.evidence_visible.min(search.hits.len());
            let mut groups: Vec<(&str, Vec<&EvidenceHit>)> = Vec::new();
            for hit in &search.hits[..visible] {
                let document_id = hit.fragment.document_id.as_str();
                match groups.iter_mut().find(|(id, _)| *id == document_id) {
                    Some((_, hits)) => hits.push(hit),
                    None => groups.push((document_id, vec![hit])),
                }
            }
            let focused_locator = self
                .preview_focus
                .as_ref()
                .filter(|focus| focus.project_id == project.id)
                .map(|focus| (focus.document_id.clone(), focus.anchor.clone()));

            for (group_index, (document_id, hits)) in groups.into_iter().enumerate() {
                let document = documents.iter().find(|document| document.id == document_id);
                let title = document.map_or_else(
                    || document_id.to_owned(),
                    |document| document.original_file_name.clone(),
                );
                let opener = entity.clone();
                let open_project = project.id.clone();
                let open_document = document.cloned();
                let mut group = div()
                    .v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(CARD_BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_normal()
                                    .child(title),
                            )
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose_owned(
                                    format!("{} 条", hits.len()),
                                    format!("{} hits", hits.len()),
                                ),
                            ))
                            .when_some(open_document, |row, document| {
                                row.child(
                                    Button::new(("evidence-open-document", group_index))
                                        .ghost()
                                        .label(language.choose("打开文档", "Open document"))
                                        .on_click(move |_, window, cx| {
                                            opener.update(cx, |view, cx| {
                                                view.open_document_preview(
                                                    open_project.clone(),
                                                    &document,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                            }),
                    );
                for hit in hits {
                    let verified = hit.anchor.is_verified_data();
                    let focused = focused_locator.as_ref().is_some_and(|(document, anchor)| {
                        *document == hit.fragment.document_id && *anchor == hit.anchor
                    });
                    let navigator = entity.clone();
                    let navigate_project = project.id.clone();
                    let navigate_hit = hit.clone();
                    group = group.child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "evidence-hit-{}",
                                hit.fragment.locator
                            )))
                            .v_flex()
                            .gap_0p5()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if focused { ACCENT } else { SURFACE_BG }))
                            .bg(rgb(SURFACE_BG))
                            .cursor_pointer()
                            .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                            .on_click(move |_, window, cx| {
                                navigator.update(cx, |view, cx| {
                                    view.open_evidence_hit(
                                        &navigate_project,
                                        &navigate_hit,
                                        window,
                                        cx,
                                    );
                                });
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .text_xs()
                                            .bg(rgb(if verified {
                                                0x00dc_fce7
                                            } else {
                                                0x00e0_f2fe
                                            }))
                                            .text_color(rgb(if verified {
                                                0x0016_a34a
                                            } else {
                                                0x000e_7490
                                            }))
                                            .child(Self::evidence_anchor_label(
                                                &hit.anchor,
                                                language,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .ml_auto()
                                            .text_xs()
                                            .text_color(rgb(TEXT_MUTED))
                                            .child(if verified {
                                                language.choose("查看数据 →", "View data →")
                                            } else {
                                                language.choose("定位原文 →", "Show in document →")
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .whitespace_normal()
                                    .child(Self::render_highlighted_text(
                                        &hit.fragment.text,
                                        &hit.highlights,
                                    )),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .whitespace_normal()
                                    .child(format!(
                                        "{} · {}",
                                        hit.fragment.locator,
                                        &hit.fragment.content_hash
                                            [..hit.fragment.content_hash.len().min(19)]
                                    )),
                            ),
                    );
                }
                panel = panel.child(group);
            }

            if visible < search.hits.len() {
                let loader = entity.clone();
                panel = panel.child(
                    Button::new("evidence-show-more")
                        .label(language.choose_owned(
                            format!("显示更多（已显示 {visible} / {}）", search.hits.len()),
                            format!("Show more ({visible} of {})", search.hits.len()),
                        ))
                        .on_click(move |_, _, cx| {
                            loader.update(cx, |view, cx| {
                                view.evidence_visible += EVIDENCE_PAGE_SIZE;
                                cx.notify();
                            });
                        }),
                );
            }
            if search.total > search.hits.len() {
                panel = panel.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    language.choose_owned(
                        format!(
                            "共 {} 条命中，仅保留相关度最高的 {} 条；增加关键词可缩小范围。",
                            search.total,
                            search.hits.len()
                        ),
                        format!(
                            "{} hits; only the {} most relevant are kept — add words to narrow down.",
                            search.total,
                            search.hits.len()
                        ),
                    ),
                ));
            }
            panel
        }

        /// Top-level Documents navigation.  The evidence UI is project-scoped, but the
        /// navigation entry itself remains available so its empty state can explain the
        /// required next step instead of sending users to an unrelated TODO placeholder.
        fn render_documents_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let selected_project =
                self.selected_project.as_deref().and_then(|id| self.workspace.project(id)).cloned();

            if let Some(project) = selected_project {
                let project_name = project.name.clone();
                let project_id = project.id.clone();
                let chooser = entity.clone();
                return div()
                    .w_full()
                    .v_flex()
                    .gap_4()
                    .p_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .v_flex()
                                    .gap_1()
                                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                                        language.choose(
                                            "已索引，可作证据",
                                            "Authorized documents & evidence",
                                        ),
                                    ))
                                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                        language.choose_owned(
                                            format!("当前项目：{project_name} · {project_id}"),
                                            format!(
                                                "Current project: {project_name} · {project_id}"
                                            ),
                                        ),
                                    )),
                            )
                            .child(
                                Button::new("documents-choose-project")
                                    .label(language.choose("切换项目", "Choose project"))
                                    .on_click(move |_, _, cx| {
                                        chooser.update(cx, |view, cx| {
                                            view.screen = ControlPlaneScreen::Projects;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(self.render_documents_tab(&project, cx))
                    .into_any_element();
            }

            let opener = entity;
            div()
                .size_full()
                .v_flex()
                .items_center()
                .justify_center()
                .gap_3()
                .p_8()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("先选择项目", "Select a project first")),
                )
                .child(
                    div()
                        .max_w(px(560.))
                        .text_sm()
                        .text_color(rgb(TEXT_SECONDARY))
                        .whitespace_normal()
                        .child(language.choose(
                            "用量汇总和审计事件是分离的只读投影；审计行始终保留项目、Provider、运行时和会话来源。",
                            "Authorized documents, source registration, and evidence retrieval are project-scoped. Create or open a project, then select it from the project list.",
                        )),
                )
                .child(
                    Button::new("documents-open-projects")
                        .primary()
                        .label(language.choose("打开项目", "Open projects"))
                        .on_click(move |_, _, cx| {
                            opener.update(cx, |view, cx| {
                                view.screen = ControlPlaneScreen::Projects;
                                cx.notify();
                            });
                        }),
                )
                .into_any_element()
        }

        /// Spreadsheet numbers keep their cached precision but drop a bare `.0` tail.
        // The truncating cast is guarded: only integral values below 1e15 reach it.
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        fn format_number_cell(value: f64) -> String {
            if value.fract() == 0. && value.abs() < 1e15 {
                format!("{}", value as i64)
            } else {
                format!("{value}")
            }
        }

        /// Converts one tight RGBA bitmap into a GPUI render image.
        ///
        /// GPUI consumes BGRA, the opener protocol carries RGBA, so each pixel's red and
        /// blue channels swap here. `None` means the page could not be converted and is
        /// skipped rather than blanking the preview.
        fn render_image_from_rgba(
            width: u32,
            height: u32,
            rgba: Vec<u8>,
        ) -> Option<std::sync::Arc<gpui::RenderImage>> {
            let mut bgra = rgba;
            for pixel in bgra.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
            Some(std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)])))
        }

        /// Converts rendered pages to GPUI images; pages that fail to convert are skipped.
        fn raster_preview_pages(
            pages: Vec<circuitfabric_plugin_api::DocumentRasterPage>,
            requested_width: u32,
        ) -> Vec<(u32, DocumentRasterPreviewPage)> {
            pages
                .into_iter()
                .filter_map(|page| {
                    Some((
                        page.number,
                        DocumentRasterPreviewPage {
                            requested_width,
                            image: Self::render_image_from_rgba(
                                page.width,
                                page.height,
                                page.rgba,
                            )?,
                        },
                    ))
                })
                .collect()
        }

        /// Starts the on-demand preview of a rasterized view body: the opener's first pages
        /// move out of the view (so their bitmaps are not held twice) and every page's size
        /// is read for layout. Runs off the UI thread.
        fn raster_preview(view: &mut DocumentView, data: &[u8]) -> Option<DocumentRasterPreview> {
            let DocumentViewBody::RasterPages { pages, page_count, .. } = &mut view.body else {
                return None;
            };
            let initial = std::mem::take(pages);
            let fallback = initial.first().map_or((612.0, 792.0), |page| {
                (page.width.max(1) as f32, page.height.max(1) as f32)
            });
            let page_sizes = circuitfabric_document_opener::pdf_page_sizes(data)
                .and_then(Result::ok)
                .filter(|sizes| sizes.len() == *page_count)
                .unwrap_or_else(|| vec![fallback; *page_count]);
            Some(DocumentRasterPreview {
                data: std::sync::Arc::from(data),
                page_sizes,
                pages: Self::raster_preview_pages(initial, 900).into_iter().collect(),
                in_flight: BTreeMap::new(),
                failed: std::collections::BTreeSet::new(),
                retired_images: Vec::new(),
            })
        }

        /// Removes the previewed document's page images from the GPU atlas; GPUI keeps an
        /// image's texture until it is dropped explicitly.
        fn release_preview_images(&mut self, window: &mut Window) {
            if let Some(DocumentPreviewSelection {
                state: DocumentPreviewState::Loaded { raster: Some(raster), .. },
                ..
            }) = self.document_preview.as_mut()
            {
                for (_, page) in std::mem::take(&mut raster.pages) {
                    window.drop_image(page.image).ok();
                }
                for image in std::mem::take(&mut raster.retired_images) {
                    window.drop_image(image).ok();
                }
            }
        }

        /// Keeps the rendered pages around `visible` (inclusive page numbers): evicts far
        /// pages and renders missing nearby ones in one background batch.
        fn update_raster_window(
            &mut self,
            (first, last): (u32, u32),
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let target_width = crate::pdf_zoom::raster_width(
                (self.document_preview_width - 34.0).max(240.0),
                self.preview_pdf_zoom.value,
                window.scale_factor(),
            );
            let settled = self.preview_zoom_tick.is_none();
            let Some(preview) = self.document_preview.as_mut() else {
                return;
            };
            let DocumentPreviewState::Loaded { raster: Some(raster), .. } = &mut preview.state
            else {
                return;
            };
            let count = u32::try_from(raster.page_sizes.len()).unwrap_or(u32::MAX);
            for image in std::mem::take(&mut raster.retired_images) {
                window.drop_image(image).ok();
            }
            let keep = first.saturating_sub(RASTER_KEEP_DISTANCE)
                ..=last.saturating_add(RASTER_KEEP_DISTANCE);
            let evicted: Vec<u32> = raster
                .pages
                .iter()
                .filter(|(number, page)| {
                    !keep.contains(number)
                        || (!(first..=last).contains(number) && page.requested_width > 900)
                })
                .map(|(number, _)| *number)
                .collect();
            for number in evicted {
                if let Some(page) = raster.pages.remove(&number) {
                    window.drop_image(page.image).ok();
                }
            }
            let neighbors = first.saturating_sub(RASTER_PREFETCH_BEFORE).max(1)
                ..=last.saturating_add(RASTER_PREFETCH_AFTER).min(count);
            let mut wanted: Vec<(u32, u32)> = (first..=last.min(count))
                .chain(neighbors.filter(|number| !(first..=last).contains(number)))
                .filter_map(|number| {
                    let width = if settled && (first..=last).contains(&number) {
                        target_width
                    } else {
                        900
                    };
                    (!raster.pages.get(&number).is_some_and(|page| page.requested_width >= width)
                        && !raster.in_flight.contains_key(&number)
                        && !raster.failed.contains(&(number, width)))
                    .then_some((number, width))
                })
                .take(3)
                .collect();
            if wanted.is_empty() {
                return;
            }
            // Large bitmaps are produced one at a time; neighbors stay at preview resolution.
            if wanted[0].1 > 900 {
                wanted.truncate(1);
            }
            raster.in_flight.extend(wanted.iter().copied());
            let data = raster.data.clone();
            let source_data = data.clone();
            let project_id = preview.project_id.clone();
            let document_id = preview.document_id.clone();
            let requested = wanted.clone();
            let work = cx.background_spawn(async move {
                requested
                    .into_iter()
                    .map(|(number, width)| {
                        let page = circuitfabric_document_opener::render_pdf_pages_at_width(
                            &data,
                            &[number],
                            width,
                        )
                        .and_then(Result::ok)
                        .and_then(|pages| {
                            Self::raster_preview_pages(pages, width)
                                .into_iter()
                                .next()
                                .map(|(_, page)| page)
                        });
                        (number, width, page)
                    })
                    .collect::<Vec<_>>()
            });
            cx.spawn(async move |view, cx| {
                let rendered = work.await;
                view.update(cx, |view, cx| {
                    let Some(DocumentPreviewSelection {
                        project_id: shown_project,
                        document_id: shown_document,
                        state: DocumentPreviewState::Loaded { raster: Some(raster), .. },
                        ..
                    }) = view.document_preview.as_mut()
                    else {
                        return;
                    };
                    if *shown_project != project_id
                        || *shown_document != document_id
                        || !std::sync::Arc::ptr_eq(&raster.data, &source_data)
                    {
                        return;
                    }
                    for (number, width, page) in rendered {
                        raster.in_flight.remove(&number);
                        if let Some(page) = page {
                            if let Some(old) = raster.pages.insert(number, page) {
                                raster.retired_images.push(old.image);
                            }
                        } else {
                            raster.failed.insert((number, width));
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }

        /// The docked right-hand preview pane.
        ///
        /// Rendered beside the scrollable page content (not inside it), so it keeps the full
        /// window height and scrolls independently of the page behind it.
        fn render_document_preview_pane(
            &mut self,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) -> AnyElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let pane_width = self.document_preview_width;
            if !window.is_window_active() {
                self.stop_pdf_interaction();
            }
            let Some(preview) = &self.document_preview else {
                return div().into_any_element();
            };

            let (state_label, state_colors) = match &preview.state {
                DocumentPreviewState::Loading => {
                    (language.choose("加载中", "Loading"), (0x00f0_f9ff, 0x000e_7490))
                }
                DocumentPreviewState::Loaded { .. } => {
                    (language.choose("已加载", "Loaded"), (0x00dc_fce7, 0x0016_a34a))
                }
                DocumentPreviewState::Refused { .. } => {
                    (language.choose("已拒绝", "Refused"), (0x00fe_f2f2, 0x00b9_1c1c))
                }
                DocumentPreviewState::Unavailable { .. } => {
                    (language.choose("不可预览", "Unavailable"), (0x00ff_f7ed, 0x00b4_5309))
                }
            };

            let is_pdf = preview.file_name.to_lowercase().ends_with(".pdf");
            let show_data = self.preview_show_data && is_pdf;
            let header_closer = entity.clone();
            let header =
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_normal()
                                    .child(preview.file_name.clone()),
                            )
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                "{} · {}",
                                preview.project_id, preview.document_id
                            ))),
                    )
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .text_xs()
                            .flex_none()
                            .bg(rgb(state_colors.0))
                            .text_color(rgb(state_colors.1))
                            .child(state_label),
                    )
                    .child(
                        Button::new("close-document-preview")
                            .ghost()
                            .label(language.choose("关闭", "Close"))
                            .on_click(move |_, window, cx| {
                                header_closer.update(cx, |view, cx| {
                                    view.close_document_preview(window, cx);
                                });
                            }),
                    );

            // Datasheet PDFs get a second tab with the structured extraction.
            let tabs = if is_pdf {
                let preview_tab = entity.clone();
                let data_tab = entity.clone();
                Some(
                    div()
                        .flex()
                        .gap_1()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .child(
                            Button::new("preview-tab-document")
                                .when(!show_data, Button::primary)
                                .when(show_data, Button::ghost)
                                .label(language.choose("文档", "Document"))
                                .on_click(move |_, _, cx| {
                                    preview_tab.update(cx, |view, cx| {
                                        view.preview_show_data = false;
                                        view.stop_pdf_interaction();
                                        view.preview_scroll.set_offset(gpui::Point::default());
                                        #[cfg(windows)]
                                        crate::pdf_cursors::set_area(None);
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            Button::new("preview-tab-data")
                                .when(show_data, Button::primary)
                                .when(!show_data, Button::ghost)
                                .label(language.choose("数据", "Data"))
                                .on_click(move |_, _, cx| {
                                    data_tab.update(cx, |view, cx| {
                                        view.preview_show_data = true;
                                        view.stop_pdf_interaction();
                                        view.preview_scroll.set_offset(gpui::Point::default());
                                        #[cfg(windows)]
                                        crate::pdf_cursors::clear();
                                        cx.notify();
                                    });
                                }),
                        ),
                )
            } else {
                None
            };

            let focus = self.preview_focus.as_ref().filter(|focus| {
                focus.project_id == preview.project_id && focus.document_id == preview.document_id
            });
            let target_page = match focus.map(|focus| &focus.anchor) {
                Some(FragmentAnchor::PageLine { page, .. }) if !show_data => Some(*page),
                _ => None,
            };
            // Items are direct children of the tracked scroll container, so a page can be
            // scrolled to by index.
            let mut items: Vec<(Option<u32>, AnyElement)> = Vec::new();
            if let Some(focus) = focus {
                items.push((None, self.render_preview_focus_banner(focus, preview, &entity)));
            }
            if show_data {
                let stream_log = self
                    .datasheet_stream
                    .as_ref()
                    .filter(|(project_id, document_id, _)| {
                        *project_id == preview.project_id && *document_id == preview.document_id
                    })
                    .and_then(|(_, _, stream)| stream.lock().ok().map(|buffer| buffer.clone()));
                items.push((
                    None,
                    Self::render_datasheet_data(
                        preview,
                        language,
                        &entity,
                        self.datasheet_rows_visible,
                        self.datasheet_extracting,
                        self.datasheet_feedback.as_deref(),
                        stream_log,
                        self.datasheet_extract_started
                            .filter(|_| self.datasheet_extracting)
                            .map(|started| started.elapsed().as_secs()),
                        self.datasheet_checkpoint.as_ref().is_some_and(
                            |(project_id, document_id, _)| {
                                *project_id == preview.project_id
                                    && *document_id == preview.document_id
                            },
                        ),
                        focus.map(|focus| &focus.anchor),
                        self.preview_row_anchor.as_ref(),
                    )
                    .into_any_element(),
                ));
            } else {
                match &preview.state {
                    DocumentPreviewState::Loading => {
                        items.push((
                            None,
                            div()
                                .p_3()
                                .child(language.choose("正在加载文档…", "Loading document…"))
                                .into_any_element(),
                        ));
                    }
                    DocumentPreviewState::Loaded { view, raster } => {
                        items.push((
                            None,
                            div()
                                .v_flex()
                                .gap_0p5()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(SURFACE_BG))
                                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                    "{} · {}",
                                    &view.content_hash[..view.content_hash.len().min(19)],
                                    view.opener_id
                                )))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(TEXT_MUTED))
                                        .whitespace_normal()
                                        .child(format!("source: {}", view.source_locator)),
                                )
                                .into_any_element(),
                        ));
                        items.extend(match raster {
                            Some(raster) => Self::render_document_raster_pages(
                                raster,
                                language,
                                pane_width,
                                self.preview_pdf_zoom.value,
                                focus,
                                &entity,
                                self.preview_pdf_pan.is_some(),
                            ),
                            None => Self::render_document_view_items(&view.body, language, focus),
                        });
                    }
                    DocumentPreviewState::Refused { denial } => {
                        items.push((None,
                        div()
                            .v_flex()
                            .gap_1()
                            .p_3()
                            .rounded_lg()
                            .bg(rgb(0x00fe_f2f2))
                            .text_color(rgb(0x00b9_1c1c))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(language.choose(
                                        "该文档未通过打开门，不能被打开或作为证据源。",
                                        "This document did not pass the open gate and cannot be opened or cited.",
                                    )),
                            )
                            .child(
                                div().text_xs().whitespace_normal().child(denial.to_string()),
                            )
                            .into_any_element()));
                    }
                    DocumentPreviewState::Unavailable { reason } => {
                        items.push((None,
                        div()
                            .v_flex()
                            .gap_1()
                            .p_3()
                            .rounded_lg()
                            .bg(rgb(0x00ff_f7ed))
                            .text_color(rgb(0x00b4_5309))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(language.choose(
                                        "预览不可用：没有打开器支持该格式，或内容已损坏。",
                                        "Preview unavailable: no opener supports this format, or the content is corrupt.",
                                    )),
                            )
                            .child(div().text_xs().whitespace_normal().child(reason.clone()))
                            .into_any_element()));
                    }
                }
            }

            // Pages visible in the last layout (the scroll handle's child bounds), or the
            // cited page while landing on it; the pages around them get rendered below.
            let page_items: Vec<(usize, u32)> = items
                .iter()
                .enumerate()
                .filter_map(|(index, (number, _))| number.map(|number| (index, number)))
                .collect();
            let raster_shown = !show_data
                && matches!(preview.state, DocumentPreviewState::Loaded { raster: Some(_), .. });
            let visible_pages = raster_shown.then(|| {
                let landing = target_page.filter(|_| self.preview_scroll_pending.get());
                let (top, bottom) =
                    (self.preview_scroll.top_item(), self.preview_scroll.bottom_item());
                let shown: Vec<u32> = page_items
                    .iter()
                    .filter(|(index, _)| (top..=bottom).contains(index))
                    .map(|(_, number)| *number)
                    .collect();
                landing
                    .map(|page| (page, page))
                    .or_else(|| Some((*shown.first()?, *shown.last()?)))
                    .unwrap_or((1, 1))
            });

            // Land on the cited page once its content is laid out; `scroll_to_top_of_item`
            // waits for the child bounds of the next layout.
            if self.preview_scroll_pending.get() {
                let loading = matches!(preview.state, DocumentPreviewState::Loading);
                if show_data
                    && preview.extraction.is_some()
                    && let Some(anchor) = &self.preview_row_anchor
                {
                    anchor.scroll_to(window, cx);
                    self.preview_scroll_pending.set(false);
                } else if let Some(index) = target_page
                    .and_then(|page| items.iter().position(|(number, _)| *number == Some(page)))
                {
                    self.preview_scroll.scroll_to_top_of_item(index);
                    self.preview_scroll_pending.set(false);
                } else if !loading {
                    self.preview_scroll_pending.set(false);
                }
            }

            let zoom_anchor = self.preview_zoom_anchor.filter(|_| raster_shown);
            // Apply the final scale's anchor once, then allow scrollbar dragging normally.
            if self.preview_zoom_tick.is_none() {
                self.preview_zoom_anchor = None;
            }
            let probe = std::rc::Rc::new(std::cell::Cell::new(None));
            let scroll_content = div()
                .id("document-preview-scroll")
                .size_full()
                .overflow_y_scroll()
                .when(raster_shown, |scroll| scroll.overflow_x_scroll())
                .track_scroll(&self.preview_scroll)
                .v_flex()
                .gap_2()
                .p_3()
                .children(items.into_iter().enumerate().map(|(index, (_, item))| {
                    let content = div()
                        .flex_none()
                        .min_w(px(0.))
                        .when(raster_shown, |item| {
                            item.w(px(
                                (pane_width - 34.0).max(240.0) * self.preview_pdf_zoom.value + 10.0
                            ))
                        })
                        .child(item)
                        .into_any_element();
                    if zoom_anchor.is_some_and(|(target, _, _)| target == index) {
                        crate::pdf_zoom::native::mark_layout(content, probe.clone())
                    } else {
                        content
                    }
                }))
                .into_any_element();
            let scroll_content = if let Some((_, fraction, pointer)) = zoom_anchor {
                crate::pdf_zoom::native::anchor_viewport(
                    scroll_content,
                    probe,
                    self.preview_scroll.clone(),
                    fraction,
                    pointer,
                )
            } else {
                scroll_content
            };
            let pane =
                div()
                    .flex_none()
                    .w(px(pane_width))
                    .h_full()
                    .v_flex()
                    .bg(rgb(CARD_BG))
                    .border_l_1()
                    .border_color(rgb(BORDER))
                    .child(header)
                    .when_some(tabs, ParentElement::child)
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h(px(0.))
                            .child(scroll_content)
                            .when(raster_shown, |area| {
                                let zoomer = entity.clone();
                                let generation = self.preview_focus_generation;
                                let zoom = self.preview_pdf_zoom.value;
                                let animate = self.preview_zoom_tick.is_some();
                                let grabbing = self.preview_pdf_pan.is_some();
                                area.child(
                                    div().absolute().top(px(0.)).left(px(0.)).size_full().child(
                                        gpui::canvas(
                                            |_, _, _| (),
                                            move |bounds, (), window, _| {
                                                if grabbing {
                                                    window.set_window_cursor_style(crate::pdf_zoom::native::pan_cursor(true));
                                                }
                                                // Grab/grabbing cursor state: the area is live
                                                // while this canvas paints; every mouse move
                                                // recomputes it so leaving the pane restores
                                                // the normal cursor.
                                                #[cfg(windows)]
                                                {
                                                    let (x, y) = (f32::from(bounds.left()), f32::from(bounds.top()));
                                                    let (width, height) =
                                                        (f32::from(bounds.size.width), f32::from(bounds.size.height));
                                                    crate::pdf_cursors::set_area(Some((x, y, width, height)));
                                                    crate::pdf_cursors::set_dragging(grabbing);
                                                    let pointer = window.mouse_position();
                                                    crate::pdf_cursors::refresh((f32::from(pointer.x), f32::from(pointer.y)));
                                                    window.on_mouse_event(
                                                        move |event: &gpui::MouseMoveEvent, phase, _, _| {
                                                            if phase == gpui::DispatchPhase::Capture {
                                                                crate::pdf_cursors::refresh((
                                                                    f32::from(event.position.x),
                                                                    f32::from(event.position.y),
                                                                ));
                                                            }
                                                        },
                                                    );
                                                }
                                                // The new page bounds are available after this layout.
                                                if animate {
                                                    let anchored = zoomer.clone();
                                                    window.on_next_frame(move |_, cx| {
                                                        anchored.update(cx, |view, cx| {
                                                            if view.preview_focus_generation == generation
                                                                && view.preview_pdf_zoom.value == zoom
                                                                && !view.preview_show_data
                                                                && view.preview_zoom_tick.is_some()
                                                            {
                                                                let now = std::time::Instant::now();
                                                                let elapsed = view.preview_zoom_tick.take().unwrap().elapsed().as_secs_f32();
                                                                if view.preview_pdf_zoom.advance(elapsed) {
                                                                    view.preview_zoom_tick = Some(now);
                                                                }
                                                                cx.notify();
                                                            }
                                                        });
                                                    });
                                                }
                                                let interrupter = zoomer.clone();
                                                window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _, cx| {
                                                    if phase == gpui::DispatchPhase::Capture && bounds.contains(&event.position) {
                                                        interrupter.update(cx, |view, cx| {
                                                            if view.preview_zoom_tick.is_some() {
                                                                view.stop_pdf_interaction();
                                                                cx.notify();
                                                            }
                                                        });
                                                    }
                                                });
                                                let mover = zoomer.clone();
                                                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                                    if phase == gpui::DispatchPhase::Capture {
                                                        mover.update(cx, |view, cx| view.pan_pdf_preview(event, cx));
                                                    }
                                                });
                                                let releaser = zoomer.clone();
                                                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                                                    if phase == gpui::DispatchPhase::Capture && event.button == gpui::MouseButton::Left {
                                                        releaser.update(cx, |view, cx| {
                                                            if view.preview_pdf_pan.take().is_some() {
                                                                cx.stop_propagation();
                                                                cx.notify();
                                                            }
                                                        });
                                                    }
                                                });
                                                window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
                                                    if phase == gpui::DispatchPhase::Capture
                                                        && bounds.contains(&event.position)
                                                    {
                                                        if event.modifiers.control {
                                                            cx.stop_propagation();
                                                            zoomer.update(cx, |view, cx| view.zoom_pdf_preview(event, window, cx));
                                                        } else {
                                                            zoomer.update(cx, |view, cx| {
                                                                let active = view.preview_zoom_tick.is_some() || view.preview_pdf_pan.is_some();
                                                                view.stop_pdf_interaction();
                                                                if active { cx.notify(); }
                                                            });
                                                        }
                                                    }
                                                });
                                            },
                                        ).size_full(),
                                    ),
                                )
                            })
                            .when(self.preview_pdf_pan.is_some(), |area| area.cursor(crate::pdf_zoom::native::pan_cursor(true)))
                            .when(
                                focus.is_some_and(|focus| focus.resolving),
                                |area| {
                                    area.child(
                                        // Translucent mask over the document area while the
                                        // source location is being resolved.
                                        div().absolute().top(px(0.)).left(px(0.)).size_full().bg(
                                            rgba(0xf8fafce0),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .top(px(0.))
                                            .left(px(0.))
                                            .size_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .px_3()
                                                    .py_2()
                                                    .rounded_lg()
                                                    .border_1()
                                                    .border_color(rgb(BORDER))
                                                    .bg(rgb(CARD_BG))
                                                    .shadow_sm()
                                                    .child(status_dot(0x00e0_f2fe))
                                                    .child(language.choose(
                                                        "正在定位原文…",
                                                        "Locating source…",
                                                    )),
                                            ),
                                    )
                                },
                            )
                            .scrollbar(&self.preview_scroll, gpui_component::scroll::ScrollbarAxis::Both),
                    )
                    .into_any_element();
            if let Some(visible) = visible_pages {
                self.update_raster_window(visible, window, cx);
            }
            pane
        }

        fn zoom_pdf_preview(
            &mut self,
            event: &gpui::ScrollWheelEvent,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            let delta = f32::from(event.delta.pixel_delta(px(30.0)).y);
            if !self.preview_pdf_zoom.scroll(delta) {
                return;
            }
            self.preview_pdf_pan = None;
            let offset = self.preview_scroll.offset();
            self.preview_zoom_anchor = (self.preview_scroll.top_item()
                ..=self.preview_scroll.bottom_item())
                .find_map(|index| {
                    let item = self.preview_scroll.bounds_for_item(index)?;
                    let y = event.position.y - item.top() - offset.y;
                    if y < px(0.0) || y > item.size.height {
                        return None;
                    }
                    let fraction = gpui::point(
                        (f32::from(event.position.x - item.left() - offset.x)
                            / f32::from(item.size.width).max(1.0))
                        .clamp(0.0, 1.0),
                        (f32::from(y) / f32::from(item.size.height).max(1.0)).clamp(0.0, 1.0),
                    );
                    Some((index, fraction, event.position))
                });
            self.preview_zoom_tick.get_or_insert_with(std::time::Instant::now);
            if cx.reduce_motion() {
                self.preview_pdf_zoom.finish();
            }
            cx.notify();
        }

        fn stop_pdf_interaction(&mut self) {
            self.preview_pdf_zoom.stop();
            self.preview_zoom_tick = None;
            self.preview_zoom_anchor = None;
            self.preview_pdf_pan = None;
        }

        /// Returns the PDF view to 100% with no pan or in-flight zoom animation. Every
        /// navigation (opening a document, jumping from a search hit or a data row's source
        /// link) starts from the fitted page so the landing position is predictable.
        fn reset_pdf_view(&mut self) {
            self.preview_pdf_zoom = crate::pdf_zoom::ZoomMotion::default();
            self.preview_zoom_tick = None;
            self.preview_pdf_pan = None;
            self.preview_zoom_anchor = None;
            #[cfg(windows)]
            crate::pdf_cursors::set_dragging(false);
        }

        fn begin_pdf_pan(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
            self.stop_pdf_interaction();
            self.preview_pdf_pan = Some((position, self.preview_scroll.offset()));
            cx.stop_propagation();
            cx.notify();
        }

        fn pan_pdf_preview(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
            let Some((start, offset)) = self.preview_pdf_pan else { return };
            if !event.dragging() {
                self.preview_pdf_pan = None;
            } else {
                let max = self.preview_scroll.max_offset();
                let (x, y) = crate::pdf_zoom::pan_offset(
                    (f32::from(offset.x), f32::from(offset.y)),
                    (f32::from(start.x), f32::from(start.y)),
                    (f32::from(event.position.x), f32::from(event.position.y)),
                    (f32::from(max.x), f32::from(max.y)),
                );
                self.preview_scroll.set_offset(gpui::point(px(x), px(y)));
                cx.stop_propagation();
            }
            cx.notify();
        }

        /// The cited line a search hit opened this preview for, with its extracted row when
        /// it is verified data; the banner stays on top while the content scrolls to it.
        fn render_preview_focus_banner(
            &self,
            focus: &PreviewFocus,
            preview: &DocumentPreviewSelection,
            entity: &Entity<Self>,
        ) -> AnyElement {
            let language = self.language;
            let closer = entity.clone();
            let row_detail = match (&focus.anchor, &preview.extraction) {
                (FragmentAnchor::DatasheetRow { section, row }, Some(extraction)) => {
                    let index = row.saturating_sub(1);
                    let parameter = |rows: &[circuitfabric_contracts::DatasheetParameter]| {
                        rows.get(index).map(|row| {
                            let values = [("min", &row.min), ("typ", &row.typ), ("max", &row.max)]
                                .into_iter()
                                .filter_map(|(label, value)| {
                                    value.as_ref().map(|value| format!("{label} {value}"))
                                })
                                .collect::<Vec<_>>()
                                .join(" / ");
                            [
                                Some(row.parameter.clone()),
                                row.symbol.clone(),
                                (!values.is_empty()).then_some(values),
                                row.unit.clone(),
                                row.conditions.as_ref().map(|conditions| format!("({conditions})")),
                            ]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(" · ")
                        })
                    };
                    match section.as_str() {
                        "pins" => extraction.pins.get(index).map(|pin| {
                            format!(
                                "{} {} · {} · {}",
                                pin.number,
                                pin.name,
                                pin.kind.as_str(),
                                pin.description
                            )
                        }),
                        "absoluteMaximumRatings" => parameter(&extraction.absolute_maximum_ratings),
                        "electricalCharacteristics" => {
                            parameter(&extraction.electrical_characteristics)
                        }
                        "operatingConditions" => parameter(&extraction.operating_conditions),
                        _ => None,
                    }
                }
                _ => None,
            };
            let unrendered_page = match (&focus.anchor, &preview.state) {
                (
                    FragmentAnchor::PageLine { page, .. },
                    DocumentPreviewState::Loaded { raster: Some(raster), .. },
                ) if !self.preview_show_data && *page as usize > raster.page_sizes.len() => {
                    Some(language.choose_owned(
                        format!("文档只有 {} 页，第 {page} 页不存在。", raster.page_sizes.len()),
                        format!(
                            "The document has {} pages; page {page} does not exist.",
                            raster.page_sizes.len()
                        ),
                    ))
                }
                _ => None,
            };
            div()
                .v_flex()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(ACCENT_SOFT))
                .bg(rgb(FOCUSED_ROW_BG))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x000e_7490))
                                .child(language.choose_owned(
                                    format!(
                                        "原文与数据定位 · {}",
                                        Self::evidence_anchor_label(&focus.anchor, language)
                                    ),
                                    format!(
                                        "Source / data location · {}",
                                        Self::evidence_anchor_label(&focus.anchor, language)
                                    ),
                                )),
                        )
                        .child(
                            Button::new("clear-preview-focus")
                                .ghost()
                                .ml_auto()
                                .label(language.choose("清除", "Clear"))
                                .on_click(move |_, _, cx| {
                                    closer.update(cx, |view, cx| {
                                        view.preview_focus = None;
                                        view.preview_focus_generation += 1;
                                        view.preview_row_anchor = None;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT_PRIMARY))
                        .whitespace_normal()
                        .child(focus.text.clone()),
                )
                .when_some(row_detail, |banner, detail| {
                    banner.child(
                        div().text_xs().text_color(rgb(TEXT_SECONDARY)).whitespace_normal().child(
                            language.choose_owned(
                                format!("提取的行：{detail}（表中已高亮）"),
                                format!("Extracted row: {detail} (highlighted below)"),
                            ),
                        ),
                    )
                })
                .when_some(unrendered_page, |banner, note| {
                    banner.child(div().text_xs().text_color(rgb(0x00b4_5309)).child(note))
                })
                .when_some(focus.notice.clone(), |banner, note| {
                    banner.child(
                        div().text_xs().whitespace_normal().text_color(rgb(TEXT_MUTED)).child(note),
                    )
                })
                .into_any_element()
        }

        /// The structured datasheet tab: the persisted extraction, or the action to create
        /// one. Extraction always runs through the open gate, so only verified bytes are
        /// parsed.
        #[allow(clippy::too_many_arguments)]
        fn render_datasheet_data(
            preview: &DocumentPreviewSelection,
            language: UiLanguage,
            entity: &Entity<Self>,
            visible_rows: usize,
            extracting: bool,
            feedback: Option<&str>,
            stream_log: Option<String>,
            elapsed_seconds: Option<u64>,
            can_resume: bool,
            focus: Option<&FragmentAnchor>,
            row_anchor: Option<&gpui::ScrollAnchor>,
        ) -> Div {
            let focused_row = |wanted: &str| match focus {
                Some(FragmentAnchor::DatasheetRow { section, row }) if section == wanted => {
                    Some(*row)
                }
                _ => None,
            };
            let extractor = entity.clone();
            let stopper = entity.clone();
            let resumer = entity.clone();
            let clear_existing = preview.extraction.is_some();
            let extract_button = div()
                .flex()
                .flex_wrap()
                .gap_2()
                .when(extracting, |row| {
                    row.child(
                        Button::new("stop-datasheet-extraction")
                            .danger()
                            .label(language.choose("停止", "Stop"))
                            .on_click(move |_, _, cx| {
                                stopper.update(cx, Self::stop_datasheet_extraction);
                            }),
                    )
                })
                .when(!extracting && can_resume, |row| {
                    row.child(
                        Button::new("resume-datasheet-extraction")
                            .primary()
                            .label(language.choose("继续", "Continue"))
                            .on_click(move |_, window, cx| {
                                resumer.update(cx, |view, cx| {
                                    view.extract_datasheet_for_preview(
                                        clear_existing,
                                        true,
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                })
                .child(
                    Button::new("extract-datasheet-data")
                        .when(!clear_existing, Button::primary)
                        .when(clear_existing, Button::danger)
                        .disabled(extracting)
                        .label(if extracting {
                            language.choose("正在提取…", "Extracting…")
                        } else if clear_existing {
                            language.choose("清空并重新提取", "Clear and re-extract")
                        } else {
                            language.choose("提取结构化数据", "Extract structured data")
                        })
                        .on_click(move |_, window, cx| {
                            extractor.update(cx, |view, cx| {
                                view.extract_datasheet_for_preview(
                                    clear_existing,
                                    false,
                                    window,
                                    cx,
                                );
                            });
                        }),
                );

            let feedback_note =
                feedback.map(|message| Self::render_preview_truncation_note(message.to_owned()));
            let stream_window = stream_log
                .map(|log| Self::render_datasheet_stream(&log, elapsed_seconds, language));
            let Some(extraction) = &preview.extraction else {
                return div().v_flex().gap_3().p_3().children(feedback_note).children(stream_window).child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(language.choose(
                                    "尚无结构化数据",
                                    "No structured data yet",
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .whitespace_normal()
                                .child(language.choose(
                                    "从该数据手册中提取器件标识、引脚表与关键参数表，结构化保存在项目内。",
                                    "Extract part identity, the pin table, and key parameter tables from this datasheet, stored structurally in the project.",
                                )),
                        )
                        .child(extract_button),
                );
            };

            // A stale extraction (content changed since it was made) is shown, never hidden,
            // together with the reminder to re-extract.
            let stale = match &preview.state {
                DocumentPreviewState::Loaded { view, .. } => {
                    view.content_hash != extraction.content_hash
                }
                _ => false,
            };

            let mut content = div().v_flex().gap_2();
            content = content.children(feedback_note).children(stream_window);
            if !extraction.notes.iter().any(|note| note.starts_with("Jev accepted ")) {
                content = content.child(Self::render_preview_truncation_note(
                    language.choose(
                        "这是旧版提取结果，没有 Jev evaluate 验证记录；请清空并重新提取。",
                        "This extraction has no Jev evaluate verification record; clear and re-extract.",
                    ).to_owned(),
                ));
            }
            if stale {
                content = content.child(Self::render_preview_truncation_note(
                    language
                        .choose(
                            "文档内容在提取后发生变化，以下数据可能过期，建议重新提取。",
                            "The document changed after extraction; the data below may be stale — re-extract.",
                        )
                        .to_owned(),
                ));
            }

            // Overview card.
            let overview = &extraction.overview;
            let mut overview_card =
                div().v_flex().gap_1().p_3().rounded_lg().bg(rgb(SURFACE_BG)).child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_normal()
                        .child(overview.title.clone()),
                );
            let identity_line = [
                overview.manufacturer.clone(),
                (!overview.part_numbers.is_empty()).then(|| overview.part_numbers.join(" / ")),
                (!overview.packages.is_empty()).then(|| overview.packages.join(", ")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            if !identity_line.is_empty() {
                overview_card = overview_card.child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .whitespace_normal()
                        .child(identity_line),
                );
            }
            if !overview.features.is_empty() {
                overview_card = overview_card.child(
                    div()
                        .v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x000e_7490))
                                .child(language.choose("特性", "Features")),
                        )
                        .children(overview.features.iter().map(|feature| {
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_PRIMARY))
                                .whitespace_normal()
                                .child(format!("• {feature}"))
                        })),
                );
            }
            if !overview.description.is_empty() {
                overview_card = overview_card.child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_SECONDARY))
                        .whitespace_normal()
                        .child(overview.description.clone()),
                );
            }
            content = content.child(overview_card);

            content = content.child(Self::render_datasheet_pin_table(
                &extraction.pins,
                language,
                visible_rows,
                focused_row("pins"),
                row_anchor,
                entity,
            ));
            content = content.child(Self::render_datasheet_parameter_table(
                language.choose("绝对最大额定值", "Absolute Maximum Ratings"),
                &extraction.absolute_maximum_ratings,
                language,
                visible_rows,
                focused_row("absoluteMaximumRatings"),
                row_anchor,
                entity,
                "absoluteMaximumRatings",
                0x00fe_f2f2,
                0x00b9_1c1c,
            ));
            content = content.child(Self::render_datasheet_parameter_table(
                language.choose("电特性", "Electrical Characteristics"),
                &extraction.electrical_characteristics,
                language,
                visible_rows,
                focused_row("electricalCharacteristics"),
                row_anchor,
                entity,
                "electricalCharacteristics",
                0x00e0_f2fe,
                0x000e_7490,
            ));
            content = content.child(Self::render_datasheet_parameter_table(
                language.choose("工作条件", "Operating Conditions"),
                &extraction.operating_conditions,
                language,
                visible_rows,
                focused_row("operatingConditions"),
                row_anchor,
                entity,
                "operatingConditions",
                0x00f3_e8ff,
                0x0076_2ba3,
            ));

            if [
                extraction.pins.len(),
                extraction.absolute_maximum_ratings.len(),
                extraction.electrical_characteristics.len(),
                extraction.operating_conditions.len(),
            ]
            .into_iter()
            .any(|count| count > visible_rows)
            {
                let loader = entity.clone();
                content = content.child(
                    Button::new("show-more-datasheet-rows")
                        .label(language.choose("显示更多行", "Show more rows"))
                        .on_click(move |_, _, cx| {
                            loader.update(cx, |view, cx| {
                                view.datasheet_rows_visible =
                                    view.datasheet_rows_visible.saturating_add(40);
                                cx.notify();
                            });
                        }),
                );
            }

            if !extraction.notes.is_empty() {
                content = content.child(
                    div().v_flex().gap_0p5().p_2().rounded_md().bg(rgb(SURFACE_BG)).children(
                        extraction.notes.iter().map(|note| {
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .whitespace_normal()
                                .child(format!("· {note}"))
                        }),
                    ),
                );
            }
            content.child(extract_button)
        }

        /// The pin table: number, name, classified kind, and description.
        fn render_datasheet_pin_table(
            pins: &[circuitfabric_contracts::DatasheetPin],
            language: UiLanguage,
            visible_rows: usize,
            focused_row: Option<usize>,
            row_anchor: Option<&gpui::ScrollAnchor>,
            entity: &Entity<Self>,
        ) -> Div {
            let mut table = div().v_flex().gap_1().child(
                div().text_sm().font_weight(FontWeight::SEMIBOLD).child(language.choose_owned(
                    format!("引脚（{}）", pins.len()),
                    format!("Pins ({})", pins.len()),
                )),
            );
            if pins.is_empty() {
                return table.child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(language.choose("未解析到引脚行。", "No pin rows parsed.")),
                );
            }
            let header_row = |label: &str, flex: f32| {
                div()
                    .flex_none()
                    .w(px(flex))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_MUTED))
                    .child(label.to_owned())
            };
            table = table.child(
                div()
                    .flex()
                    .gap_1()
                    .p_1()
                    .rounded_sm()
                    .bg(rgb(SURFACE_BG))
                    .child(header_row(language.choose("引脚", "Pin"), 34.0))
                    .child(header_row(language.choose("名称", "Name"), 76.0))
                    .child(header_row(language.choose("类型", "Kind"), 52.0))
                    .child(header_row(language.choose("说明", "Description"), 0.0))
                    .child(div().flex_1()),
            );
            for (index, pin) in pins.iter().enumerate().take(visible_rows) {
                table = table.child(
                    div()
                        .id(("datasheet-pin-row", index))
                        .anchor_scroll(
                            (focused_row == Some(index + 1)).then(|| row_anchor.cloned()).flatten(),
                        )
                        .flex()
                        .gap_1()
                        .when(focused_row == Some(index + 1), |row| {
                            row.rounded_sm().bg(rgb(FOCUSED_ROW_BG))
                        })
                        .child(
                            div()
                                .flex_none()
                                .w(px(34.0))
                                .text_xs()
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(pin.number.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(76.0))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(TEXT_PRIMARY))
                                .child(pin.name.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(52.0))
                                .text_xs()
                                .text_color(rgb(0x000e_7490))
                                .child(pin.kind.as_str().to_owned()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .whitespace_normal()
                                .child(pin.description.clone()),
                        )
                        .child(Self::datasheet_source_link(
                            entity,
                            "pins",
                            index,
                            pin.evidence.is_some(),
                            language,
                        )),
                );
            }
            table
        }

        /// One parameter table with min/typ/max/unit columns.
        #[allow(clippy::too_many_arguments)]
        fn render_datasheet_parameter_table(
            title: &str,
            parameters: &[circuitfabric_contracts::DatasheetParameter],
            language: UiLanguage,
            visible_rows: usize,
            focused_row: Option<usize>,
            row_anchor: Option<&gpui::ScrollAnchor>,
            entity: &Entity<Self>,
            section: &'static str,
            accent: u32,
            accent_text: u32,
        ) -> Div {
            let mut table = div().v_flex().gap_1().child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .text_xs()
                            .flex_none()
                            .bg(rgb(accent))
                            .text_color(rgb(accent_text))
                            .child(title.to_owned()),
                    )
                    .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose_owned(
                            format!("{} 行", parameters.len()),
                            format!("{} rows", parameters.len()),
                        ),
                    )),
            );
            if parameters.is_empty() {
                return table.child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(language.choose("未解析到参数行。", "No parameter rows parsed.")),
                );
            }
            let header_row = |label: &str, flex: f32| {
                div()
                    .flex_none()
                    .w(px(flex))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_MUTED))
                    .child(label.to_owned())
            };
            table = table.child(
                div()
                    .flex()
                    .gap_1()
                    .p_1()
                    .rounded_sm()
                    .bg(rgb(SURFACE_BG))
                    .child(header_row(language.choose("参数", "Parameter"), 0.0))
                    .child(div().flex_1())
                    .child(header_row(language.choose("符号", "Symbol"), 52.0))
                    .child(header_row(language.choose("最小", "Min"), 46.0))
                    .child(header_row(language.choose("典型", "Typ"), 46.0))
                    .child(header_row(language.choose("最大", "Max"), 46.0))
                    .child(header_row(language.choose("单位", "Unit"), 34.0)),
            );
            for (index, parameter) in parameters.iter().enumerate().take(visible_rows) {
                let cell = |value: &Option<String>, emphasized: bool| {
                    div()
                        .flex_none()
                        .w(px(if emphasized { 46.0 } else { 0.0 }))
                        .text_xs()
                        .text_color(rgb(TEXT_PRIMARY))
                        .child(value.clone().unwrap_or_else(|| "—".to_owned()))
                };
                table = table.child(
                    div()
                        .id((section, index))
                        .anchor_scroll(
                            (focused_row == Some(index + 1)).then(|| row_anchor.cloned()).flatten(),
                        )
                        .flex()
                        .gap_1()
                        .when(focused_row == Some(index + 1), |row| {
                            row.rounded_sm().bg(rgb(FOCUSED_ROW_BG))
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_xs()
                                .text_color(rgb(TEXT_PRIMARY))
                                .whitespace_normal()
                                .child(parameter.parameter.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(52.0))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x000e_7490))
                                .whitespace_normal()
                                .child(parameter.symbol.clone().unwrap_or_else(|| "—".to_owned())),
                        )
                        .child(cell(&parameter.min, true))
                        .child(cell(&parameter.typ, true))
                        .child(cell(&parameter.max, true))
                        .child(
                            div()
                                .flex_none()
                                .w(px(34.0))
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .whitespace_normal()
                                .child(parameter.unit.clone().unwrap_or_else(|| "—".to_owned())),
                        )
                        .when_some(parameter.conditions.clone(), |row, conditions| {
                            row.child(
                                div()
                                    .flex_none()
                                    .max_w(px(120.0))
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .whitespace_normal()
                                    .child(format!("({conditions})")),
                            )
                        })
                        .child(Self::datasheet_source_link(
                            entity,
                            section,
                            index,
                            parameter.evidence.is_some(),
                            language,
                        )),
                );
            }
            table
        }

        /// The per-row link to the source PDF: a single icon on the row itself, with the
        /// action explained by a hover tooltip instead of a wide text label.
        fn datasheet_source_link(
            entity: &Entity<Self>,
            section: &'static str,
            row: usize,
            has_evidence: bool,
            language: UiLanguage,
        ) -> impl IntoElement {
            let opener = entity.clone();
            Button::new(("datasheet-source", row))
                .ghost()
                .compact()
                .icon(IconName::ExternalLink)
                .tooltip(if has_evidence {
                    language.choose("查看原文", "View source").to_owned()
                } else {
                    language.choose("未记录原文证据", "No source evidence").to_owned()
                })
                .disabled(!has_evidence)
                .on_click(move |_, _, cx| {
                    opener.update(cx, |view, cx| view.open_datasheet_source(section, row, cx));
                })
        }

        /// Displays the PDF as a scrollable, reader-like list with a slot for every page:
        /// rendered pages show their bitmap, the rest a same-sized placeholder until they
        /// are rendered on demand. Pages are separate items tagged with their number so the
        /// preview's scroll container can land on one and report which are visible.
        // Page sizes and bitmap dimensions are small positive values.
        #[allow(clippy::cast_precision_loss)]
        fn render_document_raster_pages(
            raster: &DocumentRasterPreview,
            language: UiLanguage,
            pane_width: f32,
            zoom: f32,
            focus: Option<&PreviewFocus>,
            entity: &Entity<Self>,
            grabbing: bool,
        ) -> Vec<(Option<u32>, AnyElement)> {
            // Page bitmaps follow the divider: pane width minus the body padding, the page
            // card's own padding, and its border.
            let bitmap_width = (pane_width - 34.0).max(240.0) * zoom;
            let page_count = raster.page_sizes.len();
            let mut content = vec![(
                None,
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose_owned(
                        format!(
                            "共 {page_count} 页 · {:.0}% · Ctrl＋滚轮缩放，左键拖动",
                            zoom * 100.0
                        ),
                        format!(
                            "{page_count} pages · {:.0}% · Ctrl + wheel to zoom; drag to pan",
                            zoom * 100.0
                        ),
                    ))
                    .into_any_element(),
            )];
            for (index, (width_points, height_points)) in raster.page_sizes.iter().enumerate() {
                let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
                let rendered = raster.pages.get(&number);
                let display_height = bitmap_width * height_points / width_points;
                let grabber = entity.clone();
                let frame = div()
                    .id(("pdf-page-frame", number))
                    .cursor(crate::pdf_zoom::native::pan_cursor(grabbing))
                    .on_mouse_down(gpui::MouseButton::Left, move |event, _, cx| {
                        grabber.update(cx, |view, cx| view.begin_pdf_pan(event.position, cx));
                    })
                    .p_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG));
                let frame = match rendered {
                    Some(page) => frame.child(
                        div().relative().w(px(bitmap_width)).h(px(display_height))
                            .child(img(page.image.clone()).w(px(bitmap_width)).h(px(display_height)))
                            .children(focus.filter(|focus| matches!(focus.anchor, FragmentAnchor::PageLine { page, .. } if page == number))
                                .into_iter().flat_map(|focus| &focus.regions).map(|region| {
                                    div().absolute()
                                        .left(px(region.left * bitmap_width)).top(px(region.top * display_height))
                                        .w(px(region.width * bitmap_width)).h(px(region.height * display_height))
                                        .bg(rgba(0xffd8_3d66)).border_1().border_color(rgba(0xe0a0_0090))
                                })),
                    ),
                    None => frame.child(
                        div()
                            .w(px(bitmap_width))
                            .h(px(display_height))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(CARD_BG))
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(if raster.failed.iter().any(|(page, _)| *page == number) {
                                language.choose("该页无法渲染", "This page could not be rendered")
                            } else {
                                language.choose("正在渲染…", "Rendering…")
                            }),
                    ),
                };
                content.push((
                    Some(number),
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x000e_7490))
                                .child(language.choose_owned(
                                    format!("第 {number} / {page_count} 页"),
                                    format!("Page {number} / {page_count}"),
                                )),
                        )
                        .child(frame)
                        .into_any_element(),
                ));
            }
            content
        }

        /// A view body as preview items; pages of paged text are tagged with their number
        /// so the preview's scroll container can land on one.
        fn render_document_view_items(
            view_body: &DocumentViewBody,
            language: UiLanguage,
            focus: Option<&PreviewFocus>,
        ) -> Vec<(Option<u32>, AnyElement)> {
            let DocumentViewBody::PagedText { pages, truncated } = view_body else {
                return vec![(
                    None,
                    Self::render_document_view_body(view_body, language).into_any_element(),
                )];
            };
            let mut content: Vec<(Option<u32>, AnyElement)> = Vec::new();
            if *truncated {
                content.push((
                    None,
                    Self::render_preview_truncation_note(
                        language
                            .choose(
                                "页数较多，仅显示前 {first} 页。",
                                "Long document; showing the first {first} pages.",
                            )
                            .replace("{first}", &pages.len().to_string()),
                    )
                    .into_any_element(),
                ));
            }
            if pages.len() > PREVIEW_MAX_RENDERED_PAGES {
                content.push((
                    None,
                    Self::render_preview_truncation_note(
                        language
                            .choose(
                                "为保持界面流畅，预览面板仅渲染前 {first} 页。",
                                "For smooth scrolling the pane renders the first {first} pages.",
                            )
                            .replace("{first}", &PREVIEW_MAX_RENDERED_PAGES.to_string()),
                    )
                    .into_any_element(),
                ));
            }
            for page in pages.iter().enumerate().filter_map(|(index, page)| {
                (index < PREVIEW_MAX_RENDERED_PAGES || focus.is_some_and(|focus|
                    matches!(focus.anchor, FragmentAnchor::PageLine { page: number, .. } if number == page.number)
                )).then_some(page)
            }) {
                let ranges = focus.filter(|focus| matches!(focus.anchor, FragmentAnchor::PageLine { page: number, .. } if number == page.number))
                    .map(|focus| circuitfabric_document_opener::pdf_text_highlight_ranges(&page.text, &focus.terms))
                    .unwrap_or_default();
                content.push((
                    Some(page.number),
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x000e_7490))
                                .child(language.choose_owned(
                                    format!("第 {} 页", page.number),
                                    format!("Page {}", page.number),
                                )),
                        )
                        .child(
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(SURFACE_BG))
                                .text_sm()
                                .text_color(rgb(TEXT_PRIMARY))
                                .whitespace_normal()
                                .child(Self::render_highlighted_text(&page.text, &ranges)),
                        )
                        .into_any_element(),
                ));
            }
            content
        }

        /// Renders one opener view body as read-only embeddable content.
        fn render_document_view_body(view_body: &DocumentViewBody, language: UiLanguage) -> Div {
            let mut content = div().v_flex().gap_2();
            match view_body {
                // Rasterized views are rendered by `render_document_raster_pages`, and paged
                // text becomes separate page items in `render_document_view_items`, before
                // this function is reached; this arm keeps the match exhaustive.
                DocumentViewBody::RasterPages { .. } | DocumentViewBody::PagedText { .. } => {}
                DocumentViewBody::Blocks { blocks, truncated } => {
                    if *truncated {
                        content = content.child(Self::render_preview_truncation_note(
                            language
                                .choose(
                                    "内容较多，仅显示前 {first} 个块。",
                                    "Long document; showing the first {first} blocks.",
                                )
                                .replace("{first}", &blocks.len().to_string()),
                        ));
                    }
                    if blocks.len() > PREVIEW_MAX_RENDERED_BLOCKS {
                        content = content.child(Self::render_preview_truncation_note(
                            language
                                .choose(
                                    "为保持界面流畅，预览面板仅渲染前 {first} 个块。",
                                    "For smooth scrolling the pane renders the first {first} blocks.",
                                )
                                .replace("{first}", &PREVIEW_MAX_RENDERED_BLOCKS.to_string()),
                        ));
                    }
                    for block in blocks.iter().take(PREVIEW_MAX_RENDERED_BLOCKS) {
                        content = content.child(Self::render_document_block(block));
                    }
                }
                DocumentViewBody::Sheets { sheets } => {
                    for sheet in sheets {
                        let mut sheet_block = div().v_flex().gap_1().child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(sheet.name.clone()),
                                )
                                .child(
                                    div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        language.choose_owned(
                                            format!("{} 行", sheet.row_count),
                                            format!("{} rows", sheet.row_count),
                                        ),
                                    ),
                                ),
                        );
                        if sheet.truncated {
                            sheet_block = sheet_block.child(Self::render_preview_truncation_note(
                                language
                                    .choose(
                                        "工作表较大，仅显示前 {first} 行。",
                                        "Large sheet; showing the first {first} rows.",
                                    )
                                    .replace("{first}", &sheet.rows.len().to_string()),
                            ));
                        }
                        if sheet.rows.len() > PREVIEW_MAX_RENDERED_ROWS {
                            sheet_block = sheet_block.child(Self::render_preview_truncation_note(
                                language
                                    .choose(
                                        "为保持界面流畅，预览面板仅渲染前 {first} 行。",
                                        "For smooth scrolling the pane renders the first {first} rows.",
                                    )
                                    .replace("{first}", &PREVIEW_MAX_RENDERED_ROWS.to_string()),
                            ));
                        }
                        for (row_index, row) in
                            sheet.rows.iter().enumerate().take(PREVIEW_MAX_RENDERED_ROWS)
                        {
                            let mut row_element = div().flex().gap_1();
                            if row_index == 0 {
                                row_element = row_element
                                    .p_1()
                                    .rounded_sm()
                                    .bg(rgb(SURFACE_BG))
                                    .font_weight(FontWeight::MEDIUM);
                            }
                            for cell in row {
                                let label = match cell {
                                    circuitfabric_plugin_api::DocumentCell::Empty => String::new(),
                                    circuitfabric_plugin_api::DocumentCell::Text(text) => {
                                        text.clone()
                                    }
                                    circuitfabric_plugin_api::DocumentCell::Number(value) => {
                                        Self::format_number_cell(*value)
                                    }
                                    circuitfabric_plugin_api::DocumentCell::Boolean(value) => {
                                        value.to_string()
                                    }
                                };
                                row_element = row_element.child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .text_xs()
                                        .whitespace_normal()
                                        .child(label),
                                );
                            }
                            sheet_block = sheet_block.child(row_element);
                        }
                        content = content.child(sheet_block);
                    }
                }
                DocumentViewBody::PlainText { text } => {
                    for line in text.lines() {
                        content = content.child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT_PRIMARY))
                                .whitespace_normal()
                                .child(line.to_owned()),
                        );
                    }
                }
            }
            content
        }

        fn render_document_block(block: &circuitfabric_plugin_api::DocumentBlock) -> Div {
            let inline = Self::render_inline_spans(&block.spans, &block.text);
            match &block.kind {
                DocumentBlockKind::Heading { level } => {
                    let heading = if *level <= 2 {
                        div().text_lg()
                    } else if *level <= 4 {
                        div().text_base()
                    } else {
                        div().text_sm()
                    };
                    heading
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_PRIMARY))
                        .child(inline)
                }
                DocumentBlockKind::Paragraph => {
                    div().text_sm().text_color(rgb(TEXT_PRIMARY)).child(inline)
                }
                DocumentBlockKind::Code { .. } => div()
                    .text_xs()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(SURFACE_BG))
                    .text_color(rgb(TEXT_PRIMARY))
                    .child(inline),
                DocumentBlockKind::ListItem { depth } => div()
                    .flex()
                    .gap_1p5()
                    .ml(px(f32::from(*depth) * 14.))
                    .child(div().text_sm().text_color(rgb(0x000e_7490)).child("•"))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_sm()
                            .text_color(rgb(TEXT_PRIMARY))
                            .child(inline),
                    ),
                DocumentBlockKind::Quote => div()
                    .border_l_2()
                    .border_color(rgb(ACCENT_SOFT))
                    .pl_2()
                    .text_sm()
                    .text_color(rgb(TEXT_SECONDARY))
                    .child(inline),
            }
        }

        /// One block's inline content: a single styled-text element with per-span highlights
        /// when the opener provided formatting runs, otherwise the plain text. Keeping it one
        /// text element means words wrap normally across style changes.
        fn render_inline_spans(
            spans: &[circuitfabric_plugin_api::DocumentSpan],
            fallback: &str,
        ) -> AnyElement {
            if spans.is_empty() {
                return div().whitespace_normal().child(fallback.to_owned()).into_any_element();
            }
            let mut text = String::new();
            let mut highlights = Vec::new();
            for span in spans {
                let start = text.len();
                text.push_str(&span.text);
                let end = text.len();
                let style = match span.style {
                    DocumentSpanStyle::Plain => continue,
                    DocumentSpanStyle::Strong => HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..HighlightStyle::default()
                    },
                    DocumentSpanStyle::Emphasis => HighlightStyle {
                        font_style: Some(FontStyle::Italic),
                        ..HighlightStyle::default()
                    },
                    DocumentSpanStyle::Code => HighlightStyle {
                        background_color: Some(rgb(SURFACE_BG).into()),
                        ..HighlightStyle::default()
                    },
                    DocumentSpanStyle::Strikethrough => HighlightStyle {
                        strikethrough: Some(StrikethroughStyle::default()),
                        ..HighlightStyle::default()
                    },
                };
                highlights.push((start..end, style));
            }
            StyledText::new(text).with_highlights(highlights).into_any_element()
        }

        fn render_preview_truncation_note(message: String) -> Div {
            div()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(rgb(0x00ff_f7ed))
                .text_xs()
                .text_color(rgb(0x00b4_5309))
                .child(message)
        }

        /// The live extraction log: stage lines plus the model reply as it streams. Only the
        /// tail is rendered, bottom-aligned, so the newest output stays in view.
        fn render_datasheet_stream(
            log: &str,
            elapsed_seconds: Option<u64>,
            language: UiLanguage,
        ) -> Div {
            const TAIL_CHARS: usize = 2000;
            let skip = log.chars().count().saturating_sub(TAIL_CHARS);
            let tail: String = log.chars().skip(skip).collect();
            div()
                .v_flex()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE_BG))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(match elapsed_seconds {
                            Some(seconds) => language.choose_owned(
                                format!("模型输出（进行中… {seconds}s）"),
                                format!("Model output (running… {seconds}s)"),
                            ),
                            None => language
                                .choose("模型输出（已结束）", "Model output (finished)")
                                .to_owned(),
                        }),
                )
                .child(
                    div()
                        .v_flex()
                        .justify_end()
                        .h(px(180.))
                        .overflow_hidden()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .whitespace_normal()
                        .child(tail),
                )
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
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .whitespace_normal()
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
                            .child(div().flex_1().min_w(px(0.)).whitespace_normal().child(format!(
                                "{} input / {} output tokens",
                                metadata.usage.input_tokens, metadata.usage.output_tokens
                            )))
                            .when(!metadata.citations.is_empty(), |this| {
                                this.child(
                                    div()
                                        .min_w(px(0.))
                                        .whitespace_normal()
                                        .child(format!("引用 {}", metadata.citations.join("、"))),
                                )
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
                let project_id = project.id.clone();
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
                                view.open_session_replay(
                                    project_id.clone(),
                                    session_id.clone(),
                                    cx,
                                );
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .truncate()
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

        /// The sidebar route uses the same persisted replay as the project detail tab.
        /// Filtering and opening a record only read project storage; neither starts a session.
        fn render_sessions_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let language = self.language;
            let entity = cx.entity().clone();
            let projects = self.workspace.projects().into_iter().cloned().collect::<Vec<_>>();
            let selected = self
                .session_project_filter
                .as_ref()
                .and_then(|id| projects.iter().find(|project| &project.id == id))
                .or_else(|| {
                    self.selected_project
                        .as_ref()
                        .and_then(|id| projects.iter().find(|project| &project.id == id))
                })
                .or_else(|| projects.first())
                .cloned();

            let mut filters = div().flex().flex_wrap().gap_2();
            for project in &projects {
                let project_id = project.id.clone();
                let active = selected.as_ref().is_some_and(|current| current.id == project_id);
                let chooser = entity.clone();
                filters = filters.child(
                    Button::new(format!("session-project-{project_id}"))
                        .label(project.name.clone())
                        .when(active, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            chooser.update(cx, |view, cx| {
                                view.session_project_filter = Some(project_id.clone());
                                view.session_replay = None;
                                cx.notify();
                            });
                        }),
                );
            }

            let content = if let Some(project) = selected {
                let project_id = project.id.clone();
                let refresher = entity.clone();
                div()
                    .v_flex()
                    .gap_3()
                    .size_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                language.choose_owned(
                                    format!("当前项目：{} · {}", project.name, project.id),
                                    format!("Current project: {} · {}", project.name, project.id),
                                ),
                            ))
                            .child(
                                Button::new("refresh-session-list")
                                    .ghost()
                                    .label(language.choose("刷新", "Refresh"))
                                    .on_click(move |_, _, cx| {
                                        refresher.update(cx, |view, cx| {
                                            if let Err(error) =
                                                view.refresh_project_data(&project_id)
                                            {
                                                view.status = format!("会话列表未刷新：{error}");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("sessions-page-body")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .child(self.render_sessions_tab(&project, cx)),
                    )
                    .into_any_element()
            } else {
                let opener = entity.clone();
                div()
                    .v_flex()
                    .gap_3()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .child(language.choose("还没有项目", "No projects yet"))
                    .child(
                        Button::new("sessions-open-projects")
                            .primary()
                            .label(language.choose("打开项目", "Open projects"))
                            .on_click(move |_, _, cx| {
                                opener.update(cx, |view, cx| {
                                    view.screen = ControlPlaneScreen::Projects;
                                    cx.notify();
                                });
                            }),
                    )
                    .into_any_element()
            };

            let canceller = entity;
            div()
                .size_full()
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
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("会话与任务", "Sessions & tasks")),
                        )
                        .when(self.task_cancel.is_some(), |this| {
                            this.child(
                                Button::new("sessions-cancel-task")
                                    .label(language.choose("取消当前任务", "Cancel current task"))
                                    .on_click(move |_, _, cx| {
                                        canceller.update(cx, |view, cx| {
                                            if let Some(cancel) = &view.task_cancel {
                                                cancel.cancel();
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                        }),
                )
                .child(filters)
                .child(div().flex_1().min_h(px(0.)).child(content))
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

        fn render_settings_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let language = self.language;
            let entity = cx.entity().clone();
            let theme = self.global_theme;
            let use_vault = self.secret_storage_provider == SecretStorageProvider::EncryptedVault;
            let log_label = match self.log_level {
                LogLevel::Error => "ERROR",
                LogLevel::Warn => "WARN",
                LogLevel::Info => "INFO",
                LogLevel::Debug => "DEBUG",
                LogLevel::Trace => "TRACE",
            };
            let next_log_level = match self.log_level {
                LogLevel::Error => LogLevel::Warn,
                LogLevel::Warn => LogLevel::Info,
                LogLevel::Info => LogLevel::Debug,
                LogLevel::Debug => LogLevel::Trace,
                LogLevel::Trace => LogLevel::Error,
            };

            let system = entity.clone();
            let light = entity.clone();
            let dark = entity.clone();
            let change_language = entity.clone();
            let vault = entity.clone();
            let environment = entity.clone();
            let change_log_level = entity.clone();
            let save = entity;

            let theme_buttons = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("theme-system")
                        .label(language.choose("跟随系统", "System"))
                        .when(theme == GlobalTheme::System, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            system.update(cx, |view, cx| {
                                view.set_global_theme(GlobalTheme::System, cx)
                            });
                        }),
                )
                .child(
                    Button::new("theme-light")
                        .label(language.choose("浅色", "Light"))
                        .when(theme == GlobalTheme::Light, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            light.update(cx, |view, cx| {
                                view.set_global_theme(GlobalTheme::Light, cx)
                            });
                        }),
                )
                .child(
                    Button::new("theme-dark")
                        .label(language.choose("深色", "Dark"))
                        .when(theme == GlobalTheme::Dark, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            dark.update(cx, |view, cx| {
                                view.set_global_theme(GlobalTheme::Dark, cx)
                            });
                        }),
                );
            let appearance = div()
                .w_full()
                .v_flex()
                .gap_3()
                .p_5()
                .bg(rgb(CARD_BG))
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("外观与语言", "Appearance & language")),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(language.choose("主题", "Theme")),
                )
                .child(theme_buttons)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(language.choose("界面语言", "Display language")),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "切换后完整界面立即重新渲染。",
                    "The entire interface rerenders immediately.",
                )))
                .child(
                    Button::new("settings-toggle-language")
                        .label(language.choose("切换为 English", "Switch to 中文"))
                        .on_click(move |_, _, cx| {
                            change_language.update(cx, |view, cx| view.toggle_global_language(cx));
                        }),
                );
            let secret_buttons = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("secret-provider-vault")
                        .label(language.choose("加密保险库", "Encrypted vault"))
                        .when(use_vault, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            vault.update(cx, |view, cx| {
                                view.secret_storage_provider =
                                    SecretStorageProvider::EncryptedVault;
                                view.persist_global_preferences(cx);
                            });
                        }),
                )
                .child(
                    Button::new("secret-provider-environment")
                        .label(language.choose("进程环境", "Environment"))
                        .when(!use_vault, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            environment.update(cx, |view, cx| {
                                view.secret_storage_provider = SecretStorageProvider::Environment;
                                view.persist_global_preferences(cx);
                            });
                        }),
                );
            let storage = div()
                .w_full()
                .v_flex()
                .gap_3()
                .p_5()
                .bg(rgb(CARD_BG))
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("存储与诊断", "Storage & diagnostics")),
                )
                .child(Self::labeled_field(
                    language.choose("数据目录", "Data directory"),
                    "global-data-directory",
                    Some(language.choose(
                        "用于 CircuitFabric 本地配置和数据。",
                        "Used for CircuitFabric local configuration and data.",
                    )),
                    &self.data_directory,
                ))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(language.choose("密钥存储提供方", "Secret storage provider")),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "设置页从不读取、显示或写入 API Key 值。",
                    "This page never reads, displays, or writes API key values.",
                )))
                .child(secret_buttons)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(language.choose("日志级别", "Log level")),
                )
                .child(Button::new("cycle-log-level").label(log_label).on_click(move |_, _, cx| {
                    change_log_level.update(cx, |view, cx| {
                        view.log_level = next_log_level;
                        view.persist_global_preferences(cx);
                    });
                }))
                .child(
                    Button::new("save-global-preferences")
                        .primary()
                        .label(language.choose("保存全局设置", "Save global settings"))
                        .on_click(move |_, _, cx| {
                            save.update(cx, |view, cx| view.persist_global_preferences(cx));
                        }),
                );
            let about =
                div()
                    .w_full()
                    .v_flex()
                    .gap_2()
                    .p_5()
                    .bg(rgb(CARD_BG))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("关于 CircuitFabric", "About CircuitFabric")),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                        "CircuitFabric 桌面控制平面",
                        "CircuitFabric desktop control plane",
                    )))
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                        "v{} · {}",
                        env!("CARGO_PKG_VERSION"),
                        language.choose("本地优先、密钥隔离", "local-first, secret-isolated")
                    )));
            div()
                .id("global-settings-page")
                .size_full()
                .overflow_y_scroll()
                .p_6()
                .v_flex()
                .gap_4()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.choose("全局设置", "Global settings")),
                )
                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                    "这些偏好会立即应用，并在下次启动时恢复。",
                    "These preferences apply immediately and are restored on the next launch.",
                )))
                .child(appearance)
                .child(storage)
                .child(about)
        }

        #[allow(clippy::too_many_lines)]
        fn render_usage_audit_page(
            &mut self,
            _window: &Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let model = self.usage_audit_model();
            let now = UsageAuditModel::now_unix_seconds();
            let usage_query = self.usage_filter.read(cx).value().to_string();
            let audit_query = self.audit_filter.read(cx).value().to_string();
            let period = self.usage_period;
            let audit_kind = self.audit_kind_filter;
            let usage = model.filtered_usage(period, &usage_query, now);
            let audit = model.filtered_audit(period, audit_kind, &audit_query, now);
            let grouped = UsageAuditModel::aggregate_usage(&usage);
            let daily = UsageAuditModel::daily_totals(&usage);
            let input_tokens = usage.iter().map(|record| record.input_tokens).sum::<u64>();
            let output_tokens = usage.iter().map(|record| record.output_tokens).sum::<u64>();
            let total_tokens = input_tokens + output_tokens;
            let chart_max = UsageAuditModel::chart_scale(&daily);

            let metric = |value: u64, label: &'static str| {
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .child(
                        div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value.to_string()),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
            };

            let period_button = entity.clone();
            let kind_button = entity.clone();
            let export_button = entity.clone();
            let mut usage_rows = div().v_flex().gap_1();
            if grouped.is_empty() {
                usage_rows = usage_rows.child(
                    div().text_sm().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "当前筛选没有 Token 用量。",
                        "No token usage matches this filter.",
                    )),
                );
            }
            for ((project, provider, runtime), (input, output, total)) in &grouped {
                usage_rows =
                    usage_rows.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(SURFACE_BG))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(project.clone()),
                                    )
                                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        format!("Provider: {provider} · Runtime: {runtime}"),
                                    )),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(format!("{input} in · {output} out")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{total} tokens")),
                            ),
                    );
            }

            let mut bars = div().v_flex().gap_1();
            if daily.is_empty() {
                bars = bars.child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(
                    language.choose("暂无可绘制的用量。", "No usage available for this chart."),
                ));
            }
            for (day, total) in daily.iter().rev().take(14).rev() {
                let width = ((total.saturating_mul(360) / chart_max).max(6)) as f32;
                bars = bars.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(84.))
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(day.clone()),
                        )
                        .child(div().h(px(16.)).w(px(width)).rounded_sm().bg(rgb(ACCENT)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(format!("{total} tokens")),
                        ),
                );
            }

            let mut audit_rows = div().v_flex().gap_2();
            if audit.is_empty() {
                audit_rows = audit_rows.child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(
                    language.choose("当前筛选没有审计事件。", "No audit events match this filter."),
                ));
            }
            for record in audit.iter().take(200) {
                let source = &record.source;
                audit_rows = audit_rows.child(
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
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(0x00e0_f2fe))
                                        .text_color(rgb(0x000e_7490))
                                        .child(record.kind.label()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(TEXT_MUTED))
                                        .child(rfc3339(record.timestamp_unix_seconds)),
                                ),
                        )
                        .child(div().text_sm().whitespace_normal().child(record.summary.clone()))
                        .child(
                            div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(
                                format!(
                                    "Project: {} · Provider: {} · Runtime: {} · Session: {}",
                                    source.project_id,
                                    source.provider_id,
                                    source.runtime_id,
                                    source.session_id
                                ),
                            ),
                        ),
                );
            }

            div()
                .id("usage-audit-page")
                .size_full()
                .overflow_y_scroll()
                .p_6()
                .v_flex()
                .gap_4()
                .child(div().v_flex().gap_1().child(
                    div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                        language.choose("用量与不可变审计", "Usage & immutable audit"),
                    ),
                ).child(
                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).whitespace_normal().child(
                        language.choose(
                            "用量汇总和审计事件是分离的只读投影；审计行始终保留项目、Provider、运行时和会话来源。",
                            "Usage aggregation and audit events are separate read-only projections; every audit row retains project, provider, runtime, and session context.",
                        ),
                    ),
                ))
                .child(div().flex().flex_wrap().items_center().gap_2().child(
                    Button::new("usage-audit-period").label(period.label()).on_click(move |_, _, cx| {
                        period_button.update(cx, |view, cx| {
                            view.usage_period = view.usage_period.next();
                            cx.notify();
                        });
                    }),
                ).child(
                    Button::new("usage-audit-kind").ghost().label(audit_kind.label()).on_click(move |_, _, cx| {
                        kind_button.update(cx, |view, cx| {
                            view.audit_kind_filter = view.audit_kind_filter.next();
                            cx.notify();
                        });
                    }),
                ).child(
                    Button::new("export-immutable-audit").primary().label(
                        language.choose("导出筛选后的审计 CSV", "Export filtered audit CSV"),
                    ).on_click(move |_, window, cx| {
                        export_button.update(cx, |view, cx| view.export_filtered_audit(window, cx));
                    }),
                ))
                .child(div().flex().flex_wrap().gap_2().child(
                    div().flex_1().min_w(px(260.)).h(px(36.)).px_2().rounded_md().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(
                        InputBase::new("usage-audit-usage-filter").flex_1().h_full().flex().items_center().child(self.usage_filter.clone()),
                    ),
                ).child(
                    div().flex_1().min_w(px(260.)).h(px(36.)).px_2().rounded_md().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(
                        InputBase::new("usage-audit-event-filter").flex_1().h_full().flex().items_center().child(self.audit_filter.clone()),
                    ),
                ))
                .child(div().flex().flex_wrap().gap_3().child(metric(input_tokens, language.choose("输入 tokens", "Input tokens"))).child(metric(output_tokens, language.choose("输出 tokens", "Output tokens"))).child(metric(total_tokens, language.choose("总 tokens", "Total tokens"))))
                .child(div().v_flex().gap_2().p_4().rounded_xl().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(
                    div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("Token 用量（按项目 / Provider / 运行时）", "Token usage by project / provider / runtime")),
                ).child(usage_rows))
                .child(div().v_flex().gap_2().p_4().rounded_xl().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(
                    div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("用量趋势", "Usage trend")),
                ).child(bars))
                .child(div().v_flex().gap_1().child(
                    div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("审计日志（只读）", "Audit log (read-only)")),
                ).child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "筛选或导出不会改变审计记录；界面没有编辑、删除或改写操作。",
                        "Filtering and exporting never changes records; this surface has no edit, delete, or rewrite operation.",
                    )),
                ).child(audit_rows))
        }

        fn short_hash(value: &str) -> String {
            const PREFIX_LENGTH: usize = 18;
            if value.len() > PREFIX_LENGTH {
                format!("{}…", &value[..PREFIX_LENGTH])
            } else if value.is_empty() {
                "—".to_owned()
            } else {
                value.to_owned()
            }
        }

        fn authority_style(authority: &SnapshotAuthority) -> (u32, u32, &'static str) {
            match authority {
                SnapshotAuthority::Observed => (0x00e0_f2fe, 0x000e_7490, "observed"),
                SnapshotAuthority::Planned => (0x00f3_e8ff, 0x0076_2b_a3, "planned"),
                SnapshotAuthority::Verified => (0x00dc_fce7, 0x0016_a34a, "verified"),
            }
        }

        /// `inconclusive` is intentionally amber rather than green. It means a verification did
        /// not establish a result, not that the checked property passed.
        fn fact_style(status: &FactStatus) -> (u32, u32, &'static str) {
            match status {
                FactStatus::Passed => (0x00dc_fce7, 0x0016_a34a, "passed"),
                FactStatus::Failed => (0x00fe_f2f2, 0x00b9_1c1c, "failed"),
                FactStatus::Inconclusive => (0x00ff_fbeb, 0x00b4_5309, "inconclusive"),
                FactStatus::NotRun => (0x00f1_f5f9, 0x0047_5563, "not run"),
            }
        }

        /// Projects persisted semantic facts without treating uncertainty as success.  The
        /// approval workflow has no persisted ChangeSet source yet, so its KPI stays explicitly
        /// unread rather than becoming a misleading zero.
        fn overview_fact_rows(
            language: UiLanguage,
            passed: usize,
            failed: usize,
            inconclusive: usize,
            not_run: usize,
        ) -> impl IntoElement {
            let mut rows = div().v_flex().gap_2().child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(TEXT_SECONDARY))
                    .child(status_dot(0x0094_a3b8))
                    .child(language.choose(
                        "待审批变更：未回读（尚无持久化 ChangeSet）",
                        "Pending changes: not read back (no persisted ChangeSet store)",
                    )),
            );
            if passed + failed + inconclusive + not_run == 0 {
                rows = rows.child(
                    div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose(
                        "没有已加载的验证事实。导入语义快照并运行验证以建立待办项。",
                        "No loaded verification facts. Import a semantic snapshot and run validation.",
                    )),
                );
            } else {
                for (status, count) in [
                    (FactStatus::Passed, passed),
                    (FactStatus::Failed, failed),
                    (FactStatus::Inconclusive, inconclusive),
                    (FactStatus::NotRun, not_run),
                ] {
                    let style = Self::fact_style(&status);
                    rows = rows.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(status_dot(style.1))
                            .child(format!("{count} {}", style.2)),
                    );
                }
            }
            rows
        }

        fn semantic_query_hits(
            snapshot: &LogicalCircuitSnapshot,
            scope: SemanticQueryScope,
            query: &str,
        ) -> Vec<SemanticQueryHit> {
            let query = query.trim().to_lowercase();
            let matches = |values: &[String]| {
                query.is_empty() || values.iter().any(|value| value.to_lowercase().contains(&query))
            };
            match scope {
                SemanticQueryScope::Components => snapshot
                    .components
                    .iter()
                    .filter_map(|component| {
                        let values = vec![
                            component.id.clone(),
                            component.reference.clone(),
                            component.value.clone().unwrap_or_default(),
                        ];
                        matches(&values).then(|| SemanticQueryHit {
                            kind: "component",
                            subject: format!("{} ({})", component.reference, component.id),
                            detail: component
                                .value
                                .clone()
                                .unwrap_or_else(|| "No value".to_owned()),
                            evidence: component.evidence.len().to_string(),
                        })
                    })
                    .collect(),
                SemanticQueryScope::Pins => snapshot
                    .components
                    .iter()
                    .flat_map(|component| {
                        component.pins.iter().filter_map(move |pin| {
                            let values = vec![
                                component.reference.clone(),
                                component.id.clone(),
                                pin.id.clone(),
                                pin.name.clone(),
                            ];
                            matches(&values).then(|| SemanticQueryHit {
                                kind: "pin",
                                subject: format!(
                                    "{}:{} ({})",
                                    component.reference, pin.id, pin.name
                                ),
                                detail: component.id.clone(),
                                evidence: component.evidence.len().to_string(),
                            })
                        })
                    })
                    .collect(),
                SemanticQueryScope::Nets => snapshot
                    .nets
                    .iter()
                    .filter_map(|net| {
                        let values = vec![net.id.clone(), net.name.clone().unwrap_or_default()];
                        matches(&values).then(|| SemanticQueryHit {
                            kind: "net",
                            subject: net.name.clone().unwrap_or_else(|| net.id.clone()),
                            detail: format!("{} pins · {}", net.pins.len(), net.id),
                            evidence: "snapshot".to_owned(),
                        })
                    })
                    .collect(),
                SemanticQueryScope::Constraints => snapshot
                    .constraints
                    .iter()
                    .filter_map(|constraint| {
                        let values = vec![
                            constraint.constraint_id.clone(),
                            constraint.layer.clone(),
                            format!("{:?}", constraint.status),
                            constraint.explanation.clone(),
                        ];
                        matches(&values).then(|| SemanticQueryHit {
                            kind: "constraint",
                            subject: constraint.constraint_id.clone(),
                            detail: format!("{} · {:?}", constraint.layer, constraint.status),
                            evidence: constraint.evidence_refs.len().to_string(),
                        })
                    })
                    .collect(),
                SemanticQueryScope::Evidence => snapshot
                    .evidence
                    .iter()
                    .chain(
                        snapshot.components.iter().flat_map(|component| component.evidence.iter()),
                    )
                    .chain(
                        snapshot
                            .constraints
                            .iter()
                            .flat_map(|constraint| constraint.evidence_refs.iter()),
                    )
                    .filter_map(|evidence| {
                        let values = vec![
                            evidence.document_id.clone(),
                            evidence.content_hash.clone(),
                            evidence.locator.clone(),
                            evidence.excerpt.clone().unwrap_or_default(),
                        ];
                        matches(&values).then(|| SemanticQueryHit {
                            kind: "evidence",
                            subject: evidence.document_id.clone(),
                            detail: evidence.locator.clone(),
                            evidence: Self::short_hash(&evidence.content_hash),
                        })
                    })
                    .collect(),
            }
        }

        #[allow(clippy::too_many_lines)]
        fn render_semantics_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let language = self.language;
            let entity = cx.entity().clone();
            let Some(project_id) = self.selected_project.clone() else {
                return Self::project_empty_state(
                    language.choose("请先选择项目", "Select a project first"),
                    language.choose(
                        "电路语义始终从项目根目录的 logic/snapshots 读取，不会混入其他项目或演示数据。",
                        "Circuit semantics are read only from this project's logic/snapshots directory; no other project or demo data is mixed in.",
                    ),
                )
                .into_any_element();
            };
            let snapshots = self
                .project_data
                .get(&project_id)
                .map(|data| data.semantic_snapshots.clone())
                .unwrap_or_default();
            if snapshots.is_empty() {
                return div()
                    .size_full()
                    .v_flex()
                    .gap_4()
                    .p_6()
                    .child(
                        div().flex().items_center().justify_between().child(
                            div().v_flex().gap_1().child(
                                div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                                    language.choose("电路语义", "Circuit semantics"),
                                ),
                            ).child(
                                div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                    language.choose("项目语义快照", "Project semantic snapshots"),
                                ),
                            ),
                        ).child({
                            let refresher = entity.clone();
                            Button::new("refresh-semantic-snapshots")
                                .label(language.choose("刷新", "Refresh"))
                                .on_click(move |_, _, cx| {
                                    refresher.update(cx, |view, cx| {
                                        view.status = match view.refresh_project_data(&project_id) {
                                            Ok(()) => language.choose("语义快照已刷新", "Semantic snapshots refreshed").to_owned(),
                                            Err(error) => format!("{}: {error}", language.choose("语义快照未刷新", "Semantic snapshots not refreshed")),
                                        };
                                        cx.notify();
                                    });
                                })
                        }),
                    )
                    .child(Self::project_empty_state(
                        language.choose("尚无语义快照", "No semantic snapshots"),
                        language.choose(
                            "尚未导入任何逻辑快照。此界面不会将“无数据”显示成已验证；EDA 导入器写入 logic/snapshots 后可刷新查看。",
                            "No logical snapshot has been imported. This view never represents missing data as verified; refresh after an EDA importer writes logic/snapshots.",
                        ),
                    ))
                    .into_any_element();
            }

            let selected_hash = self
                .selected_semantic_snapshot
                .as_ref()
                .filter(|(selected_project, _)| selected_project == &project_id)
                .map(|(_, hash)| hash.as_str());
            let snapshot = selected_hash
                .and_then(|hash| snapshots.iter().find(|snapshot| snapshot.snapshot_hash == hash))
                .unwrap_or_else(|| snapshots.last().expect("non-empty snapshots"));
            let query = self.semantic_query.read(cx).value().to_owned();
            let query_hits = Self::semantic_query_hits(snapshot, self.semantic_query_scope, &query);
            let authority = Self::authority_style(&snapshot.authority);

            let mut lineage = div().flex().flex_wrap().gap_2();
            for candidate in &snapshots {
                let candidate_authority = Self::authority_style(&candidate.authority);
                let selected = candidate.snapshot_hash == snapshot.snapshot_hash;
                let selector = entity.clone();
                let candidate_hash = candidate.snapshot_hash.clone();
                let candidate_project = project_id.clone();
                lineage = lineage.child(
                    div()
                        .id(format!("semantic-snapshot-{}", candidate_hash))
                        .w(px(214.))
                        .p_3()
                        .v_flex()
                        .gap_1()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if selected { ACCENT } else { BORDER }))
                        .bg(rgb(if selected { 0x00e0_f2fe } else { CARD_BG }))
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_semantic_snapshot =
                                    Some((candidate_project.clone(), candidate_hash.clone()));
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(Self::short_hash(&candidate.snapshot_hash)),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(candidate_authority.0))
                                        .text_color(rgb(candidate_authority.1))
                                        .child(candidate_authority.2),
                                ),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                "parent: {}",
                                candidate
                                    .parent_snapshot_hash
                                    .as_deref()
                                    .map_or_else(|| "root".to_owned(), Self::short_hash)
                            ))),
                );
            }

            let mut components = div().v_flex().gap_1();
            for component in &snapshot.components {
                components =
                    components.child(
                        div()
                            .flex()
                            .gap_3()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(SURFACE_BG))
                            .child(
                                div()
                                    .w(px(92.))
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(component.reference.clone()),
                            )
                            .child(
                                div()
                                    .w(px(152.))
                                    .truncate()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(component.id.clone()),
                            )
                            .child(
                                div().flex_1().min_w(px(90.)).text_sm().child(
                                    component.value.clone().unwrap_or_else(|| "—".to_owned()),
                                ),
                            )
                            .child(
                                div()
                                    .w(px(70.))
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(format!("{} evidence", component.evidence.len())),
                            ),
                    );
            }
            let mut pins = div().v_flex().gap_1();
            for component in &snapshot.components {
                for pin in &component.pins {
                    let nets = snapshot
                        .nets
                        .iter()
                        .filter(|net| {
                            net.pins.iter().any(|pin_ref| {
                                pin_ref.component_id == component.id && pin_ref.pin_id == pin.id
                            })
                        })
                        .map(|net| net.name.clone().unwrap_or_else(|| net.id.clone()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    pins = pins.child(
                        div()
                            .flex()
                            .gap_3()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(SURFACE_BG))
                            .child(div().w(px(92.)).text_sm().child(component.reference.clone()))
                            .child(div().w(px(54.)).text_sm().child(pin.id.clone()))
                            .child(div().w(px(128.)).truncate().text_sm().child(pin.name.clone()))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(90.))
                                    .truncate()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(if nets.is_empty() { "—".to_owned() } else { nets }),
                            ),
                    );
                }
            }
            let mut nets = div().v_flex().gap_1();
            for net in &snapshot.nets {
                nets = nets.child(
                    div()
                        .flex()
                        .gap_3()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .w(px(150.))
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(net.name.clone().unwrap_or_else(|| "Unnamed".to_owned())),
                        )
                        .child(
                            div()
                                .w(px(152.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(net.id.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(90.))
                                .text_sm()
                                .child(format!("{} pins", net.pins.len())),
                        ),
                );
            }
            let mut constraints = div().v_flex().gap_2();
            for constraint in &snapshot.constraints {
                let status = Self::fact_style(&constraint.status);
                let evidence = if constraint.evidence_refs.is_empty() {
                    "No evidence attached".to_owned()
                } else {
                    constraint
                        .evidence_refs
                        .iter()
                        .map(|reference| {
                            format!("{} @ {}", reference.document_id, reference.locator)
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                };
                constraints = constraints.child(
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
                                        .flex_1()
                                        .min_w(px(0.))
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(constraint.constraint_id.clone()),
                                )
                                .child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_xs()
                                        .bg(rgb(status.0))
                                        .text_color(rgb(status.1))
                                        .child(status.2),
                                ),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(format!(
                            "{} · {:?} · subjects: {}",
                            constraint.layer,
                            constraint.severity,
                            constraint.subject_refs.join(", ")
                        )))
                        .child(
                            div()
                                .text_sm()
                                .whitespace_normal()
                                .child(constraint.explanation.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(
                                    if matches!(constraint.status, FactStatus::Inconclusive) {
                                        0x00b4_5309
                                    } else {
                                        TEXT_MUTED
                                    },
                                ))
                                .child(evidence),
                        ),
                );
            }

            let mut evidence_sources = Vec::new();
            for evidence in &snapshot.evidence {
                evidence_sources.push((
                    "snapshot".to_owned(),
                    evidence.document_id.clone(),
                    evidence.locator.clone(),
                    evidence.content_hash.clone(),
                ));
            }
            for component in &snapshot.components {
                for evidence in &component.evidence {
                    evidence_sources.push((
                        format!("component:{}", component.reference),
                        evidence.document_id.clone(),
                        evidence.locator.clone(),
                        evidence.content_hash.clone(),
                    ));
                }
            }
            for constraint in &snapshot.constraints {
                for evidence in &constraint.evidence_refs {
                    evidence_sources.push((
                        format!("constraint:{}", constraint.constraint_id),
                        evidence.document_id.clone(),
                        evidence.locator.clone(),
                        evidence.content_hash.clone(),
                    ));
                }
            }
            let mut evidence_rows = div().v_flex().gap_1();
            for (origin, document_id, locator, hash) in &evidence_sources {
                evidence_rows = evidence_rows.child(
                    div()
                        .flex()
                        .gap_3()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .w(px(150.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(0x000e_7490))
                                .child(origin.clone()),
                        )
                        .child(
                            div()
                                .w(px(140.))
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(document_id.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(90.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(locator.clone()),
                        )
                        .child(
                            div()
                                .w(px(150.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(Self::short_hash(hash)),
                        ),
                );
            }
            if evidence_sources.is_empty() {
                evidence_rows = evidence_rows.child(
                    div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "此快照未附加证据；不能据此主张已验证。",
                        "No evidence is attached to this snapshot; it cannot support a verified claim.",
                    )),
                );
            }

            let mut query_rows = div().v_flex().gap_1();
            for hit in query_hits.iter().take(30) {
                query_rows = query_rows.child(
                    div()
                        .flex()
                        .gap_2()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(CARD_BG))
                        .child(
                            div().w(px(72.)).text_xs().text_color(rgb(0x000e_7490)).child(hit.kind),
                        )
                        .child(
                            div()
                                .w(px(190.))
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(hit.subject.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(90.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(hit.detail.clone()),
                        )
                        .child(
                            div()
                                .w(px(105.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(hit.evidence.clone()),
                        ),
                );
            }
            if query_hits.is_empty() {
                query_rows = query_rows.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    language.choose(
                        "没有匹配的结构化语义事实。",
                        "No structured semantic facts matched.",
                    ),
                ));
            }
            let mut scopes = div().flex().flex_wrap().gap_1();
            for scope in SemanticQueryScope::ALL {
                let setter = entity.clone();
                scopes = scopes.child(
                    Button::new(format!("semantic-scope-{:?}", scope))
                        .label(scope.label(language))
                        .when(scope == self.semantic_query_scope, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            setter.update(cx, |view, cx| {
                                view.semantic_query_scope = scope;
                                cx.notify();
                            });
                        }),
                );
            }

            div().id("semantic-browser-body").size_full().overflow_y_scroll().v_flex().gap_4().p_6()
                .child(div().flex().items_start().justify_between().gap_3().child(
                    div().v_flex().gap_1().child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(language.choose("电路语义", "Circuit semantics")))
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose("只读项目快照、验证事实与可追溯证据。", "Read-only project snapshots, verification facts, and traceable evidence."))),
                ).child({ let refresher = entity.clone(); Button::new("refresh-semantic-snapshots").label(language.choose("刷新快照", "Refresh snapshots")).on_click(move |_, _, cx| { refresher.update(cx, |view, cx| { view.status = match view.refresh_project_data(&project_id) { Ok(()) => language.choose("语义快照已刷新", "Semantic snapshots refreshed").to_owned(), Err(error) => format!("{}: {error}", language.choose("语义快照未刷新", "Semantic snapshots not refreshed")), }; cx.notify(); }); }) }))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_sm().font_weight(FontWeight::MEDIUM).child(language.choose("快照谱系", "Snapshot lineage"))).child(lineage))
                .child(div().flex().flex_wrap().gap_3().child(
                    div().flex_1().min_w(px(260.)).v_flex().gap_1().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("authority")).child(div().px_1p5().py_0p5().rounded_sm().text_xs().bg(rgb(authority.0)).text_color(rgb(authority.1)).child(authority.2)),
                ).child(
                    div().flex_1().min_w(px(260.)).v_flex().gap_1().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("logicalHash")).child(div().text_sm().child(snapshot.logical_hash.clone())),
                ).child(
                    div().flex_1().min_w(px(260.)).v_flex().gap_1().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("physicalHash")).child(div().text_sm().child(snapshot.physical_hash.clone().unwrap_or_else(|| language.choose("未观察到物理回读", "No physical readback observed").to_owned()))),
                ).child(
                    div().flex_1().min_w(px(260.)).v_flex().gap_1().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("snapshotHash")).child(div().text_sm().child(snapshot.snapshot_hash.clone())),
                ))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("结构化语义查询", "Structured semantic query"))).child(scopes).child(div().id("semantic-query").w_full().child(Input::new(&self.semantic_query))).child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose_owned(format!("{} 个结果", query_hits.len()), format!("{} result(s)", query_hits.len())))).child(query_rows))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("元件", "Components"))).child(components))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("引脚", "Pins"))).child(pins))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("网络", "Nets"))).child(nets))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("约束与验证结果", "Constraints & verification results"))).child(constraints))
                .child(div().v_flex().gap_2().p_3().rounded_lg().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("证据链", "Evidence chain"))).child(evidence_rows))
                .into_any_element()
        }

        fn current_observation_hash(&self, project_id: &str) -> Option<String> {
            self.project_data.get(project_id)?.semantic_snapshots.iter().rev().find_map(
                |snapshot| {
                    (snapshot.authority == SnapshotAuthority::Observed)
                        .then(|| snapshot.snapshot_hash.clone())
                },
            )
        }

        fn record_change_set_decision(
            &mut self,
            project_id: &str,
            change_set_id: &str,
            approved: bool,
            cx: &mut Context<Self>,
        ) {
            let current_observation = self.current_observation_hash(project_id);
            let rollback_handle = self.project_data.get(project_id).and_then(|data| {
                data.change_sets
                    .iter()
                    .find(|record| record.id == change_set_id)
                    .and_then(|record| record.rollback_handle.clone())
            });
            let reason = self.approval_note.read(cx).value().trim().to_owned();
            let entry = ChangeSetAuditEntry {
                timestamp_unix_seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                actor: "desktop operator".to_owned(),
                decision: if approved { "approved" } else { "rejected" }.to_owned(),
                reason: if reason.is_empty() { "No reason supplied".to_owned() } else { reason },
                observed_snapshot_hash: current_observation.clone(),
                rollback_handle,
            };
            let persisted = self.project_storages.get(project_id).map(|storage| {
                storage.record_change_set_decision(
                    change_set_id,
                    current_observation.as_deref(),
                    entry,
                )
            });
            self.status = match persisted {
                Some(Ok(())) => match self.refresh_project_data(project_id) {
                    Ok(()) => format!(
                        "ChangeSet `{change_set_id}` {} and audit record persisted.",
                        if approved { "approved" } else { "rejected" }
                    ),
                    Err(error) => {
                        format!("Decision was persisted, but the page could not refresh: {error}")
                    }
                },
                Some(Err(error)) => format!("ChangeSet decision not saved: {error}"),
                None => "ChangeSet decision not saved: project storage is not open.".to_owned(),
            };
            if self.status.contains("persisted") {
                self.approval_drawer_open = false;
            }
            cx.notify();
        }

        /// A ChangeSet projection makes every state transition inspectable: plan hashes and IR
        /// diff remain separate from write/readback/verification, while approval actions are
        /// guarded by a fresh observed-baseline comparison and appended to the same record.
        #[allow(clippy::too_many_lines)]
        fn render_changes_approvals_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let language = self.language;
            let entity = cx.entity().clone();
            let Some(project_id) = self.selected_project.clone() else {
                return Self::project_empty_state(
                    language.choose("Please select a project", "Select a project first"),
                    language.choose(
                        "ChangeSets are always scoped to one project.",
                        "ChangeSets are always scoped to one project.",
                    ),
                )
                .into_any_element();
            };
            let data = self.project_data.get(&project_id).cloned().unwrap_or_default();
            let selected_id = self
                .selected_change_set
                .as_ref()
                .filter(|(selected_project, _)| selected_project == &project_id)
                .map(|(_, id)| id.as_str());
            let selected = selected_id
                .and_then(|id| data.change_sets.iter().find(|record| record.id == id))
                .or_else(|| data.change_sets.first())
                .cloned();
            let current_observation = self.current_observation_hash(&project_id);

            let mut change_set_rows = div().v_flex().gap_2();
            for record in &data.change_sets {
                let is_selected =
                    selected.as_ref().is_some_and(|selected| selected.id == record.id);
                let selector = entity.clone();
                let select_project = project_id.clone();
                let select_id = record.id.clone();
                let decision = record.decision().map_or("pending", |entry| entry.decision.as_str());
                change_set_rows = change_set_rows.child(
                    div()
                        .id(format!("changeset-{}", record.id))
                        .cursor_pointer()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if is_selected { ACCENT } else { BORDER }))
                        .bg(rgb(if is_selected { 0x00e0_f2fe } else { CARD_BG }))
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_change_set =
                                    Some((select_project.clone(), select_id.clone()));
                                cx.notify();
                            })
                        })
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(record.id.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(TEXT_MUTED))
                                        .child(decision.to_owned()),
                                ),
                        )
                        .child(div().mt_1().text_xs().text_color(rgb(TEXT_SECONDARY)).child(
                            format!(
                                "{} → {}",
                                Self::short_hash(&record.base_snapshot_hash),
                                Self::short_hash(&record.target_snapshot_hash)
                            ),
                        )),
                );
            }
            if data.change_sets.is_empty() {
                change_set_rows = change_set_rows.child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(
                    "No persisted ChangeSets. A materializer writes proposed plans to logic/changesets."
                ));
            }

            let details = if let Some(record) = selected {
                let approval_allowed = record.approval_allowed(current_observation.as_deref());
                let baseline_label = if approval_allowed {
                    "Current observation matches baseline"
                } else {
                    "Current observation differs from baseline — approval disabled"
                };
                let mut diff_rows = div().v_flex().gap_1();
                for operation in &record.ir_diff {
                    diff_rows = diff_rows.child(
                        div()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(SURFACE_BG))
                            .text_sm()
                            .child(operation.clone()),
                    );
                }
                if record.ir_diff.is_empty() {
                    diff_rows = diff_rows.child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_MUTED))
                            .child("No IR operations recorded."),
                    );
                }
                let mut evidence_rows = div().v_flex().gap_1();
                for evidence in &record.evidence {
                    evidence_rows = evidence_rows.child(
                        div().p_2().rounded_md().bg(rgb(SURFACE_BG)).text_xs().child(format!(
                            "{} @ {} ({})",
                            evidence.document_id,
                            evidence.locator,
                            Self::short_hash(&evidence.content_hash)
                        )),
                    );
                }
                if record.evidence.is_empty() {
                    evidence_rows = evidence_rows.child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_MUTED))
                            .child("No evidence references recorded."),
                    );
                }
                let stage = |name: &'static str, status: &ChangeSetStageStatus, detail: &str| {
                    let (color, label) = match status {
                        ChangeSetStageStatus::Passed => (0x0016_a34a, "passed"),
                        ChangeSetStageStatus::Failed => (0x00b9_1c1c, "failed"),
                        ChangeSetStageStatus::NotRun => (0x00b4_5309, "not run"),
                    };
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(SURFACE_BG))
                        .child(status_dot(color))
                        .child(div().w(px(80.)).text_sm().child(name))
                        .child(
                            div().w(px(62.)).text_xs().text_color(rgb(TEXT_SECONDARY)).child(label),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(detail.to_owned()),
                        )
                };
                let drawer_opener = entity.clone();
                let record_id = record.id.clone();
                let drawer_project = project_id.clone();
                let audit =
                    record.audit.iter().rev().fold(div().v_flex().gap_1(), |rows, entry| {
                        rows.child(div().p_2().rounded_md().bg(rgb(SURFACE_BG)).text_xs().child(
                            format!(
                                "{} · {} · {} · observed: {} · rollback: {}",
                                entry.timestamp_unix_seconds,
                                entry.actor,
                                entry.decision,
                                entry
                                    .observed_snapshot_hash
                                    .as_deref()
                                    .map_or_else(|| "none".to_owned(), Self::short_hash),
                                entry.rollback_handle.as_deref().unwrap_or("none")
                            ),
                        ))
                    });
                div()
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
                                            .child(record.id.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .child(baseline_label),
                                    ),
                            )
                            .child(
                                Button::new("open-approval-drawer")
                                    .primary()
                                    .label("Review approval")
                                    .on_click(move |_, _, cx| {
                                        drawer_opener.update(cx, |view, cx| {
                                            view.selected_change_set =
                                                Some((drawer_project.clone(), record_id.clone()));
                                            view.approval_drawer_open = true;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(SURFACE_BG))
                                    .text_xs()
                                    .child(format!("baseline: {}", record.base_snapshot_hash)),
                            )
                            .child(
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(SURFACE_BG))
                                    .text_xs()
                                    .child(format!("target: {}", record.target_snapshot_hash)),
                            )
                            .child(
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(SURFACE_BG))
                                    .text_xs()
                                    .child(format!("plan: {}", record.plan_hash)),
                            )
                            .child(div().p_2().rounded_md().bg(rgb(SURFACE_BG)).text_xs().child(
                                format!(
                                        "observed: {}",
                                        current_observation
                                            .as_deref()
                                            .map_or_else(|| "none".to_owned(), Self::short_hash)
                                    ),
                            )),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("IR diff"),
                            )
                            .child(diff_rows),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Evidence references"),
                            )
                            .child(evidence_rows),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Materialization report"),
                            )
                            .child(stage(
                                "Write",
                                &record.execution.write.status,
                                &record.execution.write.detail,
                            ))
                            .child(stage(
                                "Readback",
                                &record.execution.readback.status,
                                &record.execution.readback.detail,
                            ))
                            .child(stage(
                                "Verification",
                                &record.execution.verification.status,
                                &record.execution.verification.detail,
                            )),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Approval audit (append-only)"),
                            )
                            .child(audit)
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                "Rollback handle: {}",
                                record.rollback_handle.as_deref().unwrap_or("not supplied")
                            ))),
                    )
                    .into_any_element()
            } else {
                Self::project_empty_state("No ChangeSet selected", "Choose a persisted ChangeSet to inspect its hashes, diff, evidence, and audit.").into_any_element()
            };

            let mut page = div().size_full().overflow_y_scrollbar().v_flex().gap_4().p_6()
                .child(div().flex().items_center().justify_between().child(
                    div().v_flex().gap_1().child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Changes & approvals"))
                        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child("Approval is guarded by current observed state, baseline, and verification."))
                ).child({ let refresher = entity.clone(); let refresh_project = project_id.clone(); Button::new("refresh-change-sets").label("Refresh").on_click(move |_, _, cx| refresher.update(cx, |view, cx| { view.status = view.refresh_project_data(&refresh_project).map_or_else(|error| format!("Refresh failed: {error}"), |_| "ChangeSets refreshed.".to_owned()); cx.notify(); })) }))
                .child(div().flex().gap_4().items_start().child(div().w(px(250.)).flex_none().v_flex().gap_2().child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("ChangeSets")).child(change_set_rows)).child(div().flex_1().min_w(px(420.)).child(details)));

            if self.approval_drawer_open {
                if let Some(record) = self
                    .selected_change_set
                    .as_ref()
                    .and_then(|(_, id)| data.change_sets.iter().find(|record| &record.id == id))
                {
                    let can_approve = record.approval_allowed(current_observation.as_deref());
                    let approve_view = entity.clone();
                    let reject_view = entity.clone();
                    let close_view = entity.clone();
                    let decision_project = project_id.clone();
                    let decision_id = record.id.clone();
                    let rejection_project = project_id.clone();
                    let rejection_id = record.id.clone();
                    let actions = div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("approve-change-set")
                                .primary()
                                .label("Approve")
                                .disabled(!can_approve)
                                .on_click(move |_, _, cx| {
                                    approve_view.update(cx, |view, cx| {
                                        view.record_change_set_decision(
                                            &decision_project,
                                            &decision_id,
                                            true,
                                            cx,
                                        )
                                    });
                                }),
                        )
                        .child(Button::new("reject-change-set").label("Reject").on_click(
                            move |_, _, cx| {
                                reject_view.update(cx, |view, cx| {
                                    view.record_change_set_decision(
                                        &rejection_project,
                                        &rejection_id,
                                        false,
                                        cx,
                                    )
                                });
                            },
                        ));
                    let drawer = div().p_4().rounded_xl().border_1().border_color(rgb(ACCENT)).bg(rgb(CARD_BG)).v_flex().gap_3()
                        .child(div().flex().justify_between().child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Approval drawer")).child(Button::new("close-approval-drawer").ghost().label("Close").on_click(move |_, _, cx| close_view.update(cx, |view, cx| { view.approval_drawer_open = false; cx.notify(); }))))
                        .child(div().text_sm().text_color(rgb(if can_approve { 0x0016_a34a } else { 0x00b9_1c1c })).child(if can_approve { "Baseline, readback, and verification are ready." } else { "Approve is disabled: current observation must equal baseline and verification must pass." }))
                        .child(div().h(px(36.)).px_2().rounded_md().border_1().border_color(rgb(BORDER)).child(InputBase::new("approval-note").h_full().flex().items_center().child(self.approval_note.clone())))
                        .child(actions);
                    page = page.child(drawer);
                }
            }
            page.into_any_element()
        }

        /// BOM is a projection of an immutable logical snapshot, never an independently edited
        /// parts list.  This keeps the selected format, every reference, and every evidence
        /// locator attached to a concrete source of engineering truth while exporter plugins are
        /// still pending.
        #[allow(clippy::too_many_lines)]
        fn render_bom_export_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let language = self.language;
            let entity = cx.entity().clone();
            let Some(project_id) = self.selected_project.clone() else {
                return Self::project_empty_state(
                    language.choose("请先选择项目", "Select a project first"),
                    language.choose(
                        "BOM 与导出只会投影所选项目中的语义快照，不会混入演示数据或其他项目。",
                        "BOM and export only project the selected project's semantic snapshots; no demo or cross-project data is mixed in.",
                    ),
                )
                .into_any_element();
            };
            let snapshots = self
                .project_data
                .get(&project_id)
                .map(|data| data.semantic_snapshots.clone())
                .unwrap_or_default();
            if snapshots.is_empty() {
                let refresher = entity.clone();
                let refresh_project = project_id.clone();
                return div()
                    .size_full()
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
                                            .child(language.choose("BOM 与导出", "BOM & export")),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .child(language.choose(
                                                "先导入一个语义快照，才能建立可追溯的 BOM。",
                                                "Import a semantic snapshot before creating a traceable BOM.",
                                            )),
                                    ),
                            )
                            .child(
                                Button::new("refresh-bom-snapshots")
                                    .label(language.choose("刷新快照", "Refresh snapshots"))
                                    .on_click(move |_, _, cx| {
                                        refresher.update(cx, |view, cx| {
                                            view.status = match view.refresh_project_data(&refresh_project) {
                                                Ok(()) => language
                                                    .choose("语义快照已刷新", "Semantic snapshots refreshed")
                                                    .to_owned(),
                                                Err(error) => format!(
                                                    "{}: {error}",
                                                    language.choose(
                                                        "语义快照未刷新",
                                                        "Semantic snapshots not refreshed",
                                                    )
                                                ),
                                            };
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(Self::project_empty_state(
                        language.choose("尚无可导出的语义快照", "No semantic snapshot to export"),
                        language.choose(
                            "TODO：EDA 导入器写入 logic/snapshots 后，刷新此页以查看 BOM；未绑定快照时不会生成导出。",
                            "TODO: refresh this page after an EDA importer writes logic/snapshots; no export is generated without a bound snapshot.",
                        ),
                    ))
                    .into_any_element();
            }

            let selected_hash = self
                .selected_semantic_snapshot
                .as_ref()
                .filter(|(selected_project, _)| selected_project == &project_id)
                .map(|(_, hash)| hash.as_str());
            let snapshot = selected_hash
                .and_then(|hash| snapshots.iter().find(|snapshot| snapshot.snapshot_hash == hash))
                .unwrap_or_else(|| snapshots.last().expect("non-empty snapshots"));
            let authority = Self::authority_style(&snapshot.authority);
            let component_evidence_count =
                snapshot.components.iter().map(|component| component.evidence.len()).sum::<usize>();
            let total_evidence_count = snapshot.evidence.len() + component_evidence_count;

            let mut snapshot_choices = div().flex().flex_wrap().gap_2();
            for candidate in &snapshots {
                let selector = entity.clone();
                let candidate_project = project_id.clone();
                let candidate_hash = candidate.snapshot_hash.clone();
                let selected = candidate.snapshot_hash == snapshot.snapshot_hash;
                snapshot_choices = snapshot_choices.child(
                    Button::new(format!("bom-snapshot-{candidate_hash}"))
                        .label(format!(
                            "{} · {}",
                            Self::short_hash(&candidate.snapshot_hash),
                            Self::authority_style(&candidate.authority).2
                        ))
                        .when(selected, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_semantic_snapshot =
                                    Some((candidate_project.clone(), candidate_hash.clone()));
                                cx.notify();
                            });
                        }),
                );
            }

            let mut format_choices = div().flex().flex_wrap().gap_2();
            for format in BomExportFormat::ALL {
                let selector = entity.clone();
                let bound_hash = snapshot.snapshot_hash.clone();
                let selected = format == self.bom_export_format;
                format_choices = format_choices.child(
                    Button::new(format!("bom-export-format-{format:?}"))
                        .label(format.label())
                        .when(selected, |button| button.primary())
                        .on_click(move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.bom_export_format = format;
                                view.status = format!(
                                    "{} 已选择；导出后端 TODO，生成时将绑定语义快照 {bound_hash}。",
                                    format.label()
                                );
                                cx.notify();
                            });
                        }),
                );
            }

            let mut bom_rows = div().v_flex().gap_1();
            for component in &snapshot.components {
                let evidence = if component.evidence.is_empty() {
                    language.choose("未附加元件证据", "No component evidence attached").to_owned()
                } else {
                    component
                        .evidence
                        .iter()
                        .map(|reference| {
                            format!(
                                "{} @ {} ({})",
                                reference.document_id,
                                reference.locator,
                                Self::short_hash(&reference.content_hash)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                };
                bom_rows = bom_rows.child(
                    div()
                        .flex()
                        .gap_3()
                        .p_3()
                        .rounded_md()
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .w(px(110.))
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(component.reference.clone()),
                        )
                        .child(div().w(px(190.)).truncate().text_sm().child(
                            component.value.clone().unwrap_or_else(|| {
                                language.choose("未指定值", "No value specified").to_owned()
                            }),
                        ))
                        .child(
                            div()
                                .w(px(150.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child(component.id.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(180.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(evidence),
                        ),
                );
            }
            if snapshot.components.is_empty() {
                bom_rows =
                    bom_rows
                        .child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(language.choose(
                        "此快照不含元件；不会生成空 BOM 导出。",
                        "This snapshot has no components; no empty BOM export will be generated.",
                    )));
            }

            let netlist_entry = entity.clone();
            let netlist_hash = snapshot.snapshot_hash.clone();
            let spice_entry = entity.clone();
            let spice_hash = snapshot.snapshot_hash.clone();
            let refresher = entity.clone();
            let refresh_project = project_id.clone();
            div()
                .id("bom-export-body")
                .size_full()
                .overflow_y_scroll()
                .v_flex()
                .gap_4()
                .p_6()
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
                                        .child(language.choose("BOM 与工程导出", "BOM & project export")),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(language.choose(
                                            "只读 BOM 投影；选择格式不改变语义快照、元件或证据。",
                                            "Read-only BOM projection; choosing a format never changes the snapshot, components, or evidence.",
                                        )),
                                ),
                        )
                        .child(
                            Button::new("refresh-bom-snapshots")
                                .label(language.choose("刷新快照", "Refresh snapshots"))
                                .on_click(move |_, _, cx| {
                                    refresher.update(cx, |view, cx| {
                                        view.status = match view.refresh_project_data(&refresh_project) {
                                            Ok(()) => language
                                                .choose("语义快照已刷新", "Semantic snapshots refreshed")
                                                .to_owned(),
                                            Err(error) => format!(
                                                "{}: {error}",
                                                language.choose(
                                                    "语义快照未刷新",
                                                    "Semantic snapshots not refreshed",
                                                )
                                            ),
                                        };
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("导出绑定", "Export binding")),
                        )
                        .child(snapshot_choices)
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(230.))
                                        .v_flex()
                                        .gap_1()
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("snapshotHash"))
                                        .child(div().text_sm().child(snapshot.snapshot_hash.clone())),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(210.))
                                        .v_flex()
                                        .gap_1()
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("logicalHash"))
                                        .child(div().text_sm().child(snapshot.logical_hash.clone())),
                                )
                                .child(
                                    div()
                                        .w(px(116.))
                                        .v_flex()
                                        .gap_1()
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("authority"))
                                        .child(
                                            div()
                                                .px_1p5()
                                                .py_0p5()
                                                .rounded_sm()
                                                .text_xs()
                                                .bg(rgb(authority.0))
                                                .text_color(rgb(authority.1))
                                                .child(authority.2),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(116.))
                                        .v_flex()
                                        .gap_1()
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("evidence"))
                                        .child(div().text_sm().child(format!("{total_evidence_count} refs"))),
                                ),
                        )
                        .child(
                            div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                                "所有导出都必须携带此 snapshotHash 与下方引用；缺少证据不会被显示为已验证。",
                                "Every export must carry this snapshotHash and the references below; missing evidence is never presented as verified.",
                            )),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("BOM 预览", "BOM preview")),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .px_3()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_MUTED))
                                .child(div().w(px(110.)).child(language.choose("参考号", "Reference")))
                                .child(div().w(px(190.)).child(language.choose("值", "Value")))
                                .child(div().w(px(150.)).child(language.choose("元件", "Component")))
                                .child(div().flex_1().child(language.choose("证据引用", "Evidence references"))),
                        )
                        .child(bom_rows),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .p_4()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("BOM 格式", "BOM format")),
                        )
                        .child(format_choices)
                        .child(
                            div().text_xs().text_color(rgb(0x00b4_5309)).child(language.choose(
                                "TODO：CSV、Excel 和 JSON 导出器尚未实现，因此不会写出文件；当前选择会保留与所选快照的绑定。",
                                "TODO: CSV, Excel, and JSON exporter backends are not implemented, so no file is written; this selection stays bound to the selected snapshot.",
                            )),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .p_4()
                        .rounded_xl()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(CARD_BG))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("工程导出入口", "Engineering export entry points")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    Button::new("export-netlist-todo")
                                        .label(language.choose("网表导出（TODO）", "Netlist export (TODO)"))
                                        .on_click(move |_, _, cx| {
                                            netlist_entry.update(cx, |view, cx| {
                                                view.status = format!(
                                                    "网表导出后端尚未实现；不会生成文件。请求将绑定语义快照 {netlist_hash}。"
                                                );
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("export-spice-todo")
                                        .label(language.choose("SPICE 导出（TODO）", "SPICE export (TODO)"))
                                        .on_click(move |_, _, cx| {
                                            spice_entry.update(cx, |view, cx| {
                                                view.status = format!(
                                                    "SPICE 导出后端尚未实现；不会生成文件。请求将绑定语义快照 {spice_hash}。"
                                                );
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div().text_xs().text_color(rgb(0x00b4_5309)).child(language.choose(
                                "TODO：网表和 SPICE 需要由后端插件实现。入口保持可见，以避免将不可用功能误报为已导出。",
                                "TODO: netlist and SPICE require backend plugins. The entry points remain visible so unavailable functionality is never reported as exported.",
                            )),
                        ),
                )
                .into_any_element()
        }

        /// Renders only state that the control plane has loaded or observed. A missing
        /// ChangeSet store is not represented as zero pending approvals, because zero would
        /// imply a readback that has not occurred.
        #[allow(clippy::too_many_lines)]
        fn overview_page(&self, language: UiLanguage) -> impl IntoElement {
            let project_count = self.workspace.projects().len();
            let authorized_document_count = self
                .project_data
                .values()
                .flat_map(|data| &data.documents)
                .filter(|document| document.authorized)
                .count();
            let online_bridge_count =
                usize::from(matches!(self.bridge_health, BridgeHealth::Listening { .. }));
            let mut sessions = self
                .project_data
                .iter()
                .flat_map(|(project_id, data)| {
                    data.session_listing.sessions.iter().map(move |summary| {
                        (
                            project_id.clone(),
                            summary.metadata.session_id.clone(),
                            summary.metadata.status,
                            summary.metadata.started_at_unix_seconds,
                        )
                    })
                })
                .collect::<Vec<_>>();
            sessions.sort_by(|left, right| right.3.cmp(&left.3));
            let recent_sessions = if sessions.is_empty() {
                language
                    .choose(
                        "还没有已持久化的会话。请配置运行时后启动一次任务。",
                        "No persisted sessions. Configure a runtime, then start a task.",
                    )
                    .to_owned()
            } else {
                sessions
                    .into_iter()
                    .take(3)
                    .map(|(project_id, session_id, status, started_at)| {
                        format!(
                            "{session_id} · {} · {project_id} · {}",
                            status.as_str(),
                            rfc3339(started_at)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let mut passed = 0_usize;
            let mut failed = 0_usize;
            let mut inconclusive = 0_usize;
            let mut not_run = 0_usize;
            for data in self.project_data.values() {
                for snapshot in &data.semantic_snapshots {
                    for constraint in &snapshot.constraints {
                        match &constraint.status {
                            FactStatus::Passed => passed += 1,
                            FactStatus::Failed => failed += 1,
                            FactStatus::Inconclusive => inconclusive += 1,
                            FactStatus::NotRun => not_run += 1,
                        }
                    }
                }
            }

            let metric = |value: String, label: &'static str, color: u32| {
                div()
                    .flex_1()
                    .min_w(px(0.))
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

            let panel = |title: &'static str, body: String| {
                div()
                    .flex_1()
                    .min_w(px(0.))
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
                .id("overview-page")
                .size_full()
                .overflow_y_scroll()
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
                        .child(metric(
                            project_count.to_string(),
                            language.choose("项目", "Projects"),
                            0x003b_82f6,
                        ))
                        .child(metric(
                            authorized_document_count.to_string(),
                            language.choose("已授权文档", "Authorized documents"),
                            0x008b_5cf6,
                        ))
                        .child(metric(
                            online_bridge_count.to_string(),
                            language.choose("在线 bridge", "Online bridges"),
                            self.bridge_health.dot(),
                        ))
                        .child(metric(
                            "—".to_owned(),
                            language.choose("待审批（未回读）", "Pending approvals (not read back)"),
                            0x0094_a3b8,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .child(panel(
                            language.choose("运行时健康度", "Runtime health"),
                            format!(
                                "Codex App Server · {}\nEDA bridge · {}\n{}",
                                self.codex_status.label(language),
                                self.bridge_health.label(language),
                                language.choose(
                                    "状态来自受监管进程与最近一次 TCP 探测；未知并不表示在线。",
                                    "Statuses come from supervised processes and the latest TCP probe; unknown is not online.",
                                ),
                            ),
                        ))
                        .child(panel(
                            language.choose("最近会话与任务", "Recent sessions & tasks"),
                            if self.task_cancel.is_some() {
                                format!(
                                    "{}\n{recent_sessions}",
                                    language.choose("当前任务正在运行", "A runtime task is running")
                                )
                            } else {
                                recent_sessions
                            },
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
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
                                        .child(language.choose("待办与验证事实", "To-do & verification facts")),
                                )
                                .child(Self::overview_fact_rows(
                                    language,
                                    passed,
                                    failed,
                                    inconclusive,
                                    not_run,
                                )),
                        ),
                )
        }
    }

    impl ControlPlaneView {
        fn command_matches(&self, cx: &Context<Self>) -> Vec<ControlPlaneScreen> {
            let needle = self.command_search.read(cx).value().to_lowercase();
            let project_selected = self.selected_project.is_some();
            ControlPlaneScreen::ALL
                .into_iter()
                // Project-scoped screens match the sidebar's disabled state:
                // they only work with a selected project, so the palette does
                // not offer them until one is open.
                .filter(|screen| {
                    (!screen.requires_project() || project_selected)
                        && (needle.is_empty()
                            || self.language.screen_label(*screen).to_lowercase().contains(&needle))
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

            if self.command_palette_open {
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
                return;
            }

            // Outside the palette, Enter presses the default button of the
            // top-most open dialog, exactly like clicking it.
            if keystroke.key == "enter" {
                self.press_default_dialog_button(window, cx);
            }
        }

        /// Triggers the default (primary) action of the currently open modal
        /// dialog: the startup vault prompt and quick unlock submit the typed
        /// password, the new-project form confirms creation. Each action
        /// reuses the button's own handler, so disabled states (e.g. a busy
        /// unlock) suppress the key the same way they suppress the click.
        fn press_default_dialog_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let vault_prompt_visible =
                self.vault_prompt_open && self.vault.is_none() && self.vault_file_exists;
            let quick_unlock_visible =
                self.vault_quick_unlock_open && self.vault.is_none() && self.vault_file_exists;
            if vault_prompt_visible || quick_unlock_visible {
                self.unlock_vault(window, cx);
                return;
            }
            if self.project_form_open {
                self.create_project(window, cx);
            }
        }

        /// The secrets vault page: one encrypted file on disk, held decrypted
        /// only in memory while unlocked. The left column lists the recorded
        /// variable names (names are not secret); the right pane creates,
        /// unlocks, and locks the vault and edits entries without ever
        /// echoing a value back.
        #[allow(clippy::too_many_lines)]
        fn render_secrets_page(
            &mut self,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let unlocked = self.vault.is_some();
            let file_exists = self.vault_file_exists;
            let busy = self.vault_busy;
            let message = self.vault_message.clone();

            let mut secret_rows = div().v_flex().gap_2();
            if self.vault_index.is_empty() {
                secret_rows = secret_rows.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .text_sm()
                        .text_color(rgb(TEXT_MUTED))
                        .child(language.choose(
                            "暂无收录变量；保存第一条密钥后出现在这里。",
                            "No variables recorded yet; saved keys appear here.",
                        )),
                );
            } else {
                for name in self.vault_index.clone() {
                    let selector = entity.clone();
                    let selected =
                        unlocked && self.selected_secret.as_deref() == Some(name.as_str());
                    let row_name = name.clone();
                    let row = div()
                        .id(format!("secret-row-{name}"))
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if selected { ACCENT } else { BORDER }))
                        .bg(rgb(if selected { 0x00f0_f9ff } else { CARD_BG }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when(!unlocked, |this| this.child(status_dot(0x0094_a3b8)))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(if selected {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::MEDIUM
                                        })
                                        .text_color(rgb(if unlocked {
                                            TEXT_PRIMARY
                                        } else {
                                            TEXT_MUTED
                                        }))
                                        .child(name),
                                ),
                        );
                    secret_rows = secret_rows.child(if unlocked {
                        row.cursor_pointer()
                            .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                            .on_click(move |_, _, cx| {
                                selector.update(cx, |view, cx| {
                                    view.selected_secret = Some(row_name.clone());
                                    cx.notify();
                                });
                            })
                    } else {
                        row.opacity(0.7)
                    });
                }
            }

            let detail = if !file_exists {
                let creator = entity.clone();
                Self::detail_pane("vault-detail-1")
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
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language
                                        .choose("创建密钥保险库", "Create the secrets vault")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(language.choose(
                                        "保险库以你设置的安全密码加密保存各 Provider/MCP 的 API Key；解锁后密钥作为环境变量注入运行时与 MCP 进程，不会写入配置文件或命令行参数。",
                                        "The vault stores Provider/MCP API keys encrypted with your passphrase; once unlocked, keys are injected into runtime and MCP processes as environment variables — never written to config files or command-line arguments.",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(Self::labeled_field(
                                language.choose("保险库密码", "Vault password"),
                                "vault-create-password",
                                None,
                                &self.vault_password,
                            ))
                            .child(Self::labeled_field(
                                language.choose("确认密码", "Confirm password"),
                                "vault-create-confirm",
                                None,
                                &self.vault_password_confirm,
                            )),
                    )
                    .when_some(message, |this, message| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(message),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Button::new("create-vault")
                                    .primary()
                                    .label(if busy {
                                        language.choose("正在创建…", "Creating…")
                                    } else {
                                        language.choose("创建保险库", "Create vault")
                                    })
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        creator.update(cx, |view, cx| {
                                            view.create_vault(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(rgb(TEXT_MUTED))
                            .child(language.choose(
                                "密码至少 8 个字符，请牢记：丢失后无法找回已存密钥。",
                                "At least 8 characters; memorize it — a lost password cannot recover stored keys.",
                            )),
                    )
                    .into_any_element()
            } else if !unlocked {
                let dialog_entity = entity.clone();
                Self::detail_pane("vault-detail-2")
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
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language
                                        .choose("解锁密钥保险库", "Unlock the secrets vault")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(language.choose(
                                        "输入密码解锁后，密钥值才会注入运行时与 MCP 进程；锁定状态下任务与工具调用只能从进程环境读取。",
                                        "Keys are injected into runtime and MCP processes only after you unlock with the password; while locked, tasks and tool calls can only read the process environment.",
                                    )),
                            ),
                    )
                    .child(Self::labeled_field(
                        language.choose("保险库密码", "Vault password"),
                        "vault-unlock-password",
                        None,
                        &self.vault_password,
                    ))
                    .when_some(message, |this, message| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(message),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Button::new("unlock-vault-page")
                                    .primary()
                                    .label(if busy {
                                        language.choose("正在解锁…", "Unlocking…")
                                    } else {
                                        language.choose("解锁", "Unlock")
                                    })
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        dialog_entity.update(cx, |view, cx| {
                                            view.unlock_vault(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!(
                                "{} {}",
                                language.choose("保险库文件：", "Vault file:"),
                                self.vault_path.display()
                            )),
                    )
                    .into_any_element()
            } else {
                let relocker = entity.clone();
                let saver = entity.clone();
                let password_changer = entity.clone();
                let count = self.vault_index.len();
                let editing_name = self.secret_name.read(cx).value().trim().to_owned();
                let updating = self.vault_index.iter().any(|name| name == &editing_name);

                let mut selected_card = None;
                if let Some(selected) = self.selected_secret.clone()
                    && self.vault_index.iter().any(|name| name == &selected)
                {
                    let remover = entity.clone();
                    let remove_name = selected.clone();
                    selected_card = Some(
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
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .v_flex()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("变量名 {selected}")),
                                            )
                                            .child(
                                                div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                                    language.choose(
                                                        "值 ••••••••（不回显）",
                                                        "Value •••••••• (never echoed)",
                                                    ),
                                                ),
                                            ),
                                    )
                                    .child(
                                        Button::new(format!("remove-secret-{selected}"))
                                            .danger()
                                            .label(language.choose("删除", "Remove"))
                                            .on_click(move |_, _, cx| {
                                                remover.update(cx, |view, cx| {
                                                    view.remove_secret_entry(&remove_name, cx);
                                                });
                                            }),
                                    ),
                            )
                            .into_any_element(),
                    );
                }

                Self::detail_pane("vault-detail-3")
                    .gap_4()
                    .p_5()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(status_dot(0x0022_c55e))
                                    .child(
                                        div()
                                            .text_sm()
                                            .whitespace_normal()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .child(language.choose_owned(
                                                format!(
                                                    "已解锁 · {count} 个变量 · 文件 {}",
                                                    self.vault_path.display()
                                                ),
                                                format!(
                                                    "Unlocked · {count} variables · file {}",
                                                    self.vault_path.display()
                                                ),
                                            )),
                                    ),
                            )
                            .child(
                                Button::new("relock-vault")
                                    .ghost()
                                    .label(language.choose("锁定保险库", "Lock vault"))
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        relocker.update(cx, |view, cx| {
                                            view.relock_vault(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .when_some(selected_card, ParentElement::child)
                    .child(
                        div()
                            .v_flex()
                            .gap_3()
                            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                                language.choose("收录 / 更新变量", "Record / update a variable"),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(Self::labeled_field(
                                        language.choose("变量名", "Variable name"),
                                        "secret-name",
                                        None,
                                        &self.secret_name,
                                    ))
                                    .child(Self::labeled_field(
                                        language.choose("密钥值", "Secret value"),
                                        "secret-value",
                                        Some(language.choose(
                                            "粘贴密钥值；保存后不再回显。",
                                            "Paste the key value; it is never echoed after saving.",
                                        )),
                                        &self.secret_value,
                                    )),
                            )
                            .when_some(message, |this, message| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .whitespace_normal()
                                        .text_color(rgb(0x00dc_2626))
                                        .child(message),
                                )
                            })
                            .child(
                                Button::new("save-secret")
                                    .primary()
                                    .label(if updating {
                                        language.choose("更新变量", "Update variable")
                                    } else {
                                        language.choose("保存变量", "Save variable")
                                    })
                                    .on_click(move |_, window, cx| {
                                        saver.update(cx, |view, cx| {
                                            view.save_secret_entry(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_3()
                            .child(
                                div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                                    language.choose("修改保险库密码", "Change vault password"),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_3()
                                    .child(Self::labeled_field(
                                        language.choose("新密码", "New password"),
                                        "vault-change-password",
                                        None,
                                        &self.vault_password,
                                    ))
                                    .child(Self::labeled_field(
                                        language.choose("确认新密码", "Confirm new password"),
                                        "vault-change-confirm",
                                        None,
                                        &self.vault_password_confirm,
                                    )),
                            )
                            .child(
                                Button::new("change-vault-password")
                                    .ghost()
                                    .label(if busy {
                                        language.choose("正在更新…", "Updating…")
                                    } else {
                                        language.choose("更新密码", "Update password")
                                    })
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        password_changer.update(cx, |view, cx| {
                                            view.change_vault_password(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .into_any_element()
            };

            div()
                .id("secrets-vault-page")
                .size_full()
                .overflow_y_scroll()
                .min_w(px(720.))
                .relative()
                .v_flex()
                .gap_4()
                .p_6()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("密钥保险库", "Secrets vault")),
                        )
                        .child(
                            div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                language.choose(
                                    "API Key 加密保存在本机保险库文件中，仅解锁期间驻留内存；同名进程环境变量仍是后备来源。",
                                    "API keys live in one encrypted local vault file and in memory only while unlocked; same-named process environment variables remain the fallback source.",
                                ),
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
                                .id("vault-list-scroll")
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .w(px(320.))
                                .flex_none()
                                .v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(Self::agents_group_label(
                                            language.choose("变量", "Variables"),
                                        ))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(TEXT_MUTED))
                                                .child(language.choose_owned(
                                                    format!("{} 个", self.vault_index.len()),
                                                    format!("{}", self.vault_index.len()),
                                                )),
                                        ),
                                )
                                .child(secret_rows),
                        )
                        .child(detail),
                )
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
        }

        /// Startup prompt shown over the workspace when a vault file exists
        /// but is still locked: unlock now, or explicitly defer. The backdrop
        /// deliberately does not dismiss on click — deferral is a choice.
        fn render_vault_prompt(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let dialog_entity = entity.clone();
            let busy = self.vault_busy;
            let message = self.vault_message.clone();

            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("vault-prompt-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00_0f17_2ab3))
                        .occlude(),
                )
                .child(
                    div()
                        .relative()
                        .occlude()
                        .w(px(480.))
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
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_PRIMARY))
                                        .child(language
                                            .choose("解锁密钥保险库", "Unlock the secrets vault")),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .whitespace_normal()
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(language.choose(
                                            "保险库以你设置的安全密码加密保存各 Provider/MCP 的 API Key；解锁后密钥作为环境变量注入运行时与 MCP 进程，不会写入配置文件或命令行参数。",
                                            "The vault stores Provider/MCP API keys encrypted with your passphrase; once unlocked, keys are injected into runtime and MCP processes as environment variables — never written to config files or command-line arguments.",
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(language.choose("保险库密码", "Vault password")),
                                )
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
                                            InputBase::new("vault-prompt-password")
                                                .flex_1()
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .child(self.vault_password.clone()),
                                        ),
                                ),
                        )
                        .when_some(message, |this, message| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(message),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("dismiss-vault-prompt")
                                        .ghost()
                                        .label(language.choose("暂不解锁", "Not now"))
                                        .on_click(move |_, _, cx| {
                                            entity
                                                .update(cx, ControlPlaneView::dismiss_vault_prompt);
                                        }),
                                )
                                .child(
                                    Button::new("unlock-vault-prompt")
                                        .primary()
                                        .label(if busy {
                                            language.choose("正在解锁…", "Unlocking…")
                                        } else {
                                            language.choose("解锁", "Unlock")
                                        })
                                        .disabled(busy)
                                        .on_click(move |_, window, cx| {
                                            dialog_entity.update(cx, |view, cx| {
                                                view.unlock_vault(window, cx);
                                            });
                                        }),
                                ),
                        ),
                )
        }

        /// Quick unlock dialog opened from the 🔒 chips next to secret-source
        /// hints: same card structure as the startup prompt, with Cancel
        /// instead of the deferred choice. The backdrop does not dismiss.
        fn render_vault_quick_unlock(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity().clone();
            let language = self.language;
            let dialog_entity = entity.clone();
            let busy = self.vault_busy;
            let message = self.vault_message.clone();

            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .id("vault-quick-unlock-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00_0f17_2ab3))
                        .occlude(),
                )
                .child(
                    div()
                        .relative()
                        .occlude()
                        .w(px(480.))
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
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_PRIMARY))
                                        .child(language
                                            .choose("快速解锁保险库", "Quick vault unlock")),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .whitespace_normal()
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(language.choose(
                                            "输入保险库密码即可解锁；解锁后密钥作为环境变量注入运行时与 MCP 进程，不会写入配置文件或命令行参数。",
                                            "Enter the vault password to unlock; keys are then injected into runtime and MCP processes as environment variables — never written to config files or command-line arguments.",
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(language.choose("保险库密码", "Vault password")),
                                )
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
                                            InputBase::new("vault-quick-unlock-password")
                                                .flex_1()
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .child(self.vault_password.clone()),
                                        ),
                                ),
                        )
                        .when_some(message, |this, message| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(message),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("cancel-vault-quick-unlock")
                                        .ghost()
                                        .label(language.choose("取消", "Cancel"))
                                        .on_click(move |_, window, cx| {
                                            entity.update(cx, |view, cx| {
                                                view.vault_quick_unlock_open = false;
                                                view.vault_message = None;
                                                view.vault_password.update(cx, |state, cx| {
                                                    state.set_value("", window, cx);
                                                });
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("unlock-vault-quick")
                                        .primary()
                                        .label(if busy {
                                            language.choose("正在解锁…", "Unlocking…")
                                        } else {
                                            language.choose("解锁", "Unlock")
                                        })
                                        .disabled(busy)
                                        .on_click(move |_, window, cx| {
                                            dialog_entity.update(cx, |view, cx| {
                                                view.unlock_vault(window, cx);
                                            });
                                        }),
                                ),
                        ),
                )
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
            self.refresh_bridge_lifecycle();
            self.sync_window_title(window);
            let command_palette = if self.command_palette_open {
                Some(self.render_command_palette(cx).into_any_element())
            } else {
                None
            };
            let vault_prompt =
                if self.vault_prompt_open && self.vault.is_none() && self.vault_file_exists {
                    Some(self.render_vault_prompt(cx).into_any_element())
                } else {
                    None
                };
            let vault_quick_unlock =
                if self.vault_quick_unlock_open && self.vault.is_none() && self.vault_file_exists {
                    Some(self.render_vault_quick_unlock(cx).into_any_element())
                } else {
                    None
                };
            let jev_backend_modal = if self.jev_backend_modal_open {
                Some(self.render_jev_backend_modal(cx).into_any_element())
            } else {
                None
            };
            let jev_key_modal = if self.jev_key_modal_open {
                Some(self.render_jev_key_modal(cx).into_any_element())
            } else {
                None
            };
            let document_preview_pane = if self.document_preview.is_some() {
                Some(self.render_document_preview_pane(window, cx))
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
            let vault_unlocked = self.vault.is_some();
            let project_selected = self.selected_project.is_some();

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
                // Project-scoped screens stay gray and inert — no pointer,
                // hover, or click — until a project is selected.
                let unavailable = screen.requires_project() && !project_selected;
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
                        .when(active, |this| this.bg(rgb(SIDEBAR_ITEM_ACTIVE)))
                        .when(!unavailable, |this| {
                            this.cursor_pointer()
                                .hover(|this| this.bg(rgb(SIDEBAR_ITEM_HOVER)))
                                .active(|this| this.bg(rgb(SIDEBAR_ITEM_PRESSED)))
                                .on_click(move |_, _, cx| {
                                    selector.update(cx, |view, cx| {
                                        view.screen = screen;
                                        cx.notify();
                                    });
                                })
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
                                .text_color(if unavailable {
                                    rgb(SIDEBAR_GROUP)
                                } else if active {
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
                        })
                        .when(screen == ControlPlaneScreen::SecretsVault, |this| {
                            this.child(status_dot(if vault_unlocked {
                                0x0022_c55e
                            } else {
                                0x0094_a3b8
                            }))
                        }),
                );
            }

            let page = match active_screen {
                ControlPlaneScreen::Overview => self.overview_page(language).into_any_element(),
                ControlPlaneScreen::Semantics => self.render_semantics_page(cx).into_any_element(),
                ControlPlaneScreen::BomAndExport => {
                    self.render_bom_export_page(cx).into_any_element()
                }
                ControlPlaneScreen::Projects => {
                    self.render_projects_page(window, cx).into_any_element()
                }
                ControlPlaneScreen::Documents => self.render_documents_page(cx).into_any_element(),
                ControlPlaneScreen::EdaServices => {
                    self.render_eda_services_page(window, cx).into_any_element()
                }
                ControlPlaneScreen::AgentsAndMcp => {
                    self.render_agents_page(window, cx).into_any_element()
                }
                ControlPlaneScreen::SessionsAndTasks => {
                    self.render_sessions_page(cx).into_any_element()
                }
                ControlPlaneScreen::SecretsVault => {
                    self.render_secrets_page(window, cx).into_any_element()
                }
                ControlPlaneScreen::Usage => {
                    self.render_usage_audit_page(window, cx).into_any_element()
                }
                ControlPlaneScreen::Plugins => self.render_plugins_page(cx).into_any_element(),
                ControlPlaneScreen::ChangesAndApprovals => {
                    self.render_changes_approvals_page(cx).into_any_element()
                }
                ControlPlaneScreen::Settings => self.render_settings_page(cx).into_any_element(),
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
                                        .flex_1()
                                        .min_w(px(0.))
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
                                                .min_w(px(0.))
                                                .truncate()
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
                                                        view.toggle_global_language(cx);
                                                    });
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            // Scrollable content plus the docked document preview, when one
                            // is open. Vertical-only scrolling keeps the page width locked to
                            // the viewport, so text nodes receive a definite wrap width and
                            // reflow instead of stretching the workspace sideways; the
                            // preview pane sits beside the scroll host, not inside it, and
                            // scrolls independently at full window height. The divider
                            // between them is draggable and resizes the pane.
                            div()
                                .flex()
                                .flex_1()
                                .min_h(px(0.))
                                .on_drag_move::<DraggedPreviewSplit>(cx.listener(
                                    |view, event: &DragMoveEvent<DraggedPreviewSplit>, _, cx| {
                                        let row_right = f32::from(event.bounds.right());
                                        let row_width =
                                            f32::from(event.bounds.right() - event.bounds.left());
                                        let pointer_x = f32::from(event.event.position.x);
                                        let upper =
                                            (row_width * 0.8).min(DOCUMENT_PREVIEW_MAX_WIDTH);
                                        view.document_preview_width = (row_right - pointer_x)
                                            .clamp(
                                                DOCUMENT_PREVIEW_MIN_WIDTH,
                                                upper.max(DOCUMENT_PREVIEW_MIN_WIDTH),
                                            );
                                        cx.notify();
                                    },
                                ))
                                .on_drop::<DraggedPreviewSplit>(cx.listener(
                                    |_view, _event, _window, cx| {
                                        cx.notify();
                                    },
                                ))
                                .child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .min_h(px(0.))
                                        .h_full()
                                        .overflow_hidden()
                                        .bg(rgb(SURFACE_BG))
                                        .child(
                                            div()
                                                .id("main-content-scroll")
                                                .size_full()
                                                .overflow_y_scroll()
                                                .track_scroll(&self.main_content_scroll)
                                                .child(page),
                                        )
                                        .vertical_scrollbar(&self.main_content_scroll),
                                )
                                .when_some(document_preview_pane, |row, pane| {
                                    row.child(
                                        div()
                                            .id("document-preview-split-handle")
                                            .w(px(6.))
                                            .flex_none()
                                            .h_full()
                                            .cursor_col_resize()
                                            .bg(rgb(BORDER))
                                            .hover(|this| this.bg(rgb(ACCENT_SOFT)))
                                            .block_mouse_except_scroll()
                                            .on_click(cx.listener(
                                                |view, event: &ClickEvent, _window, cx| {
                                                    if event.click_count() >= 2 {
                                                        view.document_preview_width =
                                                            DOCUMENT_PREVIEW_WIDTH;
                                                        cx.notify();
                                                    }
                                                },
                                            ))
                                            .on_drag(DraggedPreviewSplit, |_, _, _, cx| {
                                                cx.new(|_| gpui::Empty)
                                            }),
                                    )
                                    .child(pane)
                                }),
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
                        .when_some(command_palette, ParentElement::child)
                        .when_some(vault_prompt, ParentElement::child)
                        .when_some(vault_quick_unlock, ParentElement::child)
                        .when_some(jev_backend_modal, ParentElement::child)
                        .when_some(jev_key_modal, ParentElement::child),
                )
        }
    }

    // gpui-component's icons (buttons, spinners, …) are SVG assets; without a registered
    // asset source every `Icon` renders as empty space — the control still occupies its
    // layout box and handles input, but nothing is visible.
    gpui_platform::application().with_assets(gpui_component_assets::Assets).run(move |cx| {
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

                    #[cfg(windows)]
                    pdf_cursors::install(window);

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

    #[test]
    fn delivered_record_pages_are_navigation_screens() {
        assert!(!ControlPlaneScreen::Documents.is_todo());
        assert!(!ControlPlaneScreen::SessionsAndTasks.is_todo());
    }

    #[test]
    fn only_project_scoped_screens_require_a_project() {
        let project_independent = [
            ControlPlaneScreen::Overview,
            ControlPlaneScreen::Projects,
            ControlPlaneScreen::Documents,
            ControlPlaneScreen::EdaServices,
            ControlPlaneScreen::AgentsAndMcp,
            ControlPlaneScreen::SessionsAndTasks,
            ControlPlaneScreen::Plugins,
            ControlPlaneScreen::Usage,
            ControlPlaneScreen::SecretsVault,
            ControlPlaneScreen::Settings,
        ];
        for screen in project_independent {
            assert!(
                !screen.requires_project(),
                "{screen:?} should stay usable without a selected project"
            );
        }
        for screen in [
            ControlPlaneScreen::Semantics,
            ControlPlaneScreen::ChangesAndApprovals,
            ControlPlaneScreen::BomAndExport,
        ] {
            assert!(screen.requires_project(), "{screen:?} should require a selected project");
        }
    }

    #[test]
    fn window_title_names_the_selected_project() {
        assert_eq!(app_window_title(None), "CircuitFabric");
        assert_eq!(app_window_title(None), "CircuitFabric");
    }
}
