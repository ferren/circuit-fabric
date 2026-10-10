//! Complete live extraction output, with independent inline and expanded scrolling.
use super::*;

fn stream_title(elapsed_seconds: Option<u64>, language: UiLanguage) -> String {
    match elapsed_seconds {
        Some(seconds) => language.choose_owned(
            format!("模型输出（进行中… {seconds}s）"),
            format!("Model output (running… {seconds}s)"),
        ),
        None => language.choose("模型输出（已结束）", "Model output (finished)").to_owned(),
    }
}

pub(super) fn stream_body(
    id: &'static str,
    log: &str,
    scroll: &gpui::ScrollHandle,
) -> impl IntoElement {
    // Read the previous layout's extent before the new text is measured. A reader
    // who has scrolled up keeps their offset; returning to the end resumes following.
    if scroll.max_offset().y + scroll.offset().y <= px(1.) {
        scroll.scroll_to_bottom();
    }
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .flex_1()
        .min_h(px(0.))
        .min_w(px(0.))
        .overflow_y_scroll()
        .track_scroll(scroll)
        .pr_3()
        .text_xs()
        .text_color(rgb(TEXT_MUTED))
        .whitespace_normal()
        .child(gpui_base::SelectableText::new((id, 0_usize), log.to_owned()))
        .vertical_scrollbar(scroll)
}

impl ControlPlaneView {
    pub(super) fn render_datasheet_stream(
        log: &str,
        elapsed_seconds: Option<u64>,
        language: UiLanguage,
        scroll: &gpui::ScrollHandle,
        entity: &Entity<Self>,
    ) -> Div {
        let opener = entity.clone();
        div()
            .v_flex()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(stream_title(elapsed_seconds, language)),
                    )
                    .child(
                        action_button("expand-datasheet-stream")
                            .debug_selector(|| "expand-datasheet-stream".into())
                            .ghost()
                            .icon(IconName::Maximize)
                            .tooltip(language.choose("放大模型输出", "Expand model output"))
                            .on_click(move |_, _, cx| {
                                opener.update(cx, |view, cx| {
                                    view.datasheet_stream_modal_open = true;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(div().v_flex().h(px(180.)).flex_none().overflow_hidden().child(stream_body(
                "datasheet-stream-body",
                log,
                scroll,
            )))
    }

    pub(super) fn close_datasheet_stream_modal(&mut self, cx: &mut Context<Self>) {
        self.datasheet_stream_modal_open = false;
        cx.notify();
    }

    pub(super) fn render_datasheet_stream_modal(&self, cx: &mut Context<Self>) -> Div {
        // Re-read the shared buffer on every repaint, including the final repaint
        // after extraction completes. The dialog never holds an opening-time snapshot.
        let log = self
            .datasheet_stream
            .as_ref()
            .and_then(|(_, _, stream)| stream.lock().ok().map(|buffer| buffer.clone()))
            .unwrap_or_default();
        let elapsed = self
            .datasheet_extract_started
            .filter(|_| self.datasheet_extracting)
            .map(|started| started.elapsed().as_secs());
        let closer = cx.entity().clone();
        let backdrop_closer = closer.clone();
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("datasheet-stream-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x0f17_2ab3))
                    .occlude()
                    .on_click(move |_, _, cx| {
                        backdrop_closer.update(cx, Self::close_datasheet_stream_modal);
                    }),
            )
            .child(
                div()
                    .id("datasheet-stream-modal")
                    .debug_selector(|| "datasheet-stream-modal".into())
                    .relative()
                    .occlude()
                    .w_full()
                    .max_w(px(1000.))
                    .h_full()
                    .max_h(px(860.))
                    .min_h(px(0.))
                    .v_flex()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(CARD_BG))
                    .shadow_lg()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .p_4()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(stream_title(elapsed, self.language)),
                            )
                            .child(
                                action_button("close-datasheet-stream")
                                    .debug_selector(|| "close-datasheet-stream".into())
                                    .ghost()
                                    .label(self.language.choose("关闭", "Close"))
                                    .on_click(move |_, _, cx| {
                                        closer.update(cx, Self::close_datasheet_stream_modal);
                                    }),
                            ),
                    )
                    .child(div().v_flex().flex_1().min_h(px(0.)).p_4().overflow_hidden().child(
                        stream_body(
                            "datasheet-stream-modal-body",
                            &log,
                            &self.datasheet_stream_modal_scroll,
                        ),
                    )),
            )
    }
}
