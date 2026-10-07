//! CircuitFabric desktop composition entry point.
#![deny(unsafe_code)]

#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
mod application;
#[cfg(all(feature = "native-ui", windows))]
#[allow(unsafe_code)]
mod pdf_cursors;
#[cfg(feature = "native-ui")]
mod pdf_text_layer;
#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
mod pdf_zoom;
#[cfg(feature = "native-ui")]
mod ui;
#[cfg(all(feature = "native-ui", windows))]
#[allow(unsafe_code)]
mod windows_icon;

fn main() {
    if circuitfabric_codex_runtime::judge::run_from_args() {
        return;
    }
    #[cfg(feature = "native-ui")]
    ui::run();
    #[cfg(not(feature = "native-ui"))]
    println!("CircuitFabric desktop scaffold. Rebuild with --features native-ui to start GPUI.");
}
