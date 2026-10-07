//! Preview presentation and event handlers.
use super::*;

#[cfg(all(test, feature = "ui-test-support"))]
mod selection_tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use circuitfabric_plugin_api::{
        DocumentOpenerOutcome, DocumentOpenerRequest, VerifiedDocumentCopy,
    };
    use gpui::{Bounds, FocusHandle, MouseButton, Pixels, TestAppContext, point};

    use super::*;

    struct MarkdownSelectionView {
        focus: FocusHandle,
        body: DocumentViewBody,
        bounds: Rc<RefCell<Bounds<Pixels>>>,
        pressed: Rc<Cell<bool>>,
    }

    impl Render for MarkdownSelectionView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let focus = self.focus.clone();
            let bounds = self.bounds.clone();
            div()
                .track_focus(&self.focus)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| window.focus(&focus, cx))
                .child(crate::pdf_text_layer::selection_gesture_guard(self.pressed.clone()))
                .child(
                    ControlPlaneView::render_document_view_body(&self.body, UiLanguage::English)
                        .relative()
                        .w(px(180.))
                        .child(
                            gpui::canvas(
                                move |rect, _, _| *bounds.borrow_mut() = rect,
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        ),
                )
        }
    }

    #[gpui::test]
    fn rendered_markdown_wraps_and_copies_formatted_text_across_blocks(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        // Keep the fixture digest stable on checkouts that convert LF to CRLF.
        let bytes = include_str!("testfixtures/selection.md").replace("\r\n", "\n").into_bytes();
        let hash =
            "sha256:b4b53642c6815f13d7677c5d2086704fa5636808510aebdd9fdfe24f079cbf47".to_owned();
        let request = DocumentOpenerRequest {
            project_id: "markdown-test".into(),
            document_id: "markdown-selection".into(),
            file_name: "selection.md".into(),
            document_kind: DocumentKind::Text,
            content_hash: hash.clone(),
            source_locator: "selection.md".into(),
            managed_copy: VerifiedDocumentCopy::from_verified(bytes, hash).unwrap(),
        };
        let DocumentOpenerOutcome::Loaded { view } =
            DocumentOpenerRegistry::with_builtin_openers().open(&request)
        else {
            panic!("Markdown fixture must load");
        };
        let bounds = Rc::new(RefCell::new(Bounds::default()));
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| MarkdownSelectionView {
                focus: cx.focus_handle(),
                body: view.body,
                bounds: bounds.clone(),
                pressed: Rc::new(Cell::new(false)),
            });
            Root::new(view, window, cx).bordered(false)
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let rect = *bounds.borrow();
        let start = point(rect.left() + px(0.5), rect.top() + px(7.));
        let end = point(rect.right() - px(1.), rect.bottom() - px(7.));
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
        cx.simulate_keystrokes("ctrl-c");
        // Table text follows the opener's existing flattened-row representation.
        let expected = "Title\nAlpha 中文 and italic with code and old.\nlist item\nquote\nlet x = 1;\nlet y = 2;\n\nA | B | 1 | 2\nlast";
        cx.update(|_, cx| {
            assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), expected);
        });
        // Moving after release must keep the selected text, without extending it.
        cx.simulate_mouse_move(start, None, Default::default());
        cx.simulate_keystrokes("ctrl-c");
        cx.update(|_, cx| {
            assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), expected);
        });
    }
}

impl ControlPlaneView {
    /// Spreadsheet numbers keep their cached precision but drop a bare `.0` tail.
    // The truncating cast is guarded: only integral values below 1e15 reach it.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub(super) fn format_number_cell(value: f64) -> String {
        if value.fract() == 0. && value.abs() < 1e15 {
            format!("{}", value as i64)
        } else {
            format!("{value}")
        }
    }

    /// Converts one tight RGBA bitmap into a GPUI render image.
    ///
    /// GPUI consumes BGRA, the opener protocol carries RGBA, so each pixel's red and
    /// blue channels swap here. `None` means the page could not be converted and is
    /// skipped rather than blanking the preview.
    pub(super) fn render_image_from_rgba(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Option<std::sync::Arc<gpui::RenderImage>> {
        let mut bgra = rgba;
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
        Some(std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)])))
    }

    /// Converts rendered pages to GPUI images; pages that fail to convert are skipped.
    pub(super) fn raster_preview_pages(
        pages: Vec<circuitfabric_plugin_api::DocumentRasterPage>,
        requested_width: u32,
    ) -> Vec<(u32, DocumentRasterPreviewPage)> {
        pages
            .into_iter()
            .filter_map(|page| {
                Some((
                    page.number,
                    DocumentRasterPreviewPage {
                        requested_width,
                        image: Self::render_image_from_rgba(page.width, page.height, page.rgba)?,
                    },
                ))
            })
            .collect()
    }

    /// Starts the on-demand preview of a rasterized view body: the opener's first pages
    /// move out of the view (so their bitmaps are not held twice) and every page's size
    /// is read for layout. Runs off the UI thread.
    pub(super) fn raster_preview(
        view: &mut DocumentView,
        data: &[u8],
    ) -> Option<DocumentRasterPreview> {
        let DocumentViewBody::RasterPages { pages, page_count, .. } = &mut view.body else {
            return None;
        };
        let initial = std::mem::take(pages);
        let text_pages = circuitfabric_document_opener::pdf_page_text(
            data,
            &initial.iter().map(|page| page.number).collect::<Vec<_>>(),
        )
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .map(|page| (page.number, std::sync::Arc::new(page)))
        .collect();
        let fallback = initial
            .first()
            .map_or((612.0, 792.0), |page| (page.width.max(1) as f32, page.height.max(1) as f32));
        let page_sizes = circuitfabric_document_opener::pdf_page_sizes(data)
            .and_then(Result::ok)
            .filter(|sizes| sizes.len() == *page_count)
            .unwrap_or_else(|| vec![fallback; *page_count]);
        Some(DocumentRasterPreview {
            data: std::sync::Arc::from(data),
            content_hash: view.content_hash.clone(),
            page_sizes,
            pages: Self::raster_preview_pages(initial, 900).into_iter().collect(),
            in_flight: BTreeMap::new(),
            failed: std::collections::BTreeSet::new(),
            retired_images: Vec::new(),
            text_pages,
        })
    }

    /// Removes the previewed document's page images from the GPU atlas; GPUI keeps an
    /// image's texture until it is dropped explicitly.
    pub(super) fn release_preview_images(&mut self, window: &mut Window) {
        if let Some(DocumentPreviewSelection {
            state: DocumentPreviewState::Loaded { raster: Some(raster), .. },
            ..
        }) = self.document_preview.as_mut()
        {
            for (_, page) in std::mem::take(&mut raster.pages) {
                window.drop_image(page.image).ok();
            }
            for image in std::mem::take(&mut raster.retired_images) {
                window.drop_image(image).ok();
            }
        }
    }

    /// Keeps the rendered pages around `visible` (inclusive page numbers): evicts far
    /// pages and renders missing nearby ones in one background batch.
    pub(super) fn update_raster_window(
        &mut self,
        (first, last): (u32, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_width = crate::pdf_zoom::raster_width(
            (self.document_preview_width - 34.0).max(240.0),
            self.preview_pdf_zoom.value,
            window.scale_factor(),
        );
        let settled = self.preview_zoom_tick.is_none();
        let Some(preview) = self.document_preview.as_mut() else {
            return;
        };
        let DocumentPreviewState::Loaded { raster: Some(raster), .. } = &mut preview.state else {
            return;
        };
        let count = u32::try_from(raster.page_sizes.len()).unwrap_or(u32::MAX);
        for image in std::mem::take(&mut raster.retired_images) {
            window.drop_image(image).ok();
        }
        let keep =
            first.saturating_sub(RASTER_KEEP_DISTANCE)..=last.saturating_add(RASTER_KEEP_DISTANCE);
        let evicted: Vec<u32> = raster
            .pages
            .iter()
            .filter(|(number, page)| {
                !keep.contains(number)
                    || (!(first..=last).contains(number) && page.requested_width > 900)
            })
            .map(|(number, _)| *number)
            .collect();
        for number in evicted {
            if let Some(page) = raster.pages.remove(&number) {
                window.drop_image(page.image).ok();
            }
        }
        let neighbors = first.saturating_sub(RASTER_PREFETCH_BEFORE).max(1)
            ..=last.saturating_add(RASTER_PREFETCH_AFTER).min(count);
        let mut wanted: Vec<(u32, u32)> = (first..=last.min(count))
            .chain(neighbors.filter(|number| !(first..=last).contains(number)))
            .filter_map(|number| {
                let width =
                    if settled && (first..=last).contains(&number) { target_width } else { 900 };
                (!raster.pages.get(&number).is_some_and(|page| page.requested_width >= width)
                    && !raster.in_flight.contains_key(&number)
                    && !raster.failed.contains(&(number, width)))
                .then_some((number, width))
            })
            .take(3)
            .collect();
        if wanted.is_empty() {
            return;
        }
        // Large bitmaps are produced one at a time; neighbors stay at preview resolution.
        if wanted[0].1 > 900 {
            wanted.truncate(1);
        }
        raster.in_flight.extend(wanted.iter().copied());
        let data = raster.data.clone();
        let source_data = data.clone();
        let project_id = preview.project_id.clone();
        let document_id = preview.document_id.clone();
        let requested = wanted.clone();
        let missing_text: Vec<_> = wanted
            .iter()
            .map(|(number, _)| *number)
            .filter(|number| !raster.text_pages.contains_key(number))
            .collect();
        let work = cx.background_spawn(async move {
            let mut text_pages: BTreeMap<_, _> = if missing_text.is_empty() {
                BTreeMap::new()
            } else {
                circuitfabric_document_opener::pdf_page_text(&data, &missing_text)
                    .and_then(Result::ok)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|page| (page.number, std::sync::Arc::new(page)))
                    .collect()
            };
            requested
                .into_iter()
                .map(|(number, width)| {
                    let page = circuitfabric_document_opener::render_pdf_pages_at_width(
                        &data,
                        &[number],
                        width,
                    )
                    .and_then(Result::ok)
                    .and_then(|pages| {
                        Self::raster_preview_pages(pages, width)
                            .into_iter()
                            .next()
                            .map(|(_, page)| page)
                    });
                    (number, width, page, text_pages.remove(&number))
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |view, cx| {
            let rendered = work.await;
            view.update(cx, |view, cx| {
                let Some(DocumentPreviewSelection {
                    project_id: shown_project,
                    document_id: shown_document,
                    state: DocumentPreviewState::Loaded { raster: Some(raster), .. },
                    ..
                }) = view.document_preview.as_mut()
                else {
                    return;
                };
                if *shown_project != project_id
                    || *shown_document != document_id
                    || !std::sync::Arc::ptr_eq(&raster.data, &source_data)
                {
                    return;
                }
                for (number, width, page, text) in rendered {
                    raster.in_flight.remove(&number);
                    if let Some(text) = text {
                        raster.text_pages.insert(number, text);
                    }
                    if let Some(page) = page {
                        if let Some(old) = raster.pages.insert(number, page) {
                            raster.retired_images.push(old.image);
                        }
                    } else {
                        raster.failed.insert((number, width));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The docked right-hand preview pane.
    ///
    /// Rendered beside the scrollable page content (not inside it), so it keeps the full
    /// window height and scrolls independently of the page behind it.
    pub(super) fn render_document_preview_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let pane_width = self.document_preview_width;
        if !window.is_window_active() {
            self.stop_pdf_interaction();
        }
        let Some(preview) = &self.document_preview else {
            return div().into_any_element();
        };

        let (state_label, state_colors) = match &preview.state {
            DocumentPreviewState::Loading => {
                (language.choose("加载中", "Loading"), (0x00f0_f9ff, 0x000e_7490))
            }
            DocumentPreviewState::Loaded { .. } => {
                (language.choose("已加载", "Loaded"), (0x00dc_fce7, 0x0016_a34a))
            }
            DocumentPreviewState::Refused { .. } => {
                (language.choose("已拒绝", "Refused"), (0x00fe_f2f2, 0x00b9_1c1c))
            }
            DocumentPreviewState::Unavailable { .. } => {
                (language.choose("不可预览", "Unavailable"), (0x00ff_f7ed, 0x00b4_5309))
            }
        };

        let is_pdf = preview.file_name.to_lowercase().ends_with(".pdf");
        let show_data = self.preview_show_data && is_pdf;
        let header_closer = entity.clone();
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .p_3()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_normal()
                            .child(preview.file_name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("{} · {}", preview.project_id, preview.document_id)),
                    ),
            )
            .child(
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .text_xs()
                    .flex_none()
                    .bg(rgb(state_colors.0))
                    .text_color(rgb(state_colors.1))
                    .child(state_label),
            )
            .child(
                action_button("close-document-preview")
                    .ghost()
                    .label(language.choose("关闭", "Close"))
                    .on_click(move |_, window, cx| {
                        header_closer.update(cx, |view, cx| {
                            view.close_document_preview(window, cx);
                        });
                    }),
            );

        // Datasheet PDFs get a second tab with the structured extraction.
        let tabs = if is_pdf {
            let preview_tab = entity.clone();
            let data_tab = entity.clone();
            Some(
                div()
                    .flex()
                    .gap_1()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        action_button("preview-tab-document")
                            .when(!show_data, Button::primary)
                            .when(show_data, Button::ghost)
                            .label(language.choose("文档", "Document"))
                            .on_click(move |_, window, cx| {
                                gpui_base::TextSelection::clear(window, cx);
                                preview_tab.update(cx, |view, cx| {
                                    view.preview_show_data = false;
                                    view.stop_pdf_interaction();
                                    view.preview_scroll.set_offset(gpui::Point::default());
                                    #[cfg(windows)]
                                    crate::pdf_cursors::set_area(None);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        action_button("preview-tab-data")
                            .when(show_data, Button::primary)
                            .when(!show_data, Button::ghost)
                            .label(language.choose("数据", "Data"))
                            .on_click(move |_, window, cx| {
                                gpui_base::TextSelection::clear(window, cx);
                                data_tab.update(cx, |view, cx| {
                                    view.preview_show_data = true;
                                    view.stop_pdf_interaction();
                                    view.preview_scroll.set_offset(gpui::Point::default());
                                    #[cfg(windows)]
                                    crate::pdf_cursors::clear();
                                    cx.notify();
                                });
                            }),
                    ),
            )
        } else {
            None
        };

        let focus = self.preview_focus.as_ref().filter(|focus| {
            focus.project_id == preview.project_id && focus.document_id == preview.document_id
        });
        let target_page = match focus.map(|focus| &focus.anchor) {
            Some(FragmentAnchor::PageLine { page, .. }) if !show_data => Some(*page),
            _ => None,
        };
        // Items are direct children of the tracked scroll container, so a page can be
        // scrolled to by index.
        let mut items: Vec<(Option<u32>, AnyElement)> = Vec::new();
        if let Some(focus) = focus {
            items.push((None, self.render_preview_focus_banner(focus, preview, &entity)));
        }
        if show_data {
            let stream_log = self
                .datasheet_stream
                .as_ref()
                .filter(|(project_id, document_id, _)| {
                    *project_id == preview.project_id && *document_id == preview.document_id
                })
                .and_then(|(_, _, stream)| stream.lock().ok().map(|buffer| buffer.clone()));
            items.push((
                None,
                Self::render_datasheet_data(
                    preview,
                    language,
                    &entity,
                    self.datasheet_rows_visible,
                    self.datasheet_extracting,
                    self.datasheet_feedback.as_deref(),
                    stream_log,
                    self.datasheet_extract_started
                        .filter(|_| self.datasheet_extracting)
                        .map(|started| started.elapsed().as_secs()),
                    self.datasheet_checkpoint.as_ref().is_some_and(
                        |(project_id, document_id, _)| {
                            *project_id == preview.project_id && *document_id == preview.document_id
                        },
                    ),
                    focus.map(|focus| &focus.anchor),
                    self.preview_row_anchor.as_ref(),
                )
                .into_any_element(),
            ));
        } else {
            match &preview.state {
                DocumentPreviewState::Loading => {
                    items.push((
                        None,
                        div()
                            .p_3()
                            .child(language.choose("正在加载文档…", "Loading document…"))
                            .into_any_element(),
                    ));
                }
                DocumentPreviewState::Loaded { view, raster } => {
                    items.push((
                        None,
                        div()
                            .v_flex()
                            .gap_0p5()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(SURFACE_BG))
                            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                "{} · {}",
                                &view.content_hash[..view.content_hash.len().min(19)],
                                view.opener_id
                            )))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .whitespace_normal()
                                    .child(format!("source: {}", view.source_locator)),
                            )
                            .into_any_element(),
                    ));
                    items.extend(match raster {
                        Some(raster) => Self::render_document_raster_pages(
                            raster,
                            language,
                            pane_width,
                            self.preview_pdf_zoom.value,
                            focus,
                            &entity,
                            self.preview_pdf_pan.is_some(),
                        ),
                        None => Self::render_document_view_items(&view.body, language, focus),
                    });
                }
                DocumentPreviewState::Refused { denial } => {
                    items.push((None,
                    div()
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .bg(rgb(0x00fe_f2f2))
                        .text_color(rgb(0x00b9_1c1c))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(language.choose(
                                    "该文档未通过打开门，不能被打开或作为证据源。",
                                    "This document did not pass the open gate and cannot be opened or cited.",
                                )),
                        )
                        .child(
                            div().text_xs().whitespace_normal().child(denial.to_string()),
                        )
                        .into_any_element()));
                }
                DocumentPreviewState::Unavailable { reason } => {
                    items.push((None,
                    div()
                        .v_flex()
                        .gap_1()
                        .p_3()
                        .rounded_lg()
                        .bg(rgb(0x00ff_f7ed))
                        .text_color(rgb(0x00b4_5309))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(language.choose(
                                    "预览不可用：没有打开器支持该格式，或内容已损坏。",
                                    "Preview unavailable: no opener supports this format, or the content is corrupt.",
                                )),
                        )
                        .child(div().text_xs().whitespace_normal().child(reason.clone()))
                        .into_any_element()));
                }
            }
        }

        // Pages visible in the last layout (the scroll handle's child bounds), or the
        // cited page while landing on it; the pages around them get rendered below.
        let page_items: Vec<(usize, u32)> = items
            .iter()
            .enumerate()
            .filter_map(|(index, (number, _))| number.map(|number| (index, number)))
            .collect();
        let raster_shown = !show_data
            && matches!(preview.state, DocumentPreviewState::Loaded { raster: Some(_), .. });
        let visible_pages = raster_shown.then(|| {
            let landing = target_page.filter(|_| self.preview_scroll_pending.get());
            let (top, bottom) = (self.preview_scroll.top_item(), self.preview_scroll.bottom_item());
            let shown: Vec<u32> = page_items
                .iter()
                .filter(|(index, _)| (top..=bottom).contains(index))
                .map(|(_, number)| *number)
                .collect();
            landing
                .map(|page| (page, page))
                .or_else(|| Some((*shown.first()?, *shown.last()?)))
                .unwrap_or((1, 1))
        });

        // Land on the cited page once its content is laid out; `scroll_to_top_of_item`
        // waits for the child bounds of the next layout.
        if self.preview_scroll_pending.get() {
            let loading = matches!(preview.state, DocumentPreviewState::Loading);
            if show_data
                && preview.extraction.is_some()
                && let Some(anchor) = &self.preview_row_anchor
            {
                anchor.scroll_to(window, cx);
                self.preview_scroll_pending.set(false);
            } else if let Some(index) = target_page
                .and_then(|page| items.iter().position(|(number, _)| *number == Some(page)))
            {
                self.preview_scroll.scroll_to_top_of_item(index);
                self.preview_scroll_pending.set(false);
            } else if !loading {
                self.preview_scroll_pending.set(false);
            }
        }

        let zoom_anchor = self.preview_zoom_anchor.filter(|_| raster_shown);
        // Apply the final scale's anchor once, then allow scrollbar dragging normally.
        if self.preview_zoom_tick.is_none() {
            self.preview_zoom_anchor = None;
        }
        let probe = std::rc::Rc::new(std::cell::Cell::new(None));
        let scroll_content = div()
            .id("document-preview-scroll")
            .size_full()
            .overflow_y_scroll()
            .when(raster_shown, |scroll| scroll.overflow_x_scroll())
            .track_scroll(&self.preview_scroll)
            .v_flex()
            .gap_2()
            .p_3()
            .children(items.into_iter().enumerate().map(|(index, (_, item))| {
                let content = div()
                    .flex_none()
                    .min_w(px(0.))
                    .when(raster_shown, |item| {
                        item.w(px(
                            (pane_width - 34.0).max(240.0) * self.preview_pdf_zoom.value + 10.0
                        ))
                    })
                    .child(item)
                    .into_any_element();
                if zoom_anchor.is_some_and(|(target, _, _)| target == index) {
                    crate::pdf_zoom::native::mark_layout(content, probe.clone())
                } else {
                    content
                }
            }))
            .into_any_element();
        let scroll_content = if let Some((_, fraction, pointer)) = zoom_anchor {
            crate::pdf_zoom::native::anchor_viewport(
                scroll_content,
                probe,
                self.preview_scroll.clone(),
                fraction,
                pointer,
            )
        } else {
            scroll_content
        };
        let text_focus = self.preview_text_focus.clone();
        let pane = div()
            .track_focus(&self.preview_text_focus)
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.focus(&text_focus, cx);
            })
            .flex_none()
            .w(px(pane_width))
            .h_full()
            .v_flex()
            .bg(rgb(CARD_BG))
            .border_l_1()
            .border_color(rgb(BORDER))
            .child(header)
            .when_some(tabs, ParentElement::child)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .child(scroll_content)
                    .when(raster_shown, |area| {
                        let zoomer = entity.clone();
                        let generation = self.preview_focus_generation;
                        let zoom = self.preview_pdf_zoom.value;
                        let animate = self.preview_zoom_tick.is_some();
                        let grabbing = self.preview_pdf_pan.is_some();
                        area.child(
                            div().absolute().top(px(0.)).left(px(0.)).size_full().child(
                                gpui::canvas(
                                    |_, _, _| (),
                                    move |bounds, (), window, _| {
                                        if grabbing {
                                            window.set_window_cursor_style(
                                                crate::pdf_zoom::native::pan_cursor(true),
                                            );
                                        }
                                        // Grab/grabbing cursor state: the area is live
                                        // while this canvas paints; every mouse move
                                        // recomputes it so leaving the pane restores
                                        // the normal cursor.
                                        #[cfg(windows)]
                                        {
                                            let (x, y) =
                                                (f32::from(bounds.left()), f32::from(bounds.top()));
                                            let (width, height) = (
                                                f32::from(bounds.size.width),
                                                f32::from(bounds.size.height),
                                            );
                                            crate::pdf_cursors::set_area(Some((
                                                x, y, width, height,
                                            )));
                                            crate::pdf_cursors::set_dragging(grabbing);
                                            let pointer = window.mouse_position();
                                            crate::pdf_cursors::refresh((
                                                f32::from(pointer.x),
                                                f32::from(pointer.y),
                                            ));
                                            window.on_mouse_event(
                                                move |event: &gpui::MouseMoveEvent, phase, _, _| {
                                                    if phase == gpui::DispatchPhase::Capture {
                                                        crate::pdf_cursors::refresh((
                                                            f32::from(event.position.x),
                                                            f32::from(event.position.y),
                                                        ));
                                                    }
                                                },
                                            );
                                        }
                                        // The new page bounds are available after this layout.
                                        if animate {
                                            let anchored = zoomer.clone();
                                            window.on_next_frame(move |_, cx| {
                                                anchored.update(cx, |view, cx| {
                                                    if view.preview_focus_generation == generation
                                                        && view.preview_pdf_zoom.value == zoom
                                                        && !view.preview_show_data
                                                        && view.preview_zoom_tick.is_some()
                                                    {
                                                        let now = std::time::Instant::now();
                                                        let elapsed = view
                                                            .preview_zoom_tick
                                                            .take()
                                                            .unwrap()
                                                            .elapsed()
                                                            .as_secs_f32();
                                                        if view.preview_pdf_zoom.advance(elapsed) {
                                                            view.preview_zoom_tick = Some(now);
                                                        }
                                                        cx.notify();
                                                    }
                                                });
                                            });
                                        }
                                        let interrupter = zoomer.clone();
                                        window.on_mouse_event(
                                            move |event: &gpui::MouseDownEvent, phase, _, cx| {
                                                if phase == gpui::DispatchPhase::Capture
                                                    && bounds.contains(&event.position)
                                                {
                                                    interrupter.update(cx, |view, cx| {
                                                        if view.preview_zoom_tick.is_some() {
                                                            view.stop_pdf_interaction();
                                                            cx.notify();
                                                        }
                                                    });
                                                }
                                            },
                                        );
                                        let mover = zoomer.clone();
                                        window.on_mouse_event(
                                            move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                                if phase == gpui::DispatchPhase::Capture {
                                                    mover.update(cx, |view, cx| {
                                                        view.pan_pdf_preview(event, cx)
                                                    });
                                                }
                                            },
                                        );
                                        let releaser = zoomer.clone();
                                        window.on_mouse_event(
                                            move |event: &gpui::MouseUpEvent, phase, _, cx| {
                                                if phase == gpui::DispatchPhase::Capture
                                                    && event.button == gpui::MouseButton::Left
                                                {
                                                    releaser.update(cx, |view, cx| {
                                                        if view.preview_pdf_pan.take().is_some() {
                                                            cx.stop_propagation();
                                                            cx.notify();
                                                        }
                                                    });
                                                }
                                            },
                                        );
                                        window.on_mouse_event(
                                            move |event: &gpui::ScrollWheelEvent,
                                                  phase,
                                                  window,
                                                  cx| {
                                                if phase == gpui::DispatchPhase::Capture
                                                    && bounds.contains(&event.position)
                                                {
                                                    if event.modifiers.control {
                                                        cx.stop_propagation();
                                                        zoomer.update(cx, |view, cx| {
                                                            view.zoom_pdf_preview(event, window, cx)
                                                        });
                                                    } else {
                                                        zoomer.update(cx, |view, cx| {
                                                            let active = view
                                                                .preview_zoom_tick
                                                                .is_some()
                                                                || view.preview_pdf_pan.is_some();
                                                            view.stop_pdf_interaction();
                                                            if active {
                                                                cx.notify();
                                                            }
                                                        });
                                                    }
                                                }
                                            },
                                        );
                                    },
                                )
                                .size_full(),
                            ),
                        )
                    })
                    .when(self.preview_pdf_pan.is_some(), |area| {
                        area.cursor(crate::pdf_zoom::native::pan_cursor(true))
                    })
                    .when(focus.is_some_and(|focus| focus.resolving), |area| {
                        area.child(
                            // Translucent mask over the document area while the
                            // source location is being resolved.
                            div()
                                .absolute()
                                .top(px(0.))
                                .left(px(0.))
                                .size_full()
                                .bg(rgba(0xf8fafce0)),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(0.))
                                .left(px(0.))
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .px_3()
                                        .py_2()
                                        .rounded_lg()
                                        .border_1()
                                        .border_color(rgb(BORDER))
                                        .bg(rgb(CARD_BG))
                                        .shadow_sm()
                                        .child(status_dot(0x00e0_f2fe))
                                        .child(
                                            language.choose("正在定位原文…", "Locating source…"),
                                        ),
                                ),
                        )
                    })
                    .scrollbar(&self.preview_scroll, gpui_component::scroll::ScrollbarAxis::Both),
            )
            .into_any_element();
        if let Some(visible) = visible_pages {
            self.update_raster_window(visible, window, cx);
        }
        pane
    }

    pub(super) fn zoom_pdf_preview(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f32::from(event.delta.pixel_delta(px(30.0)).y);
        if !self.preview_pdf_zoom.scroll(delta) {
            return;
        }
        self.preview_pdf_pan = None;
        let offset = self.preview_scroll.offset();
        self.preview_zoom_anchor = (self.preview_scroll.top_item()
            ..=self.preview_scroll.bottom_item())
            .find_map(|index| {
                let item = self.preview_scroll.bounds_for_item(index)?;
                let y = event.position.y - item.top() - offset.y;
                if y < px(0.0) || y > item.size.height {
                    return None;
                }
                let fraction = gpui::point(
                    (f32::from(event.position.x - item.left() - offset.x)
                        / f32::from(item.size.width).max(1.0))
                    .clamp(0.0, 1.0),
                    (f32::from(y) / f32::from(item.size.height).max(1.0)).clamp(0.0, 1.0),
                );
                Some((index, fraction, event.position))
            });
        self.preview_zoom_tick.get_or_insert_with(std::time::Instant::now);
        if cx.reduce_motion() {
            self.preview_pdf_zoom.finish();
        }
        cx.notify();
    }

    pub(super) fn stop_pdf_interaction(&mut self) {
        self.preview_pdf_zoom.stop();
        self.preview_zoom_tick = None;
        self.preview_zoom_anchor = None;
        self.preview_pdf_pan = None;
    }

    /// Returns the PDF view to 100% with no pan or in-flight zoom animation. Every
    /// navigation (opening a document, jumping from a search hit or a data row's source
    /// link) starts from the fitted page so the landing position is predictable.
    pub(super) fn reset_pdf_view(&mut self) {
        self.preview_pdf_zoom = crate::pdf_zoom::ZoomMotion::default();
        self.preview_zoom_tick = None;
        self.preview_pdf_pan = None;
        self.preview_zoom_anchor = None;
        #[cfg(windows)]
        crate::pdf_cursors::set_dragging(false);
    }

    pub(super) fn begin_pdf_pan(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        // Let the window's text-selection layer own gestures starting on glyphs.
        // A drag which began on blank paper stays a pan even when it crosses text.
        if crate::pdf_text_layer::over_text(position) {
            return;
        }
        self.stop_pdf_interaction();
        self.preview_pdf_pan = Some((position, self.preview_scroll.offset()));
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn pan_pdf_preview(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((start, offset)) = self.preview_pdf_pan else { return };
        if !event.dragging() {
            self.preview_pdf_pan = None;
        } else {
            let max = self.preview_scroll.max_offset();
            let (x, y) = crate::pdf_zoom::pan_offset(
                (f32::from(offset.x), f32::from(offset.y)),
                (f32::from(start.x), f32::from(start.y)),
                (f32::from(event.position.x), f32::from(event.position.y)),
                (f32::from(max.x), f32::from(max.y)),
            );
            self.preview_scroll.set_offset(gpui::point(px(x), px(y)));
            cx.stop_propagation();
        }
        cx.notify();
    }

    /// The cited line a search hit opened this preview for, with its extracted row when
    /// it is verified data; the banner stays on top while the content scrolls to it.
    pub(super) fn render_preview_focus_banner(
        &self,
        focus: &PreviewFocus,
        preview: &DocumentPreviewSelection,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let language = self.language;
        let closer = entity.clone();
        let row_detail = match (&focus.anchor, &preview.extraction) {
            (FragmentAnchor::DatasheetRow { section, row }, Some(extraction)) => {
                let index = row.saturating_sub(1);
                let parameter = |rows: &[circuitfabric_contracts::DatasheetParameter]| {
                    rows.get(index).map(|row| {
                        let values = [("min", &row.min), ("typ", &row.typ), ("max", &row.max)]
                            .into_iter()
                            .filter_map(|(label, value)| {
                                value.as_ref().map(|value| format!("{label} {value}"))
                            })
                            .collect::<Vec<_>>()
                            .join(" / ");
                        [
                            Some(row.parameter.clone()),
                            row.symbol.clone(),
                            (!values.is_empty()).then_some(values),
                            row.unit.clone(),
                            row.conditions.as_ref().map(|conditions| format!("({conditions})")),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ")
                    })
                };
                match section.as_str() {
                    "pins" => extraction.pins.get(index).map(|pin| {
                        format!(
                            "{} {} · {} · {}",
                            pin.number,
                            pin.name,
                            pin.kind.as_str(),
                            pin.description
                        )
                    }),
                    "absoluteMaximumRatings" => parameter(&extraction.absolute_maximum_ratings),
                    "electricalCharacteristics" => {
                        parameter(&extraction.electrical_characteristics)
                    }
                    "operatingConditions" => parameter(&extraction.operating_conditions),
                    _ => None,
                }
            }
            _ => None,
        };
        let unrendered_page = match (&focus.anchor, &preview.state) {
            (
                FragmentAnchor::PageLine { page, .. },
                DocumentPreviewState::Loaded { raster: Some(raster), .. },
            ) if !self.preview_show_data && *page as usize > raster.page_sizes.len() => {
                Some(language.choose_owned(
                    format!("文档只有 {} 页，第 {page} 页不存在。", raster.page_sizes.len()),
                    format!(
                        "The document has {} pages; page {page} does not exist.",
                        raster.page_sizes.len()
                    ),
                ))
            }
            _ => None,
        };
        div()
            .v_flex()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(ACCENT_SOFT))
            .bg(rgb(FOCUSED_ROW_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x000e_7490))
                            .child(language.choose_owned(
                                format!(
                                    "原文与数据定位 · {}",
                                    Self::evidence_anchor_label(&focus.anchor, language)
                                ),
                                format!(
                                    "Source / data location · {}",
                                    Self::evidence_anchor_label(&focus.anchor, language)
                                ),
                            )),
                    )
                    .child(
                        action_button("clear-preview-focus")
                            .ghost()
                            .ml_auto()
                            .label(language.choose("清除", "Clear"))
                            .on_click(move |_, _, cx| {
                                closer.update(cx, |view, cx| {
                                    view.preview_focus = None;
                                    view.preview_focus_generation += 1;
                                    view.preview_row_anchor = None;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT_PRIMARY))
                    .whitespace_normal()
                    .child(focus.text.clone()),
            )
            .when_some(row_detail, |banner, detail| {
                banner.child(
                    div().text_xs().text_color(rgb(TEXT_SECONDARY)).whitespace_normal().child(
                        language.choose_owned(
                            format!("提取的行：{detail}（表中已高亮）"),
                            format!("Extracted row: {detail} (highlighted below)"),
                        ),
                    ),
                )
            })
            .when_some(unrendered_page, |banner, note| {
                banner.child(div().text_xs().text_color(rgb(0x00b4_5309)).child(note))
            })
            .when_some(focus.notice.clone(), |banner, note| {
                banner.child(
                    div().text_xs().whitespace_normal().text_color(rgb(TEXT_MUTED)).child(note),
                )
            })
            .into_any_element()
    }

    /// The structured datasheet tab: the persisted extraction, or the action to create
    /// one. Extraction always runs through the open gate, so only verified bytes are
    /// parsed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_datasheet_data(
        preview: &DocumentPreviewSelection,
        language: UiLanguage,
        entity: &Entity<Self>,
        visible_rows: usize,
        extracting: bool,
        feedback: Option<&str>,
        stream_log: Option<String>,
        elapsed_seconds: Option<u64>,
        can_resume: bool,
        focus: Option<&FragmentAnchor>,
        row_anchor: Option<&gpui::ScrollAnchor>,
    ) -> Div {
        let focused_row = |wanted: &str| match focus {
            Some(FragmentAnchor::DatasheetRow { section, row }) if section == wanted => Some(*row),
            _ => None,
        };
        let extractor = entity.clone();
        let stopper = entity.clone();
        let resumer = entity.clone();
        let clear_existing = preview.extraction.is_some();
        let extract_button = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .when(extracting, |row| {
                row.child(
                    action_button("stop-datasheet-extraction")
                        .danger()
                        .label(language.choose("停止", "Stop"))
                        .on_click(move |_, _, cx| {
                            stopper.update(cx, Self::stop_datasheet_extraction);
                        }),
                )
            })
            .when(!extracting && can_resume, |row| {
                row.child(
                    action_button("resume-datasheet-extraction")
                        .primary()
                        .label(language.choose("继续", "Continue"))
                        .on_click(move |_, window, cx| {
                            resumer.update(cx, |view, cx| {
                                view.extract_datasheet_for_preview(
                                    clear_existing,
                                    true,
                                    window,
                                    cx,
                                );
                            });
                        }),
                )
            })
            .child(
                action_button("extract-datasheet-data")
                    .when(!clear_existing, Button::primary)
                    .when(clear_existing, Button::danger)
                    .disabled(extracting)
                    .label(if extracting {
                        language.choose("正在提取…", "Extracting…")
                    } else if clear_existing {
                        language.choose("清空并重新提取", "Clear and re-extract")
                    } else {
                        language.choose("提取结构化数据", "Extract structured data")
                    })
                    .on_click(move |_, window, cx| {
                        extractor.update(cx, |view, cx| {
                            view.extract_datasheet_for_preview(clear_existing, false, window, cx);
                        });
                    }),
            );

        let feedback_note =
            feedback.map(|message| Self::render_preview_truncation_note(message.to_owned()));
        let stream_window =
            stream_log.map(|log| Self::render_datasheet_stream(&log, elapsed_seconds, language));
        let Some(extraction) = &preview.extraction else {
            return div().v_flex().gap_3().p_3().children(feedback_note).children(stream_window).child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(language.choose(
                                "尚无结构化数据",
                                "No structured data yet",
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .whitespace_normal()
                            .child(language.choose(
                                "从该数据手册中提取器件标识、引脚表与关键参数表，结构化保存在项目内。",
                                "Extract part identity, the pin table, and key parameter tables from this datasheet, stored structurally in the project.",
                            )),
                    )
                    .child(extract_button),
            );
        };

        // A stale extraction (content changed since it was made) is shown, never hidden,
        // together with the reminder to re-extract.
        let stale = match &preview.state {
            DocumentPreviewState::Loaded { view, .. } => {
                view.content_hash != extraction.content_hash
            }
            _ => false,
        };

        let mut content = div().v_flex().gap_2();
        content = content.children(feedback_note).children(stream_window);
        if !extraction.notes.iter().any(|note| note.starts_with("Jev accepted ")) {
            content = content.child(Self::render_preview_truncation_note(
                language.choose(
                    "这是旧版提取结果，没有 Jev evaluate 验证记录；请清空并重新提取。",
                    "This extraction has no Jev evaluate verification record; clear and re-extract.",
                ).to_owned(),
            ));
        }
        if stale {
            content = content.child(Self::render_preview_truncation_note(
                language
                    .choose(
                        "文档内容在提取后发生变化，以下数据可能过期，建议重新提取。",
                        "The document changed after extraction; the data below may be stale — re-extract.",
                    )
                    .to_owned(),
            ));
        }

        // Overview card.
        let overview = &extraction.overview;
        let mut overview_card =
            div().v_flex().gap_1().p_3().rounded_lg().bg(rgb(SURFACE_BG)).child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_normal()
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(gpui_base::SelectableText::new(
                        "datasheet-title",
                        overview.title.clone(),
                    )),
            );
        let identity_line = [
            overview.manufacturer.clone(),
            (!overview.part_numbers.is_empty()).then(|| overview.part_numbers.join(" / ")),
            (!overview.packages.is_empty()).then(|| overview.packages.join(", ")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        if !identity_line.is_empty() {
            overview_card = overview_card.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .whitespace_normal()
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(gpui_base::SelectableText::new("datasheet-identity", identity_line)),
            );
        }
        if !overview.features.is_empty() {
            overview_card = overview_card.child(
                div()
                    .v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x000e_7490))
                            .child(language.choose("特性", "Features")),
                    )
                    .children(overview.features.iter().enumerate().map(|(index, feature)| {
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_PRIMARY))
                            .whitespace_normal()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                ("datasheet-feature", index),
                                format!("• {feature}"),
                            ))
                    })),
            );
        }
        if !overview.description.is_empty() {
            overview_card = overview_card.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_SECONDARY))
                    .whitespace_normal()
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(gpui_base::SelectableText::new(
                        "datasheet-description",
                        overview.description.clone(),
                    )),
            );
        }
        content = content.child(overview_card);

        content = content.child(Self::render_datasheet_pin_table(
            &extraction.pins,
            language,
            visible_rows,
            focused_row("pins"),
            row_anchor,
            entity,
        ));
        content = content.child(Self::render_datasheet_parameter_table(
            language.choose("绝对最大额定值", "Absolute Maximum Ratings"),
            &extraction.absolute_maximum_ratings,
            language,
            visible_rows,
            focused_row("absoluteMaximumRatings"),
            row_anchor,
            entity,
            "absoluteMaximumRatings",
            0x00fe_f2f2,
            0x00b9_1c1c,
        ));
        content = content.child(Self::render_datasheet_parameter_table(
            language.choose("电特性", "Electrical Characteristics"),
            &extraction.electrical_characteristics,
            language,
            visible_rows,
            focused_row("electricalCharacteristics"),
            row_anchor,
            entity,
            "electricalCharacteristics",
            0x00e0_f2fe,
            0x000e_7490,
        ));
        content = content.child(Self::render_datasheet_parameter_table(
            language.choose("工作条件", "Operating Conditions"),
            &extraction.operating_conditions,
            language,
            visible_rows,
            focused_row("operatingConditions"),
            row_anchor,
            entity,
            "operatingConditions",
            0x00f3_e8ff,
            0x0076_2ba3,
        ));

        if [
            extraction.pins.len(),
            extraction.absolute_maximum_ratings.len(),
            extraction.electrical_characteristics.len(),
            extraction.operating_conditions.len(),
        ]
        .into_iter()
        .any(|count| count > visible_rows)
        {
            let loader = entity.clone();
            content = content.child(
                action_button("show-more-datasheet-rows")
                    .label(language.choose("显示更多行", "Show more rows"))
                    .on_click(move |_, _, cx| {
                        loader.update(cx, |view, cx| {
                            view.datasheet_rows_visible =
                                view.datasheet_rows_visible.saturating_add(40);
                            cx.notify();
                        });
                    }),
            );
        }

        if !extraction.notes.is_empty() {
            content = content.child(
                div().v_flex().gap_0p5().p_2().rounded_md().bg(rgb(SURFACE_BG)).children(
                    extraction.notes.iter().enumerate().map(|(index, note)| {
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .whitespace_normal()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                ("datasheet-note", index),
                                format!("· {note}"),
                            ))
                    }),
                ),
            );
        }
        content.child(extract_button)
    }

    /// The pin table: number, name, classified kind, and description.
    pub(super) fn render_datasheet_pin_table(
        pins: &[circuitfabric_contracts::DatasheetPin],
        language: UiLanguage,
        visible_rows: usize,
        focused_row: Option<usize>,
        row_anchor: Option<&gpui::ScrollAnchor>,
        entity: &Entity<Self>,
    ) -> gpui::Stateful<Div> {
        let mut table =
            div().id("datasheet-pins").v_flex().gap_1().child(
                div().text_sm().font_weight(FontWeight::SEMIBOLD).child(language.choose_owned(
                    format!("引脚（{}）", pins.len()),
                    format!("Pins ({})", pins.len()),
                )),
            );
        if pins.is_empty() {
            return table.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose("未解析到引脚行。", "No pin rows parsed.")),
            );
        }
        let header_row = |label: &str, flex: f32| {
            div()
                .flex_none()
                .w(px(flex))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_MUTED))
                .cursor(gpui::CursorStyle::IBeam)
                .child(gpui_base::SelectableText::new(
                    gpui::SharedString::from(format!("pin-header-{label}")),
                    label.to_owned(),
                ))
        };
        table = table.child(
            div()
                .flex()
                .gap_1()
                .p_1()
                .rounded_sm()
                .bg(rgb(SURFACE_BG))
                .child(header_row(language.choose("引脚", "Pin"), 34.0))
                .child(header_row(language.choose("名称", "Name"), 76.0))
                .child(header_row(language.choose("类型", "Kind"), 52.0))
                .child(header_row(language.choose("说明", "Description"), 0.0))
                .child(div().flex_1()),
        );
        for (index, pin) in pins.iter().enumerate().take(visible_rows) {
            table = table.child(
                div()
                    .id(("datasheet-pin-row", index))
                    .anchor_scroll(
                        (focused_row == Some(index + 1)).then(|| row_anchor.cloned()).flatten(),
                    )
                    .flex()
                    .gap_1()
                    .when(focused_row == Some(index + 1), |row| {
                        row.rounded_sm().bg(rgb(FOCUSED_ROW_BG))
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(34.0))
                            .text_xs()
                            .text_color(rgb(TEXT_PRIMARY))
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new("number", pin.number.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(76.0))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT_PRIMARY))
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new("name", pin.name.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(52.0))
                            .text_xs()
                            .text_color(rgb(0x000e_7490))
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                "kind",
                                pin.kind.as_str().to_owned(),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .whitespace_normal()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                "description",
                                pin.description.clone(),
                            )),
                    )
                    .child(Self::datasheet_source_link(
                        entity,
                        "pins",
                        index,
                        pin.evidence.is_some(),
                        language,
                    )),
            );
        }
        table
    }

    /// One parameter table with min/typ/max/unit columns.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_datasheet_parameter_table(
        title: &str,
        parameters: &[circuitfabric_contracts::DatasheetParameter],
        language: UiLanguage,
        visible_rows: usize,
        focused_row: Option<usize>,
        row_anchor: Option<&gpui::ScrollAnchor>,
        entity: &Entity<Self>,
        section: &'static str,
        accent: u32,
        accent_text: u32,
    ) -> gpui::Stateful<Div> {
        let mut table = div().id(section).v_flex().gap_1().child(
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
                        .flex_none()
                        .bg(rgb(accent))
                        .text_color(rgb(accent_text))
                        .cursor(gpui::CursorStyle::IBeam)
                        .child(gpui_base::SelectableText::new("section-title", title.to_owned())),
                )
                .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    language.choose_owned(
                        format!("{} 行", parameters.len()),
                        format!("{} rows", parameters.len()),
                    ),
                )),
        );
        if parameters.is_empty() {
            return table.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose("未解析到参数行。", "No parameter rows parsed.")),
            );
        }
        let header_row = |label: &str, flex: f32| {
            div()
                .flex_none()
                .w(px(flex))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_MUTED))
                .cursor(gpui::CursorStyle::IBeam)
                .child(gpui_base::SelectableText::new(
                    gpui::SharedString::from(format!("parameter-header-{label}")),
                    label.to_owned(),
                ))
        };
        table = table.child(
            div()
                .flex()
                .gap_1()
                .p_1()
                .rounded_sm()
                .bg(rgb(SURFACE_BG))
                .child(header_row(language.choose("参数", "Parameter"), 0.0))
                .child(div().flex_1())
                .child(header_row(language.choose("符号", "Symbol"), 52.0))
                .child(header_row(language.choose("最小", "Min"), 46.0))
                .child(header_row(language.choose("典型", "Typ"), 46.0))
                .child(header_row(language.choose("最大", "Max"), 46.0))
                .child(header_row(language.choose("单位", "Unit"), 34.0)),
        );
        for (index, parameter) in parameters.iter().enumerate().take(visible_rows) {
            let cell = |id: &'static str, value: &Option<String>, emphasized: bool| {
                div()
                    .flex_none()
                    .w(px(if emphasized { 46.0 } else { 0.0 }))
                    .text_xs()
                    .text_color(rgb(TEXT_PRIMARY))
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(gpui_base::SelectableText::new(
                        id,
                        value.clone().unwrap_or_else(|| "—".to_owned()),
                    ))
            };
            table = table.child(
                div()
                    .id((section, index))
                    .anchor_scroll(
                        (focused_row == Some(index + 1)).then(|| row_anchor.cloned()).flatten(),
                    )
                    .flex()
                    .gap_1()
                    .when(focused_row == Some(index + 1), |row| {
                        row.rounded_sm().bg(rgb(FOCUSED_ROW_BG))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_xs()
                            .text_color(rgb(TEXT_PRIMARY))
                            .whitespace_normal()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                "parameter",
                                parameter.parameter.clone(),
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(52.0))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x000e_7490))
                            .whitespace_normal()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                "symbol",
                                parameter.symbol.clone().unwrap_or_else(|| "—".to_owned()),
                            )),
                    )
                    .child(cell("min", &parameter.min, true))
                    .child(cell("typ", &parameter.typ, true))
                    .child(cell("max", &parameter.max, true))
                    .child(
                        div()
                            .flex_none()
                            .w(px(34.0))
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .whitespace_normal()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(gpui_base::SelectableText::new(
                                "unit",
                                parameter.unit.clone().unwrap_or_else(|| "—".to_owned()),
                            )),
                    )
                    .when_some(parameter.conditions.clone(), |row, conditions| {
                        row.child(
                            div()
                                .flex_none()
                                .max_w(px(120.0))
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .whitespace_normal()
                                .cursor(gpui::CursorStyle::IBeam)
                                .child(gpui_base::SelectableText::new(
                                    "conditions",
                                    format!("({conditions})"),
                                )),
                        )
                    })
                    .child(Self::datasheet_source_link(
                        entity,
                        section,
                        index,
                        parameter.evidence.is_some(),
                        language,
                    )),
            );
        }
        table
    }

    /// The per-row link to the source PDF: a single icon on the row itself, with the
    /// action explained by a hover tooltip instead of a wide text label.
    pub(super) fn datasheet_source_link(
        entity: &Entity<Self>,
        section: &'static str,
        row: usize,
        has_evidence: bool,
        language: UiLanguage,
    ) -> impl IntoElement {
        let opener = entity.clone();
        action_button(("datasheet-source", row))
            .ghost()
            .compact()
            .icon(IconName::ExternalLink)
            .tooltip(if has_evidence {
                language.choose("查看原文", "View source").to_owned()
            } else {
                language.choose("未记录原文证据", "No source evidence").to_owned()
            })
            .disabled(!has_evidence)
            .on_click(move |_, _, cx| {
                opener.update(cx, |view, cx| view.open_datasheet_source(section, row, cx));
            })
    }

    /// Displays the PDF as a scrollable, reader-like list with a slot for every page:
    /// rendered pages show their bitmap, the rest a same-sized placeholder until they
    /// are rendered on demand. Pages are separate items tagged with their number so the
    /// preview's scroll container can land on one and report which are visible.
    // Page sizes and bitmap dimensions are small positive values.
    #[allow(clippy::cast_precision_loss)]
    pub(super) fn render_document_raster_pages(
        raster: &DocumentRasterPreview,
        language: UiLanguage,
        pane_width: f32,
        zoom: f32,
        focus: Option<&PreviewFocus>,
        entity: &Entity<Self>,
        grabbing: bool,
    ) -> Vec<(Option<u32>, AnyElement)> {
        crate::pdf_text_layer::begin_frame();
        // Page bitmaps follow the divider: pane width minus the body padding, the page
        // card's own padding, and its border.
        let bitmap_width = (pane_width - 34.0).max(240.0) * zoom;
        let page_count = raster.page_sizes.len();
        let mut content = vec![(
            None,
            div()
                .text_xs()
                .text_color(rgb(TEXT_MUTED))
                .child(language.choose_owned(
                    format!(
                        "共 {page_count} 页 · {:.0}% · Ctrl＋滚轮缩放，空白处拖动，文字处选择并 Ctrl+C 复制",
                        zoom * 100.0
                    ),
                    format!(
                        "{page_count} pages · {:.0}% · Ctrl + wheel to zoom; drag blank paper to pan; select text and Ctrl+C to copy",
                        zoom * 100.0
                    ),
                ))
                .into_any_element(),
        )];
        for (index, (width_points, height_points)) in raster.page_sizes.iter().enumerate() {
            let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
            let rendered = raster.pages.get(&number);
            let display_height = bitmap_width * height_points / width_points;
            let grabber = entity.clone();
            let frame = div()
                .id(("pdf-page-frame", number))
                .cursor(crate::pdf_zoom::native::pan_cursor(grabbing))
                .on_mouse_down(gpui::MouseButton::Left, move |event, _, cx| {
                    grabber.update(cx, |view, cx| view.begin_pdf_pan(event.position, cx));
                })
                .p_1()
                .rounded_sm()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE_BG));
            let frame = match rendered {
                Some(page) => frame.child(
                    div().relative().w(px(bitmap_width)).h(px(display_height))
                        .child(img(page.image.clone()).w(px(bitmap_width)).h(px(display_height)))
                        .children(focus.filter(|focus| matches!(focus.anchor, FragmentAnchor::PageLine { page, .. } if page == number))
                            .into_iter().flat_map(|focus| &focus.regions).map(|region| {
                                div().absolute()
                                    .left(px(region.left * bitmap_width)).top(px(region.top * display_height))
                                    .w(px(region.width * bitmap_width)).h(px(region.height * display_height))
                                    .bg(rgba(0xffd8_3d66)).border_1().border_color(rgba(0xe0a0_0090))
                            }))
                        .when_some(raster.text_pages.get(&number), |page, text| {
                            // Namespace retained selection state by the managed copy so
                            // another document never inherits the old selection.
                            page.child(div().absolute().top(px(0.)).left(px(0.)).child(
                                crate::pdf_text_layer::PdfTextLayer::new(
                                    gpui::SharedString::from(format!("pdf-text-{}-{number}", raster.content_hash)),
                                    text.clone(), bitmap_width, display_height, grabbing,
                                ),
                            ))
                        }),
                ),
                None => frame.child(
                    div()
                        .w(px(bitmap_width))
                        .h(px(display_height))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(CARD_BG))
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(if raster.failed.iter().any(|(page, _)| *page == number) {
                            language.choose("该页无法渲染", "This page could not be rendered")
                        } else {
                            language.choose("正在渲染…", "Rendering…")
                        })
                        .relative()
                        .when_some(raster.text_pages.get(&number), |page, text| {
                            // Retain selection participants when their bitmap is evicted.
                            page.child(div().absolute().top(px(0.)).left(px(0.)).child(
                                crate::pdf_text_layer::PdfTextLayer::new(
                                    gpui::SharedString::from(format!("pdf-text-{}-{number}", raster.content_hash)),
                                    text.clone(), bitmap_width, display_height, grabbing,
                                ),
                            ))
                        }),
                ),
            };
            content.push((
                Some(number),
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x000e_7490))
                            .child(language.choose_owned(
                                format!("第 {number} / {page_count} 页"),
                                format!("Page {number} / {page_count}"),
                            )),
                    )
                    .child(frame)
                    .into_any_element(),
            ));
        }
        content
    }

    /// A view body as preview items; pages of paged text are tagged with their number
    /// so the preview's scroll container can land on one.
    pub(super) fn render_document_view_items(
        view_body: &DocumentViewBody,
        language: UiLanguage,
        focus: Option<&PreviewFocus>,
    ) -> Vec<(Option<u32>, AnyElement)> {
        let DocumentViewBody::PagedText { pages, truncated } = view_body else {
            return vec![(
                None,
                Self::render_document_view_body(view_body, language).into_any_element(),
            )];
        };
        let mut content: Vec<(Option<u32>, AnyElement)> = Vec::new();
        if *truncated {
            content.push((
                None,
                Self::render_preview_truncation_note(
                    language
                        .choose(
                            "页数较多，仅显示前 {first} 页。",
                            "Long document; showing the first {first} pages.",
                        )
                        .replace("{first}", &pages.len().to_string()),
                )
                .into_any_element(),
            ));
        }
        if pages.len() > PREVIEW_MAX_RENDERED_PAGES {
            content.push((
                None,
                Self::render_preview_truncation_note(
                    language
                        .choose(
                            "为保持界面流畅，预览面板仅渲染前 {first} 页。",
                            "For smooth scrolling the pane renders the first {first} pages.",
                        )
                        .replace("{first}", &PREVIEW_MAX_RENDERED_PAGES.to_string()),
                )
                .into_any_element(),
            ));
        }
        for page in pages.iter().enumerate().filter_map(|(index, page)| {
            (index < PREVIEW_MAX_RENDERED_PAGES || focus.is_some_and(|focus|
                matches!(focus.anchor, FragmentAnchor::PageLine { page: number, .. } if number == page.number)
            )).then_some(page)
        }) {
            let ranges = focus.filter(|focus| matches!(focus.anchor, FragmentAnchor::PageLine { page: number, .. } if number == page.number))
                .map(|focus| circuitfabric_document_opener::pdf_text_highlight_ranges(&page.text, &focus.terms))
                .unwrap_or_default();
            content.push((
                Some(page.number),
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x000e_7490))
                            .child(language.choose_owned(
                                format!("第 {} 页", page.number),
                                format!("Page {}", page.number),
                            )),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(SURFACE_BG))
                            .text_sm()
                            .text_color(rgb(TEXT_PRIMARY))
                            .whitespace_normal()
                            .child(Self::render_highlighted_text(&page.text, &ranges)),
                    )
                    .into_any_element(),
            ));
        }
        content
    }

    /// Renders one opener view body as read-only embeddable content.
    pub(super) fn render_document_view_body(
        view_body: &DocumentViewBody,
        language: UiLanguage,
    ) -> Div {
        let mut content = div().v_flex().gap_2();
        match view_body {
            // Rasterized views are rendered by `render_document_raster_pages`, and paged
            // text becomes separate page items in `render_document_view_items`, before
            // this function is reached; this arm keeps the match exhaustive.
            DocumentViewBody::RasterPages { .. } | DocumentViewBody::PagedText { .. } => {}
            DocumentViewBody::Blocks { blocks, truncated } => {
                if *truncated {
                    content = content.child(Self::render_preview_truncation_note(
                        language
                            .choose(
                                "内容较多，仅显示前 {first} 个块。",
                                "Long document; showing the first {first} blocks.",
                            )
                            .replace("{first}", &blocks.len().to_string()),
                    ));
                }
                if blocks.len() > PREVIEW_MAX_RENDERED_BLOCKS {
                    content = content.child(Self::render_preview_truncation_note(
                        language
                            .choose(
                                "为保持界面流畅，预览面板仅渲染前 {first} 个块。",
                                "For smooth scrolling the pane renders the first {first} blocks.",
                            )
                            .replace("{first}", &PREVIEW_MAX_RENDERED_BLOCKS.to_string()),
                    ));
                }
                for (index, block) in blocks.iter().take(PREVIEW_MAX_RENDERED_BLOCKS).enumerate() {
                    content = content.child(Self::render_document_block(block, index));
                }
            }
            DocumentViewBody::Sheets { sheets } => {
                for sheet in sheets {
                    let mut sheet_block = div().v_flex().gap_1().child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(sheet.name.clone()),
                            )
                            .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                language.choose_owned(
                                    format!("{} 行", sheet.row_count),
                                    format!("{} rows", sheet.row_count),
                                ),
                            )),
                    );
                    if sheet.truncated {
                        sheet_block = sheet_block.child(Self::render_preview_truncation_note(
                            language
                                .choose(
                                    "工作表较大，仅显示前 {first} 行。",
                                    "Large sheet; showing the first {first} rows.",
                                )
                                .replace("{first}", &sheet.rows.len().to_string()),
                        ));
                    }
                    if sheet.rows.len() > PREVIEW_MAX_RENDERED_ROWS {
                        sheet_block = sheet_block.child(Self::render_preview_truncation_note(
                            language
                                .choose(
                                    "为保持界面流畅，预览面板仅渲染前 {first} 行。",
                                    "For smooth scrolling the pane renders the first {first} rows.",
                                )
                                .replace("{first}", &PREVIEW_MAX_RENDERED_ROWS.to_string()),
                        ));
                    }
                    for (row_index, row) in
                        sheet.rows.iter().enumerate().take(PREVIEW_MAX_RENDERED_ROWS)
                    {
                        let mut row_element = div().flex().gap_1();
                        if row_index == 0 {
                            row_element = row_element
                                .p_1()
                                .rounded_sm()
                                .bg(rgb(SURFACE_BG))
                                .font_weight(FontWeight::MEDIUM);
                        }
                        for cell in row {
                            let label = match cell {
                                circuitfabric_plugin_api::DocumentCell::Empty => String::new(),
                                circuitfabric_plugin_api::DocumentCell::Text(text) => text.clone(),
                                circuitfabric_plugin_api::DocumentCell::Number(value) => {
                                    Self::format_number_cell(*value)
                                }
                                circuitfabric_plugin_api::DocumentCell::Boolean(value) => {
                                    value.to_string()
                                }
                            };
                            row_element = row_element.child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_xs()
                                    .whitespace_normal()
                                    .child(label),
                            );
                        }
                        sheet_block = sheet_block.child(row_element);
                    }
                    content = content.child(sheet_block);
                }
            }
            DocumentViewBody::PlainText { text } => {
                for line in text.lines() {
                    content = content.child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_PRIMARY))
                            .whitespace_normal()
                            .child(line.to_owned()),
                    );
                }
            }
        }
        content
    }

    pub(super) fn render_document_block(
        block: &circuitfabric_plugin_api::DocumentBlock,
        index: usize,
    ) -> Div {
        let inline = Self::render_inline_spans(&block.spans, &block.text, index);
        let block = match &block.kind {
            DocumentBlockKind::Heading { level } => {
                let heading = if *level <= 2 {
                    div().text_lg()
                } else if *level <= 4 {
                    div().text_base()
                } else {
                    div().text_sm()
                };
                heading
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_PRIMARY))
                    .child(inline)
            }
            DocumentBlockKind::Paragraph => {
                div().text_sm().text_color(rgb(TEXT_PRIMARY)).child(inline)
            }
            DocumentBlockKind::Code { .. } => div()
                .text_xs()
                .p_2()
                .rounded_md()
                .bg(rgb(SURFACE_BG))
                .text_color(rgb(TEXT_PRIMARY))
                .child(inline),
            DocumentBlockKind::ListItem { depth } => div()
                .flex()
                .gap_1p5()
                .ml(px(f32::from(*depth) * 14.))
                .child(div().text_sm().text_color(rgb(0x000e_7490)).child("•"))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_sm()
                        .text_color(rgb(TEXT_PRIMARY))
                        .child(inline),
                ),
            DocumentBlockKind::Quote => div()
                .border_l_2()
                .border_color(rgb(ACCENT_SOFT))
                .pl_2()
                .text_sm()
                .text_color(rgb(TEXT_SECONDARY))
                .child(inline),
        };
        block.cursor(gpui::CursorStyle::IBeam)
    }

    /// One block's inline content: a single styled-text element with per-span highlights
    /// when the opener provided formatting runs, otherwise the plain text. Keeping it one
    /// text element means words wrap normally across style changes.
    pub(super) fn render_inline_spans(
        spans: &[circuitfabric_plugin_api::DocumentSpan],
        fallback: &str,
        index: usize,
    ) -> AnyElement {
        let mut text = String::new();
        let mut highlights = Vec::new();
        for span in spans {
            let start = text.len();
            text.push_str(&span.text);
            let end = text.len();
            let style = match span.style {
                DocumentSpanStyle::Plain => continue,
                DocumentSpanStyle::Strong => HighlightStyle {
                    font_weight: Some(FontWeight::BOLD),
                    ..HighlightStyle::default()
                },
                DocumentSpanStyle::Emphasis => HighlightStyle {
                    font_style: Some(FontStyle::Italic),
                    ..HighlightStyle::default()
                },
                DocumentSpanStyle::Code => HighlightStyle {
                    background_color: Some(rgb(SURFACE_BG).into()),
                    ..HighlightStyle::default()
                },
                DocumentSpanStyle::Strikethrough => HighlightStyle {
                    strikethrough: Some(StrikethroughStyle::default()),
                    ..HighlightStyle::default()
                },
            };
            highlights.push((start..end, style));
        }
        if spans.is_empty() {
            text.push_str(fallback);
        }
        super::selectable_text::SelectableStyledText::new(
            ("document-block-text", index),
            text,
            highlights,
            index as u64,
        )
        .into_any_element()
    }

    pub(super) fn render_preview_truncation_note(message: String) -> Div {
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(rgb(0x00ff_f7ed))
            .text_xs()
            .text_color(rgb(0x00b4_5309))
            .child(message)
    }

    /// The live extraction log: stage lines plus the model reply as it streams. Only the
    /// tail is rendered, bottom-aligned, so the newest output stays in view.
    pub(super) fn render_datasheet_stream(
        log: &str,
        elapsed_seconds: Option<u64>,
        language: UiLanguage,
    ) -> Div {
        const TAIL_CHARS: usize = 2000;
        let skip = log.chars().count().saturating_sub(TAIL_CHARS);
        let tail: String = log.chars().skip(skip).collect();
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
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_SECONDARY))
                    .child(match elapsed_seconds {
                        Some(seconds) => language.choose_owned(
                            format!("模型输出（进行中… {seconds}s）"),
                            format!("Model output (running… {seconds}s)"),
                        ),
                        None => language
                            .choose("模型输出（已结束）", "Model output (finished)")
                            .to_owned(),
                    }),
            )
            .child(
                div()
                    .v_flex()
                    .justify_end()
                    .h(px(180.))
                    .overflow_hidden()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .whitespace_normal()
                    .child(tail),
            )
    }
}
