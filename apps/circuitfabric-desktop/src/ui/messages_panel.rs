//! Message-center presentation: the top-bar bell entry, its dropdown panel,
//! deep links into source pages, and the bridge-transition hooks that feed
//! the read model from UI-layer state changes.
use super::*;
use crate::application::messages::{
    BridgeEvent, DesktopMessage, MessageCenter, MessageSource, MessageTarget,
};

/// The 🔔 entry in the top-bar right cluster, with an unread badge.
pub(super) fn render_message_center_trigger(
    view: &ControlPlaneView,
    cx: &Context<ControlPlaneView>,
) -> impl IntoElement {
    let entity = cx.entity().clone();
    let language = view.language;
    let unread = view.message_center.unread_count();
    let label = language.choose("消息中心", "Message center");
    let badge = (unread > 0)
        .then(|| {
            let text = if unread > 99 { "99+".to_owned() } else { unread.to_string() };
            div()
                .id("message-center-unread")
                .debug_selector(move || "message-center-unread".into())
                .absolute()
                .top(px(-5.))
                .right(px(-6.))
                .min_w(px(16.))
                .h(px(16.))
                .px(px(3.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(rgb(0x00b4_5309))
                .text_size(px(9.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0x00ff_fbeb))
                .child(text)
        })
        .unwrap_or_else(|| div().id("message-center-unread-empty").hidden());
    div()
        .id("message-center-trigger")
        .debug_selector(move || "message-center-trigger".into())
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(30.))
        .rounded_full()
        .border_1()
        .border_color(rgb(if view.message_panel_open { ACCENT } else { BORDER }))
        .bg(rgb(SURFACE_BG))
        .cursor_pointer()
        .hover(|this| this.border_color(rgb_alpha(ACCENT, 0.6)))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(label).build(window, cx)
        })
        .on_click({
            let opener = entity.clone();
            move |_, _, cx| {
                opener.update(cx, |view, cx| {
                    view.message_panel_open = !view.message_panel_open;
                    cx.notify();
                });
            }
        })
        .child(
            Icon::default()
                .path("icons/bell.svg")
                .with_size(px(15.))
                .text_color(rgb(if view.message_panel_open { ACCENT_SOFT } else { TEXT_MUTED })),
        )
        .child(deferred(badge).with_priority(1))
}

/// One renderable row: a persisted event or a live derived entry.
struct MessageRow<'a> {
    id: &'a str,
    title: &'a str,
    body: &'a str,
    at_unix_seconds: u64,
    unread: bool,
    target: &'a MessageTarget,
}

fn selector_suffix(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

impl ControlPlaneView {
    pub(super) fn close_message_panel(&mut self, cx: &mut Context<Self>) {
        if self.message_panel_open {
            self.message_panel_open = false;
            cx.notify();
        }
    }

    /// Rate-limited scan of opened projects, called from the render loop.
    /// Persistence follows every mutation so a restart keeps read state and
    /// never replays ingested events.
    pub(super) fn refresh_message_center(&mut self) {
        if self.message_center.scan(&self.project_storages, &self.project_data)
            && let Err(error) = self.message_center.save(&self.message_store_path)
        {
            self.status = format!("消息存储未保存：{error}");
        }
    }

    /// Record a bridge transition observed by the UI layer.
    pub(super) fn note_bridge_event(&mut self, event: BridgeEvent) {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if self.message_center.push_bridge_event(&event, at)
            && let Err(error) = self.message_center.save(&self.message_store_path)
        {
            self.status = format!("消息存储未保存：{error}");
        }
    }

    /// Assign bridge health, messaging only real state changes. `Unknown` is
    /// the pre-probe baseline: a first probe result is not an event (a fresh
    /// app start with no bridge would otherwise nag "unreachable" every
    /// time), but a listening bridge going offline — or coming back — is.
    pub(super) fn set_bridge_health_noted(&mut self, health: BridgeHealth) {
        let known_before =
            !matches!(self.bridge_health, BridgeHealth::Unknown);
        let same = matches!(
            (&self.bridge_health, &health),
            (BridgeHealth::Unknown, BridgeHealth::Unknown)
                | (BridgeHealth::Listening { .. }, BridgeHealth::Listening { .. })
                | (BridgeHealth::Unreachable { .. }, BridgeHealth::Unreachable { .. })
        );
        self.bridge_health = health;
        if !same && known_before {
            match &self.bridge_health {
                BridgeHealth::Listening { .. } => {
                    self.note_bridge_event(BridgeEvent::Reachable);
                }
                BridgeHealth::Unreachable { reason } => {
                    self.note_bridge_event(BridgeEvent::Unreachable { reason: reason.clone() });
                }
                BridgeHealth::Unknown => (),
            }
        }
    }

    /// Assign the bridge lifecycle status, messaging stable states (starting
    /// is transient and stays silent).
    pub(super) fn set_bridge_status_noted(&mut self, status: RuntimeLifecycleStatus) {
        let same = matches!(
            (&self.bridge_status, &status),
            (RuntimeLifecycleStatus::Starting, RuntimeLifecycleStatus::Starting)
                | (RuntimeLifecycleStatus::Running { .. }, RuntimeLifecycleStatus::Running { .. })
                | (RuntimeLifecycleStatus::Stopped, RuntimeLifecycleStatus::Stopped)
        ) || matches!(
            (&self.bridge_status, &status),
            (RuntimeLifecycleStatus::Failed { reason: old }, RuntimeLifecycleStatus::Failed { reason: new })
                if old == new
        );
        self.bridge_status = status;
        if !same {
            match &self.bridge_status {
                RuntimeLifecycleStatus::Starting => (),
                RuntimeLifecycleStatus::Running { pid } => {
                    self.note_bridge_event(BridgeEvent::Running { pid: *pid });
                }
                RuntimeLifecycleStatus::Stopped => {
                    self.note_bridge_event(BridgeEvent::Stopped);
                }
                RuntimeLifecycleStatus::Failed { reason } => {
                    self.note_bridge_event(BridgeEvent::Failed { reason: reason.clone() });
                }
            }
        }
    }

    /// Deep-link one row's target, reusing the overview todo panel's
    /// navigation pattern: select the project first, then the screen and
    /// the preselected object.
    pub(super) fn open_message_target(&mut self, target: &MessageTarget, cx: &mut Context<Self>) {
        let project_open =
            |view: &Self, id: &str| view.project_storages.contains_key(id);
        match target {
            MessageTarget::ChangeSet { project_id, change_set_id } => {
                if project_open(self, project_id) {
                    self.select_project(project_id.clone(), cx);
                    self.navigation.screen = ControlPlaneScreen::ChangesAndApprovals;
                    self.selected_change_set = Some((project_id.clone(), change_set_id.clone()));
                } else {
                    self.navigation.screen = ControlPlaneScreen::Overview;
                }
            }
            MessageTarget::Observation { project_id, snapshot_hash } => {
                if project_open(self, project_id) {
                    self.select_project(project_id.clone(), cx);
                    self.navigation.screen = ControlPlaneScreen::Semantics;
                    self.selected_semantic_snapshot =
                        Some((project_id.clone(), snapshot_hash.clone()));
                } else {
                    self.navigation.screen = ControlPlaneScreen::Overview;
                }
            }
            MessageTarget::EdaServices => {
                self.navigation.screen = ControlPlaneScreen::EdaServices;
            }
            MessageTarget::Sessions { project_id } => {
                self.navigation.screen = ControlPlaneScreen::SessionsAndTasks;
                self.session_project_filter = Some(project_id.clone());
            }
        }
    }

    fn message_rows<'a>(
        &'a self,
        center: &'a MessageCenter,
        source: MessageSource,
    ) -> Vec<MessageRow<'a>> {
        let zh = self.language == UiLanguage::SimplifiedChinese;
        let mut rows: Vec<MessageRow<'a>> = center
            .messages()
            .iter()
            .filter(|message| message.source == source)
            .map(|message: &DesktopMessage| MessageRow {
                id: &message.id,
                title: if zh { &message.title_zh } else { &message.title_en },
                body: if zh { &message.body_zh } else { &message.body_en },
                at_unix_seconds: message.at_unix_seconds,
                unread: !message.read,
                target: &message.target,
            })
            .collect();
        rows.extend(
            center
                .derived()
                .iter()
                .filter(|entry| entry.source == source)
                .map(|entry| MessageRow {
                    id: entry.id(),
                    title: if zh { &entry.title_zh } else { &entry.title_en },
                    body: if zh { &entry.body_zh } else { &entry.body_en },
                    at_unix_seconds: entry.at_unix_seconds,
                    unread: !center.is_dismissed(entry.id()),
                    target: &entry.target,
                }),
        );
        rows.sort_by_key(|row| std::cmp::Reverse(row.at_unix_seconds));
        rows
    }

    /// The anchored dropdown: groups in a fixed order, per-row deep links,
    /// mark-all-read and clear actions.
    pub(super) fn render_message_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let zone = self.time_zone;
        let unread = self.message_center.unread_count();
        let total = self.message_center.messages().len() + self.message_center.derived().len();

        let mut body = capped_scroll_body("message-panel-body", px(560.)).px_3().pt_3().pb_6();
        let mut any_rows = false;
        for source in MessageSource::ALL {
            let rows = self.message_rows(&self.message_center, source);
            if rows.is_empty() {
                continue;
            }
            any_rows = true;
            let (label_zh, label_en) = source.label();
            let group_unread = rows.iter().filter(|row| row.unread).count();
            let group_selector =
                format!("message-group-{}", selector_suffix(&format!("{source:?}")));
            let mut group = div()
                .v_flex()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_MUTED))
                                .child(language.choose(label_zh, label_en)),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                            "{} / {}",
                            group_unread,
                            rows.len()
                        ))),
                );
            for row in rows {
                let row_id = row.id.to_owned();
                let row_target = row.target.clone();
                let title = row.title.to_owned();
                let body_text = row.body.to_owned();
                let at = row.at_unix_seconds;
                let unread_row = row.unread;
                let opener = entity.clone();
                let selector = format!("message-entry-{}", selector_suffix(&row_id));
                let entry = div()
                    .id(selector_suffix(&row_id))
                    .debug_selector(move || selector.clone())
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if unread_row { ACCENT_SOFT } else { BORDER }))
                    .bg(rgb(if unread_row { 0x00ff_fbeb } else { CARD_BG }))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(ACCENT)))
                    .on_click(move |_, _, cx| {
                        opener.update(cx, |view, cx| {
                            view.message_center.mark_read(&row_id);
                            if let Err(error) =
                                view.message_center.save(&view.message_store_path)
                            {
                                view.status = format!("消息存储未保存：{error}");
                            }
                            view.open_message_target(&row_target, cx);
                            view.message_panel_open = false;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(if unread_row {
                                status_dot(0x00b4_5309)
                            } else {
                                status_dot(0x0094_a3b8)
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_sm()
                                    .font_weight(if unread_row {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .child(title),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(time_zone::format_timestamp(at, zone)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .whitespace_normal()
                            .child(body_text),
                    );
                group = group.child(entry);
            }
            body = body.child(
                div()
                    .debug_selector(move || group_selector.clone())
                    .v_flex()
                    .gap_2()
                    .child(group),
            );
        }
        if !any_rows {
            body = body.child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose(
                        "暂无消息；待审批、观测诊断、bridge 状态与物化阶段变化会出现在这里。",
                        "No messages yet; pending approvals, observation diagnostics, bridge state, and materialization changes will appear here.",
                    )),
            );
        }

        let closer = entity.clone();
        let marker = entity.clone();
        let clearer = entity.clone();
        // Same layering as the command palette: a full-area backdrop plus a
        // flow-positioned panel inside a flex overlay, so the panel stacks
        // above the backdrop for both painting and hit-testing.
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_end()
            .pt(px(56.))
            .pr_4()
            .child(
                div()
                    .id("message-panel-backdrop")
                    .debug_selector(move || "message-panel-backdrop".into())
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_click(move |_, _, cx| {
                        closer.update(cx, ControlPlaneView::close_message_panel);
                    }),
            )
            .child(
                div()
                    .id("message-panel")
                    .debug_selector(move || "message-panel".into())
                    .relative()
                    .w(px(460.))
                    .max_h(px(640.))
                    .v_flex()
                    .overflow_hidden()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb_alpha(ACCENT, 0.35))
                    .bg(rgb(CARD_BG))
                    .shadow(accent_glow(0.25, 40.))
                    .occlude()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_6()
                            .h(px(48.))
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language.choose("消息中心", "Message center")),
                            )
                            .child(
                                div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                    "{} {} · {} {}",
                                    unread,
                                    language.choose("条未读", "unread"),
                                    total,
                                    language.choose("条", "total"),
                                )),
                            )
                            .child(div().flex_1())
                            .child(
                                action_button("messages-mark-all-read")
                                    .ghost()
                                    .xsmall()
                                    .label(language.choose("全部已读", "Mark all read"))
                                    .on_click(move |_, _, cx| {
                                        marker.update(cx, |view, cx| {
                                            view.message_center.mark_all_read();
                                            if let Err(error) =
                                                view.message_center.save(&view.message_store_path)
                                            {
                                                view.status = format!("消息存储未保存：{error}");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                action_button("messages-clear")
                                    .ghost()
                                    .xsmall()
                                    .label(language.choose("清空", "Clear"))
                                    .on_click(move |_, _, cx| {
                                        clearer.update(cx, |view, cx| {
                                            view.message_center.clear();
                                            if let Err(error) =
                                                view.message_center.save(&view.message_store_path)
                                            {
                                                view.status = format!("消息存储未保存：{error}");
                                            }
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(body),
            )
    }
}
