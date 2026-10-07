//! Graphical usage projections. Empty scaffolds never enter the record model or exports.
use super::*;
use crate::application::usage_audit::UsageRecord;
use gpui_component::{
    chart::PieChart,
    plot::{Grid, StrokeStyle, shape::Line},
};

const OUTPUT_COLOR: u32 = 0x008b_5cf6;
const COLORS: [u32; 6] = [ACCENT, OUTPUT_COLOR, 0x0010_b981, 0x00f5_9e0b, 0x00ec_4899, 0x003b_82f6];

struct Trend {
    points: Vec<(u64, u64, u64)>,
    start: u64,
    end: u64,
    maximum: u64,
}

impl Trend {
    fn new(records: &[&UsageRecord], period: UsagePeriod, now: u64) -> Self {
        let daily = UsageAuditModel::daily_input_output(records);
        let day_numbers: BTreeMap<_, _> = records
            .iter()
            .map(|record| {
                (
                    rfc3339(record.timestamp_unix_seconds)[..10].to_owned(),
                    record.timestamp_unix_seconds / 86_400,
                )
            })
            .collect();
        let mut days: BTreeMap<u64, (u64, u64)> =
            daily.into_iter().map(|(date, totals)| (day_numbers[&date], totals)).collect();
        let today = now / 86_400;
        let (mut start, mut end) = match period {
            UsagePeriod::Last7Days => (today.saturating_sub(7), today),
            UsagePeriod::Last30Days => (today.saturating_sub(30), today),
            UsagePeriod::All => (
                days.first_key_value().map_or(today.saturating_sub(6), |(day, _)| *day),
                days.last_key_value().map_or(today, |(day, _)| *day),
            ),
        };
        if start == end {
            start = start.saturating_sub(1);
            end = end.saturating_add(1);
        }
        let maximum = days.values().map(|(input, output)| (*input).max(*output)).max().unwrap_or(0);
        let maximum = if maximum == 0 { 4 } else { maximum };
        // Missing dates mean no matching sessions. Only gap endpoints are needed to draw
        // their zero baseline, even when "All" spans years of historical records.
        let recorded: Vec<_> = days.keys().copied().collect();
        for adjacent in recorded.windows(2) {
            if adjacent[1] - adjacent[0] > 1 {
                days.entry(adjacent[0] + 1).or_insert((0, 0));
                days.entry(adjacent[1] - 1).or_insert((0, 0));
            }
        }
        days.entry(start).or_insert((0, 0));
        days.entry(end).or_insert((0, 0));
        Self {
            points: days.into_iter().map(|(day, (input, output))| (day, input, output)).collect(),
            start,
            end,
            maximum,
        }
    }

    fn positions(&self, output: bool, width: f32, height: f32) -> Vec<(f32, f32)> {
        self.points
            .iter()
            .map(|(day, input, out)| {
                let x = 6.
                    + ((*day - self.start) as f64 / (self.end - self.start) as f64) as f32
                        * (width - 12.).max(0.);
                let value = if output { *out } else { *input };
                let y = 8.
                    + (1. - (value as f64 / self.maximum as f64) as f32) * (height - 16.).max(0.);
                (x, y)
            })
            .collect()
    }
}

pub(super) fn usage_trend_chart(
    records: &[&UsageRecord],
    period: UsagePeriod,
    now: u64,
    language: UiLanguage,
) -> Div {
    let trend = Trend::new(records, period, now);
    let mut ticks = div()
        .w(px(72.))
        .h(px(220.))
        .py(px(8.))
        .v_flex()
        .justify_between()
        .text_xs()
        .text_color(rgb(TEXT_MUTED));
    for numerator in [4_u64, 3, 2, 1, 0] {
        ticks = ticks.child((u128::from(trend.maximum) * u128::from(numerator) / 4).to_string());
    }
    let mut dates =
        div().ml(px(80.)).flex().justify_between().text_xs().text_color(rgb(TEXT_MUTED));
    for index in 0..4 {
        let day = trend.start + (trend.end - trend.start) * index / 3;
        dates = dates.child(rfc3339(day.saturating_mul(86_400))[..10].to_owned());
    }
    let empty = records.is_empty();
    let zero = records.iter().all(|record| record.total_tokens() == 0);
    let mut chart =
        div()
            .debug_selector(|| "usage-daily-chart".to_owned())
            .v_flex()
            .gap_2()
            .p_4()
            .rounded_xl()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                language.choose(
                    "每日输入 / 输出折线图（UTC）",
                    "Daily input / output line chart (UTC)",
                ),
            ))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(legend(ACCENT, language.choose("输入 Token", "Input tokens")))
                    .child(legend(OUTPUT_COLOR, language.choose("输出 Token", "Output tokens"))),
            )
            .child(
                div().flex().gap_2().child(ticks).child(
                    div()
                        .debug_selector(|| "usage-daily-plot".to_owned())
                        .flex_1()
                        .min_w(px(0.))
                        .h(px(220.))
                        .child(
                            gpui::canvas(
                                |_, _, _| (),
                                move |bounds, (), window, _| {
                                    let height = bounds.size.height.as_f32();
                                    Grid::new()
                                        .y((0..=4)
                                            .map(|i| 8. + (height - 16.) * i as f32 / 4.)
                                            .collect())
                                        .stroke(rgb(BORDER))
                                        .paint(&bounds, window);
                                    // Both series share one scale. A wide input stroke and smaller output
                                    // dots keep both colors visible when they coincide, including zero.
                                    for (output, color, stroke, dot) in
                                        [(false, ACCENT, 4., 8.), (true, OUTPUT_COLOR, 2., 4.)]
                                    {
                                        Line::new()
                                            .data(trend.positions(
                                                output,
                                                bounds.size.width.as_f32(),
                                                height,
                                            ))
                                            .x(|p| Some(p.0))
                                            .y(|p| Some(p.1))
                                            .stroke(rgb(color))
                                            .stroke_width(stroke)
                                            .stroke_style(StrokeStyle::Linear)
                                            .dot()
                                            .dot_size(dot)
                                            .dot_fill_color(rgb(color))
                                            .paint(&bounds, window);
                                    }
                                },
                            )
                            .size_full(),
                        ),
                ),
            )
            .child(dates);
    if empty || zero {
        chart = chart.child(div().debug_selector(move || if empty { "usage-daily-empty" } else { "usage-daily-zero" }.to_owned())
            .text_sm().text_color(rgb(TEXT_MUTED)).child(if empty {
                language.choose("暂无匹配记录 · 0 tokens；运行任务后显示真实趋势", "No matching records · 0 tokens; run a task to see usage")
            } else {
                language.choose("当前记录的 Token 计数全部为 0；折线保持零值基线，未报告数量见上方提示", "All recorded counters are 0; zero baseline shown, see unavailable count above")
            }));
    }
    chart
}

fn legend(color: u32, text: impl Into<gpui::SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .whitespace_normal()
        .child(status_dot(color))
        .child(text.into())
}

#[derive(Clone)]
struct Slice {
    weight: f32,
    color: u32,
}

fn slices(values: &BTreeMap<String, u64>) -> Vec<Slice> {
    if values.values().all(|value| *value == 0) {
        // A neutral ring is a presentation scaffold, not a fake usage category.
        return vec![Slice { weight: 1., color: BORDER }];
    }
    let maximum = UsageAuditModel::chart_scale(values);
    values
        .values()
        .enumerate()
        .filter(|(_, value)| **value > 0)
        .map(|(index, value)| Slice {
            weight: (*value as f64 / maximum as f64) as f32,
            color: COLORS[index % COLORS.len()],
        })
        .collect()
}

pub(super) fn usage_distribution_chart(
    selector: &'static str,
    title: &'static str,
    values: &BTreeMap<String, u64>,
    language: UiLanguage,
) -> Div {
    let total = values.values().fold(0_u64, |sum, value| sum.saturating_add(*value));
    let denominator: f64 = values.values().map(|value| *value as f64).sum();
    let mut labels = div().flex_1().min_w(px(160.)).v_flex().gap_2();
    for (index, (label, value)) in values.iter().enumerate() {
        let percent = if denominator > 0. { *value as f64 / denominator * 100. } else { 0. };
        labels = labels.child(legend(
            COLORS[index % COLORS.len()],
            format!("{label} · {value} tokens · {percent:.1}%"),
        ));
    }
    if total == 0 {
        labels = labels.child(
            div()
                .debug_selector(move || format!("{selector}-empty"))
                .text_sm()
                .text_color(rgb(TEXT_MUTED))
                .child(if values.is_empty() {
                    language.choose("暂无分布数据", "No distribution data")
                } else {
                    language.choose(
                        "总计数为 0，暂无可计算的占比",
                        "Total counters are 0; no proportions available",
                    )
                }),
        );
    }
    div()
        .debug_selector(move || selector.to_owned())
        .v_flex()
        .gap_2()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(CARD_BG))
        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .debug_selector(move || format!("{selector}-donut"))
                        .relative()
                        .w(px(200.))
                        .h(px(200.))
                        .flex_none()
                        .child(
                            div().size_full().child(
                                PieChart::new(slices(values))
                                    .inner_radius(62.)
                                    .outer_radius(88.)
                                    .value(|slice| slice.weight)
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
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(total.to_string()),
                                )
                                .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child("tokens")),
                        ),
                )
                .child(labels),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_extreme_distribution_have_finite_visible_slices() {
        for values in [BTreeMap::new(), BTreeMap::from([("zero".into(), 0)])] {
            let arcs = slices(&values);
            assert_eq!(arcs.len(), 1);
            assert_eq!(arcs[0].color, BORDER);
            assert_eq!(arcs[0].weight, 1.);
        }
        let arcs = slices(&BTreeMap::from([
            ("a".into(), u64::MAX),
            ("b".into(), u64::MAX / 2),
            ("zero".into(), 0),
        ]));
        assert_eq!(arcs.len(), 2);
        assert!(arcs.iter().all(|arc| arc.weight.is_finite() && arc.weight > 0.));
        assert!((arcs[0].weight / arcs[1].weight - 2.).abs() < 0.001);
    }

    #[test]
    fn trend_keeps_shared_scale_zero_baseline_and_actual_date_spacing() {
        let mut record = UsageRecord {
            project_id: "p".into(),
            provider_id: "provider".into(),
            runtime_id: "codex".into(),
            session_id: "s".into(),
            timestamp_unix_seconds: 864_000,
            input_tokens: 0,
            output_tokens: 0,
            usage_reported: true,
            source_locator: "session".into(),
        };
        for records in [vec![], vec![&record]] {
            let trend = Trend::new(&records, UsagePeriod::All, 864_000);
            let points = trend.positions(false, 400., 220.);
            assert!(points.len() >= 2);
            assert!(points.first().unwrap().0 < points.last().unwrap().0);
            assert!(points.iter().all(|p| p.1 == 212. && p.0.is_finite()));
            assert_eq!(points, trend.positions(true, 400., 220.));
        }
        record.input_tokens = u64::MAX;
        record.output_tokens = u64::MAX / 2;
        let mut later = record.clone();
        later.timestamp_unix_seconds += 3 * 86_400;
        let trend = Trend::new(&[&record, &later], UsagePeriod::All, later.timestamp_unix_seconds);
        let input = trend.positions(false, 400., 220.);
        let output = trend.positions(true, 400., 220.);
        assert_eq!(input[0].1, 8.);
        assert!((output[0].1 - 110.).abs() < 0.01);
        assert_eq!(input[1].1, 212., "missing dates use zero rather than interpolating usage");
        assert!(input.iter().chain(&output).all(|p| p.0.is_finite() && p.1.is_finite()));
    }
}
