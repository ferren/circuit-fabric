//! Native ring, grouped bars, and daily line drawn from the shared read model.
use super::*;
use crate::application::overview::OverviewModel;
use gpui_component::{
    chart::PieChart,
    plot::{Grid, StrokeStyle, shape::Line},
};

pub(super) const FACT_COLORS: [u32; 4] = [0x0016_a34a, 0x00b9_1c1c, 0x00b4_5309, 0x0047_5563];
pub(super) const RESOURCE_COLORS: [u32; 3] = [0x008b_5cf6, ACCENT, 0x003b_82f6];

fn frame(selector: &'static str, title: &'static str) -> Div {
    div()
        .debug_selector(move || selector.into())
        .v_flex()
        .flex_none()
        .gap_2()
        .p_4()
        .min_w(px(0.))
        .bg(rgb(CARD_BG))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded_xl()
        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(title))
}
fn legend(color: u32, label: impl Into<gpui::SharedString>) -> Div {
    div().flex().gap_2().items_center().text_xs().child(status_dot(color)).child(label.into())
}
fn scope(model: &OverviewModel, language: UiLanguage) -> String {
    if model.registry_unknown {
        return language
            .choose("项目注册表未读取；统计范围未知", "Project registry unread; scope unknown")
            .into();
    }
    format!(
        "{}/{} {}",
        model.loaded_projects,
        model.resources.len(),
        language
            .choose("项目已读；未知来源不计作零", "projects loaded; unknown sources are not zero")
    )
}
#[derive(Clone)]
struct Slice {
    value: f32,
    color: u32,
}

pub(super) fn fact_chart(model: &OverviewModel, language: UiLanguage) -> Div {
    let total: usize = model.facts.iter().sum();
    let mut slices: Vec<_> = model
        .facts
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > 0)
        .map(|(index, value)| Slice { value: *value as f32, color: FACT_COLORS[index] })
        .collect();
    if slices.is_empty() {
        slices.push(Slice { value: 1., color: BORDER });
    }
    let unknown = !model.complete() || model.ambiguous_projects > 0;
    let mut labels = div().v_flex().gap_2();
    for (index, label) in ["passed", "failed", "inconclusive", "not_run"].iter().enumerate() {
        labels = labels.child(legend(
            FACT_COLORS[index],
            format!(
                "{label} · {}",
                if unknown && model.fact_projects == 0 {
                    "—".into()
                } else {
                    model.facts[index].to_string()
                }
            ),
        ));
    }
    frame(
        "overview-fact-chart",
        language.choose("验证事实状态分布", "Verification fact distribution"),
    )
    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
        "{} · {} {} · {} {}",
        scope(model, language),
        model.fact_projects,
        language.choose("个唯一谱系末端快照", "unique lineage-tip snapshots"),
        model.ambiguous_projects,
        language.choose("项目当前快照不明确", "projects with ambiguous current snapshot")
    )))
    .child(
        div()
            .flex()
            .flex_wrap()
            .gap_4()
            .items_center()
            .child(
                div()
                    .debug_selector(|| "overview-fact-ring".into())
                    .relative()
                    .size(px(160.))
                    .flex_none()
                    .child(
                        div().size_full().child(
                            PieChart::new(slices)
                                .inner_radius(48.)
                                .outer_radius(70.)
                                .value(|slice| slice.value)
                                .color(|slice| rgb(slice.color)),
                        ),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .v_flex()
                            .items_center()
                            .justify_center()
                            .child(if unknown && total == 0 {
                                "—".into()
                            } else {
                                total.to_string()
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(language.choose("已读事实", "Loaded facts")),
                            ),
                    ),
            )
            .child(labels),
    )
    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(if total == 0 {
        if unknown {
            language.choose(
                "来源不完整；刷新项目以读取事实。",
                "Incomplete sources; refresh projects to load facts.",
            )
        } else {
            language.choose(
                "暂无验证事实；导入语义快照并运行验证。",
                "No verification facts; import a snapshot and run validation.",
            )
        }
    } else {
        language.choose(
            "绿色通过 · 红色失败 · 琥珀色未确定 · 灰色未运行",
            "Green passed · red failed · amber inconclusive · gray not run",
        )
    }))
}

pub(super) fn session_chart(model: &OverviewModel, language: UiLanguage) -> Div {
    let maximum = model.daily_sessions.iter().map(|(_, count)| *count).max().unwrap_or(0).max(4);
    let points = model.daily_sessions.clone();
    let known = model.complete() || model.loaded_projects > 0;
    let mut ticks = div()
        .w(px(28.))
        .h(px(160.))
        .py(px(8.))
        .v_flex()
        .justify_between()
        .text_xs()
        .text_color(rgb(TEXT_MUTED));
    for i in (0..=4).rev() {
        ticks = ticks.child((maximum * i / 4).to_string());
    }
    let mut dates =
        div().ml(px(36.)).flex().justify_between().text_xs().text_color(rgb(TEXT_MUTED));
    for index in [0, 3, 6] {
        if let Some((day, _)) = model.daily_sessions.get(index) {
            dates = dates.child(rfc3339(day * 86_400)[5..10].to_owned());
        }
    }
    frame(
        "overview-session-chart",
        language.choose("最近 7 个日历日会话（UTC）", "Session starts over 7 calendar days (UTC)"),
    )
    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(scope(model, language)))
    .child(legend(
        ACCENT,
        language.choose("会话开始数（持久记录）", "Session starts (persisted records)"),
    ))
    .child(
        div().flex().gap_2().child(ticks).child(
            div()
                .debug_selector(|| "overview-session-plot".into())
                .flex_1()
                .min_w(px(0.))
                .h(px(160.))
                .child(
                    gpui::canvas(
                        |_, _, _| (),
                        move |bounds, (), window, _| {
                            let height = bounds.size.height.as_f32();
                            let width = bounds.size.width.as_f32();
                            Grid::new()
                                .y((0..=4).map(|i| 8. + (height - 16.) * i as f32 / 4.).collect())
                                .stroke(rgb(BORDER))
                                .paint(&bounds, window);
                            if known {
                                Line::new()
                                    .data(
                                        points
                                            .iter()
                                            .enumerate()
                                            .map(|(i, (_, count))| {
                                                (
                                                    6. + (width - 12.).max(0.) * i as f32 / 6.,
                                                    8. + (height - 16.).max(0.)
                                                        * (1. - *count as f32 / maximum as f32),
                                                )
                                            })
                                            .collect::<Vec<_>>(),
                                    )
                                    .x(|p| Some(p.0))
                                    .y(|p| Some(p.1))
                                    .stroke(rgb(ACCENT))
                                    .stroke_width(2.)
                                    .stroke_style(StrokeStyle::Linear)
                                    .dot()
                                    .dot_size(6.)
                                    .dot_fill_color(rgb(ACCENT))
                                    .paint(&bounds, window);
                            }
                        },
                    )
                    .size_full(),
                ),
        ),
    )
    .child(dates)
    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
            "{} · {} {}",
            language.choose(
                "无活动保持零值基线；未读来源保留未知",
                "No activity uses zero baseline; unread sources remain unknown"
            ),
            model.unreliable_session_times,
            language.choose(
                "条时间缺失/为零/在未来，未入趋势",
                "missing/zero/future timestamps excluded"
            )
        )))
}

pub(super) fn resource_chart(model: &OverviewModel, language: UiLanguage) -> Div {
    let maximum =
        model.resources.iter().filter_map(|row| row.counts).flatten().max().unwrap_or(0).max(4);
    let mut legends = div().flex().flex_wrap().gap_4();
    for (index, label) in [
        language.choose("已授权文档", "Authorized documents"),
        language.choose("会话", "Sessions"),
        language.choose("语义快照（含历史）", "Snapshots (including history)"),
    ]
    .into_iter()
    .enumerate()
    {
        legends = legends.child(legend(RESOURCE_COLORS[index], label));
    }
    let mut rows = div().id("overview-resource-scroll").v_flex().gap_3();
    for row in &model.resources {
        let counts = row.counts;
        let mut values = div().w(px(40.)).h(px(68.)).v_flex().justify_between().text_xs();
        for index in 0..3 {
            values = values.child(counts.map_or_else(|| "—".into(), |c| c[index].to_string()));
        }
        rows = rows.child(
            div()
                .debug_selector({
                    let id = row.id.clone();
                    move || format!("overview-resource-{id}")
                })
                .v_flex()
                .gap_1()
                .flex_none()
                .child(div().text_xs().whitespace_normal().child(format!(
                    "{} / {}{}",
                    row.name,
                    row.id,
                    if counts.is_none() {
                        language.choose(" · 未知/缓存过期", " · unknown/stale")
                    } else {
                        ""
                    }
                )))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .debug_selector({
                                    let id = row.id.clone();
                                    move || format!("overview-bars-{id}")
                                })
                                .h(px(68.))
                                .flex_1()
                                .min_w(px(0.))
                                .child(
                                    gpui::canvas(
                                        |_, _, _| (),
                                        move |bounds, (), window, _| {
                                            let width = bounds.size.width.as_f32();
                                            Grid::new()
                                                .x((0..=4)
                                                    .map(|i| {
                                                        2. + (width - 4.).max(0.) * i as f32 / 4.
                                                    })
                                                    .collect())
                                                .stroke(rgb(BORDER))
                                                .paint(&bounds, window);
                                            if let Some(counts) = counts {
                                                for (index, count) in counts.into_iter().enumerate()
                                                {
                                                    if count > 0 {
                                                        let bar = gpui::Bounds::new(
                                                            bounds.origin
                                                                + gpui::point(
                                                                    px(2.),
                                                                    px(index as f32 * 24.),
                                                                ),
                                                            gpui::size(
                                                                px((width - 4.).max(0.)
                                                                    * count as f32
                                                                    / maximum as f32),
                                                                px(16.),
                                                            ),
                                                        );
                                                        window.paint_quad(gpui::fill(
                                                            bar,
                                                            rgb(RESOURCE_COLORS[index]),
                                                        ));
                                                    }
                                                }
                                            }
                                        },
                                    )
                                    .size_full(),
                                ),
                        )
                        .child(values),
                ),
        );
    }
    if model.resources.is_empty() {
        rows = rows
            .child(
                div().h(px(100.)).child(
                    gpui::canvas(
                        |_, _, _| (),
                        |bounds, (), window, _| {
                            Grid::new()
                                .x((0..=4)
                                    .map(|i| 2. + (bounds.size.width.as_f32() - 4.) * i as f32 / 4.)
                                    .collect())
                                .stroke(rgb(BORDER))
                                .paint(&bounds, window);
                        },
                    )
                    .size_full(),
                ),
            )
            .child(div().text_xs().child(language.choose(
                "暂无项目；创建或打开项目后展示资源。",
                "No projects; create or open one to see resources.",
            )));
    }
    frame(
        "overview-resource-chart",
        language.choose("各项目资源分组柱状图", "Grouped project resource bars"),
    )
    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
        "{} · {} {}",
        scope(model, language),
        model.resources.len(),
        language.choose("个登记项目，完整列表可滚动", "registered projects; scroll for full list")
    )))
    .child(legends)
    .child(
        div()
            .flex()
            .justify_between()
            .pr(px(48.))
            .text_xs()
            .text_color(rgb(TEXT_MUTED))
            .child("0")
            .child((maximum / 2).to_string())
            .child(maximum.to_string()),
    )
    .child(div().h(px(280.)).flex_none().child(rows.scroll_y()))
}
