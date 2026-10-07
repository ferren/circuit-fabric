//! Shared semantic colors and theme resolution.
use gpui::rgb as gpui_rgb;
use std::sync::atomic::{AtomicBool, Ordering};

// Design tokens: a dark-navy sidebar, a light content surface, and a cyan accent.
pub(super) const SIDEBAR_BG: u32 = 0x000f_172b;
pub(super) const SIDEBAR_DIVIDER: u32 = 0x001e_293b;
pub(super) const SIDEBAR_GROUP: u32 = 0x005f_7085;
pub(super) const SIDEBAR_TEXT: u32 = 0x009c_a7b8;
pub(super) const SIDEBAR_TEXT_ACTIVE: u32 = 0x00f1_f5fa;
pub(super) const SIDEBAR_ITEM_HOVER: u32 = 0x001a_2637;
pub(super) const SIDEBAR_ITEM_ACTIVE: u32 = 0x001e_2d46;
pub(super) const SIDEBAR_ITEM_PRESSED: u32 = 0x0026_3756;
pub(super) const ACCENT: u32 = 0x0022_d3ee;
pub(super) const ACCENT_SOFT: u32 = 0x0067_e8f9;
// Background of the data row a search hit navigated to.
pub(super) const FOCUSED_ROW_BG: u32 = 0x00fe_f9c3;

pub(super) const SURFACE_BG: u32 = 0x00f1_f5f9;
pub(super) const CARD_BG: u32 = 0x00ff_ffff;
pub(super) const BORDER: u32 = 0x00e2_e8f0;
pub(super) const TEXT_PRIMARY: u32 = 0x000f_172a;
pub(super) const TEXT_SECONDARY: u32 = 0x0047_5563;
pub(super) const TEXT_MUTED: u32 = 0x006b_7280;

// All existing UI colour calls pass through this small semantic-token resolver.
// It lets the complete shell (including pages implemented before preferences
// existed) react immediately to a theme change rather than only repainting
// the Settings page.
pub(super) static DARK_MODE: AtomicBool = AtomicBool::new(false);

pub(super) fn rgb(light_color: u32) -> gpui::Rgba {
    let color = if DARK_MODE.load(Ordering::Relaxed) {
        match light_color {
            SURFACE_BG => 0x000f_172a,
            CARD_BG => 0x0011_1827,
            BORDER => 0x0033_4155,
            TEXT_PRIMARY => 0x00f8_fafc,
            TEXT_SECONDARY => 0x00cbd5e1,
            TEXT_MUTED => 0x0094_a3b8,
            SIDEBAR_BG => 0x0002_0612,
            SIDEBAR_DIVIDER => 0x0033_4155,
            SIDEBAR_GROUP => 0x0094_a3b8,
            SIDEBAR_TEXT => 0x00cbd5e1,
            SIDEBAR_TEXT_ACTIVE => 0x00f8_fafc,
            SIDEBAR_ITEM_HOVER => 0x001e_293b,
            SIDEBAR_ITEM_ACTIVE => 0x001e_293b,
            SIDEBAR_ITEM_PRESSED => 0x0033_4155,
            // The pale informational chips need a dark surface as well.
            0x00e0_f2fe => 0x0016_344a,
            0x000e_7490 => 0x007dd3fc,
            other => other,
        }
    } else {
        light_color
    };
    gpui_rgb(color)
}
