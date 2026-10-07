//! Semantics presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn authority_style(authority: &SnapshotAuthority) -> (u32, u32, &'static str) {
        match authority {
            SnapshotAuthority::Observed => (0x00e0_f2fe, 0x000e_7490, "observed"),
            SnapshotAuthority::Planned => (0x00f3_e8ff, 0x0076_2b_a3, "planned"),
            SnapshotAuthority::Verified => (0x00dc_fce7, 0x0016_a34a, "verified"),
        }
    }

    /// `inconclusive` is intentionally amber rather than green. It means a verification did
    /// not establish a result, not that the checked property passed.
    pub(super) fn fact_style(status: &FactStatus) -> (u32, u32, &'static str) {
        match status {
            FactStatus::Passed => (0x00dc_fce7, 0x0016_a34a, "passed"),
            FactStatus::Failed => (0x00fe_f2f2, 0x00b9_1c1c, "failed"),
            FactStatus::Inconclusive => (0x00ff_fbeb, 0x00b4_5309, "inconclusive"),
            FactStatus::NotRun => (0x00f1_f5f9, 0x0047_5563, "not run"),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_semantics_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let entity = cx.entity().clone();
        let Some(project_id) = self.navigation.selected_project.clone() else {
            return project_empty_state(
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
                        action_button("refresh-semantic-snapshots")
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
                .child(project_empty_state(
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
        let query_hits = semantic_query_hits(snapshot, self.semantic_query_scope, &query);
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
                                    .child(short_hash(&candidate.snapshot_hash)),
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
                                .map_or_else(|| "root".to_owned(), short_hash)
                        ))),
            );
        }

        let mut components = div().v_flex().gap_1();
        for component in &snapshot.components {
            components = components.child(
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
                        div()
                            .flex_1()
                            .min_w(px(90.))
                            .text_sm()
                            .child(component.value.clone().unwrap_or_else(|| "—".to_owned())),
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
                    .map(|reference| format!("{} @ {}", reference.document_id, reference.locator))
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
                        div().text_sm().whitespace_normal().child(constraint.explanation.clone()),
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
                            .child(short_hash(hash)),
                    ),
            );
        }
        if evidence_sources.is_empty() {
            evidence_rows =
                evidence_rows
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "此快照未附加证据；不能据此主张已验证。",
                    "No evidence is attached to this snapshot; it cannot support a verified claim.",
                )));
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
                    .child(div().w(px(72.)).text_xs().text_color(rgb(0x000e_7490)).child(hit.kind))
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
            query_rows =
                query_rows.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
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
                action_button(format!("semantic-scope-{:?}", scope))
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

        page("semantic-browser-body")
            .child(div().flex().items_start().justify_between().gap_3().child(
                div().v_flex().gap_1().child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(language.choose("电路语义", "Circuit semantics")))
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(language.choose("只读项目快照、验证事实与可追溯证据。", "Read-only project snapshots, verification facts, and traceable evidence."))),
            ).child({ let refresher = entity.clone(); action_button("refresh-semantic-snapshots").label(language.choose("刷新快照", "Refresh snapshots")).on_click(move |_, _, cx| { refresher.update(cx, |view, cx| { view.status = match view.refresh_project_data(&project_id) { Ok(()) => language.choose("语义快照已刷新", "Semantic snapshots refreshed").to_owned(), Err(error) => format!("{}: {error}", language.choose("语义快照未刷新", "Semantic snapshots not refreshed")), }; cx.notify(); }); }) }))
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
}
