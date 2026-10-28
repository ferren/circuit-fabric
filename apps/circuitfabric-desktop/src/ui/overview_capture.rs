//! Explicit native GPU capture for validation builds. Absent from ordinary native-ui builds.
use super::*;

pub(super) fn requested_bounds() -> Option<gpui::WindowBounds> {
    std::env::var_os("CF_OVERVIEW_CAPTURE")?;
    let dimensions =
        std::env::var("CF_OVERVIEW_CAPTURE_SIZE").unwrap_or_else(|_| "1400,1400".into());
    let (width, height) = dimensions.split_once(',')?;
    let size = gpui::size(px(width.parse::<f32>().ok()?), px(height.parse::<f32>().ok()?));
    Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(gpui::point(px(40.), px(40.)), size)))
}

impl ControlPlaneView {
    pub(super) fn schedule_overview_capture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = std::env::var_os("CF_OVERVIEW_CAPTURE").map(PathBuf::from) else {
            return;
        };
        // Optional screen/panel selection for the capture: overview (default),
        // changes, eda, sessions, or messages (message panel open).
        let capture_screen = std::env::var("CF_CAPTURE_SCREEN").unwrap_or_default();
        let capture_wait = std::env::var("CF_CAPTURE_WAIT")
            .ok()
            .and_then(|wait| wait.parse::<u64>().ok())
            .unwrap_or(3);
        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(Duration::from_secs(capture_wait)).await;
            cx.update(|window, cx| {
                let model = view.update(cx, |view, cx| {
                    match capture_screen.as_str() {
                        "changes" | "eda" | "sessions" => {
                            let busiest = view
                                .project_data
                                .iter()
                                .max_by_key(|(id, data)| {
                                    (data.change_sets.len(), data.session_listing.sessions.len(), id.as_str())
                                })
                                .map(|(id, _)| id.clone());
                            if let Some(project) = busiest {
                                view.select_project(project, cx);
                            }
                            view.navigation.screen = match capture_screen.as_str() {
                                "changes" => ControlPlaneScreen::ChangesAndApprovals,
                                "eda" => ControlPlaneScreen::EdaServices,
                                _ => ControlPlaneScreen::SessionsAndTasks,
                            };
                        }
                        "messages" => {
                            view.message_panel_open = true;
                        }
                        _ => (),
                    }
                    cx.notify();
                    view.overview_model()
                })
                .ok();
                let _ = window.draw(cx);
                // Uses the real Windows platform renderer, including native glyph shaping,
                // clipping, chart paths, and layout. No mock/raster reconstruction is involved.
                match window.render_to_image() {
                    Ok(image) => {
                        if let Err(error) = image.save(&path) { eprintln!("Native capture failed: {error}"); }
                        if let Some(model) = model {
                            let metadata = serde_json::json!({
                                "renderer": "native Windows DirectX", "scale": window.scale_factor(),
                                "viewport": [window.viewport_size().width.as_f32(), window.viewport_size().height.as_f32()],
                                "projects": model.resources.len(), "loaded_projects": model.loaded_projects,
                                "documents": model.documents, "pending": model.pending, "sessions": model.sessions.len(),
                                "facts": model.facts, "resources": model.resources.iter().map(|row| (&row.id, row.counts)).collect::<Vec<_>>()
                            });
                            if let Err(error) = std::fs::write(path.with_extension("json"), metadata.to_string()) { eprintln!("Capture metadata failed: {error}"); }
                        }
                    }
                    Err(error) => eprintln!("Native capture failed: {error}"),
                }
                cx.quit();
            }).ok();
        }).detach();
    }
}
