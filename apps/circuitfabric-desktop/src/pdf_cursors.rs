//! Custom grab/grabbing cursors for the PDF preview (Windows).
//!
//! Windows ships no system grab cursors, and the pinned GPUI backend maps
//! `OpenHand`/`ClosedHand` to the plain arrow, so the preview previously fell back to the
//! pointing hand. This module renders its own 32×32 open-hand and closed-hand cursors
//! (white palm, black contour and finger creases), loads them as Win32 cursors,
//! and shows them through a window subclass that intercepts `WM_SETCURSOR` while the
//! pointer is over the PDF page area.
//!
//! State flow: the preview's canvas paints the page-area bounds every frame
//! (`set_area`), the pan handlers update the drag flag (`set_dragging`), and a
//! window-level mouse-move hook (`refresh`) recomputes whether the grab or grabbing
//! cursor should be active. `clear` drops the area when the pane closes.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, AtomicU32, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GWLP_WNDPROC, HCURSOR, IDC_IBEAM, IMAGE_CURSOR, LR_LOADFROMFILE, LR_SHARED,
    LoadCursorW, LoadImageW, SetCursor, SetWindowLongPtrW, WM_SETCURSOR, WM_USER,
};
use windows::core::PCWSTR;

/// Which pan cursor is active over the PDF area.
const PAN_NONE: u8 = 0;
const PAN_GRAB: u8 = 1;
const PAN_GRABBING: u8 = 2;
const PAN_TEXT: u8 = 3;

static PAN_STATE: AtomicU8 = AtomicU8::new(PAN_NONE);
static PAN_DRAGGING: AtomicBool = AtomicBool::new(false);
static PAN_AREA: PageArea = PageArea::new();
static ORIGINAL_PROC: AtomicIsize = AtomicIsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static GRAB_CURSOR: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
static GRABBING_CURSOR: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
static TEXT_CURSOR: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
static CURSOR_LOAD_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// The page-area rectangle in window coordinates. Paint and mouse handlers both run on
/// the UI thread, so a mutex is enough.
struct PageArea(std::sync::Mutex<Option<(f32, f32, f32, f32)>>);

impl PageArea {
    const fn new() -> Self {
        Self(std::sync::Mutex::new(None))
    }
}

/// Installs the cursor intercept on this window and builds the cursor images. Safe to call
/// repeatedly; only the first call takes effect.
pub fn install(window: &gpui::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if INSTALLED.load(Ordering::Acquire) {
        return;
    }
    let Ok(raw) = HasWindowHandle::window_handle(window) else { return };
    let RawWindowHandle::Win32(win32) = raw.as_raw() else { return };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    // The cursors are lazily rendered on first use so a failure never blocks startup.
    unsafe {
        let previous =
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, intercept_proc as *const () as usize as isize);
        if previous != 0 {
            ORIGINAL_PROC.store(previous, Ordering::Release);
            INSTALLED.store(true, Ordering::Release);
        }
    }
}

/// Records the PDF page area in window coordinates; `None` when the pane is not showing
/// raster pages (closed or on the data tab).
pub fn set_area(area: Option<(f32, f32, f32, f32)>) {
    *PAN_AREA.0.lock().expect("pan area lock") = area;
    if area.is_none() {
        PAN_STATE.store(PAN_NONE, Ordering::Release);
    }
}

/// Updates whether a pan drag is currently held.
pub fn set_dragging(dragging: bool) {
    PAN_DRAGGING.store(dragging, Ordering::Release);
}

/// Drops all pan-cursor state; called when the preview pane closes.
pub fn clear() {
    set_area(None);
    PAN_DRAGGING.store(false, Ordering::Release);
    PAN_STATE.store(PAN_NONE, Ordering::Release);
}

/// Recomputes the active pan cursor for a pointer position in window coordinates.
pub fn refresh(position: (f32, f32)) {
    let area = *PAN_AREA.0.lock().expect("pan area lock");
    let state = match area {
        Some((x, y, width, height))
            if position.0 >= x
                && position.0 <= x + width
                && position.1 >= y
                && position.1 <= y + height =>
        {
            if PAN_DRAGGING.load(Ordering::Acquire) {
                PAN_GRABBING
            } else if crate::pdf_text_layer::over_text(gpui::point(
                gpui::px(position.0),
                gpui::px(position.1),
            )) {
                PAN_TEXT
            } else {
                PAN_GRAB
            }
        }
        _ => PAN_NONE,
    };
    if PAN_STATE.swap(state, Ordering::AcqRel) != state
        && let Some(handle) = cursor_for(state)
    {
        // Update immediately even when moving between text and paper inside one hitbox.
        unsafe { SetCursor(Some(HCURSOR(handle as *mut _))) };
    }
}

fn cursor_for(state: u8) -> Option<isize> {
    if state == PAN_TEXT {
        let handle = *TEXT_CURSOR.get_or_init(|| unsafe {
            LoadCursorW(None, IDC_IBEAM).map_or(0, |cursor| cursor.0 as isize)
        });
        return (handle != 0).then_some(handle);
    }
    let slot = match state {
        PAN_GRAB => &GRAB_CURSOR,
        PAN_GRABBING => &GRABBING_CURSOR,
        _ => return None,
    };
    let handle = *slot.get_or_init(|| load_cursor(state == PAN_GRABBING));
    (handle != 0).then_some(handle)
}

/// Renders one hand cursor, packs it into a `.cur` file (32bpp BMP entry — the classic
/// format `LoadImage` accepts for cursors; PNG-compressed entries are icon-only), and
/// loads it as an HCURSOR.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn load_cursor(grabbing: bool) -> isize {
    let size = 32_u32;
    let rgba = render_hand(grabbing);

    // BITMAPINFOHEADER: height counts the color bitmap plus the AND mask.
    let mut entry = Vec::with_capacity(40 + (size * size * 4) as usize + 128);
    entry.extend_from_slice(&40u32.to_le_bytes()); // biSize
    entry.extend_from_slice(&size.to_le_bytes()); // biWidth
    entry.extend_from_slice(&(size * 2).to_le_bytes()); // biHeight (color + mask)
    entry.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    entry.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    entry.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    entry.extend_from_slice(&((size * size * 4) + 128).to_le_bytes()); // biSizeImage
    entry.extend_from_slice(&[0u8; 16]); // resolution, colors, important
    // Color bitmap: bottom-up BGRA rows.
    for y in (0..size).rev() {
        for x in 0..size {
            let index = ((y * size) + x) * 4;
            entry.push(rgba[index as usize + 2]); // blue
            entry.push(rgba[index as usize + 1]); // green
            entry.push(rgba[index as usize]); // red
            entry.push(rgba[index as usize + 3]); // alpha
        }
    }
    // AND mask: 1bpp rows bottom-up, padded to 32 bits; zero paints the color bitmap.
    for _ in 0..size {
        entry.extend_from_slice(&[0u8; 4]);
    }

    // CUR container: header, one directory entry with the hotspot, then the image payload.
    let mut cursor = Vec::with_capacity(entry.len() + 22);
    cursor.extend_from_slice(&0u16.to_le_bytes()); // reserved
    cursor.extend_from_slice(&2u16.to_le_bytes()); // 2 = cursor
    cursor.extend_from_slice(&1u16.to_le_bytes()); // one image
    cursor.push(size as u8);
    cursor.push(size as u8);
    cursor.push(0); // color count
    cursor.push(0); // reserved
    cursor.extend_from_slice(&16u16.to_le_bytes()); // hotspot x
    cursor.extend_from_slice(&16u16.to_le_bytes()); // hotspot y
    cursor.extend_from_slice(&(entry.len() as u32).to_le_bytes());
    cursor.extend_from_slice(&22u32.to_le_bytes()); // image offset
    cursor.extend_from_slice(&entry);

    let path = std::env::temp_dir().join(format!(
        "circuitfabric-hand-{}-{}.cur",
        std::process::id(),
        CURSOR_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    ));
    if std::fs::write(&path, cursor).is_err() {
        return 0;
    }
    // PCWSTR is a raw pointer, so the UTF-16 path must carry its own NUL terminator.
    let mut wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
    wide.push(0);
    // SAFETY: a plain path load with no module handle; the shared cursor outlives the
    // process and is never destroyed.
    let handle = unsafe {
        LoadImageW(None, PCWSTR(wide.as_ptr()), IMAGE_CURSOR, 0, 0, LR_SHARED | LR_LOADFROMFILE)
            .map_or(0, |handle| handle.0 as isize)
    };
    // LoadImage has copied the payload; no shared filename or persistent file is needed.
    let _ = std::fs::remove_file(path);
    handle
}

unsafe extern "system" fn intercept_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = PAN_STATE.load(Ordering::Acquire);
    if state != PAN_NONE {
        // GPUI's own cursor-style message would stomp the pan cursor; the pinned backend
        // cannot represent grab cursors, so its updates are dropped while a pan cursor is
        // active over the page area.
        if message == WM_USER + 1 {
            return LRESULT(0);
        }
        if message == WM_SETCURSOR
            && let Some(handle) = cursor_for(state)
            && handle != 0
        {
            // SAFETY: `handle` came from LoadImageW and stays valid for the process.
            unsafe { SetCursor(Some(HCURSOR(handle as *mut _))) };
            return LRESULT(1);
        }
    }
    let previous = ORIGINAL_PROC.load(Ordering::Acquire);
    if previous == 0 {
        return LRESULT(0);
    }
    // SAFETY: the stored procedure is the window's original GPUI handler.
    unsafe {
        CallWindowProcW(
            Some(std::mem::transmute::<
                isize,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(previous)),
            hwnd,
            message,
            wparam,
            lparam,
        )
    }
}

/// Renders a 32×32 hand cursor as RGBA: white palm with black outline, drawn with
/// signed-distance shapes and 4× supersampling. `false` is the open grab hand, `true` the
/// closed grabbing fist.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn render_hand(grabbing: bool) -> Vec<u8> {
    const SIZE: usize = 32;
    const SCALE: usize = 4;
    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut inside = 0_usize;
            let mut outline = 0_usize;
            for sub_y in 0..SCALE {
                for sub_x in 0..SCALE {
                    let point = (
                        x as f32 + (sub_x as f32 + 0.5) / SCALE as f32,
                        y as f32 + (sub_y as f32 + 0.5) / SCALE as f32,
                    );
                    let distance = hand_distance(point, grabbing);
                    if distance <= -0.65 && crease_distance(point, grabbing) > 0.48 {
                        inside += 1;
                    } else if distance <= 0.65 {
                        outline += 1;
                    }
                }
            }
            let samples = SCALE * SCALE;
            let coverage = inside + outline;
            if coverage == 0 {
                continue;
            }
            // Straight-alpha compositing: white fill and black ink, with the mix
            // and opacity driven by the sub-sample coverage of each.
            let alpha = ((coverage * 255) / samples).min(255) as u8;
            let luminance = ((inside * 255) / coverage.max(1)).min(255) as u8;
            let index = (y * SIZE + x) * 4;
            pixels[index] = luminance;
            pixels[index + 1] = luminance;
            pixels[index + 2] = luminance;
            pixels[index + 3] = alpha;
        }
    }
    pixels
}

/// Short ink strokes distinguish folded fingers and the thumb from a solid silhouette.
fn crease_distance(point: (f32, f32), grabbing: bool) -> f32 {
    let strokes = if grabbing {
        &[
            ((12.5, 13.0), (12.5, 16.0)),
            ((16.0, 12.5), (16.0, 16.0)),
            ((19.5, 12.5), (19.5, 16.0)),
            ((11.0, 18.0), (15.0, 20.8)),
            ((15.0, 20.8), (19.0, 20.8)),
        ][..]
    } else {
        &[
            ((13.9, 14.5), (13.9, 17.5)),
            ((17.7, 14.5), (17.7, 17.0)),
            ((21.4, 15.0), (21.4, 17.5)),
            ((10.0, 20.0), (12.0, 22.0)),
            ((14.0, 24.0), (19.5, 24.0)),
        ][..]
    };
    strokes.iter().map(|&(start, end)| segment(point, start, end, 0.0)).fold(f32::MAX, f32::min)
}

/// Signed distance to the hand silhouette; negative inside.
fn hand_distance(point: (f32, f32), grabbing: bool) -> f32 {
    if grabbing {
        // Fist: palm, folded finger band, knuckle bumps, thumb across.
        [
            rounded_box(point, (16.5, 19.5), (6.2, 5.6), 3.5),
            rounded_box(point, (16.5, 15.5), (5.6, 2.2), 2.2),
            circle(point, (11.5, 12.8), 1.9),
            circle(point, (15.0, 12.0), 1.9),
            circle(point, (18.5, 12.0), 1.9),
            circle(point, (22.0, 12.8), 1.9),
            segment(point, (9.5, 17.5), (13.5, 21.0), 1.9),
        ]
        .into_iter()
        .fold(f32::MAX, f32::min)
    } else {
        // Open hand: four extended fingers, palm, thumb out to the left.
        [
            segment(point, (12.0, 8.0), (12.0, 15.0), 1.7),
            segment(point, (15.8, 6.0), (15.8, 15.0), 1.7),
            segment(point, (19.6, 6.5), (19.6, 15.0), 1.7),
            segment(point, (23.2, 8.5), (23.2, 15.5), 1.7),
            rounded_box(point, (17.0, 20.5), (6.8, 4.8), 3.5),
            segment(point, (7.0, 17.0), (11.5, 22.0), 1.7),
        ]
        .into_iter()
        .fold(f32::MAX, f32::min)
    }
}

fn circle(point: (f32, f32), center: (f32, f32), radius: f32) -> f32 {
    ((point.0 - center.0).powi(2) + (point.1 - center.1).powi(2)).sqrt() - radius
}

fn segment(point: (f32, f32), start: (f32, f32), end: (f32, f32), radius: f32) -> f32 {
    let length_squared = (end.0 - start.0).powi(2) + (end.1 - start.1).powi(2);
    let t = (((point.0 - start.0) * (end.0 - start.0) + (point.1 - start.1) * (end.1 - start.1))
        / length_squared)
        .clamp(0.0, 1.0);
    let closest = (start.0 + t * (end.0 - start.0), start.1 + t * (end.1 - start.1));
    ((point.0 - closest.0).powi(2) + (point.1 - closest.1).powi(2)).sqrt() - radius
}

fn rounded_box(point: (f32, f32), center: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let corner = (
        (point.0 - center.0).abs() - half.0 + radius,
        (point.1 - center.1).abs() - half.1 + radius,
    );
    let outside = (corner.0.max(0.0), corner.1.max(0.0));
    let length = (outside.0.powi(2) + outside.1.powi(2)).sqrt();
    length + corner.0.max(corner.1).min(0.0) - radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_images_render_covered_shapes_with_transparent_margins() {
        for grabbing in [false, true] {
            let pixels = render_hand(grabbing);
            assert_eq!(pixels.len(), 32 * 32 * 4);
            let mut covered = 0;
            let mut filled = 0;
            for pixel in pixels.chunks_exact(4) {
                if pixel[3] > 200 {
                    covered += 1;
                    if pixel[0] < 64 {
                        filled += 1;
                    }
                }
            }
            assert!(covered > 120, "the {grabbing} hand silhouette covers a real area");
            assert!(filled < 400, "the {grabbing} hand is a cursor, not a blob");
            assert!(covered > filled + 40, "the palm must have a visible white interior");
            // Margins stay transparent: corners of the canvas are outside every shape.
            for (x, y) in [(0_usize, 0_usize), (31, 0), (0, 31), (31, 31)] {
                assert_eq!(pixels[(y * 32 + x) * 4 + 3], 0);
            }
        }
        assert_ne!(render_hand(false), render_hand(true));
        if let Some(path) = std::env::var_os("CIRCUITFABRIC_CURSOR_PREVIEW") {
            let mut preview =
                image::RgbaImage::from_pixel(64, 32, image::Rgba([200, 200, 200, 255]));
            for (x, grabbing) in [(0_i64, false), (32, true)] {
                let hand = image::RgbaImage::from_raw(32, 32, render_hand(grabbing)).unwrap();
                image::imageops::overlay(&mut preview, &hand, x, 0);
            }
            image::imageops::resize(&preview, 512, 256, image::imageops::FilterType::Nearest)
                .save(path)
                .unwrap();
        }
    }

    #[test]
    fn pan_state_follows_pointer_drag_and_area() {
        clear();
        refresh((0.0, 0.0));
        set_area(Some((10.0, 10.0, 100.0, 200.0)));
        refresh((50.0, 50.0));
        assert_eq!(PAN_STATE.load(Ordering::Relaxed), PAN_GRAB);
        set_dragging(true);
        refresh((50.0, 50.0));
        assert_eq!(PAN_STATE.load(Ordering::Relaxed), PAN_GRABBING);
        refresh((5.0, 5.0));
        assert_eq!(PAN_STATE.load(Ordering::Relaxed), PAN_NONE);
        clear();
    }

    #[test]
    fn cursors_load_through_the_win32_pipeline() {
        for grabbing in [false, true] {
            let handle = load_cursor(grabbing);
            if handle == 0 {
                panic!("the {grabbing} hand cursor must load as a Win32 cursor");
            }
        }
        assert_eq!(cursor_for(PAN_GRAB).unwrap(), *GRAB_CURSOR.get().unwrap());
        assert_eq!(cursor_for(PAN_GRABBING).unwrap(), *GRABBING_CURSOR.get().unwrap());
        assert_eq!(cursor_for(PAN_NONE), None);
    }
}
