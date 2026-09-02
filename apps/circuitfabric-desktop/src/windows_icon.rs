//! Windows-specific application icon support for GPUI windows.
//!
//! GPUI exposes the native window through `raw-window-handle`, while its public
//! window icon option is currently relevant to X11 only. Applying `WM_SETICON`
//! here keeps the platform adjustment within `CircuitFabric` instead of patching
//! GPUI or depending on its local source cache.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{ICON_BIG, ICON_SMALL, LoadIconW, SendMessageW, WM_SETICON},
    },
    core::w,
};

const RESOURCE_ID: windows::core::PCWSTR = w!("IDI_APP_ICON");

/// Applies the executable's embedded application icon to both Win32 icon slots.
///
/// `ICON_SMALL` drives the native title bar and Alt+Tab glyph; `ICON_BIG` is
/// used by surfaces such as the taskbar. The icon resource is shared by Windows
/// and therefore must not be destroyed by this process.
pub fn apply(window: &Window) -> Result<(), String> {
    let raw_window = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("GPUI did not provide a Win32 window handle: {error}"))?;
    let RawWindowHandle::Win32(win32) = raw_window.as_raw() else {
        return Ok(());
    };
    let hwnd = HWND(win32.hwnd.get() as *mut _);

    // `None` asks for this executable's module, which owns IDI_APP_ICON from
    // resources/circuitfabric.rc. The calls run while the GPUI window is alive.
    unsafe {
        let module = GetModuleHandleW(None)
            .map_err(|error| format!("could not find the CircuitFabric module: {error}"))?;
        let icon = LoadIconW(Some(HINSTANCE(module.0)), RESOURCE_ID)
            .map_err(|error| format!("could not load the embedded application icon: {error}"))?;

        let _ = SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_SMALL as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
        let _ = SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
    }

    Ok(())
}
