//! Evidence presentation and event handlers.
use super::*;

impl ControlPlaneView {
    /// Extracts the full text of this project's not-yet-indexed PDFs off the UI thread
    /// and registers it as evidence. Each document is attempted once per session.
    pub(super) fn schedule_pdf_indexing(&mut self, project_id: &str, cx: &mut Context<Self>) {
        let Some(storage) = self.project_storages.get(project_id).cloned() else {
            return;
        };
        let Ok(pending) = self.workspace.pending_pdf_documents(project_id, &storage) else {
            return;
        };
        for document in pending {
            let key = (project_id.to_owned(), document.id.clone());
            if self.pdf_index_state.contains_key(&key) {
                continue;
            }
            self.pdf_index_state.insert(key.clone(), PdfIndexState::Running);
            let work = cx.background_spawn({
                let storage = storage.clone();
                let document = document.clone();
                async move {
                    let pages = circuitfabric_project::extract_pdf_pages(&storage, &document)?;
                    let source_index = std::sync::Arc::new(
                        circuitfabric_document_opener::datasheet::DatasheetEvidenceIndex::from_pages(
                            &document.content_hash, &pages,
                        ),
                    );
                    Some((pages, source_index))
                }
            });
            cx.spawn(async move |view, cx| {
                let pages = work.await;
                view.update(cx, |view, cx| {
                    let indexed = pages.is_some_and(|(pages, source_index)| {
                        if view.workspace.register_pdf_pages(&key.0, &document, &pages).is_err() {
                            return false;
                        }
                        view.datasheet_source_indexes.insert(key.clone(), source_index);
                        true
                    });
                    if indexed {
                        view.pdf_index_state.remove(&key);
                        view.refresh_evidence_search(cx);
                    } else {
                        view.pdf_index_state.insert(key, PdfIndexState::Failed);
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// Searches the selected project's evidence after `delay`, off the UI thread.
    ///
    /// The corpus is snapshotted when the delay ends, so a burst of keystrokes costs one
    /// search over the newest index; a result is dropped when a newer request exists.
    pub(super) fn schedule_evidence_search(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.evidence_generation += 1;
        let generation = self.evidence_generation;
        let query = self.evidence_query.read(cx).value().trim().to_owned();
        let project_id = self.navigation.selected_project.clone();
        let (Some(project_id), false) = (project_id, query.is_empty()) else {
            self.evidence_searching = false;
            self.evidence_result = None;
            cx.notify();
            return;
        };
        self.evidence_searching = true;
        let scope = self.evidence_scope;
        cx.spawn(async move |view, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let Ok(Some((corpus, admitted))) = view.update(cx, |view, _| {
                (view.evidence_generation == generation)
                    .then(|| view.evidence_snapshot(&project_id))
                    .flatten()
            }) else {
                return;
            };
            let started = Instant::now();
            let search = cx
                .background_executor()
                .spawn(async move {
                    corpus.search(
                        &query,
                        scope,
                        |fragment| admitted.contains(&fragment.document_id),
                        EVIDENCE_SEARCH_LIMIT,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                if view.evidence_generation != generation {
                    return;
                }
                view.evidence_searching = false;
                view.evidence_visible = EVIDENCE_PAGE_SIZE;
                view.evidence_result = Some(EvidenceSearchResult {
                    project_id,
                    scope,
                    search: std::sync::Arc::new(search),
                    elapsed: started.elapsed(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Re-runs an active search after the evidence index changed.
    pub(super) fn refresh_evidence_search(&mut self, cx: &mut Context<Self>) {
        if !self.evidence_query.read(cx).value().trim().is_empty() {
            self.schedule_evidence_search(Duration::ZERO, cx);
        }
    }

    /// The project's fragment snapshot plus the documents allowed to appear in results:
    /// integrity verified at listing time and currently citable.
    pub(super) fn evidence_snapshot(
        &self,
        project_id: &str,
    ) -> Option<(EvidenceCorpus, std::collections::BTreeSet<String>)> {
        let corpus = self.workspace.evidence_corpus(project_id).ok()?;
        let admitted = self
            .project_data
            .get(project_id)
            .map(|data| {
                data.documents
                    .iter()
                    .filter(|document| {
                        data.document_integrity.get(&document.id).copied().unwrap_or(false)
                            && self
                                .workspace
                                .is_document_evidence_available(project_id, &document.id)
                    })
                    .map(|document| document.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        Some((corpus, admitted))
    }

    /// Opens the document a search hit cites and lands on its page or data row.
    pub(super) fn open_evidence_hit(
        &mut self,
        project_id: &str,
        hit: &EvidenceHit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_pdf_interaction();
        let document = self.project_data.get(project_id).and_then(|data| {
            data.documents.iter().find(|document| document.id == hit.fragment.document_id)
        });
        let Some(document) = document.cloned() else {
            self.status = format!("文档 {} 已不在项目中", hit.fragment.document_id);
            cx.notify();
            return;
        };
        let already_open = self.document_preview.as_ref().is_some_and(|preview| {
            preview.project_id == project_id
                && preview.document_id == document.id
                && !matches!(preview.state, DocumentPreviewState::Refused { .. })
        });
        if !already_open {
            self.open_document_preview(project_id.to_owned(), &document, window, cx);
        }
        self.preview_show_data = hit.anchor.is_verified_data();
        if let FragmentAnchor::DatasheetRow { row, .. } = hit.anchor {
            self.datasheet_rows_visible = self.datasheet_rows_visible.max(row);
        }
        self.reset_pdf_view();
        self.preview_focus = Some(PreviewFocus {
            project_id: project_id.to_owned(),
            document_id: document.id,
            // The document's own content hash, not the fragment's: full-text fragments
            // carry the hash of the extracted text corpus, which never equals the
            // file's hash and would make the resolver reject every search navigation
            // as "content changed". Verified-data rows keep their staleness signal
            // through the evidence lookup itself.
            content_hash: document.content_hash.clone(),
            anchor: hit.anchor.clone(),
            text: hit.fragment.text.clone(),
            terms: hit
                .highlights
                .iter()
                .filter_map(|range| hit.fragment.text.get(range.clone()).map(str::to_owned))
                .collect(),
            regions: Vec::new(),
            notice: None,
            resolving: false,
        });
        self.preview_focus_generation += 1;
        self.preview_row_anchor = self
            .preview_show_data
            .then(|| gpui::ScrollAnchor::for_handle(self.preview_scroll.clone()));
        self.preview_scroll.set_offset(gpui::Point::default());
        self.preview_scroll_pending.set(true);
        if matches!(hit.anchor, FragmentAnchor::PageLine { .. }) {
            self.resolve_preview_pdf_focus(None, cx);
        }
        cx.notify();
    }

    /// Resolve a row quote or highlight a known PDF page using a newly verified copy.
    /// Only the latest focus request may publish its result.
    pub(super) fn resolve_preview_pdf_focus(
        &mut self,
        section: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(focus) = self.preview_focus.as_mut() else { return };
        let Some(storage) = self.project_storages.get(&focus.project_id).cloned() else {
            return;
        };
        focus.notice = Some(self.language.choose("正在定位原文…", "Locating source…").to_owned());
        focus.resolving = true;
        let focus = focus.clone();
        let source_key = (focus.project_id.clone(), focus.document_id.clone());
        let mut source_index = self
            .datasheet_source_indexes
            .get(&source_key)
            .filter(|index| index.content_hash() == focus.content_hash)
            .cloned();
        let generation = self.preview_focus_generation;
        let work = cx.background_spawn(async move {
            let request = storage
                .prepare_document_open(&focus.project_id, &focus.document_id)
                .map_err(|error| error.to_string())?;
            if request.content_hash != focus.content_hash {
                return Err("文档内容已变化，请刷新检索或重新提取后定位。".to_owned());
            }
            let (page, regions) = match focus.anchor {
                FragmentAnchor::PageLine { page, .. } => {
                    // The hit's page number and its terms both come from the extractor
                    // that indexed the document, which can disagree with pdfium's
                    // pagination and text spacing. Search from that page outward with
                    // the terms plus the full matched line — the strongest locator —
                    // so the keywords still get highlighted on the page that really
                    // holds them.
                    let mut terms = focus.terms.clone();
                    let line = focus.text.trim().to_owned();
                    if !line.is_empty() && !terms.iter().any(|term| *term == line) {
                        terms.push(line);
                    }
                    circuitfabric_document_opener::pdf_locate_highlight(
                        request.managed_copy.data(),
                        page,
                        &terms,
                    )
                    .map(|result| result.map_err(|error| error.clone()))
                    .transpose()?
                    .unwrap_or((page, Vec::new()))
                }
                _ => {
                    // Cold navigation before search indexing finishes builds once off the
                    // UI thread; its result is retained for subsequent row clicks too.
                    if source_index.is_none() {
                        source_index = Some(std::sync::Arc::new(
                            circuitfabric_document_opener::datasheet::DatasheetEvidenceIndex::from_request(&request)?,
                        ));
                    }
                    let page = source_index.as_ref().expect("source index prepared")
                        .locate(&request, section.as_deref().unwrap_or_default(), &focus.text)?
                        .ok_or_else(|| {
                            "未能在当前 PDF 中定位这条证据，请对照原文核对。".to_owned()
                        })?;
                    let regions = circuitfabric_document_opener::pdf_highlight_regions(
                        request.managed_copy.data(),
                        page,
                        &focus.terms,
                    )
                    .transpose()?
                    .unwrap_or_default();
                    (page, regions)
                }
            };
            Ok::<_, String>((page, regions, source_index))
        });
        cx.spawn(async move |view, cx| {
            let result = work.await;
            view.update(cx, |view, cx| {
                if view.preview_focus_generation != generation { return }
                let Some(focus) = view.preview_focus.as_mut() else { return };
                match result {
                    Ok((page, regions, source_index)) => {
                        if let Some(index) = source_index {
                            view.datasheet_source_indexes.insert(source_key, index);
                        }
                        focus.resolving = false;
                        match focus.anchor {
                            FragmentAnchor::PageLine { page: anchored, line } if page != anchored => {
                                // The keywords live on a different page than the hit's
                                // anchor claimed; navigate to where they were found.
                                focus.anchor = FragmentAnchor::PageLine { page, line };
                                view.preview_scroll_pending.set(true);
                            }
                            FragmentAnchor::PageLine { .. } => {}
                            _ => {
                                focus.anchor = FragmentAnchor::PageLine { page, line: 1 };
                                view.preview_scroll_pending.set(true);
                            }
                        }
                        focus.notice = regions.is_empty().then(|| view.language.choose(
                            "已定位页面；该页文本层未匹配到高亮位置。", "Page located; no matching highlight coordinates in its text layer.",
                        ).to_owned());
                        focus.regions = regions;
                    }
                    Err(error) => {
                        focus.resolving = false;
                        focus.notice = Some(error);
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    pub(super) fn open_datasheet_source(
        &mut self,
        section: &str,
        row: usize,
        cx: &mut Context<Self>,
    ) {
        self.stop_pdf_interaction();
        let Some(preview) = &self.document_preview else { return };
        let Some(extraction) = &preview.extraction else { return };
        let evidence = match section {
            "pins" => extraction.pins.get(row).and_then(|row| row.evidence.clone()),
            "absoluteMaximumRatings" => {
                extraction.absolute_maximum_ratings.get(row).and_then(|row| row.evidence.clone())
            }
            "electricalCharacteristics" => {
                extraction.electrical_characteristics.get(row).and_then(|row| row.evidence.clone())
            }
            "operatingConditions" => {
                extraction.operating_conditions.get(row).and_then(|row| row.evidence.clone())
            }
            _ => None,
        };
        let Some(evidence) = evidence else { return };
        let (project_id, document_id, content_hash) = (
            preview.project_id.clone(),
            preview.document_id.clone(),
            extraction.content_hash.clone(),
        );
        self.reset_pdf_view();
        self.preview_focus = Some(PreviewFocus {
            project_id,
            document_id,
            content_hash,
            anchor: FragmentAnchor::Unknown,
            text: evidence.clone(),
            terms: vec![evidence],
            regions: Vec::new(),
            notice: None,
            resolving: false,
        });
        self.preview_focus_generation += 1;
        self.preview_row_anchor = None;
        self.preview_show_data = false;
        self.preview_scroll.set_offset(gpui::Point::default());
        self.resolve_preview_pdf_focus(Some(section.to_owned()), cx);
        cx.notify();
    }

    /// Human-readable position of a search hit.
    pub(super) fn evidence_anchor_label(anchor: &FragmentAnchor, language: UiLanguage) -> String {
        match anchor {
            FragmentAnchor::PageLine { page, line } => language.choose_owned(
                format!("第 {page} 页 · 第 {line} 行"),
                format!("Page {page} · line {line}"),
            ),
            FragmentAnchor::Line { line } => {
                language.choose_owned(format!("第 {line} 行"), format!("Line {line}"))
            }
            FragmentAnchor::DatasheetRow { section, row } => {
                let section = Self::datasheet_section_label(section, language);
                language.choose_owned(
                    format!("已校验数据 · {section} 第 {row} 行"),
                    format!("Verified data · {section} row {row}"),
                )
            }
            FragmentAnchor::Unknown => language.choose("位置未知", "Unknown position").to_owned(),
        }
    }

    pub(super) fn datasheet_section_label(section: &str, language: UiLanguage) -> &'static str {
        match section {
            "pins" => language.choose("引脚", "Pins"),
            "absoluteMaximumRatings" => {
                language.choose("绝对最大额定值", "Absolute Maximum Ratings")
            }
            "electricalCharacteristics" => language.choose("电特性", "Electrical Characteristics"),
            "operatingConditions" => language.choose("工作条件", "Operating Conditions"),
            _ => language.choose("数据", "Data"),
        }
    }

    /// Fragment text with the matched terms emphasized.
    pub(super) fn render_highlighted_text(
        text: &str,
        highlights: &[std::ops::Range<usize>],
    ) -> StyledText {
        let style = HighlightStyle {
            background_color: Some(rgb(0x00fe_f08a).into()),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..HighlightStyle::default()
        };
        StyledText::new(text.to_owned())
            .with_highlights(highlights.iter().map(|range| (range.clone(), style)))
    }

    /// Project-scoped evidence search: query, scope filter, and ranked hits grouped by
    /// document. Searching happens off the UI thread (`schedule_evidence_search`); this
    /// only renders the newest result.
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_evidence_search_panel(
        &mut self,
        project: &Project,
        documents: &[ProjectDocument],
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity().clone();
        let language = self.language;
        let query = self.evidence_query.read(cx).value().trim().to_owned();
        // A result left over from another project is searched again for this one.
        if !query.is_empty()
            && !self.evidence_searching
            && self.navigation.selected_project() == Some(project.id.as_str())
            && self.evidence_result.as_ref().is_none_or(|result| result.project_id != project.id)
        {
            self.schedule_evidence_search(Duration::ZERO, cx);
        }
        let result = self
            .evidence_result
            .as_ref()
            .filter(|result| result.project_id == project.id && !query.is_empty());

        let summary = if self.evidence_searching {
            language.choose("检索中…", "Searching…").to_owned()
        } else if let Some(result) = result {
            let search = &result.search;
            language.choose_owned(
                format!(
                    "{} 条命中 · {} 份文档 · {} ms",
                    search.total,
                    search.documents,
                    result.elapsed.as_millis()
                ),
                format!(
                    "{} hits · {} documents · {} ms",
                    search.total,
                    search.documents,
                    result.elapsed.as_millis()
                ),
            )
        } else {
            String::new()
        };

        let scope_button = |id: &'static str, scope: EvidenceScope, label: &'static str| {
            let chooser = entity.clone();
            let active = self.evidence_scope == scope;
            action_button(id)
                .when(active, Button::primary)
                .when(!active, Button::ghost)
                .label(label)
                .on_click(move |_, _, cx| {
                    chooser.update(cx, |view, cx| {
                        if view.evidence_scope != scope {
                            view.evidence_scope = scope;
                            view.schedule_evidence_search(Duration::ZERO, cx);
                        }
                    });
                })
        };

        let mut panel = div()
            .v_flex()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(language.choose("项目内证据检索", "Project-scoped evidence search")),
                    )
                    .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(summary)),
            )
            .child(div().id("evidence-query").w_full().child(Input::new(&self.evidence_query)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(scope_button(
                        "evidence-scope-all",
                        EvidenceScope::All,
                        language.choose("全部", "All"),
                    ))
                    .child(scope_button(
                        "evidence-scope-text",
                        EvidenceScope::FullText,
                        language.choose("文档全文", "Full text"),
                    ))
                    .child(scope_button(
                        "evidence-scope-verified",
                        EvidenceScope::VerifiedData,
                        language.choose("已校验数据", "Verified data"),
                    ))
                    .child(div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                        language.choose(
                            "多个词需同时命中 · \"引号\" 保持短语 · Enter 立即检索 · 点击结果跳转",
                            "All words must match · \"quotes\" keep phrases · Enter searches now · click a hit to jump",
                        ),
                    )),
            );

        let Some(result) = result else {
            if query.is_empty() {
                panel = panel.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                    language.choose(
                        "在本项目已索引的文档全文与已校验的数据手册行中检索；结果附来源定位符与内容哈希。",
                        "Search this project's indexed full text and verified datasheet rows; hits carry their locator and content hash.",
                    ),
                ));
            }
            return panel;
        };
        let search = result.search.clone();
        if search.hits.is_empty() {
            return panel.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                if result.scope == EvidenceScope::All {
                    language.choose(
                        "没有命中；试试更少或更短的关键词。尚未索引的文档（见下方标签）不会被检索到。",
                        "No hits; try fewer or shorter words. Documents not indexed yet (see labels below) are not searched.",
                    )
                } else {
                    language.choose(
                        "当前范围内没有命中；切换到“全部”试试。",
                        "No hits in this scope; try \"All\".",
                    )
                },
            ));
        }

        // Groups keep rank order: a document appears where its best hit ranks.
        let visible = self.evidence_visible.min(search.hits.len());
        let mut groups: Vec<(&str, Vec<&EvidenceHit>)> = Vec::new();
        for hit in &search.hits[..visible] {
            let document_id = hit.fragment.document_id.as_str();
            match groups.iter_mut().find(|(id, _)| *id == document_id) {
                Some((_, hits)) => hits.push(hit),
                None => groups.push((document_id, vec![hit])),
            }
        }
        let focused_locator = self
            .preview_focus
            .as_ref()
            .filter(|focus| focus.project_id == project.id)
            .map(|focus| (focus.document_id.clone(), focus.anchor.clone()));

        for (group_index, (document_id, hits)) in groups.into_iter().enumerate() {
            let document = documents.iter().find(|document| document.id == document_id);
            let title = document.map_or_else(
                || document_id.to_owned(),
                |document| document.original_file_name.clone(),
            );
            let opener = entity.clone();
            let open_project = project.id.clone();
            let open_document = document.cloned();
            let mut group = div()
                .v_flex()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(rgb(CARD_BG))
                .border_1()
                .border_color(rgb(BORDER))
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
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_normal()
                                .child(title),
                        )
                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                            language.choose_owned(
                                format!("{} 条", hits.len()),
                                format!("{} hits", hits.len()),
                            ),
                        ))
                        .when_some(open_document, |row, document| {
                            row.child(
                                action_button(("evidence-open-document", group_index))
                                    .ghost()
                                    .label(language.choose("打开文档", "Open document"))
                                    .on_click(move |_, window, cx| {
                                        opener.update(cx, |view, cx| {
                                            view.open_document_preview(
                                                open_project.clone(),
                                                &document,
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                            )
                        }),
                );
            for hit in hits {
                let verified = hit.anchor.is_verified_data();
                let focused = focused_locator.as_ref().is_some_and(|(document, anchor)| {
                    *document == hit.fragment.document_id && *anchor == hit.anchor
                });
                let navigator = entity.clone();
                let navigate_project = project.id.clone();
                let navigate_hit = hit.clone();
                group = group.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "evidence-hit-{}",
                            hit.fragment.locator
                        )))
                        .v_flex()
                        .gap_0p5()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(if focused { ACCENT } else { SURFACE_BG }))
                        .bg(rgb(SURFACE_BG))
                        .cursor_pointer()
                        .hover(|this| this.border_color(rgb(ACCENT_SOFT)))
                        .on_click(move |_, window, cx| {
                            navigator.update(cx, |view, cx| {
                                view.open_evidence_hit(
                                    &navigate_project,
                                    &navigate_hit,
                                    window,
                                    cx,
                                );
                            });
                        })
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
                                        .bg(rgb(if verified { 0x00dc_fce7 } else { 0x00e0_f2fe }))
                                        .text_color(rgb(if verified {
                                            0x0016_a34a
                                        } else {
                                            0x000e_7490
                                        }))
                                        .child(Self::evidence_anchor_label(&hit.anchor, language)),
                                )
                                .child(
                                    div().ml_auto().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                        if verified {
                                            language.choose("查看数据 →", "View data →")
                                        } else {
                                            language.choose("定位原文 →", "Show in document →")
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT_PRIMARY))
                                .whitespace_normal()
                                .child(Self::render_highlighted_text(
                                    &hit.fragment.text,
                                    &hit.highlights,
                                )),
                        )
                        .child(
                            div().text_xs().text_color(rgb(TEXT_MUTED)).whitespace_normal().child(
                                format!(
                                    "{} · {}",
                                    hit.fragment.locator,
                                    &hit.fragment.content_hash
                                        [..hit.fragment.content_hash.len().min(19)]
                                ),
                            ),
                        ),
                );
            }
            panel = panel.child(group);
        }

        if visible < search.hits.len() {
            let loader = entity.clone();
            panel = panel.child(
                action_button("evidence-show-more")
                    .label(language.choose_owned(
                        format!("显示更多（已显示 {visible} / {}）", search.hits.len()),
                        format!("Show more ({visible} of {})", search.hits.len()),
                    ))
                    .on_click(move |_, _, cx| {
                        loader.update(cx, |view, cx| {
                            view.evidence_visible += EVIDENCE_PAGE_SIZE;
                            cx.notify();
                        });
                    }),
            );
        }
        if search.total > search.hits.len() {
            panel =
                panel
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(language.choose_owned(
                    format!(
                        "共 {} 条命中，仅保留相关度最高的 {} 条；增加关键词可缩小范围。",
                        search.total,
                        search.hits.len()
                    ),
                    format!(
                        "{} hits; only the {} most relevant are kept — add words to narrow down.",
                        search.total,
                        search.hits.len()
                    ),
                )));
        }
        panel
    }

    /// Top-level Documents navigation.  The evidence UI is project-scoped, but the
    /// navigation entry itself remains available so its empty state can explain the
    /// required next step instead of sending users to an unrelated TODO placeholder.
    pub(super) fn render_documents_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let selected_project =
            self.navigation.selected_project().and_then(|id| self.workspace.project(id)).cloned();

        if let Some(project) = selected_project {
            let project_name = project.name.clone();
            let project_id = project.id.clone();
            let chooser = entity.clone();
            return page("documents-page")
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .v_flex()
                                .gap_1()
                                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                                    language.choose(
                                        "已索引，可作证据",
                                        "Authorized documents & evidence",
                                    ),
                                ))
                                .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                                    language.choose_owned(
                                        format!("当前项目：{project_name} · {project_id}"),
                                        format!("Current project: {project_name} · {project_id}"),
                                    ),
                                )),
                        )
                        .child(
                            action_button("documents-choose-project")
                                .label(language.choose("切换项目", "Choose project"))
                                .on_click(move |_, _, cx| {
                                    chooser.update(cx, |view, cx| {
                                        view.navigation.screen = ControlPlaneScreen::Projects;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(self.render_documents_tab(&project, cx))
                .into_any_element();
        }

        let opener = entity;
        div()
            .size_full()
            .v_flex()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.choose("先选择项目", "Select a project first")),
            )
            .child(
                div()
                    .max_w(px(560.))
                    .text_sm()
                    .text_color(rgb(TEXT_SECONDARY))
                    .whitespace_normal()
                    .child(language.choose(
                        "用量汇总和审计事件是分离的只读投影；审计行始终保留项目、Provider、运行时和会话来源。",
                        "Authorized documents, source registration, and evidence retrieval are project-scoped. Create or open a project, then select it from the project list.",
                    )),
            )
            .child(
                action_button("documents-open-projects")
                    .primary()
                    .label(language.choose("打开项目", "Open projects"))
                    .on_click(move |_, _, cx| {
                        opener.update(cx, |view, cx| {
                            view.navigation.screen = ControlPlaneScreen::Projects;
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    }
}
