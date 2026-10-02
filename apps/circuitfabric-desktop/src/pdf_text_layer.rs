//! Invisible PDF glyph geometry joins GPUI's window selection and clipboard pipeline.
//! It uses PDF coordinates rather than reshaping the text over the page bitmap.

use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::Rc,
    sync::Arc,
};

use circuitfabric_document_opener::PdfPageText;
use gpui::{
    AnyElement, App, AppContext, Bounds, Element, ElementId, Entity, GlobalElementId,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels, Point, Size, Styled, Window,
    div, px, rgba,
};
use gpui_base::{
    TextSelectionContentKey, TextSelectionCoverage, TextSelectionHandle, TextSelectionRegistration,
};

thread_local! {
    static GLYPH_BOUNDS: RefCell<Vec<Bounds<Pixels>>> = const { RefCell::new(Vec::new()) };
}

pub fn begin_frame() {
    GLYPH_BOUNDS.with(|bounds| bounds.borrow_mut().clear());
}

pub fn over_text(point: Point<Pixels>) -> bool {
    GLYPH_BOUNDS.with(|bounds| bounds.borrow().iter().any(|bounds| bounds.contains(&point)))
}

/// Work around gpui-base 731e33c restarting auto-scroll for a retained selection.
/// Mount as the view's FIRST child under gpui_component::Root: controls handle bubble
/// events first, then this guard keeps idle movement/wheels out of Root's selection
/// layer. Capture handlers and ordinary hover, pan and wheel handling stay available.
pub fn selection_gesture_guard(pressed: Rc<Cell<bool>>) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let down = pressed.clone();
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _, _| {
                if phase.capture() && event.button == gpui::MouseButton::Left {
                    down.set(true);
                }
            });
            let up = pressed.clone();
            window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
                if phase.capture() && event.button == gpui::MouseButton::Left {
                    up.set(false);
                    gpui_base::TextSelection::end(window, cx);
                }
            });
            let moving = pressed.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                if phase.capture() {
                    moving.set(event.pressed_button == Some(gpui::MouseButton::Left));
                    if !moving.get() {
                        gpui_base::TextSelection::end(window, cx);
                    }
                } else if !moving.get() {
                    cx.stop_propagation();
                }
            });
            let wheeling = pressed.clone();
            window.on_mouse_event(move |_: &gpui::ScrollWheelEvent, phase, _, cx| {
                if phase.bubble() && !wheeling.get() {
                    cx.stop_propagation();
                }
            });
        },
    )
    .w(px(0.))
    .h(px(0.))
    .flex_none()
}

pub struct PageState {
    handle: TextSelectionHandle,
    text: Arc<PdfPageText>,
    size: Size<Pixels>,
}

impl PageState {
    fn index(&self, point: Point<Pixels>) -> usize {
        self.text.nearest_boundary((
            f32::from(point.x) / f32::from(self.size.width).max(1.),
            f32::from(point.y) / f32::from(self.size.height).max(1.),
        ))
    }

    fn range(&self, cx: &App) -> Option<Range<usize>> {
        let snapshot = self.handle.snapshot(cx)?;
        let offset = |endpoint: gpui_base::TextSelectionEndpoint| {
            endpoint
                .content_key()
                .and_then(|key| usize::try_from(key.value()).ok())
                .unwrap_or_else(|| self.index(endpoint.content_point()))
        };
        let anchor = offset(snapshot.anchor());
        let cursor = offset(snapshot.cursor());
        let own = if snapshot.anchor().entity_id() == Some(self.handle.entity_id()) {
            anchor
        } else {
            cursor
        };
        Some(match snapshot.coverage() {
            TextSelectionCoverage::Bounded => anchor.min(cursor)..anchor.max(cursor),
            TextSelectionCoverage::FromStart => 0..own,
            TextSelectionCoverage::ToEnd => own..self.text.text.len(),
            TextSelectionCoverage::Full => 0..self.text.text.len(),
        })
    }
}

pub struct PdfTextLayer {
    id: ElementId,
    text: Arc<PdfPageText>,
    content: AnyElement,
    grabbing: bool,
}

impl PdfTextLayer {
    pub fn new(
        id: impl Into<ElementId>,
        text: Arc<PdfPageText>,
        width: f32,
        height: f32,
        grabbing: bool,
    ) -> Self {
        Self {
            id: id.into(),
            text,
            grabbing,
            content: div().w(px(width)).h(px(height)).flex_none().into_any_element(),
        }
    }
}

impl IntoElement for PdfTextLayer {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for PdfTextLayer {
    type RequestLayoutState = Entity<PageState>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let state = window.with_element_state(
            id.expect("PDF text layer has a stable id"),
            |retained: Option<Entity<PageState>>, window| {
                let state = retained.unwrap_or_else(|| {
                    let handle = TextSelectionHandle::new("", cx);
                    let state = cx.new(|_| PageState {
                        handle: handle.clone(),
                        text: self.text.clone(),
                        size: Size::default(),
                    });
                    let weak = state.downgrade();
                    handle.copy_with(
                        move |cx| {
                            weak.upgrade()
                                .and_then(|state| {
                                    let state = state.read(cx);
                                    state.range(cx).and_then(|range| {
                                        state.text.text.get(range).map(str::to_owned)
                                    })
                                })
                                .unwrap_or_default()
                        },
                        cx,
                    );
                    let weak = state.downgrade();
                    handle.resolve_content_key_with(
                        move |point, cx| {
                            weak.upgrade().map(|state| {
                                TextSelectionContentKey::new(state.read(cx).index(point) as u64)
                            })
                        },
                        cx,
                    );
                    handle.refresh_window_on_change(window, cx).detach();
                    state
                });
                (state.clone(), state)
            },
        );
        (self.content.request_layout(window, cx), state)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.content.prepaint(window, cx);
        let rectangles: Vec<_> = if bounds.intersects(&window.content_mask().bounds) {
            self.text
                .characters
                .iter()
                .map(|character| {
                    let rect = character.bounds;
                    Bounds::new(
                        bounds.origin
                            + gpui::point(
                                bounds.size.width * rect.left,
                                bounds.size.height * rect.top,
                            ),
                        gpui::size(
                            bounds.size.width * rect.width,
                            bounds.size.height * rect.height,
                        ),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        state.update(cx, |state, _| {
            state.size = bounds.size;
            state.text = self.text.clone();
        });
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let handle = state.read(cx).handle.clone();
        handle.register(
            TextSelectionRegistration::new(hitbox, bounds)
                .with_document_order(u64::from(self.text.number))
                .with_text_bounds(rectangles),
            window,
            cx,
        );
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if !bounds.intersects(&window.content_mask().bounds) {
            self.content.paint(window, cx);
            return;
        }
        let state = state.read(cx);
        let rectangles = |rect: circuitfabric_document_opener::PdfHighlightRect| {
            Bounds::new(
                bounds.origin
                    + gpui::point(bounds.size.width * rect.left, bounds.size.height * rect.top),
                gpui::size(bounds.size.width * rect.width, bounds.size.height * rect.height),
            )
        };
        GLYPH_BOUNDS.with(|all| {
            all.borrow_mut().extend(self.text.characters.iter().map(|ch| rectangles(ch.bounds)))
        });
        let pointer = window.mouse_position() - bounds.origin;
        let normalized = (
            f32::from(pointer.x) / f32::from(bounds.size.width).max(1.),
            f32::from(pointer.y) / f32::from(bounds.size.height).max(1.),
        );
        if !self.grabbing && self.text.hit_test(normalized) {
            window.set_window_cursor_style(gpui::CursorStyle::IBeam);
        }
        if let Some(range) = state.range(cx) {
            for rect in self.text.selection_rects(range) {
                window.paint_quad(gpui::fill(rectangles(rect), rgba(0x3988ee66)));
            }
        }
        self.content.paint(window, cx);
    }
}

#[cfg(all(test, feature = "ui-test-support"))]
mod tests {
    use super::*;
    use circuitfabric_document_opener::{PdfHighlightRect, PdfTextCharacter};
    use gpui::{
        InteractiveElement, MouseButton, ParentElement, Render, StatefulInteractiveElement,
        TestAppContext,
    };

    struct SelectionView {
        focus: gpui::FocusHandle,
        pages: u32,
    }

    impl Render for SelectionView {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            begin_frame();
            let text = "A 中\nµB";
            let characters: Vec<_> = text
                .char_indices()
                .filter(|(_, ch)| !ch.is_whitespace())
                .enumerate()
                .map(|(index, (start, ch))| {
                    let left = 0.1 + index as f32 * 0.1;
                    PdfTextCharacter {
                        bytes: start..start + ch.len_utf8(),
                        bounds: PdfHighlightRect { left, top: 0.1, width: 0.1, height: 0.1 },
                        leading: (left, 0.15),
                        trailing: (left + 0.1, 0.15),
                    }
                })
                .collect();
            let focus = self.focus.clone();
            div()
                .track_focus(&self.focus)
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    if over_text(event.position) {
                        window.focus(&focus, cx);
                    } else {
                        cx.stop_propagation();
                    }
                })
                .children((1..=self.pages).map(|number| {
                    PdfTextLayer::new(
                        ("page", number),
                        Arc::new(PdfPageText {
                            number,
                            text: text.into(),
                            characters: characters.clone(),
                        }),
                        500.,
                        500.,
                        false,
                    )
                }))
        }
    }

    #[gpui::test]
    fn dragging_real_pointer_events_selects_pdf_unicode_and_ignores_blank_paper(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SelectionView { focus: cx.focus_handle(), pages: 1 });
            gpui_component::Root::new(view, window, cx).bordered(false)
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let start = gpui::point(px(100.5), px(75.));
        let end = gpui::point(px(149.5), px(75.));
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
        cx.update(|window, cx| {
            assert_eq!(gpui_base::TextSelection::selected_text(window, cx), "中");
        });
        cx.simulate_keystrokes("ctrl-c");
        cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "中"));
        cx.update(|window, cx| {
            gpui_base::TextSelection::clear(window, cx);
        });
        let blank = gpui::point(px(450.), px(450.));
        cx.simulate_mouse_down(blank, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
        cx.update(|window, cx| assert_eq!(gpui_base::TextSelection::selected_text(window, cx), ""));
    }

    #[gpui::test]
    fn selection_spans_pdf_pages_in_document_order(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| SelectionView { focus: cx.focus_handle(), pages: 2 });
            gpui_component::Root::new(view, window, cx).bordered(false)
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let start = gpui::point(px(50.5), px(75.));
        let end = gpui::point(px(149.5), px(575.));
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
        cx.update(|window, cx| {
            assert_eq!(gpui_base::TextSelection::selected_text(window, cx), "A 中\nµB\nA 中")
        });
    }

    struct DataView {
        focus: gpui::FocusHandle,
    }

    struct ScrollSelectionView {
        focus: gpui::FocusHandle,
        scroll: gpui::ScrollHandle,
        pdf: bool,
        pressed: Rc<Cell<bool>>,
    }

    impl Render for ScrollSelectionView {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            begin_frame();
            let focus = self.focus.clone();
            let page = if self.pdf {
                PdfTextLayer::new(
                    "pdf",
                    Arc::new(PdfPageText {
                        number: 1,
                        text: "AB".into(),
                        characters: vec![
                            PdfTextCharacter {
                                bytes: 0..1,
                                bounds: PdfHighlightRect {
                                    left: 0.1,
                                    top: 0.1,
                                    width: 0.1,
                                    height: 0.1,
                                },
                                leading: (0.1, 0.15),
                                trailing: (0.2, 0.15),
                            },
                            PdfTextCharacter {
                                bytes: 1..2,
                                bounds: PdfHighlightRect {
                                    left: 0.2,
                                    top: 0.1,
                                    width: 0.1,
                                    height: 0.1,
                                },
                                leading: (0.2, 0.15),
                                trailing: (0.3, 0.15),
                            },
                        ],
                    }),
                    500.,
                    500.,
                    false,
                )
                .into_any_element()
            } else {
                div()
                    .h(px(500.))
                    .flex_none()
                    .child(gpui_base::SelectableText::new("parameter", "VOUT 3.3 V"))
                    .into_any_element()
            };
            div()
                .track_focus(&self.focus)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| window.focus(&focus, cx))
                .child(selection_gesture_guard(self.pressed.clone()))
                .child(
                    div()
                        .id("viewport")
                        .w(px(500.))
                        .h(px(200.))
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .flex()
                        .flex_col()
                        .child(page),
                )
        }
    }

    #[gpui::test]
    fn releasing_selection_keeps_hover_and_wheel_from_starting_auto_scroll(
        cx: &mut TestAppContext,
    ) {
        for pdf in [false, true] {
            cx.update(gpui_component::init);
            let scroll = gpui::ScrollHandle::new();
            let (_, cx) = cx.add_window_view(|window, cx| {
                let view = cx.new(|cx| ScrollSelectionView {
                    focus: cx.focus_handle(),
                    scroll: scroll.clone(),
                    pdf,
                    pressed: Rc::new(Cell::new(false)),
                });
                gpui_component::Root::new(view, window, cx).bordered(false)
            });
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let (start, end) = if pdf {
                (gpui::point(px(50.5), px(75.)), gpui::point(px(149.5), px(75.)))
            } else {
                (gpui::point(px(0.5), px(7.)), gpui::point(px(200.), px(7.)))
            };
            cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
            cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
            cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
            let selected =
                cx.update(|window, cx| gpui_base::TextSelection::selected_text(window, cx));
            assert!(!selected.is_empty(), "pdf={pdf}");
            scroll.set_offset(gpui::point(px(0.), px(-100.)));
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            for position in [
                gpui::point(px(100.), px(1.)),
                gpui::point(px(100.), px(199.)),
                gpui::point(px(550.), px(400.)),
            ] {
                let before = scroll.offset();
                cx.simulate_mouse_move(position, None, Default::default());
                cx.executor().advance_clock(std::time::Duration::from_millis(64));
                cx.run_until_parked();
                assert_eq!(
                    scroll.offset(),
                    before,
                    "idle hover must not scroll, pdf={pdf}, position={position:?}"
                );
                cx.update(|window, cx| {
                    assert_eq!(gpui_base::TextSelection::selected_text(window, cx), selected)
                });
            }
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: gpui::point(px(100.), px(199.)),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-20.))),
                ..Default::default()
            });
            let after_wheel = scroll.offset();
            assert!(after_wheel.y < px(-100.), "ordinary wheel scrolling must work, pdf={pdf}");
            cx.executor().advance_clock(std::time::Duration::from_millis(64));
            cx.run_until_parked();
            assert_eq!(
                scroll.offset(),
                after_wheel,
                "wheel must not restart selection auto-scroll"
            );
            cx.simulate_keystrokes("ctrl-c");
            cx.update(|_, cx| {
                assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), selected)
            });

            // Only an actual held selection drag may auto-scroll at the viewport edge.
            scroll.set_offset(gpui::Point::default());
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
            let edge = gpui::point(px(100.), px(199.));
            cx.simulate_mouse_move(edge, Some(MouseButton::Left), Default::default());
            cx.executor().advance_clock(std::time::Duration::from_millis(64));
            cx.run_until_parked();
            assert!(scroll.offset().y < px(0.), "held drag must still auto-scroll, pdf={pdf}");
            cx.simulate_mouse_up(
                gpui::point(px(550.), px(400.)),
                MouseButton::Left,
                Default::default(),
            );
            let released = scroll.offset();
            cx.executor().advance_clock(std::time::Duration::from_millis(64));
            cx.run_until_parked();
            assert_eq!(
                scroll.offset(),
                released,
                "releasing outside the pane must stop auto-scroll"
            );
        }
    }

    impl Render for DataView {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            let focus = self.focus.clone();
            div()
                .track_focus(&self.focus)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| window.focus(&focus, cx))
                .child(gpui_base::SelectableText::new("parameter", "VOUT 3.3 V"))
        }
    }

    #[gpui::test]
    fn data_text_can_be_drag_selected_and_copied_with_the_application_shortcut(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| DataView { focus: cx.focus_handle() });
            gpui_component::Root::new(view, window, cx).bordered(false)
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let start = gpui::point(px(0.5), px(7.));
        let end = gpui::point(px(200.), px(7.));
        cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
        cx.simulate_keystrokes("ctrl-c");
        cx.update(|_, cx| {
            assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "VOUT 3.3 V")
        });
    }
}
