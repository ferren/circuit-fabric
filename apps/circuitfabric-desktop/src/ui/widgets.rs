//! Stateless reusable presentation primitives.
use super::{language::UiLanguage, layout::ScrollRegionExt as _, theme::*};
use gpui::{
    Div, ElementId, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled,
    div, prelude::FluentBuilder as _, px,
};
use gpui_base::InputBase;
use gpui_component::{StyledExt, button::Button, input::InputState};

/// Actions keep their intrinsic width even inside a stretching column or grid.
/// Pages customize labels, variants and handlers through the component API.
pub(super) fn action_button(id: impl Into<ElementId>) -> Button {
    let id = id.into();
    let selector = format!("action-button-{id}");
    Button::new(id).self_start().flex_none().debug_selector(move || selector)
}

/// One read-only key/value row in a settings summary.
pub(super) fn settings_summary_row(label: &str, value: String) -> Div {
    div()
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .w(px(96.))
                .flex_none()
                .text_xs()
                .text_color(rgb(TEXT_MUTED))
                .child(label.to_owned()),
        )
        .child(div().flex_1().min_w(px(0.)).text_sm().whitespace_normal().child(value))
}

/// The right-hand pane of a two-pane page: bounded to the available height and
/// self-scrolling, so overflowing content scrolls inside the card's own frame instead
/// of drawing past it — the pattern the Jev judgment page already established.
pub(super) fn detail_pane(
    id: &'static str,
) -> gpui_component::scroll::Scrollable<gpui::Stateful<Div>> {
    div().id(id).flex_1().min_w(px(0.)).min_h(px(0.)).scroll_y().v_flex()
}
pub(super) fn agents_group_label(label: &'static str) -> impl IntoElement {
    div()
        .pt_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_MUTED))
        .child(label)
}

/// The API-key boundary note shown wherever credentials are referenced.
pub(super) fn info_note(
    zh: &'static str,
    en: &'static str,
    language: UiLanguage,
) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap_2p5()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(rgb(ACCENT_SOFT))
        .bg(rgb(0x00f0_f9ff))
        .child(
            div()
                .px_1p5()
                .py_0p5()
                .flex_none()
                .rounded_sm()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x000e_7490))
                .bg(rgb(0x00e0_f2fe))
                .child(language.choose("密钥边界", "Key boundary")),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .whitespace_normal()
                .text_xs()
                .text_color(rgb(TEXT_SECONDARY))
                .child(language.choose(zh, en)),
        )
}

pub(super) fn labeled_field(
    label: &'static str,
    id: &'static str,
    hint: Option<&'static str>,
    state: &Entity<InputState>,
) -> impl IntoElement {
    div()
        .debug_selector(move || id.to_owned())
        .v_flex()
        .gap_1()
        .flex_1()
        .min_w(px(240.))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
        .child(
            div()
                .h(px(36.))
                .px_2()
                .flex()
                .items_center()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    InputBase::new(id).flex_1().h_full().flex().items_center().child(state.clone()),
                ),
        )
        .when_some(hint, |this, hint| {
            this.child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(hint))
        })
}

pub(super) fn project_empty_state(
    title: &'static str,
    description: &'static str,
) -> impl IntoElement {
    div()
        .v_flex()
        .gap_2()
        .items_center()
        .justify_center()
        .h_full()
        .text_center()
        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(description))
}
