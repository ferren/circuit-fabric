//! Shared layout policy: the shell owns flow scrolling; workspaces own their panes.
//! Every ordinary scroll area has a stable identity and a visible scrollbar.
use gpui::{
    AnyElement, App, Div, Element, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Stateful, StyleRefinement, Styled, Window, div, px,
};
use gpui_component::{
    StyledExt,
    scroll::{Scrollable, ScrollableElement},
};

use crate::application::navigation::ControlPlaneScreen;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PageLayout {
    Flow,
    Workspace,
}

impl PageLayout {
    pub(super) const fn for_screen(screen: ControlPlaneScreen) -> Self {
        match screen {
            ControlPlaneScreen::Projects
            | ControlPlaneScreen::AgentsAndMcp
            | ControlPlaneScreen::EdaServices
            | ControlPlaneScreen::SessionsAndTasks
            | ControlPlaneScreen::SecretsVault => Self::Workspace,
            ControlPlaneScreen::Overview
            | ControlPlaneScreen::Documents
            | ControlPlaneScreen::Semantics
            | ControlPlaneScreen::ChangesAndApprovals
            | ControlPlaneScreen::BomAndExport
            | ControlPlaneScreen::Plugins
            | ControlPlaneScreen::Usage
            | ControlPlaneScreen::Settings => Self::Flow,
        }
    }
}

/// Ordinary vertically scrolling content. Set an explicit `.id(...)` BEFORE
/// calling this, especially when a loop or a helper creates multiple regions.
pub(super) trait ScrollRegionExt:
    InteractiveElement + Styled + ParentElement + Element + ScrollableElement + Sized
{
    fn scroll_y(mut self) -> Scrollable<Self> {
        let id = self
            .interactivity()
            .element_id
            .clone()
            .expect("scroll regions require an explicit stable element ID");
        // Preserve the intrinsic minimum height for auto-sized dialog bodies.
        // Forcing min_h(0) here collapses them before their content is measured.
        // The scroll wrapper handles flex shrink; bounded panes may opt into
        // min_h(0) explicitly once their parent supplies a viewport height.
        self.min_w(px(0.)).overflow_y_scrollbar().id(id)
    }
}
impl ScrollRegionExt for Stateful<Div> {}

/// An auto-height form grows from its content, then scrolls within an outer cap.
/// Keeping max-height off the scroll content avoids clipping its measured extent.
#[derive(IntoElement)]
pub(super) struct CappedScrollBody {
    body: Stateful<Div>,
    max_height: Pixels,
}

pub(super) fn capped_scroll_body(id: &'static str, max_height: Pixels) -> CappedScrollBody {
    CappedScrollBody { body: div().id(id).w_full().flex_1(), max_height }
}

impl Styled for CappedScrollBody {
    fn style(&mut self) -> &mut StyleRefinement {
        self.body.style()
    }
}

impl ParentElement for CappedScrollBody {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for CappedScrollBody {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().w_full().v_flex().max_h(self.max_height).child(self.body.scroll_y())
    }
}

/// Page spacing lives here. The shell supplies the scrolling policy.
pub(super) fn page(id: &'static str) -> Stateful<Div> {
    div().id(id).w_full().min_w(px(0.)).min_h(px(0.)).v_flex().gap_4().p_6()
}

/// A workspace fills its viewport so list and detail panes receive bounded heights.
pub(super) fn workspace_page(id: &'static str) -> Stateful<Div> {
    page(id).h_full().overflow_hidden()
}

pub(super) fn page_host(screen: ControlPlaneScreen, content: AnyElement) -> AnyElement {
    let host =
        div().id(format!("page-host-{}", screen.label())).size_full().min_w(px(0.)).min_h(px(0.));
    match PageLayout::for_screen(screen) {
        PageLayout::Flow => host.scroll_y().child(content).into_any_element(),
        PageLayout::Workspace => host.overflow_hidden().child(content).into_any_element(),
    }
}
