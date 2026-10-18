//! Shared semantic colors and theme resolution.
use gpui::{
    App, Background, BoxShadow, Hsla, linear_color_stop, linear_gradient, point, px,
    rgb as gpui_rgb,
};
use std::sync::atomic::{AtomicBool, Ordering};

// Design tokens: a deep-space sidebar, a layered content surface, and a
// cyan → violet accent pair used for glows and gradients.
pub(super) const SIDEBAR_BG: u32 = 0x000b_1220;
pub(super) const SIDEBAR_DIVIDER: u32 = 0x001a_2538;
pub(super) const SIDEBAR_GROUP: u32 = 0x005b_6b82;
pub(super) const SIDEBAR_TEXT: u32 = 0x009f_b0c6;
pub(super) const SIDEBAR_TEXT_ACTIVE: u32 = 0x00f0_f7ff;
pub(super) const SIDEBAR_ITEM_HOVER: u32 = 0x0012_1d30;
pub(super) const SIDEBAR_ITEM_ACTIVE: u32 = 0x0010_2438;
pub(super) const SIDEBAR_ITEM_PRESSED: u32 = 0x0014_3048;
pub(super) const ACCENT: u32 = 0x0022_d3ee;
pub(super) const ACCENT_SOFT: u32 = 0x0067_e8f9;
// Second accent: the violet end of the brand gradient.
pub(super) const ACCENT_ALT: u32 = 0x00a7_8bfa;
// Background of the data row a search hit navigated to.
pub(super) const FOCUSED_ROW_BG: u32 = 0x00fe_f9c3;

pub(super) const SURFACE_BG: u32 = 0x00f3_f6fb;
pub(super) const CARD_BG: u32 = 0x00ff_ffff;
pub(super) const BORDER: u32 = 0x00e2_e8f0;
pub(super) const TEXT_PRIMARY: u32 = 0x000f_172a;
pub(super) const TEXT_SECONDARY: u32 = 0x0047_5563;
pub(super) const TEXT_MUTED: u32 = 0x006b_7280;

// All existing UI colour calls pass through this small semantic-token resolver.
// It lets the complete shell (including pages implemented before preferences
// existed) react immediately to a theme change rather than only repainting
// the Settings page.
pub(super) static DARK_MODE: AtomicBool = AtomicBool::new(true);

pub(super) fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

pub(super) fn rgb(light_color: u32) -> gpui::Rgba {
    let color = if is_dark() {
        match light_color {
            SURFACE_BG => 0x0007_0b14,
            CARD_BG => 0x000c_1322,
            BORDER => 0x001c_2a40,
            TEXT_PRIMARY => 0x00e6_edf7,
            TEXT_SECONDARY => 0x00a9_b6c9,
            TEXT_MUTED => 0x0071_819a,
            SIDEBAR_BG => 0x0005_0810,
            SIDEBAR_DIVIDER => 0x0015_2036,
            SIDEBAR_ITEM_HOVER => 0x000e_1a2e,
            SIDEBAR_ITEM_ACTIVE => 0x000f_2236,
            SIDEBAR_ITEM_PRESSED => 0x0013_304a,
            // Pale informational / status chips need dark, tinted surfaces.
            0x00e0_f2fe => 0x000b_2a3d,
            0x00f0_f9ff => 0x000a_1d2c,
            0x000e_7490 => 0x0067_e8f9,
            0x00dc_fce7 => 0x000b_2e1f,
            0x0016_a34a => 0x004a_de80,
            0x00ff_f7ed => 0x002d_1a0a,
            0x00b4_5309 => 0x00fb_bf24,
            0x00fe_f3c7 => 0x002e_2509,
            0x00fe_f2f2 => 0x002d_0f12,
            0x00dc_2626 => 0x00f8_7171,
            0x00b9_1c1c => 0x00fc_a5a5,
            0x00fe_f08a | FOCUSED_ROW_BG => 0x003a_330a,
            other => other,
        }
    } else {
        light_color
    };
    gpui_rgb(color)
}

/// A resolved token with an alpha channel, for glows and translucent layers.
pub(super) fn rgb_alpha(color: u32, alpha: f32) -> Hsla {
    let mut hsla: Hsla = rgb(color).into();
    hsla.a = alpha;
    hsla
}

/// The cyan → violet brand gradient.
pub(super) fn accent_gradient(angle: f32) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(rgb(ACCENT), 0.),
        linear_color_stop(rgb(ACCENT_ALT), 1.),
    )
}

/// A soft accent glow around an element.
pub(super) fn accent_glow(alpha: f32, blur: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgb_alpha(ACCENT, alpha),
        offset: point(px(0.), px(0.)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// Aligns gpui-component widgets (buttons, inputs, tabs, scrollbars) with the
/// shell palette so the whole window switches mode together.
pub(super) fn sync_component_theme(dark: bool, cx: &mut App) {
    use gpui_component::{Theme, ThemeMode};
    DARK_MODE.store(dark, Ordering::Relaxed);
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    Theme::update(cx, |theme| {
        let accent: Hsla = rgb(ACCENT).into();
        theme.colors.primary = if dark { accent } else { gpui_rgb(0x0008_91b2).into() };
        theme.colors.primary_hover = if dark { rgb(ACCENT_SOFT).into() } else { accent };
        theme.colors.primary_active = gpui_rgb(0x0006_b6d4).into();
        theme.colors.primary_foreground =
            if dark { gpui_rgb(0x0005_0810).into() } else { gpui_rgb(0x00ff_ffff).into() };
        theme.colors.ring = accent;
        if dark {
            theme.colors.background = rgb(CARD_BG).into();
            theme.colors.foreground = rgb(TEXT_PRIMARY).into();
            theme.colors.border = rgb(BORDER).into();
            theme.colors.input = rgb(BORDER).into();
            theme.colors.muted_foreground = rgb(TEXT_MUTED).into();
            theme.colors.popover = rgb(CARD_BG).into();
        }
    });
}
