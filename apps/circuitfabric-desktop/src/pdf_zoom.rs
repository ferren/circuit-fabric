//! PDF zoom is relative to the preview's fitted page width.

pub(crate) fn zoom_after_scroll(current: f32, delta_pixels: f32) -> f32 {
    if !delta_pixels.is_finite() {
        return current;
    }
    let steps = (delta_pixels / 90.0).clamp(-4.0, 4.0);
    (current * 1.15_f32.powf(steps)).clamp(1.0, 8.0)
}

/// A critically damped spring: wheel events move the target, while the displayed
/// scale retains velocity and settles smoothly without oscillating at rest.
pub(crate) struct ZoomMotion {
    pub value: f32,
    target: f32,
    velocity: f32,
}

impl Default for ZoomMotion {
    fn default() -> Self {
        Self { value: 1.0, target: 1.0, velocity: 0.0 }
    }
}

impl ZoomMotion {
    pub fn scroll(&mut self, delta_pixels: f32) -> bool {
        let target = zoom_after_scroll(self.target, delta_pixels);
        if target == self.target {
            return false;
        }
        self.target = target;
        true
    }

    pub fn advance(&mut self, seconds: f32) -> bool {
        let seconds = seconds.clamp(0.0, 0.05);
        let rate = 20.0;
        let displacement = self.value - self.target;
        let decay = (-rate * seconds).exp();
        let change = (self.velocity + rate * displacement) * seconds;
        self.value = (self.target + (displacement + change) * decay).clamp(1.0, 8.0);
        self.velocity = (self.velocity - rate * change) * decay;
        if (self.value <= 1.0 && self.velocity < 0.0) || (self.value >= 8.0 && self.velocity > 0.0)
        {
            self.velocity = 0.0;
        }
        if (self.value - self.target).abs() < 0.0001 && self.velocity.abs() < 0.001 {
            self.finish();
            return false;
        }
        true
    }

    pub fn finish(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    pub fn stop(&mut self) {
        self.target = self.value;
        self.velocity = 0.0;
    }
}

pub(crate) fn pan_offset(
    start_offset: (f32, f32),
    start_pointer: (f32, f32),
    pointer: (f32, f32),
    max_offset: (f32, f32),
) -> (f32, f32) {
    (
        (start_offset.0 + pointer.0 - start_pointer.0).clamp(-max_offset.0.max(0.0), 0.0),
        (start_offset.1 + pointer.1 - start_pointer.1).clamp(-max_offset.1.max(0.0), 0.0),
    )
}

/// Match the physical display size, using buckets to reuse nearby resolutions.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn raster_width(fitted_width: f32, zoom: f32, device_scale: f32) -> u32 {
    let width = fitted_width.max(1.0) * zoom.max(1.0) * device_scale.max(1.0);
    if width <= 900.0 {
        return 900;
    }
    ((width / 256.0).ceil() * 256.0).clamp(900.0, 8192.0) as u32
}

#[cfg(feature = "native-ui")]
pub(crate) mod native {
    use gpui::{
        AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement,
        LayoutId, Pixels, Point, ScrollHandle, Window,
    };
    use std::{cell::Cell, rc::Rc};

    type LayoutProbe = Rc<Cell<Option<LayoutId>>>;

    pub fn pan_cursor(grabbing: bool) -> gpui::CursorStyle {
        // The pinned GPUI Windows backend maps OpenHand/ClosedHand to Arrow.
        if cfg!(windows) {
            gpui::CursorStyle::PointingHand
        } else if grabbing {
            gpui::CursorStyle::ClosedHand
        } else {
            gpui::CursorStyle::OpenHand
        }
    }

    enum Placement {
        Probe(LayoutProbe),
        Anchor {
            probe: LayoutProbe,
            scroll: ScrollHandle,
            fraction: Point<f32>,
            pointer: Point<Pixels>,
        },
    }

    struct ZoomLayout {
        content: AnyElement,
        placement: Placement,
    }

    pub fn mark_layout(content: AnyElement, probe: LayoutProbe) -> AnyElement {
        ZoomLayout { content, placement: Placement::Probe(probe) }.into_any_element()
    }

    pub fn anchor_viewport(
        content: AnyElement,
        probe: LayoutProbe,
        scroll: ScrollHandle,
        fraction: Point<f32>,
        pointer: Point<Pixels>,
    ) -> AnyElement {
        ZoomLayout { content, placement: Placement::Anchor { probe, scroll, fraction, pointer } }
            .into_any_element()
    }

    impl IntoElement for ZoomLayout {
        type Element = Self;
        fn into_element(self) -> Self {
            self
        }
    }

    impl Element for ZoomLayout {
        type RequestLayoutState = ();
        type PrepaintState = ();

        fn id(&self) -> Option<gpui::ElementId> {
            None
        }
        fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
            None
        }

        fn request_layout(
            &mut self,
            _: Option<&GlobalElementId>,
            _: Option<&InspectorElementId>,
            window: &mut Window,
            cx: &mut App,
        ) -> (LayoutId, ()) {
            let id = self.content.request_layout(window, cx);
            if let Placement::Probe(probe) = &self.placement {
                probe.set(Some(id));
            }
            (id, ())
        }

        fn prepaint(
            &mut self,
            _: Option<&GlobalElementId>,
            _: Option<&InspectorElementId>,
            _: Bounds<Pixels>,
            _: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            if let Placement::Anchor { probe, scroll, fraction, pointer } = &self.placement
                && let Some(id) = probe.get()
            {
                let bounds = window.layout_bounds(id);
                // Set the offset before the scroll container prepaints its children.
                // Correcting it after painting causes visible jumps on late PDF pages.
                scroll.set_offset(gpui::point(
                    pointer.x - bounds.left() - bounds.size.width * fraction.x,
                    pointer.y - bounds.top() - bounds.size.height * fraction.y,
                ));
            }
            self.content.prepaint(window, cx);
        }

        fn paint(
            &mut self,
            _: Option<&GlobalElementId>,
            _: Option<&InspectorElementId>,
            _: Bounds<Pixels>,
            _: &mut (),
            _: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            self.content.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_stops_at_fitted_width_and_reverses_wheel_direction() {
        assert_eq!(zoom_after_scroll(1.0, -90.0), 1.0);
        let enlarged = zoom_after_scroll(1.0, 90.0);
        assert!(enlarged > 1.0);
        assert!((zoom_after_scroll(enlarged, -90.0) - 1.0).abs() < 0.0001);
        assert_eq!(zoom_after_scroll(1.0, 0.0), 1.0);
        assert_eq!(zoom_after_scroll(2.0, f32::NAN), 2.0);
    }

    #[test]
    fn repeated_scrolls_remain_bounded() {
        let mut zoom = 1.0;
        for _ in 0..100 {
            zoom = zoom_after_scroll(zoom, 90.0);
        }
        assert_eq!(zoom, 8.0);
        for _ in 0..100 {
            zoom = zoom_after_scroll(zoom, -90.0);
        }
        assert_eq!(zoom, 1.0);
    }

    #[test]
    fn animation_continues_after_wheel_input_and_settles_without_overshoot() {
        let mut motion = ZoomMotion::default();
        motion.scroll(90.0);
        assert_eq!(motion.value, 1.0);
        assert!(motion.advance(1.0 / 60.0));
        assert!(motion.value > 1.0 && motion.value < 1.15);
        let mut last = motion.value;
        for _ in 0..90 {
            let active = motion.advance(1.0 / 60.0);
            assert!(motion.value >= last && motion.value <= 1.15);
            last = motion.value;
            if !active {
                break;
            }
        }
        assert_eq!(motion.value, 1.15);
        assert!(!motion.advance(1.0 / 60.0));
    }

    #[test]
    fn rapid_reversal_stays_above_fit_and_drag_can_stop_animation() {
        let mut motion = ZoomMotion::default();
        motion.scroll(360.0);
        for _ in 0..8 {
            motion.advance(1.0 / 60.0);
        }
        motion.scroll(-360.0);
        for _ in 0..90 {
            motion.advance(1.0 / 60.0);
            assert!(motion.value >= 1.0);
        }
        assert_eq!(motion.value, 1.0);
        motion.scroll(90.0);
        motion.advance(1.0 / 60.0);
        let stopped = motion.value;
        motion.stop();
        assert!(!motion.advance(1.0 / 60.0));
        assert_eq!(motion.value, stopped);
    }

    #[test]
    fn spring_follows_elapsed_time_at_different_frame_rates() {
        let mut slow = ZoomMotion::default();
        let mut fast = ZoomMotion::default();
        slow.scroll(360.0);
        fast.scroll(360.0);
        for _ in 0..12 {
            slow.advance(1.0 / 60.0);
        }
        for _ in 0..24 {
            fast.advance(1.0 / 120.0);
        }
        assert!((slow.value - fast.value).abs() < 0.00001);
    }

    #[test]
    fn raster_resolution_tracks_zoom_and_device_pixels() {
        assert_eq!(raster_width(800.0, 1.0, 1.0), 900);
        assert_eq!(raster_width(800.0, 2.0, 1.0), 1792);
        assert_eq!(raster_width(800.0, 1.0, 2.0), 1792);
        assert_eq!(raster_width(800.0, 2.0, 2.0), 3328);
        assert_eq!(raster_width(2000.0, 8.0, 2.0), 8192);
    }

    #[test]
    fn grab_moves_both_axes_and_clamps_at_document_edges() {
        assert_eq!(
            pan_offset((-100.0, -200.0), (30.0, 40.0), (50.0, 70.0), (500.0, 1000.0)),
            (-80.0, -170.0)
        );
        assert_eq!(
            pan_offset((-100.0, -200.0), (30.0, 40.0), (5000.0, -5000.0), (500.0, 1000.0)),
            (0.0, -1000.0)
        );
        assert_eq!(
            pan_offset((0.0, -200.0), (30.0, 40.0), (0.0, 70.0), (0.0, 1000.0)),
            (0.0, -170.0)
        );
    }
}
