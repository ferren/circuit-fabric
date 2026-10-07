//! Export presentation and event handlers.
use super::*;

impl ControlPlaneView {
    /// BOM is a projection of an immutable logical snapshot, never an independently edited
    /// parts list.  This keeps the selected format, every reference, and every evidence
    /// locator attached to a concrete source of engineering truth while exporter plugins are
    /// still pending.
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_bom_export_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let entity = cx.entity().clone();
        let Some(project_id) = self.navigation.selected_project.clone() else {
            return project_empty_state(
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
                            action_button("refresh-bom-snapshots")
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
                .child(project_empty_state(
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
                action_button(format!("bom-snapshot-{candidate_hash}"))
                    .label(format!(
                        "{} · {}",
                        short_hash(&candidate.snapshot_hash),
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
                action_button(format!("bom-export-format-{format:?}"))
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
                            short_hash(&reference.content_hash)
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
                bom_rows.child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(language.choose(
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
        page("bom-export-body")
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
                        action_button("refresh-bom-snapshots")
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
                                action_button("export-netlist-todo")
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
                                action_button("export-spice-todo")
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
}
