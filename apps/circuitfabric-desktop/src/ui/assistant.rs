//! The in-app assistant: callable from every screen, reads a structured snapshot of
//! the visible screen, and proposes UI actions that only run after confirmation.
use super::*;
use crate::application::assistant::{
    ACTION_FENCE_TAG, ActionStatus, AssistantAction, AssistantMaterials, AssistantSession,
    AssistantSessions, ChatMessage as AssistantMessage, ImportCategory, ProposedAction, Role,
    ScreenSnapshot, SnapshotField, SnapshotItem, Turn, build_prompt, clip as clip_text,
    derive_session_title, load_sessions, new_session_id, parse_reply, save_sessions,
};
use gpui::{App, Focusable as _};

pub(super) const ASSISTANT_SHORTCUT_LABEL: &str =
    if cfg!(target_os = "macos") { "⌘J" } else { "Ctrl J" };
const ASSISTANT_DEFAULT_WIDTH: f32 = 420.;
const ASSISTANT_DEFAULT_HEIGHT: f32 = 380.;
const ASSISTANT_MIN_WIDTH: f32 = 320.;
const ASSISTANT_MIN_HEIGHT: f32 = 260.;
/// The message box grows with its wrapped content up to this many rows, then scrolls.
const ASSISTANT_INPUT_MAX_ROWS: usize = 8;
// Gap between the floating panel and the main column's chrome.
const ASSISTANT_INSET: f32 = 12.;
const TOP_BAR_HEIGHT: f32 = 56.;
const STATUS_BAR_HEIGHT: f32 = 28.;

/// The edge of the main column the assistant panel floats against.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AssistantDock {
    Top,
    Bottom,
    Left,
    Right,
}

impl AssistantDock {
    const ALL: [Self; 4] = [Self::Left, Self::Top, Self::Bottom, Self::Right];

    const fn slug(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    const fn label(self, language: UiLanguage) -> &'static str {
        match self {
            Self::Top => language.choose("停靠到顶部", "Dock to top"),
            Self::Bottom => language.choose("停靠到底部", "Dock to bottom"),
            Self::Left => language.choose("停靠到左侧", "Dock to left"),
            Self::Right => language.choose("停靠到右侧", "Dock to right"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Top => {
                Icon::new(IconName::PanelBottom).rotate(gpui::radians(std::f32::consts::PI))
            }
            Self::Bottom => Icon::new(IconName::PanelBottom),
            Self::Left => Icon::new(IconName::PanelLeft),
            Self::Right => Icon::new(IconName::PanelRight),
        }
    }

    const fn is_horizontal_edge(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// Drag payload of the panel's inner, resizable edge.
pub(super) struct DraggedAssistantEdge;

pub(super) struct AssistantRun {
    /// The conversation the reply belongs to; switching sessions while it
    /// streams does not redirect it.
    pub(super) session_id: String,
    pub(super) cancel: circuitfabric_codex_runtime::execution::Cancellation,
    pub(super) stream: Arc<std::sync::Mutex<String>>,
    pub(super) reasoning: Arc<std::sync::Mutex<String>>,
}

pub(super) struct AssistantState {
    pub(super) open: bool,
    /// Grows with wrapped content up to [`ASSISTANT_INPUT_MAX_ROWS`], then scrolls.
    pub(super) input: Entity<gpui_component::input::TextareaState>,
    /// Every conversation, each with its own independent context; the active
    /// one is what the panel shows.
    pub(super) sessions: AssistantSessions,
    pub(super) run: Option<AssistantRun>,
    pub(super) list: gpui::ListState,
    /// Focus the message area claims on mouse-down, so Ctrl+C bubbles to the
    /// window-level copy handler instead of being swallowed by the composer.
    pub(super) text_focus: gpui::FocusHandle,
    pub(super) dock: AssistantDock,
    /// Panel width when docked left/right, height when docked top/bottom.
    pub(super) width: f32,
    pub(super) height: f32,
    /// Where the conversations are kept between launches.
    pub(super) history_path: PathBuf,
    /// The session picker popup opened from the panel header.
    pub(super) session_picker_open: bool,
}

impl AssistantState {
    /// Restores the saved conversations; returns a status note when they could
    /// not be read (the unreadable file is left untouched).
    pub(super) fn new(
        history_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<ControlPlaneView>,
    ) -> (Self, Option<String>) {
        let (sessions, load_error) = match load_sessions(&history_path) {
            Ok(sessions) => (sessions, None),
            Err(error) => {
                (AssistantSessions::fresh(), Some(format!("智能助手历史会话未恢复：{error}")))
            }
        };
        let input = cx.new(|cx| {
            gpui_component::input::TextareaState::new(window, cx)
                .auto_grow(1, ASSISTANT_INPUT_MAX_ROWS)
                .submit_on_enter(true)
                .placeholder("描述你想做的事，例如：帮我新建一个项目…")
        });
        // Enter sends; Shift+Enter inserts a newline.
        cx.subscribe_in(&input, window, |view, _, event, window, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { shift: false, .. } => view.send_assistant_message(window, cx),
            _ => {}
        })
        .detach();
        let list = gpui::ListState::new(
            sessions.active().messages.len(),
            gpui::ListAlignment::Bottom,
            px(512.),
        );
        list.set_follow_mode(gpui::FollowMode::Tail);
        let state = Self {
            open: false,
            input,
            sessions,
            run: None,
            list,
            text_focus: cx.focus_handle(),
            dock: AssistantDock::Right,
            width: ASSISTANT_DEFAULT_WIDTH,
            height: ASSISTANT_DEFAULT_HEIGHT,
            history_path,
            session_picker_open: false,
        };
        (state, load_error)
    }
}

/// Text shown while a reply streams: the action block is withheld until it is
/// complete and validated. The whole fence line is cut, whatever info string
/// decorates it.
fn visible_stream_text(stream: &str) -> &str {
    stream.find(ACTION_FENCE_TAG).map_or(stream, |index| {
        let line_start = stream[..index].rfind('\n').map_or(0, |at| at + 1);
        &stream[..line_start]
    })
}

/// Lists files under the materials directory as `/`-separated relative paths,
/// bounded so a runaway collector cannot flood the snapshot.
fn collect_material_files(
    directory: &std::path::Path,
    prefix: &str,
    depth: usize,
    files: &mut Vec<String>,
) {
    /// Keeps the snapshot bounded even if a runtime writes a large tree.
    const MAX_FILES: usize = 200;
    const MAX_DEPTH: usize = 4;
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }
        let (Ok(file_type), Ok(name)) = (entry.file_type(), entry.file_name().into_string()) else {
            continue;
        };
        let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if file_type.is_dir() {
            collect_material_files(&entry.path(), &relative, depth + 1, files);
        } else {
            files.push(relative);
        }
    }
}

impl ControlPlaneView {
    pub(super) fn toggle_assistant(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.assistant.open = !self.assistant.open;
        if self.assistant.open {
            self.assistant.input.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    fn set_assistant_dock(&mut self, dock: AssistantDock, cx: &mut Context<Self>) {
        self.assistant.dock = dock;
        cx.notify();
    }

    /// Follows the dragged inner edge. The opposite edge stays anchored to the
    /// panel's dock side, so the panel's own bounds give the new extent.
    fn resize_assistant(
        &mut self,
        event: &DragMoveEvent<DraggedAssistantEdge>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let pointer = event.event.position;
        let viewport = window.viewport_size();
        let max_width = (f32::from(viewport.width) * 0.75).max(ASSISTANT_MIN_WIDTH);
        let max_height = (f32::from(viewport.height)
            - TOP_BAR_HEIGHT
            - STATUS_BAR_HEIGHT
            - 4. * ASSISTANT_INSET)
            .max(ASSISTANT_MIN_HEIGHT);
        let state = &mut self.assistant;
        match state.dock {
            AssistantDock::Right => {
                state.width =
                    f32::from(bounds.right() - pointer.x).clamp(ASSISTANT_MIN_WIDTH, max_width);
            }
            AssistantDock::Left => {
                state.width =
                    f32::from(pointer.x - bounds.left()).clamp(ASSISTANT_MIN_WIDTH, max_width);
            }
            AssistantDock::Top => {
                state.height =
                    f32::from(pointer.y - bounds.top()).clamp(ASSISTANT_MIN_HEIGHT, max_height);
            }
            AssistantDock::Bottom => {
                state.height =
                    f32::from(bounds.bottom() - pointer.y).clamp(ASSISTANT_MIN_HEIGHT, max_height);
            }
        }
        cx.notify();
    }

    pub(super) fn assistant_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.assistant.open && self.assistant.input.read(cx).focus_handle(cx).is_focused(window)
    }

    /// Editable inputs the assistant may read and fill on the current screen.
    /// Masked secret values are never listed.
    fn assistant_fields(&self) -> Vec<(&'static str, &'static str, Entity<InputState>)> {
        let l = self.language;
        let mut fields = Vec::new();
        match self.navigation.screen {
            ControlPlaneScreen::Projects => {
                fields.push((
                    "project_search",
                    l.choose("搜索项目", "Search projects"),
                    self.project_search.clone(),
                ));
                if self.project_form_open {
                    fields.extend([
                        (
                            "new_project_id",
                            l.choose("新项目 ID", "New project ID"),
                            self.new_project_id.clone(),
                        ),
                        (
                            "new_project_name",
                            l.choose("新项目名称", "New project name"),
                            self.new_project_name.clone(),
                        ),
                        (
                            "new_project_description",
                            l.choose("新项目描述", "New project description"),
                            self.new_project_description.clone(),
                        ),
                        (
                            "new_project_root",
                            l.choose("项目根文件夹", "Project root folder"),
                            self.new_project_root.clone(),
                        ),
                    ]);
                }
            }
            ControlPlaneScreen::Documents => fields.push((
                "evidence_query",
                l.choose("证据检索", "Evidence search"),
                self.evidence_query.clone(),
            )),
            ControlPlaneScreen::Semantics => fields.push((
                "semantic_query",
                l.choose("语义查询", "Semantic query"),
                self.semantic_query.clone(),
            )),
            ControlPlaneScreen::ChangesAndApprovals => fields.push((
                "approval_note",
                l.choose("审批意见", "Approval note"),
                self.approval_note.clone(),
            )),
            ControlPlaneScreen::AgentsAndMcp => fields.extend([
                (
                    "task_prompt",
                    l.choose("验证任务提示词", "Task prompt"),
                    self.task_prompt.clone(),
                ),
                (
                    "task_image",
                    l.choose("可选图片路径", "Optional image path"),
                    self.task_image.clone(),
                ),
            ]),
            ControlPlaneScreen::EdaServices => fields.push((
                "bridge_address",
                l.choose("Bridge 监听地址", "Bridge listen address"),
                self.bridge_address.clone(),
            )),
            ControlPlaneScreen::Usage => fields.extend([
                ("usage_filter", l.choose("用量筛选", "Usage filter"), self.usage_filter.clone()),
                ("audit_filter", l.choose("审计筛选", "Audit filter"), self.audit_filter.clone()),
            ]),
            ControlPlaneScreen::Plugins => fields.push((
                "plugin_directory",
                l.choose("插件清单目录", "Plugin manifest directory"),
                self.plugin_directory.clone(),
            )),
            ControlPlaneScreen::SecretsVault => fields.push((
                "secret_name",
                l.choose("密钥变量名", "Secret variable name"),
                self.secret_name.clone(),
            )),
            ControlPlaneScreen::Settings => fields.push((
                "data_directory",
                l.choose("数据目录", "Data directory"),
                self.data_directory.clone(),
            )),
            ControlPlaneScreen::Overview
            | ControlPlaneScreen::SessionsAndTasks
            | ControlPlaneScreen::BomAndExport => {}
        }
        fields
    }

    /// The assistant's material directory for the selected project —
    /// `<project root>/assistant-materials` — and the files in it right now.
    /// The runtime may write collected material here while material collection
    /// is enabled; `None` when no project is selected.
    fn assistant_materials(&self) -> Option<AssistantMaterials> {
        let (_, root) = self.current_project_context()?;
        let directory = root.join("assistant-materials");
        let mut files = Vec::new();
        collect_material_files(&directory, "", 0, &mut files);
        Some(AssistantMaterials { directory: directory.to_string_lossy().into_owned(), files })
    }

    /// Whether the assistant's saved runtime can act on material-collection
    /// permissions (Codex via its sandbox, Claude Code via its tools).
    fn assistant_material_runtime_supported(&self) -> bool {
        let kind = self
            .saved_settings
            .assistant
            .runtime
            .unwrap_or(self.saved_settings.adapters.default_runtime);
        matches!(
            kind,
            circuitfabric_codex_runtime::execution::AgentKind::Codex
                | circuitfabric_codex_runtime::execution::AgentKind::Claude
        )
    }

    fn assistant_collects_material(&self) -> bool {
        self.navigation.selected_project.is_some() && self.assistant_material_runtime_supported()
    }

    /// Controls the assistant may press on the current screen; each maps to the
    /// same handler as the visible button.
    fn assistant_controls(&self) -> Vec<(&'static str, &'static str)> {
        let l = self.language;
        let mut controls =
            vec![("open_command_palette", l.choose("打开命令面板", "Open command palette"))];
        match self.navigation.screen {
            ControlPlaneScreen::Projects if self.project_form_open => controls.extend([
                ("choose_project_root", l.choose("选择项目根文件夹", "Choose project root folder")),
                ("create_project", l.choose("确认创建项目", "Create project")),
            ]),
            ControlPlaneScreen::Projects => {
                controls.push(("new_project", l.choose("新建项目", "New project")));
            }
            ControlPlaneScreen::Documents if self.navigation.selected_project.is_some() => {
                controls.push(("search_evidence", l.choose("立即检索证据", "Search evidence now")));
            }
            ControlPlaneScreen::AgentsAndMcp => controls.extend([
                (
                    "check_default_runtime",
                    l.choose("检查默认运行时连接", "Check default runtime connection"),
                ),
                (
                    "run_default_runtime_task",
                    l.choose("用默认运行时运行验证任务", "Run task with the default runtime"),
                ),
            ]),
            ControlPlaneScreen::EdaServices => controls.extend([
                ("start_bridge", l.choose("启动 Bridge 服务", "Start bridge service")),
                ("test_bridge", l.choose("测试 Bridge 连接", "Test bridge connection")),
            ]),
            _ => {}
        }
        controls
    }

    fn press_assistant_control(
        &mut self,
        control: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match control {
            "open_command_palette" => self.open_command_palette(window, cx),
            "new_project" => self.open_project_form(cx),
            "choose_project_root" => self.choose_project_root(window, cx),
            "create_project" => self.create_project(window, cx),
            "search_evidence" => self.schedule_evidence_search(Duration::ZERO, cx),
            "check_default_runtime" => self.check_default_runtime(window, cx),
            "run_default_runtime_task" => {
                self.run_agent_task(self.default_runtime(), window, cx);
            }
            "start_bridge" => self.start_bridge_service(window, cx),
            "test_bridge" => self.test_bridge_connection(window, cx),
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn assistant_snapshot(&self, cx: &App) -> ScreenSnapshot {
        let l = self.language;
        let screen = self.navigation.screen;
        let project_selected = self.navigation.selected_project.is_some();
        let selected = self.navigation.selected_project.as_deref();
        let mut facts = vec![
            format!("Status bar message: {}", self.status),
            format!("Secrets vault: {}", if self.vault.is_some() { "unlocked" } else { "locked" }),
            format!(
                "Default runtime: {} · check: {}",
                self.default_runtime().label(),
                self.runtime_check_status(self.default_runtime(), l)
            ),
            format!(
                "JLCircuit bridge: {} / {}",
                self.bridge_status.label(l),
                self.bridge_health.label(l)
            ),
        ];
        if let Some(id) = selected {
            if let Some(project) = self.workspace.project(id) {
                facts.push(format!(
                    "Selected project: {} ({}){}",
                    project.name,
                    project.id,
                    project.description.as_deref().map(|d| format!(" — {d}")).unwrap_or_default()
                ));
            }
            if let Some(data) = self.project_data.get(id) {
                let names: Vec<&str> =
                    data.documents.iter().take(20).map(|d| d.original_file_name.as_str()).collect();
                facts.push(format!(
                    "Project data: {} documents [{}], {} sessions, {} semantic snapshots, {} change sets",
                    data.documents.len(),
                    names.join(", "),
                    data.session_listing.sessions.len(),
                    data.semantic_snapshots.len(),
                    data.change_sets.len()
                ));
            }
        }
        match screen {
            ControlPlaneScreen::AgentsAndMcp => {
                facts.push(format!("Agents page detail pane: {:?}", self.agents_selection));
                let runtime = self.default_runtime();
                let result = &self.quick_task(runtime).result;
                if !result.is_empty() {
                    facts.push(format!("Last {} task result: {result}", runtime.label()));
                }
            }
            ControlPlaneScreen::Documents => {
                if let Some(result) = &self.evidence_result {
                    facts.push(format!(
                        "Latest evidence search {:?}: {} matches in {} documents",
                        result.search.query, result.search.total, result.search.documents
                    ));
                }
            }
            _ => {}
        }
        if let Some(preview) = &self.document_preview {
            facts.push(format!("Document preview pane open: {}", preview.file_name));
        }
        let mut open_dialogs = Vec::new();
        for (open, name) in [
            (self.project_form_open, "new project form"),
            (self.provider_editor_open, "provider editor"),
            (self.session_modal_open, "session detail"),
            (self.command_palette_open, "command palette"),
            (self.vault_prompt_open && self.vault.is_none(), "vault unlock"),
        ] {
            if open {
                open_dialogs.push(name.to_owned());
            }
        }
        ScreenSnapshot {
            screen_id: screen.slug().into(),
            screen_label: l.screen_label(screen).into(),
            screens: ControlPlaneScreen::ALL
                .into_iter()
                .map(|s| {
                    (
                        SnapshotItem { id: s.slug().into(), label: l.screen_label(s).into() },
                        !s.requires_project() || project_selected,
                    )
                })
                .collect(),
            language: l.choose("简体中文", "English").into(),
            theme: format!("{:?}", self.global_theme).to_lowercase(),
            selected_project: selected.map(str::to_owned),
            projects: self
                .workspace
                .projects()
                .into_iter()
                .map(|p| SnapshotItem { id: p.id.clone(), label: p.name.clone() })
                .collect(),
            open_dialogs,
            facts,
            fields: self
                .assistant_fields()
                .into_iter()
                .map(|(id, label, state)| SnapshotField {
                    id: id.into(),
                    label: label.into(),
                    value: state.read(cx).value().to_string(),
                })
                .collect(),
            controls: self
                .assistant_controls()
                .into_iter()
                .map(|(id, label)| SnapshotItem { id: id.into(), label: label.into() })
                .collect(),
            materials: self.assistant_materials(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn send_assistant_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use circuitfabric_codex_runtime::execution::Cancellation;
        let request = self.assistant.input.read(cx).value().trim().to_owned();
        if self.assistant.run.is_some() || request.is_empty() {
            return;
        }
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        let (kind, settings) = settings.for_assistant();
        let language = self.language;
        let snapshot = self.assistant_snapshot(cx);
        // Material collection: a folder inside the selected project's root the
        // runtime may write into, offered only to runtimes that can act on it.
        let workspace = self
            .assistant_collects_material()
            .then(|| {
                snapshot.materials.as_ref().map(|materials| PathBuf::from(&materials.directory))
            })
            .flatten();
        if let Some(directory) = &workspace
            && let Err(error) = std::fs::create_dir_all(directory)
        {
            self.status = format!("资料目录创建失败，本次不收集资料：{error}");
            cx.notify();
            return;
        }
        let capabilities = if workspace.is_some() {
            circuitfabric_codex_runtime::execution::TurnCapabilities::COLLECT_MATERIAL
        } else {
            circuitfabric_codex_runtime::execution::TurnCapabilities::default()
        };
        let prompt_materials = workspace.as_ref().and(snapshot.materials.clone());
        let history: Vec<Turn> = self
            .assistant
            .sessions
            .active()
            .messages
            .iter()
            .filter(|m| !m.pending && !m.text.is_empty())
            .map(|m| Turn { role: m.role, text: m.text.clone() })
            .collect();
        let prompt = build_prompt(
            &snapshot,
            &history,
            &request,
            language.choose("简体中文", "English"),
            prompt_materials.as_ref(),
        );
        let run_session_id = self.assistant.sessions.active_id.clone();
        // Sending bumps the conversation to the front of the picker.
        self.assistant.sessions.move_to_front(&run_session_id);
        let session = self.assistant.sessions.active_mut();
        session.messages.push(AssistantMessage {
            role: Role::User,
            text: request,
            reasoning: String::new(),
            context: snapshot.screen_label.clone(),
            pending: false,
            actions: Vec::new(),
            error: None,
            reasoning_collapsed: false,
        });
        // The topic summarizes the conversation once it has its first request.
        if session.title.is_empty() {
            session.title = derive_session_title(&session.messages);
        }
        session.messages.push(AssistantMessage {
            role: Role::Assistant,
            text: String::new(),
            reasoning: String::new(),
            context: String::new(),
            pending: true,
            actions: Vec::new(),
            error: None,
            reasoning_collapsed: false,
        });
        self.assistant.input.update(cx, |state, cx| state.set_value("", window, cx));
        let cancel = Cancellation::default();
        let stream = Arc::new(std::sync::Mutex::new(String::new()));
        let reasoning = Arc::new(std::sync::Mutex::new(String::new()));
        self.assistant.run = Some(AssistantRun {
            session_id: run_session_id.clone(),
            cancel: cancel.clone(),
            stream: stream.clone(),
            reasoning: reasoning.clone(),
        });
        let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
        let writer = stream.clone();
        let reasoning_writer = reasoning.clone();
        let work = cx.background_spawn(async move {
            circuitfabric_codex_runtime::execution::run_task_streaming(
                &settings,
                kind,
                &ToolAuthorizationSettings::default(),
                &prompt,
                None,
                workspace.as_deref(),
                capabilities,
                secrets.as_ref(),
                &cancel,
                &mut |kind, delta| {
                    let target = match kind {
                        circuitfabric_codex_runtime::TurnDelta::Answer => &writer,
                        circuitfabric_codex_runtime::TurnDelta::Reasoning => &reasoning_writer,
                    };
                    if let Ok(mut buffer) = target.lock() {
                        buffer.push_str(delta);
                    }
                },
            )
            .map_err(|error| error.to_string())
        });
        let repaint_stream = stream;
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                let active = cx
                    .update(|_, cx| {
                        view.update(cx, |view, cx| {
                            let Some(run) = &view.assistant.run else {
                                return false;
                            };
                            if !Arc::ptr_eq(&run.stream, &repaint_stream) {
                                return false;
                            }
                            let session_id = run.session_id.clone();
                            let text = run
                                .stream
                                .lock()
                                .map(|buffer| visible_stream_text(&buffer).to_owned())
                                .unwrap_or_default();
                            let thinking = run
                                .reasoning
                                .lock()
                                .map(|buffer| buffer.clone())
                                .unwrap_or_default();
                            if let Some(session) = view.assistant.sessions.session_mut(&session_id)
                                && let Some(last) = session.messages.last_mut()
                                && last.pending
                                && (last.text != text || last.reasoning != thinking)
                            {
                                last.text = text;
                                last.reasoning = thinking;
                                cx.notify();
                            }
                            true
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
        let completion_reasoning = reasoning;
        cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            let final_reasoning =
                completion_reasoning.lock().map(|buffer| buffer.clone()).unwrap_or_default();
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.assistant.run = None;
                    let chinese = view.language == UiLanguage::SimplifiedChinese;
                    if let Some(session) = view.assistant.sessions.session_mut(&run_session_id)
                        && let Some(last) = session.messages.last_mut()
                        && last.pending
                    {
                        last.pending = false;
                        if !final_reasoning.is_empty() {
                            last.reasoning = final_reasoning;
                        }
                        // Streaming shows thinking expanded; the finished reply
                        // tucks it away.
                        last.reasoning_collapsed = true;
                        match result {
                            Ok(reply) => {
                                let parsed = parse_reply(&reply);
                                last.text = parsed.text;
                                last.error = parsed.error;
                                last.actions = parsed
                                    .actions
                                    .into_iter()
                                    .map(|action| ProposedAction {
                                        description: action.describe(&snapshot, chinese),
                                        action,
                                        status: ActionStatus::Pending,
                                    })
                                    .collect();
                            }
                            Err(error) => {
                                last.error = Some(format!(
                                    "{}：{error}",
                                    view.language
                                        .choose("助手请求失败", "Assistant request failed")
                                ));
                            }
                        }
                    }
                    view.persist_assistant();
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        self.persist_assistant();
        cx.notify();
    }

    /// Writes every conversation to disk after each change; a failure is
    /// surfaced in the status bar and the in-memory state is kept.
    fn persist_assistant(&mut self) {
        if let Err(error) = save_sessions(&self.assistant.history_path, &self.assistant.sessions) {
            self.status = format!("智能助手会话未保存：{error}");
        }
    }

    fn stop_assistant(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.assistant.run {
            run.cancel.cancel();
        }
        cx.notify();
    }

    /// Switches the panel to another conversation. A streaming reply keeps
    /// flowing into the session that started it, so switching never mixes
    /// contexts.
    fn switch_assistant_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.assistant.session_picker_open = false;
        if !self.assistant.sessions.sessions.iter().any(|s| s.id == id) {
            cx.notify();
            return;
        }
        if self.assistant.sessions.active_id != id {
            self.assistant.sessions.move_to_front(id);
            id.clone_into(&mut self.assistant.sessions.active_id);
            let count = self.assistant.sessions.active().messages.len();
            self.assistant.list.reset(count);
            self.assistant.input.update(cx, |state, cx| state.focus(window, cx));
            self.persist_assistant();
        }
        cx.notify();
    }

    /// Starts a new, empty conversation and switches to it; an existing empty
    /// draft is reused so they do not pile up.
    fn new_assistant_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.assistant.sessions.active().messages.is_empty() {
            self.assistant.session_picker_open = false;
            cx.notify();
            return;
        }
        let fresh_id = if let Some(draft) =
            self.assistant.sessions.sessions.iter().find(|s| s.messages.is_empty())
        {
            draft.id.clone()
        } else {
            let session = AssistantSession::fresh(new_session_id());
            let id = session.id.clone();
            self.assistant.sessions.sessions.insert(0, session);
            id
        };
        self.switch_assistant_session(&fresh_id, window, cx);
    }

    /// Deletes a conversation; the one a streaming reply belongs to is refused
    /// until it finishes. Deleting the last conversation starts a fresh one.
    fn delete_assistant_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.assistant.run.as_ref().is_some_and(|run| run.session_id == id) {
            self.status = self.language.choose_owned(
                "会话正在回复，无法删除".to_owned(),
                "This conversation is replying and cannot be deleted".to_owned(),
            );
            cx.notify();
            return;
        }
        let Some(index) = self.assistant.sessions.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        let was_active = self.assistant.sessions.active_id == id;
        self.assistant.sessions.sessions.remove(index);
        if self.assistant.sessions.sessions.is_empty() {
            let session = AssistantSession::fresh(new_session_id());
            self.assistant.sessions.active_id.clone_from(&session.id);
            self.assistant.sessions.sessions.push(session);
        } else if was_active {
            let neighbor = index.min(self.assistant.sessions.sessions.len() - 1);
            let neighbor_id = self.assistant.sessions.sessions[neighbor].id.clone();
            self.assistant.sessions.active_id = neighbor_id;
        }
        if was_active {
            let count = self.assistant.sessions.active().messages.len();
            self.assistant.list.reset(count);
        }
        self.persist_assistant();
        cx.notify();
    }

    fn clear_assistant(&mut self, cx: &mut Context<Self>) {
        if self.assistant.run.is_none() {
            let session = self.assistant.sessions.active_mut();
            session.messages.clear();
            session.title.clear();
            self.assistant.list.reset(0);
            self.persist_assistant();
            cx.notify();
        }
    }

    fn toggle_assistant_reasoning(&mut self, message: usize, cx: &mut Context<Self>) {
        if let Some(message) = self.assistant.sessions.active_mut().messages.get_mut(message) {
            message.reasoning_collapsed = !message.reasoning_collapsed;
            cx.notify();
        }
    }

    /// Copies an on-disk file into managed storage and refreshes the project —
    /// the shared core of every assistant import path.
    fn assistant_import_file(
        &mut self,
        source: &std::path::Path,
        category: ImportCategory,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let Some((project_id, _)) = self.current_project_context() else {
            return Err("项目根目录未打开".to_owned());
        };
        let Some(storage) = self.project_storages.get(&project_id).cloned() else {
            return Err("项目根目录未打开".to_owned());
        };
        let document_category = match category {
            ImportCategory::Datasheet => DocumentCategory::Datasheet,
            ImportCategory::ReferenceDesign => DocumentCategory::ReferenceDesign,
        };
        let imported = self
            .workspace
            .import_project_document(&project_id, &storage, source, document_category)
            .map_err(|error| error.to_string())?;
        let document = &imported.document;
        if !imported.created {
            return Ok(format!(
                "`{}` 已导入过（{}），未新增记录",
                document.original_file_name, document.id
            ));
        }
        self.refresh_project_data(&project_id)
            .map_err(|error| format!("文档已导入，但列表未刷新：{error}"))?;
        self.schedule_pdf_indexing(&project_id, cx);
        self.refresh_evidence_search(cx);
        Ok(format!("已导入 `{}`（{}）", document.original_file_name, document.id))
    }

    /// Runs an import the assistant proposed: the same copy-into-storage,
    /// listing refresh and PDF indexing as the document screen's import
    /// button, but from a file the runtime already collected.
    fn assistant_import_collected(
        &mut self,
        snapshot: &ScreenSnapshot,
        path: &str,
        category: ImportCategory,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let directory = snapshot
            .materials
            .as_ref()
            .map(|materials| PathBuf::from(&materials.directory))
            .ok_or_else(|| "未选择项目，不能导入收集的文档".to_owned())?;
        self.assistant_import_file(&directory.join(path), category, cx)
    }

    /// Starts a confirmed download: the card moves to Running, the fetch and
    /// optional import run off the UI thread, and the completion handler
    /// writes the final Done/Failed status into the proposing message.
    #[allow(clippy::too_many_arguments)]
    fn assistant_fetch_document(
        &mut self,
        snapshot: &ScreenSnapshot,
        message: usize,
        index: usize,
        url: &str,
        file_name: &str,
        category: Option<ImportCategory>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = snapshot
            .materials
            .as_ref()
            .map(|materials| PathBuf::from(&materials.directory))
            .unwrap_or_default();
        let url = url.trim().to_owned();
        let file_name = file_name.to_owned();
        let session_id = self.assistant.sessions.active_id.clone();
        let project_open = self.current_project_context().is_some();
        let language = self.language;
        if let Some(proposed) = self
            .assistant
            .sessions
            .active_mut()
            .messages
            .get_mut(message)
            .and_then(|m| m.actions.get_mut(index))
        {
            proposed.status =
                ActionStatus::Running(self.language.choose("正在下载…", "Downloading…").to_owned());
        }
        cx.spawn_in(window, async move |view, cx| {
            let fetched = cx
                .background_spawn(async move {
                    circuitfabric_codex_runtime::models::fetch_url_bytes(&url)
                        .map_err(|error| error.to_string())
                })
                .await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    let outcome = fetched.and_then(|bytes| {
                        let target = directory.join(&file_name);
                        std::fs::write(&target, bytes)
                            .map_err(|error| format!("写入失败：{error}"))?;
                        let size_kb =
                            std::fs::metadata(&target).map_or(0, |meta| meta.len() / 1024);
                        match category {
                            Some(category) if project_open => {
                                view.assistant_import_file(&target, category, cx)
                            }
                            _ => Ok(language.choose_owned(
                                format!("已下载 `{file_name}`（{size_kb} KB）"),
                                format!("Downloaded `{file_name}` ({size_kb} KB)"),
                            )),
                        }
                    });
                    let Some(session) = view.assistant.sessions.session_mut(&session_id) else {
                        return;
                    };
                    if let Some(proposed) =
                        session.messages.get_mut(message).and_then(|m| m.actions.get_mut(index))
                        && let ActionStatus::Running(_) = proposed.status
                    {
                        proposed.status = match outcome {
                            Ok(text) => ActionStatus::Done(text),
                            Err(error) => ActionStatus::Failed(error),
                        };
                    }
                    view.persist_assistant();
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    /// Runs one proposed action after re-validating it against the screen as it is now.
    fn run_assistant_action(
        &mut self,
        message: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(proposed) = self
            .assistant
            .sessions
            .active()
            .messages
            .get(message)
            .and_then(|m| m.actions.get(index))
            .filter(|p| p.status == ActionStatus::Pending)
        else {
            return false;
        };
        let action = proposed.action.clone();
        let snapshot = self.assistant_snapshot(cx);
        let outcome = if let AssistantAction::ImportDocument { path, category } = &action {
            action
                .validate(&snapshot)
                .and_then(|()| self.assistant_import_collected(&snapshot, path, *category, cx))
        } else if let AssistantAction::FetchDocument { url, file_name, category } = &action {
            if let Err(error) = action.validate(&snapshot) {
                Err(error)
            } else {
                self.assistant_fetch_document(
                    &snapshot, message, index, url, file_name, *category, window, cx,
                );
                // The helper moved the card to Running and owns its
                // completion; the generic status write below must not
                // overwrite it.
                self.persist_assistant();
                cx.notify();
                return true;
            }
        } else {
            action.validate(&snapshot).map(|()| {
                match &action {
                    AssistantAction::Navigate { screen } => {
                        if let Some(target) =
                            ControlPlaneScreen::ALL.into_iter().find(|s| s.slug() == screen)
                        {
                            self.navigation.screen = target;
                        }
                    }
                    AssistantAction::SelectProject { project } => {
                        self.select_project(project.clone(), cx);
                    }
                    AssistantAction::SetInput { field, value } => {
                        if let Some((_, _, state)) =
                            self.assistant_fields().into_iter().find(|(id, _, _)| id == field)
                        {
                            state.update(cx, |state, cx| {
                                state.set_value(value.clone(), window, cx);
                            });
                        }
                    }
                    AssistantAction::Press { control } => {
                        self.press_assistant_control(control, window, cx);
                    }
                    AssistantAction::SetTheme { theme } => {
                        let theme = match theme.as_str() {
                            "light" => GlobalTheme::Light,
                            "system" => GlobalTheme::System,
                            _ => GlobalTheme::Dark,
                        };
                        self.set_global_theme(theme, cx);
                    }
                    AssistantAction::ToggleLanguage => self.toggle_global_language(window, cx),
                    // Routed through their own handlers above.
                    AssistantAction::ImportDocument { .. }
                    | AssistantAction::FetchDocument { .. } => {}
                }
                self.language.choose("已执行", "Done").to_owned()
            })
        };
        let ok = outcome.is_ok();
        if let Some(proposed) = self
            .assistant
            .sessions
            .active_mut()
            .messages
            .get_mut(message)
            .and_then(|m| m.actions.get_mut(index))
        {
            proposed.status = match outcome {
                Ok(text) => ActionStatus::Done(text),
                Err(error) => ActionStatus::Failed(error),
            };
        }
        self.persist_assistant();
        cx.notify();
        ok
    }

    fn run_all_assistant_actions(
        &mut self,
        message: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count =
            self.assistant.sessions.active().messages.get(message).map_or(0, |m| m.actions.len());
        for index in 0..count {
            let pending = self.assistant.sessions.active().messages[message].actions[index].status
                == ActionStatus::Pending;
            if pending && !self.run_assistant_action(message, index, window, cx) {
                break;
            }
        }
    }

    fn skip_assistant_action(&mut self, message: usize, index: usize, cx: &mut Context<Self>) {
        if let Some(proposed) = self
            .assistant
            .sessions
            .active_mut()
            .messages
            .get_mut(message)
            .and_then(|m| m.actions.get_mut(index))
            && proposed.status == ActionStatus::Pending
        {
            proposed.status = ActionStatus::Skipped;
            self.persist_assistant();
            cx.notify();
        }
    }

    pub(super) fn render_assistant_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let open = self.assistant.open;
        div()
            .id("assistant-trigger")
            .debug_selector(|| "assistant-trigger".into())
            .flex_none()
            .p(px(1.))
            .rounded_full()
            .bg(accent_gradient(90.))
            .when(open, |this| this.shadow(accent_glow(0.45, 14.)))
            .cursor_pointer()
            .on_click(move |_, window, cx| {
                entity.update(cx, |view, cx| view.toggle_assistant(window, cx));
            })
            .child(
                div()
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .rounded_full()
                    .when(!open, |this| this.bg(rgb(CARD_BG)))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if open { rgb(SIDEBAR_BG) } else { rgb(TEXT_PRIMARY) })
                    .child(Icon::new(IconName::Bot).small())
                    .child(self.language.choose("智能助手", "Assistant")),
            )
    }

    /// Either the floating launcher or the open assistant panel.
    pub(super) fn render_assistant_layer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().clone();
        if !self.assistant.open {
            return div()
                .id("assistant-launcher")
                .debug_selector(|| "assistant-launcher".into())
                .absolute()
                .right(px(20.))
                .bottom(px(44.))
                .size(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(accent_gradient(135.))
                .shadow(accent_glow(0.55, 22.))
                .text_color(rgb(SIDEBAR_BG))
                .cursor_pointer()
                .hover(|this| this.opacity(0.9))
                .occlude()
                .on_click(move |_, window, cx| {
                    entity.update(cx, |view, cx| view.toggle_assistant(window, cx));
                })
                .child(Icon::new(IconName::Bot))
                .into_any_element();
        }
        self.render_assistant_panel(cx).into_any_element()
    }

    #[allow(clippy::too_many_lines)]
    fn render_assistant_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let entity = cx.entity().clone();
        let busy = self.assistant.run.is_some();
        let screen_label = language.screen_label(self.navigation.screen);
        let project_selected = self.navigation.selected_project.is_some();
        let collecting = self.assistant_collects_material();
        // A selected project but a runtime without collection tools: tell the
        // user why the assistant will answer "I cannot fetch" instead.
        let collecting_unavailable =
            project_selected && !self.assistant_material_runtime_supported();
        let messages = std::rc::Rc::new(self.assistant.sessions.active().messages.clone());
        let list = self.assistant.list.clone();
        if list.item_count() != messages.len() {
            list.reset(messages.len());
        } else if !messages.is_empty() {
            list.remeasure_items(0..messages.len());
        }
        let empty = self.assistant.input.read(cx).value().trim().is_empty();
        let dock = self.assistant.dock;

        let mut dock_switcher = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_0p5()
            .p_0p5()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER));
        for target in AssistantDock::ALL {
            let entity = entity.clone();
            dock_switcher = dock_switcher.child(
                action_button(format!("assistant-dock-{}", target.slug()))
                    .compact()
                    .map(|button| if target == dock { button.primary() } else { button.ghost() })
                    .icon(target.icon())
                    .tooltip(target.label(language))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |view, cx| view.set_assistant_dock(target, cx));
                    }),
            );
        }

        // The header title is the session picker: current topic plus a caret.
        let session_title = self.assistant.sessions.active().title.clone();
        let trigger_label = if session_title.is_empty() {
            language.choose("新会话", "New chat").to_owned()
        } else {
            clip_text(&session_title, 12)
        };
        let trigger_tooltip = if session_title.is_empty() {
            language.choose("新会话 · 点击切换会话", "New chat · click to switch").to_owned()
        } else {
            session_title.clone()
        };
        let picker_entity = entity.clone();
        let session_picker = gpui_component::popover::Popover::new("assistant-session-selector")
            // The header sits at the panel's top on every dock, so the popup
            // always opens into the panel, below the trigger.
            .anchor(gpui::Anchor::TopLeft)
            .open(self.assistant.session_picker_open)
            .on_open_change({
                let entity = entity.clone();
                move |open, _, cx| {
                    entity.update(cx, |view, cx| {
                        view.assistant.session_picker_open = *open;
                        cx.notify();
                    });
                }
            })
            .trigger(
                Button::new("assistant-session-selector-trigger")
                    .debug_selector(|| "assistant-session-selector-trigger".into())
                    .ghost()
                    .xsmall()
                    .label(trigger_label)
                    .dropdown_caret(true)
                    .tooltip(trigger_tooltip),
            )
            .content(move |_, _, cx| {
                picker_entity.read(cx).render_assistant_session_panel(&picker_entity)
            });

        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2p5()
            .px_4()
            .py_3()
            // Opaque chrome: in a short top/bottom dock the body must never show
            // through. Rounded to the panel, as GPUI does not clip children.
            .rounded_t_xl()
            .bg(rgb(CARD_BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .size(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(accent_gradient(135.))
                    .shadow(accent_glow(0.4, 12.))
                    .text_color(rgb(SIDEBAR_BG))
                    .child(Icon::new(IconName::Bot).small()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .v_flex()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .min_w(px(0.))
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(session_picker),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(status_dot(if busy { 0x00f5_9e0b } else { 0x0022_c55e }))
                            .child(div().truncate().child(language.choose_owned(
                                format!("正在查看：{screen_label}"),
                                format!("Viewing: {screen_label}"),
                            ))),
                    ),
            )
            .child(dock_switcher)
            .child(
                action_button("assistant-clear")
                    .ghost()
                    .icon(IconName::Delete)
                    .disabled(busy || messages.is_empty())
                    .tooltip(language.choose("清空对话", "Clear conversation"))
                    .on_click({
                        let entity = entity.clone();
                        move |_, _, cx| entity.update(cx, Self::clear_assistant)
                    }),
            )
            .child(
                action_button("assistant-close")
                    .ghost()
                    .icon(IconName::Close)
                    .tooltip(language.choose_owned(
                        format!("关闭（{ASSISTANT_SHORTCUT_LABEL}）"),
                        format!("Close ({ASSISTANT_SHORTCUT_LABEL})"),
                    ))
                    .on_click({
                        let entity = entity.clone();
                        move |_, window, cx| {
                            entity.update(cx, |view, cx| view.toggle_assistant(window, cx));
                        }
                    }),
            );

        let body = if messages.is_empty() {
            let suggestions: [(&str, &str); 4] = [
                ("这个页面能做什么？", "What can I do on this page?"),
                ("帮我新建一个项目", "Help me create a new project"),
                ("检查默认运行时是否可用", "Check whether the default runtime works"),
                ("切换到浅色主题", "Switch to the light theme"),
            ];
            let mut chips = div().v_flex().gap_2().w_full();
            for (index, (zh, en)) in suggestions.into_iter().enumerate() {
                let entity = entity.clone();
                let text = language.choose(zh, en);
                chips = chips.child(
                    div()
                        .id(("assistant-suggestion", index))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .text_sm()
                        .text_color(rgb(TEXT_SECONDARY))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb_alpha(ACCENT, 0.6)))
                        .on_click(move |_, window, cx| {
                            entity.update(cx, |view, cx| {
                                view.assistant
                                    .input
                                    .update(cx, |state, cx| state.set_value(text, window, cx));
                                view.send_assistant_message(window, cx);
                            });
                        })
                        .child(text),
                );
            }
            div()
                .id("assistant-empty-state")
                .debug_selector(|| "assistant-empty-state".into())
                .flex_1()
                .min_h(px(0.))
                .v_flex()
                .justify_center()
                .gap_4()
                .p_5()
                .scroll_y()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("有什么可以帮你？", "How can I help?")),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(
                            language.choose(
                                "我能看到当前界面的状态与输入框，可以帮你切换页面、填写表单、触发操作——每一步都需要你确认后才会执行。",
                                "I can see the current screen's state and inputs, and can navigate, fill forms and trigger actions — every step runs only after you confirm.",
                            ),
                        )),
                )
                .child(chips)
                .into_any_element()
        } else {
            let render_messages = messages.clone();
            let item_entity = entity.clone();
            let text_focus = self.assistant.text_focus.clone();
            div()
                .flex_1()
                .min_h(px(0.))
                .relative()
                .overflow_hidden()
                .py_2()
                // Clicking or drag-selecting a message moves focus off the composer;
                // otherwise the composer's own Copy handler swallows Ctrl+C and the
                // selected message text never reaches the clipboard.
                .track_focus(&text_focus)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.focus(&text_focus, cx);
                })
                // Selectable text repaints only when a frame is drawn. Without
                // this the drag highlight appears only after the next unrelated
                // repaint — for example once the whole window is switched away.
                .on_mouse_move(|event, window, _| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        window.refresh();
                    }
                })
                .on_mouse_up(MouseButton::Left, |_, window, _| {
                    window.refresh();
                })
                .child(
                    gpui::list(list.clone(), move |ix, _, _| {
                        assistant_message_element(&render_messages[ix], ix, language, &item_entity)
                    })
                    .size_full(),
                )
                .into_any_element()
        };

        // Below the input on a side dock; to its right on a top/bottom dock,
        // where the panel is wide and short.
        let selector = div().flex().min_w(px(0.)).child(self.render_assistant_model_selector(
            super::assistant_selector::selector_anchor(
                dock == AssistantDock::Left,
                dock == AssistantDock::Top,
            ),
            cx,
        ));
        let composer = div()
            .debug_selector(|| "assistant-composer".into())
            .flex()
            .items_end()
            .gap_2()
            .pl_3()
            .pr_1()
            .py_1p5()
            .min_h(px(44.))
            .rounded_xl()
            .border_1()
            .border_color(rgb_alpha(ACCENT, 0.45))
            .bg(rgb(SURFACE_BG))
            .child(
                InputBase::new("assistant-input")
                    .flex_1()
                    .min_w(px(0.))
                    .py_1p5()
                    .text_sm()
                    .child(self.assistant.input.clone()),
            )
            // `action_button` pins itself to the start of the cross axis;
            // keep it on the last line as the input grows.
            .child(if busy {
                action_button("assistant-stop")
                    .self_end()
                    .icon(IconName::Square)
                    .tooltip(language.choose("停止", "Stop"))
                    .on_click({
                        let entity = entity.clone();
                        move |_, _, cx| entity.update(cx, Self::stop_assistant)
                    })
            } else {
                action_button("assistant-send")
                    .self_end()
                    .primary()
                    .icon(IconName::ArrowUp)
                    .disabled(empty)
                    .tooltip(language.choose("发送（Enter）", "Send (Enter)"))
                    .on_click({
                        let entity = entity.clone();
                        move |_, window, cx| {
                            entity.update(cx, |view, cx| {
                                view.send_assistant_message(window, cx);
                            });
                        }
                    })
            });
        let footer = div()
            .flex_none()
            .v_flex()
            .gap_2()
            .p_3()
            .rounded_b_xl()
            .bg(rgb(CARD_BG))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(Icon::new(IconName::Eye).xsmall())
                    .child(div().truncate().child(language.choose_owned(
                        match (collecting, collecting_unavailable) {
                            (true, _) => format!(
                                "上下文：{screen_label} · 界面快照随消息发送 · 可联网收集资料到 assistant-materials"
                            ),
                            (_, true) => format!(
                                "上下文：{screen_label} · 当前运行时不支持联网收集（Codex / Claude Code 支持）"
                            ),
                            _ => format!("上下文：{screen_label} · 界面快照随消息发送"),
                        },
                        match (collecting, collecting_unavailable) {
                            (true, _) => format!(
                                "Context: {screen_label} · snapshot per message · web collection into assistant-materials"
                            ),
                            (_, true) => format!(
                                "Context: {screen_label} · this runtime cannot collect from the web (Codex / Claude Code can)"
                            ),
                            _ => format!(
                                "Context: {screen_label} · screen snapshot is sent with each message"
                            ),
                        },
                    ))),
            )
            .map(|footer| {
                if dock.is_horizontal_edge() {
                    footer.child(
                        div()
                            .flex()
                            .items_end()
                            .gap_2()
                            .child(composer.flex_1().min_w(px(0.)))
                            .child(selector.flex_none().max_w(px(360.)).pb_1p5()),
                    )
                } else {
                    footer.child(composer).child(selector)
                }
            });

        let top = px(TOP_BAR_HEIGHT + ASSISTANT_INSET);
        let bottom = px(STATUS_BAR_HEIGHT + ASSISTANT_INSET);
        let side = px(ASSISTANT_INSET);
        let (width, height) = (px(self.assistant.width), px(self.assistant.height));
        // A transparent strip on the inner edge; only its inset accent line shows
        // on hover, so it never overhangs the rounded corners.
        let horizontal = dock.is_horizontal_edge();
        let resize_handle = div()
            .id("assistant-resize-handle")
            .debug_selector(|| "assistant-resize-handle".into())
            .absolute()
            .flex()
            .items_center()
            .justify_center()
            .map(|this| match dock {
                AssistantDock::Right => this.left_0().top(px(16.)).bottom(px(16.)).w(px(6.)),
                AssistantDock::Left => this.right_0().top(px(16.)).bottom(px(16.)).w(px(6.)),
                AssistantDock::Top => this.bottom_0().left(px(16.)).right(px(16.)).h(px(6.)),
                AssistantDock::Bottom => this.top_0().left(px(16.)).right(px(16.)).h(px(6.)),
            })
            .map(
                |this| if horizontal { this.cursor_row_resize() } else { this.cursor_col_resize() },
            )
            .group("assistant-resize")
            .child(
                div()
                    .rounded_full()
                    .map(
                        |this| {
                            if horizontal {
                                this.w_full().h(px(2.))
                            } else {
                                this.h_full().w(px(2.))
                            }
                        },
                    )
                    .group_hover("assistant-resize", |this| this.bg(rgb_alpha(ACCENT, 0.8))),
            )
            .on_click(cx.listener(|view, event: &ClickEvent, _, cx| {
                // Double-click restores the default size.
                if event.click_count() >= 2 {
                    view.assistant.width = ASSISTANT_DEFAULT_WIDTH;
                    view.assistant.height = ASSISTANT_DEFAULT_HEIGHT;
                    cx.notify();
                }
            }))
            .on_drag(DraggedAssistantEdge, |_, _, _, cx| cx.new(|_| gpui::Empty));

        div()
            .id("assistant-panel")
            .debug_selector(|| "assistant-panel".into())
            .absolute()
            .map(|this| match dock {
                AssistantDock::Right => this.top(top).bottom(bottom).right(side).w(width),
                AssistantDock::Left => this.top(top).bottom(bottom).left(side).w(width),
                AssistantDock::Top => this.top(top).left(side).right(side).h(height),
                AssistantDock::Bottom => this.bottom(bottom).left(side).right(side).h(height),
            })
            .on_drag_move::<DraggedAssistantEdge>(cx.listener(
                |view, event: &DragMoveEvent<DraggedAssistantEdge>, window, cx| {
                    view.resize_assistant(event, window, cx);
                },
            ))
            .v_flex()
            .overflow_hidden()
            .rounded_xl()
            .border_1()
            .border_color(rgb_alpha(ACCENT, 0.35))
            .bg(rgb(CARD_BG))
            .shadow(accent_glow(0.22, 36.))
            .occlude()
            .child(header)
            .child(body)
            .child(footer)
            // Accent line inset past the corner radius: GPUI does not clip children
            // to a rounded parent, so a full-width strip would overhang the corners.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(20.))
                    .right(px(20.))
                    .h(px(2.))
                    .rounded_full()
                    .bg(accent_gradient(90.))
                    .shadow(accent_glow(0.6, 8.)),
            )
            .child(resize_handle)
    }

    /// The session picker popup: every conversation, most recently used first,
    /// each with its topic, plus new and delete.
    fn render_assistant_session_panel(&self, entity: &Entity<Self>) -> Div {
        let language = self.language;
        let active_id = self.assistant.sessions.active_id.clone();
        let streaming_id = self.assistant.run.as_ref().map(|run| run.session_id.clone());
        let mut list = div()
            .id("assistant-session-list")
            .max_h(px(300.))
            .overflow_y_scroll()
            .v_flex()
            .gap_0p5();
        for (index, session) in self.assistant.sessions.sessions.iter().enumerate() {
            let active = session.id == active_id;
            let streaming = streaming_id.as_deref() == Some(session.id.as_str());
            list = list
                .child(assistant_session_row(language, entity, index, session, active, streaming));
        }
        div()
            .debug_selector(|| "assistant-session-panel".into())
            .w(px(280.))
            .v_flex()
            .gap_2()
            .child(
                action_button("assistant-session-new")
                    .primary()
                    .compact()
                    .icon(IconName::Plus)
                    .label(language.choose("新建会话", "New conversation"))
                    .on_click({
                        let entity = entity.clone();
                        move |_, window, cx| {
                            entity.update(cx, |view, cx| {
                                view.new_assistant_session(window, cx);
                            });
                        }
                    }),
            )
            .child(list)
    }
}

/// One conversation row of the session picker: topic, message count, and a
/// delete action — hidden while the session streams a reply.
fn assistant_session_row(
    language: UiLanguage,
    entity: &Entity<ControlPlaneView>,
    index: usize,
    session: &AssistantSession,
    active: bool,
    streaming: bool,
) -> gpui::Stateful<Div> {
    let switcher = entity.clone();
    let id = session.id.clone();
    let title = if session.title.is_empty() {
        language.choose("新会话", "New chat").to_owned()
    } else {
        session.title.clone()
    };
    let meta = if streaming {
        language.choose("正在回复…", "Replying…").to_owned()
    } else if session.messages.is_empty() {
        language.choose("空会话", "Empty").to_owned()
    } else {
        language.choose_owned(
            format!("{} 条消息", session.messages.len()),
            format!("{} messages", session.messages.len()),
        )
    };
    let row_selector = format!("assistant-session-item-{index}");
    let mut row = div()
        .id(gpui::ElementId::Name(row_selector.clone().into()))
        .debug_selector(move || row_selector.clone())
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .when(active, |this| this.bg(rgb(0x00f0_f9ff)).text_color(rgb(ACCENT)))
        .hover(|this| this.bg(rgb(SURFACE_BG)))
        .on_click(move |_, window, cx| {
            switcher.update(cx, |view, cx| view.switch_assistant_session(&id, window, cx));
        })
        .child(
            div()
                .flex_none()
                .text_color(if active { rgb(ACCENT) } else { rgb(TEXT_MUTED) })
                // The conversations icon is embedded with the app assets, not
                // the component bundle.
                .child(Icon::default().path("icons/messages-square.svg").xsmall()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .v_flex()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(if active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                        .child(title),
                )
                .child(div().truncate().text_xs().text_color(rgb(TEXT_MUTED)).child(meta)),
        );
    if !streaming {
        let entity = entity.clone();
        let delete_id = session.id.clone();
        row = row.child(
            action_button(format!("assistant-session-delete-{index}"))
                .ghost()
                .compact()
                .icon(IconName::Delete)
                .tooltip(language.choose("删除会话", "Delete conversation"))
                .on_click(move |_, _, cx| {
                    // Without this the row's switch handler fires too.
                    cx.stop_propagation();
                    entity.update(cx, |view, cx| {
                        view.delete_assistant_session(&delete_id, cx);
                    });
                }),
        );
    }
    row
}

#[allow(clippy::too_many_lines)]
fn assistant_message_element(
    message: &AssistantMessage,
    message_index: usize,
    language: UiLanguage,
    entity: &Entity<ControlPlaneView>,
) -> AnyElement {
    let user = message.role == Role::User;
    let mut bubble = div()
        .debug_selector(move || format!("assistant-message-bubble-{message_index}"))
        .max_w(gpui::relative(0.88))
        .v_flex()
        .gap_1p5()
        .px_3()
        .py_2()
        .rounded_xl()
        .text_sm()
        .whitespace_normal();
    bubble = if user {
        bubble
            .bg(gpui::linear_gradient(
                135.,
                gpui::linear_color_stop(rgb_alpha(ACCENT, 0.22), 0.),
                gpui::linear_color_stop(rgb_alpha(ACCENT_ALT, 0.18), 1.),
            ))
            .border_1()
            .border_color(rgb_alpha(ACCENT, 0.35))
    } else {
        bubble.bg(rgb(SURFACE_BG)).border_1().border_color(rgb(BORDER))
    };
    if !message.reasoning.is_empty() {
        let collapsed = message.reasoning_collapsed;
        let toggler = entity.clone();
        let mut block = div().w_full().v_flex().gap_1().child(
            div()
                .id(("assistant-reasoning-toggle", message_index))
                .debug_selector(move || format!("assistant-reasoning-toggle-{message_index}"))
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(TEXT_MUTED))
                .cursor_pointer()
                .child(
                    Icon::new(if collapsed {
                        IconName::ChevronRight
                    } else {
                        IconName::ChevronDown
                    })
                    .xsmall(),
                )
                .child(language.choose("思考过程", "Thinking"))
                .on_click(move |_, _, cx| {
                    toggler.update(cx, |view, cx| {
                        view.toggle_assistant_reasoning(message_index, cx);
                    });
                }),
        );
        if !collapsed {
            block = block.child(
                div()
                    .id(("assistant-reasoning-scroll", message_index))
                    .debug_selector(move || format!("assistant-reasoning-{message_index}"))
                    .w_full()
                    .max_h(px(112.))
                    .scroll_y()
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(gpui_base::SelectableText::new(
                        ("assistant-reasoning-text", message_index),
                        message.reasoning.clone(),
                    )),
            );
        }
        bubble = bubble.child(block);
    }
    if message.pending && message.text.is_empty() {
        bubble = bubble.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(rgb(TEXT_MUTED))
                .child(Icon::new(IconName::LoaderCircle).small())
                .child(language.choose("正在分析当前界面…", "Analyzing the current screen…")),
        );
    } else if !message.text.is_empty() {
        bubble = bubble.child(div().cursor(gpui::CursorStyle::IBeam).child(
            gpui_base::SelectableText::new(
                ("assistant-message", message_index),
                message.text.clone(),
            ),
        ));
    }
    if let Some(error) = &message.error {
        bubble = bubble.child(
            div().text_xs().text_color(rgb(0x00dc_2626)).whitespace_normal().child(error.clone()),
        );
    }
    // A guaranteed copy path that never depends on focus or selection state.
    if !message.pending && !message.text.is_empty() {
        let copier = entity.clone();
        let copied_text = message.text.clone();
        bubble = bubble.child(
            div().flex().justify_end().pt_0p5().child(
                action_button(("assistant-copy", message_index))
                    .ghost()
                    .compact()
                    .icon(IconName::Copy)
                    .tooltip(language.choose("复制本条消息", "Copy this message"))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(copied_text.clone()));
                        copier.update(cx, |view, cx| {
                            view.status = view.language.choose_owned(
                                "已复制该消息".to_owned(),
                                "Message copied".to_owned(),
                            );
                            cx.notify();
                        });
                    }),
            ),
        );
    }
    let pending_count =
        message.actions.iter().filter(|a| a.status == ActionStatus::Pending).count();
    if !message.actions.is_empty() {
        let mut cards = div().v_flex().gap_1p5().pt_1();
        cards = cards.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(ACCENT_SOFT))
                        .child(language.choose("待确认的操作", "Proposed actions")),
                )
                .when(pending_count > 1, |row| {
                    let entity = entity.clone();
                    row.child(
                        action_button(("assistant-run-all", message_index))
                            .primary()
                            .compact()
                            .label(language.choose("全部执行", "Run all"))
                            .on_click(move |_, window, cx| {
                                entity.update(cx, |view, cx| {
                                    view.run_all_assistant_actions(message_index, window, cx);
                                });
                            }),
                    )
                }),
        );
        for (index, proposed) in message.actions.iter().enumerate() {
            let (badge, color) = match &proposed.status {
                ActionStatus::Pending => (String::new(), ACCENT),
                ActionStatus::Running(text) => (text.clone(), ACCENT),
                ActionStatus::Done(text) => (text.clone(), 0x0016_a34a),
                ActionStatus::Failed(error) => (error.clone(), 0x00dc_2626),
                ActionStatus::Skipped => {
                    (language.choose("已忽略", "Skipped").to_owned(), 0x0094_a3b8)
                }
            };
            let pending = proposed.status == ActionStatus::Pending;
            let runner = entity.clone();
            let skipper = entity.clone();
            cards = cards.child(
                div()
                    .v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(if pending { rgb_alpha(ACCENT, 0.4) } else { rgb(BORDER).into() })
                    .bg(rgb(CARD_BG))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(div().pt(px(5.)).child(status_dot(color)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_xs()
                                    .whitespace_normal()
                                    .child(format!("{}. {}", index + 1, proposed.description)),
                            ),
                    )
                    .when(!badge.is_empty(), |card| {
                        card.child(
                            div()
                                .pl_4()
                                .text_xs()
                                .text_color(rgb(color))
                                .whitespace_normal()
                                .child(badge),
                        )
                    })
                    .when(pending, |card| {
                        card.child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_1()
                                .child(
                                    action_button(("assistant-skip", message_index * 100 + index))
                                        .ghost()
                                        .compact()
                                        .label(language.choose("忽略", "Skip"))
                                        .on_click(move |_, _, cx| {
                                            skipper.update(cx, |view, cx| {
                                                view.skip_assistant_action(
                                                    message_index,
                                                    index,
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                                .child(
                                    action_button(("assistant-run", message_index * 100 + index))
                                        .primary()
                                        .compact()
                                        .icon(IconName::Check)
                                        .label(language.choose("执行", "Run"))
                                        .on_click(move |_, window, cx| {
                                            runner.update(cx, |view, cx| {
                                                view.run_assistant_action(
                                                    message_index,
                                                    index,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }),
                                ),
                        )
                    }),
            );
        }
        bubble = bubble.child(cards);
    }
    div()
        .w_full()
        .px_4()
        .py_1p5()
        .flex()
        .flex_col()
        .map(|this| if user { this.items_end() } else { this.items_start() })
        .when(user && !message.context.is_empty(), |this| {
            this.child(
                div()
                    .pb_0p5()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(format!("@ {}", message.context)),
            )
        })
        .child(bubble)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::visible_stream_text;

    #[test]
    fn streaming_text_hides_an_unfinished_action_block() {
        assert_eq!(visible_stream_text("Hello"), "Hello");
        assert_eq!(visible_stream_text("Sure.\n```circuitfabric-ui-actions\n[{\"act"), "Sure.\n");
    }
}
