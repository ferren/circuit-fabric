//! Usage presentation and event handlers.
use super::usage_charts::{usage_distribution_chart, usage_trend_chart};
use super::*;
use crate::application::usage_audit::{AUDIT_PAGE_SIZE, UsageDimension, audit_page_range};

impl ControlPlaneView {
    /// Reloads one project's cached document, session, and semantic projections from its root.

    /// Builds the two read-only dashboard projections from persisted session records.
    /// Usage summaries and audit events intentionally do not share a mutable UI model.
    ///
    /// The result is cached for a few seconds: rebuilding it reads every session replay
    /// from disk, and this projection is consulted on every render of the usage page —
    /// re-reading all session files per frame made the whole window feel sluggish.
    pub(super) fn usage_audit_model(&mut self) -> UsageAuditModel {
        const CACHE_TTL: Duration = Duration::from_secs(5);
        let fresh = self
            .usage_audit_cached
            .as_ref()
            .is_some_and(|(cached_at, _)| cached_at.elapsed() < CACHE_TTL);
        if !fresh {
            let model = crate::application::project_data::build_usage_audit_model(
                &self.project_data,
                &self.project_storages,
            );
            self.usage_audit_cached = Some((Instant::now(), model));
        }
        self.usage_audit_cached.as_ref().expect("the cache was just populated").1.clone()
    }

    /// Exports exactly the currently filtered audit projection.  It never changes the source
    /// session files, and every CSV row repeats its project/provider/runtime/session context.
    pub(super) fn export_filtered_projection(
        &mut self,
        usage: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let model = self.usage_audit_model();
        let now = UsageAuditModel::now_unix_seconds();
        let csv = if usage {
            let query = self.usage_filter.read(cx).value().to_string();
            UsageAuditModel::usage_csv(&model.filtered_usage(self.usage_period, &query, now))
        } else {
            let query = self.audit_filter.read(cx).value().to_string();
            UsageAuditModel::audit_csv(&model.filtered_audit(
                self.usage_period,
                self.audit_kind_filter,
                &query,
                now,
            ))
        };
        let label = if usage { "用量" } else { "审计" };
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(format!("导出 CircuitFabric {label}"))
            .set_file_name(if usage {
                "circuitfabric-usage.csv"
            } else {
                "circuitfabric-audit.csv"
            })
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
                        Ok(()) => format!("{label}导出已保存：{}", destination.path().display()),
                        Err(error) => format!("{label}导出失败：{error}"),
                    };
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_usage_audit_page(
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
        let input_tokens =
            usage.iter().fold(0_u64, |sum, record| sum.saturating_add(record.input_tokens));
        let output_tokens =
            usage.iter().fold(0_u64, |sum, record| sum.saturating_add(record.output_tokens));
        let total_tokens = input_tokens.saturating_add(output_tokens);
        let unavailable = usage.iter().filter(|record| !record.usage_reported).count();
        let page_filter = format!("{period:?}/{audit_kind:?}/{audit_query}");
        if self.audit_page_filter != page_filter {
            self.audit_page = 0;
            self.audit_page_filter = page_filter;
        }
        let page_range = audit_page_range(audit.len(), self.audit_page);
        self.audit_page = page_range.start / AUDIT_PAGE_SIZE;
        let current_page = self.audit_page;
        let page_count = audit.len().div_ceil(AUDIT_PAGE_SIZE).max(1);
        let previous_button = entity.clone();
        let next_button = entity.clone();

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
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value.to_string()))
                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(label))
        };

        let period_button = entity.clone();
        let kind_button = entity.clone();
        let export_button = entity.clone();
        let usage_export_button = entity.clone();
        let mut usage_rows = div().v_flex().gap_1();
        if grouped.is_empty() {
            usage_rows = usage_rows.child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(
                language.choose("当前筛选没有 Token 用量。", "No token usage matches this filter."),
            ));
        }
        for ((project, provider, runtime), (input, output, total)) in &grouped {
            usage_rows = usage_rows.child(
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
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(format!("Provider: {provider} · Runtime: {runtime}")),
                            ),
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

        let mut audit_rows = div().v_flex().gap_2();
        if audit.is_empty() {
            audit_rows = audit_rows.child(div().text_sm().text_color(rgb(TEXT_MUTED)).child(
                language.choose("当前筛选没有审计事件。", "No audit events match this filter."),
            ));
        }
        for record in &audit[page_range.clone()] {
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
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                if record.timestamp_known {
                                    rfc3339(record.timestamp_unix_seconds)
                                } else {
                                    language
                                        .choose(
                                            "事件时间未记录（阶段快照）",
                                            "Event time unavailable (stage snapshot)",
                                        )
                                        .to_owned()
                                },
                            )),
                    )
                    .child(div().text_sm().whitespace_normal().child(record.summary.clone()))
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(
                        format!(
                            "Project: {} · Provider: {} · Runtime: {} · Session: {}",
                            source.project_id,
                            source.provider_id,
                            source.runtime_id,
                            source.session_id
                        ),
                    ))
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(
                        format!(
                            "{} / {} · {}",
                            source.source_type, source.source_id, source.locator
                        ),
                    )),
            );
        }

        page("usage-audit-page")
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
                action_button("usage-audit-period").label(period.label()).on_click(move |_, _, cx| {
                    period_button.update(cx, |view, cx| {
                        view.usage_period = view.usage_period.next();
                        cx.notify();
                    });
                }),
            ).child(
                action_button("usage-audit-kind").ghost().label(audit_kind.label()).on_click(move |_, _, cx| {
                    kind_button.update(cx, |view, cx| {
                        view.audit_kind_filter = view.audit_kind_filter.next();
                        cx.notify();
                    });
                }),
            ).child(
                action_button("export-immutable-audit").primary().label(
                    language.choose("导出筛选后的审计 CSV", "Export filtered audit CSV"),
                ).on_click(move |_, window, cx| {
                    export_button.update(cx, |view, cx| view.export_filtered_projection(false, window, cx));
                }),
            ).child(
                action_button("export-filtered-usage").label(language.choose("导出筛选后的用量 CSV", "Export filtered usage CSV"))
                    .on_click(move |_, window, cx| {
                        usage_export_button.update(cx, |view, cx| view.export_filtered_projection(true, window, cx));
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
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(
                format!("{}: {} · {}: {unavailable} · {}", language.choose("筛选会话", "Filtered sessions"), usage.len(), language.choose("未报告用量", "Usage unavailable"), language.choose("未报告不等于零用量；周期按会话完成时间（未完成按开始时间），日期按 UTC。", "Unavailable counters are not measured zero; periods use completion time (start time for active sessions), dates use UTC."))))
            .children(model.diagnostics.iter().map(|message| div().text_sm().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(message.clone())))
            .child(usage_trend_chart(&usage, period, now, language))
            .child(usage_distribution_chart("usage-project-chart", language.choose("项目分布", "Project distribution"), &UsageAuditModel::distribution(&usage, UsageDimension::Project), language))
            .child(usage_distribution_chart("usage-provider-chart", language.choose("Provider 分布", "Provider distribution"), &UsageAuditModel::distribution(&usage, UsageDimension::Provider), language))
            .child(usage_distribution_chart("usage-runtime-chart", language.choose("运行时分布", "Runtime distribution"), &UsageAuditModel::distribution(&usage, UsageDimension::Runtime), language))
            .child(div().debug_selector(|| "usage-summary".to_owned()).v_flex().gap_2().p_4().rounded_xl().border_1().border_color(rgb(BORDER)).bg(rgb(CARD_BG)).child(
                div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("Token 用量明细（按项目 / Provider / 运行时）", "Token usage details by project / provider / runtime")),
            ).child(usage_rows))
            .child(div().v_flex().gap_1().child(
                div().text_base().font_weight(FontWeight::SEMIBOLD).child(language.choose("审计日志（只读）", "Audit log (read-only)")),
            ).child(
                div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose(
                    "筛选或导出不会改变审计记录；界面没有编辑、删除或改写操作。",
                    "Filtering and exporting never changes records; this surface has no edit, delete, or rewrite operation.",
                )),
            ).child(div().flex().flex_wrap().items_center().gap_2()
                .child(action_button("audit-previous-page").label(language.choose("上一页", "Previous")).disabled(current_page == 0)
                    .on_click(move |_, _, cx| { previous_button.update(cx, |view, cx| { view.audit_page = view.audit_page.saturating_sub(1); cx.notify(); }); }))
                .child(div().text_xs().child(format!("{}–{} / {} · {} / {page_count}", if audit.is_empty() { 0 } else { page_range.start + 1 }, page_range.end, audit.len(), current_page + 1)))
                .child(action_button("audit-next-page").label(language.choose("下一页", "Next")).disabled(current_page + 1 >= page_count)
                    .on_click(move |_, _, cx| { next_button.update(cx, |view, cx| { view.audit_page = view.audit_page.saturating_add(1); cx.notify(); }); })))
            .child(audit_rows))
    }
}
