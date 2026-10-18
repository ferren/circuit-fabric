//! The in-app assistant: callable from every screen, reads a structured snapshot of
//! the visible screen, and proposes UI actions that only run after confirmation.
use super::*;
use crate::application::assistant::{
    ACTION_FENCE, ActionStatus, AssistantAction, ChatMessage as AssistantMessage, ProposedAction,
    Role, ScreenSnapshot, SnapshotField, SnapshotItem, Turn, build_prompt, load_history,
    parse_reply, save_history,
};
use gpui::{App, Focusable as _};

pub(super) const ASSISTANT_SHORTCUT_LABEL: &str =
    if cfg!(target_os = "macos") { "⌘J" } else { "Ctrl J" };
const ASSISTANT_DEFAULT_WIDTH: f32 = 420.;
const ASSISTANT_DEFAULT_HEIGHT: f32 = 380.;
const ASSISTANT_MIN_WIDTH: f32 = 320.;
const ASSISTANT_MIN_HEIGHT: f32 = 260.;
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
    pub(super) cancel: circuitfabric_codex_runtime::execution::Cancellation,
    pub(super) stream: Arc<std::sync::Mutex<String>>,
}

pub(super) struct AssistantState {
    pub(super) open: bool,
    pub(super) input: Entity<InputState>,
    pub(super) messages: Vec<AssistantMessage>,
    pub(super) run: Option<AssistantRun>,
    pub(super) list: gpui::ListState,
    pub(super) dock: AssistantDock,
    /// Panel width when docked left/right, height when docked top/bottom.
    pub(super) width: f32,
    pub(super) height: f32,
    /// Where the conversation is kept between launches.
    pub(super) history_path: PathBuf,
}

impl AssistantState {
    /// Restores the saved conversation; returns a status note when it could not
    /// be read (the unreadable file is left untouched).
    pub(super) fn new(
        history_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<ControlPlaneView>,
    ) -> (Self, Option<String>) {
        let (messages, load_error) = match load_history(&history_path) {
            Ok(messages) => (messages, None),
            Err(error) => (Vec::new(), Some(format!("智能助手历史会话未恢复：{error}"))),
        };
        let input = ControlPlaneView::input(
            window,
            String::new(),
            "描述你想做的事，例如：帮我新建一个项目…",
            cx,
        );
        cx.subscribe_in(&input, window, |view, _, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.send_assistant_message(window, cx);
            }
        })
        .detach();
        let list = gpui::ListState::new(messages.len(), gpui::ListAlignment::Bottom, px(512.));
        list.set_follow_mode(gpui::FollowMode::Tail);
        let state = Self {
            open: false,
            input,
            messages,
            run: None,
            list,
            dock: AssistantDock::Right,
            width: ASSISTANT_DEFAULT_WIDTH,
            height: ASSISTANT_DEFAULT_HEIGHT,
            history_path,
        };
        (state, load_error)
    }
}

/// Text shown while a reply streams: the action block is withheld until it is
/// complete and validated.
fn visible_stream_text(stream: &str) -> &str {
    stream.find(ACTION_FENCE).map_or(stream, |index| &stream[..index])
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
                ("check_codex_runtime", l.choose("检查 Codex 连接", "Check Codex connection")),
                ("run_codex_task", l.choose("用 Codex 运行验证任务", "Run task with Codex")),
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
            "check_codex_runtime" => self.check_codex_runtime(window, cx),
            "run_codex_task" => self.run_agent_task(RuntimeAdapter::CodexAppServer, window, cx),
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
            format!("Codex runtime check: {}", self.codex_check_label(l)),
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
                let result = &self.quick_task(RuntimeAdapter::CodexAppServer).result;
                if !result.is_empty() {
                    facts.push(format!("Last Codex task result: {result}"));
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
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn send_assistant_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use circuitfabric_codex_runtime::execution::{AgentKind, Cancellation};
        let request = self.assistant.input.read(cx).value().trim().to_owned();
        if self.assistant.run.is_some() || request.is_empty() {
            return;
        }
        let Some(settings) = self.settings_for_execution(cx) else {
            return;
        };
        let language = self.language;
        let snapshot = self.assistant_snapshot(cx);
        let history: Vec<Turn> = self
            .assistant
            .messages
            .iter()
            .filter(|m| !m.pending && !m.text.is_empty())
            .map(|m| Turn { role: m.role, text: m.text.clone() })
            .collect();
        let prompt =
            build_prompt(&snapshot, &history, &request, language.choose("简体中文", "English"));
        self.assistant.messages.push(AssistantMessage {
            role: Role::User,
            text: request,
            context: snapshot.screen_label.clone(),
            pending: false,
            actions: Vec::new(),
            error: None,
        });
        self.assistant.messages.push(AssistantMessage {
            role: Role::Assistant,
            text: String::new(),
            context: String::new(),
            pending: true,
            actions: Vec::new(),
            error: None,
        });
        self.assistant.input.update(cx, |state, cx| state.set_value("", window, cx));
        let cancel = Cancellation::default();
        let stream = Arc::new(std::sync::Mutex::new(String::new()));
        self.assistant.run = Some(AssistantRun { cancel: cancel.clone(), stream: stream.clone() });
        let secrets = self.vault.as_ref().map(|vault| vault.values().clone());
        let writer = stream.clone();
        let work = cx.background_spawn(async move {
            circuitfabric_codex_runtime::execution::run_task_streaming(
                &settings,
                AgentKind::Codex,
                &ToolAuthorizationSettings::default(),
                &prompt,
                None,
                None,
                secrets.as_ref(),
                &cancel,
                &mut |kind, delta| {
                    if kind == circuitfabric_codex_runtime::TurnDelta::Answer
                        && let Ok(mut buffer) = writer.lock()
                    {
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
                            let text = run
                                .stream
                                .lock()
                                .map(|buffer| visible_stream_text(&buffer).to_owned())
                                .unwrap_or_default();
                            if let Some(last) = view.assistant.messages.last_mut()
                                && last.pending
                                && last.text != text
                            {
                                last.text = text;
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
        cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.assistant.run = None;
                    let chinese = view.language == UiLanguage::SimplifiedChinese;
                    if let Some(last) = view.assistant.messages.last_mut()
                        && last.pending
                    {
                        last.pending = false;
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

    /// Writes the finished conversation to disk after every change; a failure is
    /// surfaced in the status bar and the in-memory conversation is kept.
    fn persist_assistant(&mut self) {
        if let Err(error) = save_history(&self.assistant.history_path, &self.assistant.messages) {
            self.status = format!("智能助手会话未保存：{error}");
        }
    }

    fn stop_assistant(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.assistant.run {
            run.cancel.cancel();
        }
        cx.notify();
    }

    fn clear_assistant(&mut self, cx: &mut Context<Self>) {
        if self.assistant.run.is_none() {
            self.assistant.messages.clear();
            self.persist_assistant();
            cx.notify();
        }
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
            .messages
            .get(message)
            .and_then(|m| m.actions.get(index))
            .filter(|p| p.status == ActionStatus::Pending)
        else {
            return false;
        };
        let action = proposed.action.clone();
        let snapshot = self.assistant_snapshot(cx);
        let outcome = action.validate(&snapshot).map(|()| {
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
                        state.update(cx, |state, cx| state.set_value(value.clone(), window, cx));
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
                AssistantAction::ToggleLanguage => self.toggle_global_language(cx),
            }
            self.language.choose("已执行", "Done").to_owned()
        });
        let ok = outcome.is_ok();
        if let Some(proposed) =
            self.assistant.messages.get_mut(message).and_then(|m| m.actions.get_mut(index))
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
        let count = self.assistant.messages.get(message).map_or(0, |m| m.actions.len());
        for index in 0..count {
            let pending =
                self.assistant.messages[message].actions[index].status == ActionStatus::Pending;
            if pending && !self.run_assistant_action(message, index, window, cx) {
                break;
            }
        }
    }

    fn skip_assistant_action(&mut self, message: usize, index: usize, cx: &mut Context<Self>) {
        if let Some(proposed) =
            self.assistant.messages.get_mut(message).and_then(|m| m.actions.get_mut(index))
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
        let messages = std::rc::Rc::new(self.assistant.messages.clone());
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
                    .child(div().truncate().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                        language.choose("CircuitFabric 智能助手", "CircuitFabric Assistant"),
                    ))
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
                ("检查 Codex 运行时是否可用", "Check whether the Codex runtime works"),
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
            div()
                .flex_1()
                .min_h(px(0.))
                .relative()
                .overflow_hidden()
                .py_2()
                .child(
                    gpui::list(list.clone(), move |ix, _, _| {
                        assistant_message_element(&render_messages[ix], ix, language, &item_entity)
                    })
                    .size_full(),
                )
                .into_any_element()
        };

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
                    .child(language.choose_owned(
                        format!("上下文：{screen_label} · 界面快照随消息发送"),
                        format!(
                            "Context: {screen_label} · screen snapshot is sent with each message"
                        ),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl_3()
                    .pr_1()
                    .h(px(44.))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb_alpha(ACCENT, 0.45))
                    .bg(rgb(SURFACE_BG))
                    .child(
                        InputBase::new("assistant-input")
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .text_sm()
                            .child(self.assistant.input.clone()),
                    )
                    // `action_button` pins itself to the start of the cross axis;
                    // center it in the input row instead of hugging the top border.
                    .child(if busy {
                        action_button("assistant-stop")
                            .self_center()
                            .icon(IconName::Square)
                            .tooltip(language.choose("停止", "Stop"))
                            .on_click({
                                let entity = entity.clone();
                                move |_, _, cx| entity.update(cx, Self::stop_assistant)
                            })
                    } else {
                        action_button("assistant-send")
                            .self_center()
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
                    }),
            );

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
        bubble = bubble.child(gpui_base::SelectableText::new(
            ("assistant-message", message_index),
            message.text.clone(),
        ));
    }
    if let Some(error) = &message.error {
        bubble = bubble.child(
            div().text_xs().text_color(rgb(0x00dc_2626)).whitespace_normal().child(error.clone()),
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
