//! Formatted text using the same window selection and copy contract as PDF/data views.
use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, StyledText, Window,
};
use gpui_base::{TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};

pub(super) struct SelectableStyledText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    order: u64,
}

impl SelectableStyledText {
    pub(super) fn new(
        id: impl Into<ElementId>,
        text: impl Into<SharedString>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        order: u64,
    ) -> Self {
        let text = text.into();
        Self {
            id: id.into(),
            styled: StyledText::new(text.clone()).with_highlights(highlights),
            text,
            order,
        }
    }
}

impl IntoElement for SelectableStyledText {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for SelectableStyledText {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, TextSelectionHandle) {
        let handle = window.with_element_state(
            global_id.expect("selectable document text requires a stable id"),
            |retained: Option<TextSelectionHandle>, _| {
                let handle =
                    retained.unwrap_or_else(|| TextSelectionHandle::new(self.text.clone(), cx));
                (handle.clone(), handle)
            },
        );
        let (layout, ()) = self.styled.request_layout(global_id, inspector_id, window, cx);
        (layout, handle)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut TextSelectionHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        self.styled.prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        handle.register(
            TextSelectionRegistration::new(hitbox.clone(), bounds)
                .with_document_order(self.order)
                .with_text_bounds(vec![bounds]),
            window,
            cx,
        );
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut TextSelectionHandle,
        _: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled.layout().clone();
        let before = TextSelection::selected_text(window, cx);
        let projection = handle.update_runs(
            &[TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)
                .with_document_order(self.order)],
            cx,
        );
        if before != TextSelection::selected_text(window, cx) {
            window.refresh();
        }
        let color = gpui_base::Theme::global(cx).tokens.colors.selection;
        for range in projection.ranges().iter().flatten() {
            let (Some(start), Some(end)) =
                (layout.position_for_index(range.start), layout.position_for_index(range.end))
            else {
                continue;
            };
            let height = layout.line_height();
            if start.y == end.y {
                window.paint_quad(gpui::fill(
                    Bounds::from_corners(start, Point::new(end.x, end.y + height)),
                    color,
                ));
            } else {
                window.paint_quad(gpui::fill(
                    Bounds::from_corners(start, Point::new(bounds.right(), start.y + height)),
                    color,
                ));
                if end.y > start.y + height {
                    window.paint_quad(gpui::fill(
                        Bounds::from_corners(
                            Point::new(bounds.left(), start.y + height),
                            Point::new(bounds.right(), end.y),
                        ),
                        color,
                    ));
                }
                window.paint_quad(gpui::fill(
                    Bounds::from_corners(
                        Point::new(bounds.left(), end.y),
                        Point::new(end.x, end.y + height),
                    ),
                    color,
                ));
            }
        }
        self.styled.paint(global_id, inspector_id, bounds, &mut (), &mut (), window, cx);
    }
}
