//! UI drafts, selections and native resource lifetimes.
use super::*;

pub(super) struct ProviderFields {
    pub(super) id: Entity<InputState>,
    pub(super) name: Entity<InputState>,
    pub(super) base_url: Entity<InputState>,
    pub(super) model: Entity<InputState>,
    pub(super) api_key_environment_variable: Entity<InputState>,
    pub(super) native_vision: bool,
    pub(super) vision_base_url: Entity<InputState>,
    pub(super) vision_model: Entity<InputState>,
    pub(super) vision_api_key_environment_variable: Entity<InputState>,
    pub(super) enabled: bool,
}

/// One selectable runtime adapter on the Agents & tools page. Claude Code and DSH are
/// deliberate placeholders: they show the planned surface without claiming to work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeAdapter {
    CodexAppServer,
    ClaudeCode,
    Dsh,
}

impl RuntimeAdapter {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::CodexAppServer => "Codex App Server",
            Self::ClaudeCode => "Claude Code",
            Self::Dsh => "DSH",
        }
    }

    /// Stable session `backendId` for runs through this adapter.
    pub(super) const fn backend_id(self) -> &'static str {
        match self {
            Self::CodexAppServer => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Dsh => "dsh",
        }
    }

    pub(super) const fn summary(self, language: UiLanguage) -> &'static str {
        match self {
            Self::CodexAppServer => language
                .choose("本地子进程 · stdio JSON-RPC", "Local child process · stdio JSON-RPC"),
            Self::ClaudeCode => language.choose("本地任务 · JSON 输出", "Local task · JSON output"),
            Self::Dsh => {
                language.choose("DeepSeek Harness · headless", "DeepSeek Harness · headless")
            }
        }
    }
}

/// What the Agents & tools detail pane currently shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AgentsSelection {
    Runtime(RuntimeAdapter),
    Provider,
    SkillsAndMcp,
    BundledJev,
}

/// One manageable EDA backend service on the EDA services page. The list
/// is the multi-EDA service registry surface: further backends slot in as
/// new variants with their own supervised bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EdaServiceSelection {
    JlcircuitBridge,
}

impl EdaServiceSelection {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::JlcircuitBridge => "JLCircuit EDA",
        }
    }
}

/// Observable state of the supervised bridge process.  Codex no longer keeps a
/// resident child: its page shows the one-shot [`CodexCheckResult`] instead.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RuntimeLifecycleStatus {
    Starting,
    Running { pid: u32 },
    Stopped,
    Failed { reason: String },
}

impl RuntimeLifecycleStatus {
    pub(super) const fn dot(&self) -> u32 {
        match self {
            Self::Starting => 0x00f5_9e0b,
            Self::Running { .. } => 0x0022_c55e,
            Self::Stopped => 0x0094_a3b8,
            Self::Failed { .. } => 0x00dc_2626,
        }
    }

    pub(super) fn label(&self, language: UiLanguage) -> String {
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
pub(super) enum BridgeHealth {
    /// No probe has completed yet; nothing is claimed either way.
    Unknown,
    /// The last probe could not connect: nothing is listening.
    Unreachable { reason: String },
    /// The last probe connected; `at` is when that succeeded.
    Listening { at: Instant },
}

impl BridgeHealth {
    pub(super) const fn dot(&self) -> u32 {
        match self {
            Self::Unknown => 0x0094_a3b8,
            Self::Unreachable { .. } => 0x00dc_2626,
            Self::Listening { .. } => 0x0022_c55e,
        }
    }

    pub(super) fn label(&self, language: UiLanguage) -> String {
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
pub(super) struct BridgeTestResult {
    pub(super) at: Instant,
    pub(super) outcome: Result<BridgeStatusReport, String>,
}

/// Outcome of the one-shot Codex connection check. The saved configuration is
/// launched once, must complete the JSON-RPC initialization handshake, and the
/// process is stopped immediately — no child stays resident. Tasks, datasheet
/// extraction, and EDA bridge sessions each spawn their own isolated process
/// from the same saved settings, so this check is the endpoint's only role.
pub(super) struct CodexCheckResult {
    pub(super) at: Instant,
    pub(super) outcome: Result<CodexCheckReport, String>,
}

/// What a passing check actually verified, shown next to the result.
#[derive(Clone)]
pub(super) struct CodexCheckReport {
    pub(super) provider: String,
    pub(super) command: String,
    pub(super) elapsed: Duration,
}

pub(super) fn elapsed_label(language: UiLanguage, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        format!("{seconds}{}", language.choose(" 秒", "s"))
    } else {
        format!("{}{}", seconds / 60, language.choose(" 分钟", "m"))
    }
}

/// Where a skill or MCP-server authorization is stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ToolScope {
    Global,
    Project,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectDetailTab {
    Overview,
    Documents,
    Sessions,
    AgentConfiguration,
    Usage,
}

impl ProjectDetailTab {
    pub(super) const ALL: [Self; 5] =
        [Self::Overview, Self::Documents, Self::Sessions, Self::AgentConfiguration, Self::Usage];

    pub(super) const fn label(self, language: UiLanguage) -> &'static str {
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
pub(super) enum ProjectFilter {
    All,
    NeedsConfiguration,
}

/// The semantic browser deliberately asks for a typed subject before applying free-text
/// matching. It avoids a single ambiguous "search everything" result being interpreted as
/// an engineering fact.

impl SemanticQueryScope {
    pub(super) const ALL: [Self; 5] =
        [Self::Components, Self::Pins, Self::Nets, Self::Constraints, Self::Evidence];

    pub(super) const fn label(self, language: UiLanguage) -> &'static str {
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
pub(super) enum BomExportFormat {
    Csv,
    Excel,
    Json,
}

impl BomExportFormat {
    pub(super) const ALL: [Self; 3] = [Self::Csv, Self::Excel, Self::Json];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Excel => "Excel (.xlsx)",
            Self::Json => "JSON",
        }
    }
}

/// The replay selected for the detail modal and optional runtime continuation.
pub(super) struct SessionReplaySelection {
    pub(super) project_id: ProjectId,
    pub(super) replay: SessionReplay,
    pub(super) markdown: Entity<gpui_component::text::TextViewState>,
}

/// Default width of the docked document-preview pane; the window grows by this much when
/// the preview opens, mirroring the extend-to-the-right behavior of document tools. The
/// user can then drag the divider to resize the pane within the clamp range below.
pub(super) const DOCUMENT_PREVIEW_WIDTH: f32 = 480.;
pub(super) const DOCUMENT_PREVIEW_MIN_WIDTH: f32 = 320.;
pub(super) const DOCUMENT_PREVIEW_MAX_WIDTH: f32 = 1040.;
/// Preview-pane render caps: the scroll pane lays out every element each frame, so a
/// huge document must not become thousands of text nodes inside the pane.
pub(super) const PREVIEW_MAX_RENDERED_PAGES: usize = 20;
pub(super) const PREVIEW_MAX_RENDERED_BLOCKS: usize = 120;
pub(super) const PREVIEW_MAX_RENDERED_ROWS: usize = 40;

/// Marker for the drag value carried while the preview divider is being dragged.
pub(super) struct DraggedPreviewSplit;

/// One page bitmap converted to a GPUI render image off the UI thread.
#[derive(Clone)]
pub(super) struct DocumentRasterPreviewPage {
    pub(super) image: std::sync::Arc<gpui::RenderImage>,
    pub(super) requested_width: u32,
}

/// Pages rendered around the viewport are kept; farther ones are evicted (and removed
/// from the GPU atlas), so memory stays bounded however long the document is.
pub(super) const RASTER_PREFETCH_BEFORE: u32 = 1;
pub(super) const RASTER_PREFETCH_AFTER: u32 = 2;
pub(super) const RASTER_KEEP_DISTANCE: u32 = 6;

/// A pdfium-backed PDF preview whose pages are rendered on demand as they scroll into
/// view. Every page keeps its place (sized from `page_sizes`) whether rendered or not.
#[derive(Clone)]
pub(super) struct DocumentRasterPreview {
    /// The verified bytes pages are rendered from.
    pub(super) data: std::sync::Arc<[u8]>,
    pub(super) content_hash: String,
    /// Size in points of every page.
    pub(super) page_sizes: Vec<(f32, f32)>,
    pub(super) pages: BTreeMap<u32, DocumentRasterPreviewPage>,
    pub(super) in_flight: BTreeMap<u32, u32>,
    pub(super) failed: std::collections::BTreeSet<(u32, u32)>,
    pub(super) retired_images: Vec<std::sync::Arc<gpui::RenderImage>>,
    /// Small, resolution-independent glyph metadata is reused across zoom renders.
    pub(super) text_pages:
        BTreeMap<u32, std::sync::Arc<circuitfabric_document_opener::PdfPageText>>,
}

/// What the preview pane shows for one selected document.
#[derive(Clone)]
pub(super) enum DocumentPreviewState {
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
pub(super) enum PdfIndexState {
    Running,
    Failed,
}

pub(super) use crate::application::datasheet_checkpoint::DatasheetCheckpoint;

/// The document currently shown in the right-hand preview pane.
#[derive(Clone)]
pub(super) struct DocumentPreviewSelection {
    pub(super) project_id: ProjectId,
    pub(super) file_name: String,
    pub(super) document_id: String,
    pub(super) state: DocumentPreviewState,
    /// The persisted structured datasheet extraction, when one exists for this exact
    /// content. Shared via `Arc` so per-frame rendering never clones the payload.
    pub(super) extraction: Option<std::sync::Arc<DatasheetExtraction>>,
}

/// Search typing is debounced; Enter and filter changes search immediately.
pub(super) const EVIDENCE_SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
/// Ranked hits kept per search, and how many are rendered per "show more" step.
pub(super) const EVIDENCE_SEARCH_LIMIT: usize = 500;
pub(super) const EVIDENCE_PAGE_SIZE: usize = 30;

/// The newest completed project-scoped evidence search.
pub(super) struct EvidenceSearchResult {
    pub(super) project_id: ProjectId,
    pub(super) scope: EvidenceScope,
    pub(super) search: std::sync::Arc<EvidenceSearch>,
    pub(super) elapsed: Duration,
}

/// The search hit a preview was opened from: the pane lands on its page or data row
/// and shows the cited line above the content.
#[derive(Clone, Debug)]
pub(super) struct PreviewFocus {
    pub(super) project_id: ProjectId,
    pub(super) document_id: String,
    pub(super) content_hash: String,
    pub(super) anchor: FragmentAnchor,
    pub(super) text: String,
    pub(super) terms: Vec<String>,
    pub(super) regions: Vec<circuitfabric_document_opener::PdfHighlightRect>,
    pub(super) notice: Option<String>,
    /// `true` while the source location is being resolved off the UI thread; the pane
    /// shows a loading mask over the document area until it clears.
    pub(super) resolving: bool,
}

// One GPUI view struct accumulates the whole control plane's UI state; the
// independent booleans track one asynchronous in-flight action each.
#[allow(clippy::struct_excessive_bools)]
pub(super) struct ControlPlaneView {
    pub(super) sidebar_mark: Arc<Image>,
    pub(super) command: Entity<InputState>,
    pub(super) bridge_address: Entity<InputState>,
    pub(super) providers: Vec<ProviderFields>,
    pub(super) default_provider_id: String,
    pub(super) selected_provider: usize,
    pub(super) navigation: DesktopShell,
    pub(super) language: UiLanguage,
    pub(super) global_theme: GlobalTheme,
    pub(super) data_directory: Entity<InputState>,
    pub(super) secret_storage_provider: SecretStorageProvider,
    pub(super) log_level: LogLevel,
    pub(super) settings_path: std::path::PathBuf,
    pub(super) status: String,
    pub(super) command_palette_open: bool,
    pub(super) command_search: Entity<InputState>,
    pub(super) command_selected: usize,
    pub(super) workspace: ProjectWorkspace,
    pub(super) project_registry: ProjectRegistry,
    pub(super) project_registry_path: PathBuf,
    pub(super) project_storages: BTreeMap<ProjectId, ProjectStorage>,
    pub(super) project_data: BTreeMap<ProjectId, ProjectWorkspaceData>,
    pub(super) session_replay: Option<SessionReplaySelection>,
    pub(super) session_modal_open: bool,
    pub(super) session_project_filter: Option<ProjectId>,
    pub(super) session_category_filter: circuitfabric_project::SessionCategory,
    // Docked document preview: the selection and, when the pane widened the window on
    // open, the size to restore when it closes. `document_preview_width` follows the
    // divider drag.
    pub(super) document_preview: Option<DocumentPreviewSelection>,
    pub(super) document_preview_width: f32,
    // Whether the preview pane shows the structured datasheet tab instead of the
    // rendered document.
    pub(super) preview_show_data: bool,
    pub(super) datasheet_extracting: bool,
    pub(super) datasheet_feedback: Option<String>,
    // Live log of the latest extraction (stage lines plus the streamed model reply),
    // keyed by the document it belongs to.
    pub(super) datasheet_stream:
        Option<(ProjectId, String, std::sync::Arc<std::sync::Mutex<String>>)>,
    pub(super) datasheet_stream_scroll: gpui::ScrollHandle,
    pub(super) datasheet_stream_modal_scroll: gpui::ScrollHandle,
    pub(super) datasheet_stream_modal_open: bool,
    pub(super) datasheet_extract_started: Option<Instant>,
    pub(super) datasheet_cancel: Option<circuitfabric_codex_runtime::execution::Cancellation>,
    pub(super) datasheet_checkpoint: Option<(ProjectId, String, DatasheetCheckpoint)>,
    // Background PDF full-text indexing per (project, document); indexed documents
    // leave the map, failed ones stay so they are not retried every time.
    pub(super) pdf_index_state: BTreeMap<(ProjectId, String), PdfIndexState>,
    /// Source-page lookup reuses the full-text indexing pass. Scoped by project/document
    /// and checked against the newly verified file hash before every navigation.
    pub(super) datasheet_source_indexes: BTreeMap<
        (ProjectId, String),
        std::sync::Arc<circuitfabric_document_opener::datasheet::DatasheetEvidenceIndex>,
    >,
    pub(super) datasheet_rows_visible: usize,
    pub(super) pre_preview_window_size: Option<Size<gpui::Pixels>>,
    pub(super) usage_period: UsagePeriod,
    pub(super) usage_filter: Entity<InputState>,
    pub(super) audit_kind_filter: AuditKindFilter,
    pub(super) audit_filter: Entity<InputState>,
    pub(super) audit_page: usize,
    pub(super) audit_page_filter: String,
    // Short-lived cache for the usage/audit projection; see `usage_audit_model`.
    pub(super) usage_audit_cached: Option<(Instant, UsageAuditModel)>,
    pub(super) overview_cached: crate::application::overview::OverviewCache,
    pub(super) project_read_errors: BTreeMap<String, String>,
    pub(super) overview_restore_diagnostics: Vec<String>,
    pub(super) evidence_query: Entity<InputState>,
    pub(super) evidence_scope: EvidenceScope,
    // Only the newest search request may publish its result.
    pub(super) evidence_generation: u64,
    pub(super) evidence_searching: bool,
    pub(super) evidence_result: Option<EvidenceSearchResult>,
    pub(super) evidence_visible: usize,
    pub(super) preview_focus: Option<PreviewFocus>,
    pub(super) preview_scroll: gpui::ScrollHandle,
    pub(super) preview_text_focus: gpui::FocusHandle,
    pub(super) selection_pressed: std::rc::Rc<std::cell::Cell<bool>>,
    pub(super) preview_pdf_zoom: crate::pdf_zoom::ZoomMotion,
    pub(super) preview_zoom_tick: Option<std::time::Instant>,
    pub(super) preview_pdf_pan: Option<(gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>)>,
    pub(super) preview_zoom_anchor: Option<(usize, gpui::Point<f32>, gpui::Point<gpui::Pixels>)>,
    pub(super) preview_row_anchor: Option<gpui::ScrollAnchor>,
    pub(super) preview_focus_generation: u64,
    // Set when the preview must scroll to `preview_focus` once its content is laid out.
    pub(super) preview_scroll_pending: std::cell::Cell<bool>,
    pub(super) semantic_query: Entity<InputState>,
    pub(super) semantic_query_scope: SemanticQueryScope,
    pub(super) selected_semantic_snapshot: Option<(ProjectId, String)>,
    pub(super) selected_change_set: Option<(ProjectId, String)>,
    pub(super) approval_drawer_open: bool,
    pub(super) approval_note: Entity<InputState>,
    pub(super) bom_export_format: BomExportFormat,
    pub(super) window_title: String,
    pub(super) project_search: Entity<InputState>,
    pub(super) project_filter: ProjectFilter,
    pub(super) project_tab: ProjectDetailTab,
    pub(super) project_form_open: bool,
    pub(super) new_project_id: Entity<InputState>,
    pub(super) new_project_name: Entity<InputState>,
    pub(super) new_project_description: Entity<InputState>,
    pub(super) new_project_root: Entity<InputState>,
    pub(super) agents_selection: AgentsSelection,
    pub(super) adapter_settings_open: Option<RuntimeAdapter>,
    pub(super) provider_editor_open: bool,
    pub(super) dialog_error: Option<String>,
    pub(super) codex_check_pending: bool,
    pub(super) codex_check: Option<CodexCheckResult>,
    pub(super) bridge_process: Option<BridgeProcessHandle>,
    pub(super) bridge_status: RuntimeLifecycleStatus,
    pub(super) bridge_active_address: Option<String>,
    pub(super) bridge_health: BridgeHealth,
    pub(super) bridge_probed_at: Option<Instant>,
    pub(super) bridge_probe_pending: bool,
    pub(super) bridge_test_pending: bool,
    pub(super) bridge_test: Option<BridgeTestResult>,
    pub(super) eda_services_selection: EdaServiceSelection,
    pub(super) tool_authorizations: ToolAuthorizationSettings,
    pub(super) new_tool_id: Entity<InputState>,
    pub(super) new_tool_kind: ToolAuthorizationKind,
    pub(super) new_tool_scope: ToolScope,
    pub(super) catalog: circuitfabric_codex_runtime::tools::ToolCatalog,
    pub(super) catalog_id: Entity<InputState>,
    pub(super) catalog_source: Entity<InputState>,
    pub(super) catalog_args: Entity<InputState>,
    pub(super) catalog_env: Entity<InputState>,
    pub(super) catalog_name: Entity<InputState>,
    pub(super) catalog_search: Entity<InputState>,
    pub(super) selected_skill: Option<String>,
    pub(super) selected_mcp: Option<String>,
    pub(super) catalog_editor: Option<CatalogEditor>,
    pub(super) catalog_error: Option<String>,
    pub(super) tool_feedback: String,
    pub(super) tool_reports: BTreeMap<String, ToolReport>,
    pub(super) tool_revision: u64,
    pub(super) tool_call_name: Entity<InputState>,
    pub(super) tool_call_args: Entity<InputState>,
    // Non-secret LLM adapter form and a write-only masked credential entry.
    pub(super) jev_api_key: Entity<InputState>,
    pub(super) jev_base_url: Entity<InputState>,
    pub(super) jev_model: Entity<InputState>,
    pub(super) jev_key_env: Entity<InputState>,
    pub(super) jev_timeout: Entity<InputState>,
    pub(super) jev_retries: Entity<InputState>,
    pub(super) jev_llm_settings: circuitfabric_codex_runtime::judge::LlmJudgeSettings,
    // Backend branch currently shown in the Jev page selector — a draft
    // until "save and use" applies it to the catalog.
    pub(super) jev_llm_draft: bool,
    // Jev configuration dialogs; edits inside stay draft until 保存.
    pub(super) jev_backend_modal_open: bool,
    pub(super) jev_key_modal_open: bool,
    pub(super) adapters: circuitfabric_codex_runtime::execution::AdapterSettings,
    pub(super) saved_settings: RuntimeSettings,
    pub(super) codex_provider: Entity<InputState>,
    pub(super) claude_command: Entity<InputState>,
    pub(super) claude_provider: Entity<InputState>,
    pub(super) dsh_command: Entity<InputState>,
    pub(super) dsh_provider: Entity<InputState>,
    pub(super) task_prompt: Entity<InputState>,
    pub(super) task_image: Entity<InputState>,
    pub(super) task_result: String,
    pub(super) task_cancel: Option<circuitfabric_codex_runtime::execution::Cancellation>,
    pub(super) task_identity: Option<(Option<ProjectId>, Option<String>, String)>,
    pub(super) project_agent_adapter: RuntimeAdapter,
    pub(super) plugin_directory: Entity<InputState>,
    pub(super) plugin_governance_path: PathBuf,
    pub(super) plugin_governance: PluginGovernanceStore,
    // Secrets vault: the encrypted API-key store. `vault` holds decrypted
    // key material only while unlocked; `vault_index` is the plaintext
    // variable-name index, readable even while locked.
    pub(super) vault_path: PathBuf,
    pub(super) vault: Option<UnlockedVault>,
    pub(super) vault_file_exists: bool,
    pub(super) vault_index: Vec<String>,
    pub(super) vault_prompt_open: bool,
    pub(super) vault_quick_unlock_open: bool,
    pub(super) vault_busy: bool,
    pub(super) vault_message: Option<String>,
    pub(super) vault_password: Entity<InputState>,
    pub(super) vault_password_confirm: Entity<InputState>,
    pub(super) secret_name: Entity<InputState>,
    pub(super) secret_value: Entity<InputState>,
    pub(super) selected_secret: Option<String>,
}

impl Drop for ControlPlaneView {
    fn drop(&mut self) {
        if let Some(cancel) = &self.task_cancel {
            cancel.cancel();
        }
        // Dropping `vault` zeroizes the derived key and all decrypted values.
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::DatasheetCheckpoint;
    use serde_json::json;

    #[test]
    fn legacy_null_parameter_output_is_feedback_instead_of_a_reusable_step() {
        let invalid = json!({"electricalCharacteristics":[{"parameter":null,"evidence":"IDD Supply current"}]}).to_string();
        let mut checkpoint = DatasheetCheckpoint::default();
        checkpoint.model_steps.push(("PDF source".into(), invalid.clone()));
        assert!(
            checkpoint.cached_model_reply(0, "electricalCharacteristics", "PDF source").is_none()
        );
        assert!(checkpoint.model_steps.is_empty());
        let feedback = &checkpoint.model_feedback["electricalCharacteristics"];
        assert_eq!(feedback.response, invalid);
        assert!(feedback.error.contains("parameter"));
    }

    #[test]
    fn resume_keeps_completed_batches_and_retries_incomplete_legacy_results() {
        let arguments = json!({"items":{"0":{}}});
        let good = json!({"content":[{"text":json!({"results":{"0":{"answers":{
            "category":{"choice":"pin","confidence":0.9},"faithful":{"noul":0.96}
        }}},"errors":{}}).to_string()}]});
        let failed = json!({"content":[{"text":json!({
            "results":{},"errors":{"0":"HTTP 429"}
        }).to_string()}]});
        let mut checkpoint = DatasheetCheckpoint::default();
        checkpoint.cache_jev_result(0, &arguments, &good).unwrap();
        assert!(checkpoint.cache_jev_result(1, &arguments, &failed).is_err());
        assert_eq!(checkpoint.jev_results.len(), 1);
        // Older code stored partial MCP successes along with subsequent batches.
        checkpoint.jev_results.push((arguments.clone(), failed));
        checkpoint.jev_results.push((arguments.clone(), good.clone()));
        assert_eq!(checkpoint.cached_jev_result(0, &arguments), Some(good.clone()));
        assert!(checkpoint.cached_jev_result(1, &arguments).is_none());
        assert_eq!(checkpoint.jev_results.len(), 1);
        // The retried batch can now complete and be reused on another continuation.
        checkpoint.cache_jev_result(1, &arguments, &good).unwrap();
        assert_eq!(checkpoint.cached_jev_result(1, &arguments), Some(good));
        assert!(
            checkpoint.cached_jev_result(1, &json!({"items":{"0":{"changed":true}}})).is_none()
        );
        assert_eq!(checkpoint.jev_results.len(), 1);
    }
}
