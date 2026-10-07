//! Changes presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn current_observation_hash(&self, project_id: &str) -> Option<String> {
        self.project_data.get(project_id)?.semantic_snapshots.iter().rev().find_map(|snapshot| {
            (snapshot.authority == SnapshotAuthority::Observed)
                .then(|| snapshot.snapshot_hash.clone())
        })
    }

    pub(super) fn record_change_set_decision(
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
            storage.record_change_set_decision(change_set_id, current_observation.as_deref(), entry)
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
    pub(super) fn render_changes_approvals_page(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.language;
        let entity = cx.entity().clone();
        let Some(project_id) = self.navigation.selected_project.clone() else {
            return project_empty_state(
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
            let is_selected = selected.as_ref().is_some_and(|selected| selected.id == record.id);
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
                    .child(div().mt_1().text_xs().text_color(rgb(TEXT_SECONDARY)).child(format!(
                        "{} → {}",
                        short_hash(&record.base_snapshot_hash),
                        short_hash(&record.target_snapshot_hash)
                    ))),
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
                    div().p_2().rounded_md().bg(rgb(SURFACE_BG)).text_sm().child(operation.clone()),
                );
            }
            if record.ir_diff.is_empty() {
                diff_rows = diff_rows.child(
                    div().text_sm().text_color(rgb(TEXT_MUTED)).child("No IR operations recorded."),
                );
            }
            let mut evidence_rows = div().v_flex().gap_1();
            for evidence in &record.evidence {
                evidence_rows = evidence_rows.child(
                    div().p_2().rounded_md().bg(rgb(SURFACE_BG)).text_xs().child(format!(
                        "{} @ {} ({})",
                        evidence.document_id,
                        evidence.locator,
                        short_hash(&evidence.content_hash)
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
                    .child(div().w(px(62.)).text_xs().text_color(rgb(TEXT_SECONDARY)).child(label))
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
            let audit = record.audit.iter().rev().fold(div().v_flex().gap_1(), |rows, entry| {
                rows.child(div().p_2().rounded_md().bg(rgb(SURFACE_BG)).text_xs().child(format!(
                            "{} · {} · {} · observed: {} · rollback: {}",
                            entry.timestamp_unix_seconds,
                            entry.actor,
                            entry.decision,
                            entry
                                .observed_snapshot_hash
                                .as_deref()
                                .map_or_else(|| "none".to_owned(), short_hash),
                            entry.rollback_handle.as_deref().unwrap_or("none")
                        )))
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
                            action_button("open-approval-drawer")
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
                                        .map_or_else(|| "none".to_owned(), short_hash)
                                ),
                        )),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_2()
                        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("IR diff"))
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
            project_empty_state(
                "No ChangeSet selected",
                "Choose a persisted ChangeSet to inspect its hashes, diff, evidence, and audit.",
            )
            .into_any_element()
        };

        let mut page = page("changes-approvals-page")
            .child(div().flex().items_center().justify_between().child(
                div().v_flex().gap_1().child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Changes & approvals"))
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child("Approval is guarded by current observed state, baseline, and verification."))
            ).child({ let refresher = entity.clone(); let refresh_project = project_id.clone(); action_button("refresh-change-sets").label("Refresh").on_click(move |_, _, cx| refresher.update(cx, |view, cx| { view.status = view.refresh_project_data(&refresh_project).map_or_else(|error| format!("Refresh failed: {error}"), |_| "ChangeSets refreshed.".to_owned()); cx.notify(); })) }))
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
                        action_button("approve-change-set")
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
                    .child(action_button("reject-change-set").label("Reject").on_click(
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
                    .child(div().flex().justify_between().child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Approval drawer")).child(action_button("close-approval-drawer").ghost().label("Close").on_click(move |_, _, cx| close_view.update(cx, |view, cx| { view.approval_drawer_open = false; cx.notify(); }))))
                    .child(div().text_sm().text_color(rgb(if can_approve { 0x0016_a34a } else { 0x00b9_1c1c })).child(if can_approve { "Baseline, readback, and verification are ready." } else { "Approve is disabled: current observation must equal baseline and verification must pass." }))
                    .child(div().h(px(36.)).px_2().rounded_md().border_1().border_color(rgb(BORDER)).child(InputBase::new("approval-note").h_full().flex().items_center().child(self.approval_note.clone())))
                    .child(actions);
                page = page.child(drawer);
            }
        }
        page.into_any_element()
    }
}
