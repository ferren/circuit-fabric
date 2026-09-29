//! PDF opener: page rendering via pdfium, text extraction via `pdf-extract`.
//!
//! When a pdfium library can be located (see `native/README.md` for the search order), pages
//! are rasterized in-process into tight RGBA bitmaps — a paginated reader-like preview. The
//! vendored build ships with `pdf_enable_v8 = false` and `pdf_enable_xfa = false`, so it
//! contains no JavaScript engine and embedded document scripts cannot execute by
//! construction. When no library is available, the opener degrades to page-wise text
//! extraction instead of failing. In both modes embedded launch actions, attachments, and
//! other active content are never interpreted.

use circuitfabric_plugin_api::{
    DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest, DocumentPage, DocumentViewBody,
    MAX_DOCUMENT_VIEW_PAGES, OpenerCapability,
};

use crate::{capped_text, view};

/// Opens PDF managed copies as a read-only view: rasterized pages when pdfium is available,
/// paged text otherwise.
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOpener;

impl DocumentOpener for PdfOpener {
    fn capability(&self) -> OpenerCapability {
        OpenerCapability {
            opener_id: "builtin-pdf".to_owned(),
            label: "PDF document".to_owned(),
            extensions: vec!["pdf".to_owned()],
            mime_types: vec!["application/pdf".to_owned(), "application/x-pdf".to_owned()],
        }
    }

    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        #[cfg(feature = "raster-pdf")]
        if let Some(outcome) = raster::open(request) {
            return outcome;
        }
        open_text(request)
    }
}

/// The fallback view: page-wise text extraction.
fn open_text(request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
    let extracted = match pdf_extract::extract_text_from_mem_by_pages(request.managed_copy.data()) {
        Ok(pages) => pages,
        Err(error) => {
            return DocumentOpenerOutcome::Failed {
                reason: format!("the PDF could not be parsed: {error}"),
            };
        }
    };
    let truncated = extracted.len() > MAX_DOCUMENT_VIEW_PAGES;
    let pages: Vec<DocumentPage> = extracted
        .into_iter()
        .take(MAX_DOCUMENT_VIEW_PAGES)
        .enumerate()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(index, text)| DocumentPage {
            number: u32::try_from(index + 1).unwrap_or(u32::MAX),
            text: capped_text(text.trim()),
        })
        .collect();
    DocumentOpenerOutcome::Loaded {
        view: view("builtin-pdf", request, DocumentViewBody::PagedText { pages, truncated }),
    }
}

/// Every page's size in points (`width`, `height`), read without loading or rendering the
/// pages, so a host can lay out placeholders for pages it renders on demand. `None` when
/// pdfium is unavailable.
#[cfg(feature = "raster-pdf")]
#[must_use]
pub fn pdf_page_sizes(data: &[u8]) -> Option<Result<Vec<(f32, f32)>, String>> {
    raster::with_pdfium(|pdfium| raster::page_sizes(pdfium, data))
}

/// Renders the given 1-based pages at the preview resolution. The pdfium lock is held for
/// this batch only, so hosts render a few pages at a time as they scroll into view. `None`
/// when pdfium is unavailable.
#[cfg(feature = "raster-pdf")]
#[must_use]
pub fn render_pdf_pages(
    data: &[u8],
    numbers: &[u32],
) -> Option<Result<Vec<circuitfabric_plugin_api::DocumentRasterPage>, String>> {
    raster::with_pdfium(|pdfium| raster::render_numbers(pdfium, data, numbers))
}

/// Positioned text lines for the datasheet extractor; `None` when pdfium is unavailable.
#[cfg(feature = "raster-pdf")]
pub(super) fn positioned_lines(data: &[u8]) -> Option<Vec<crate::datasheet::Line>> {
    raster::with_pdfium(|pdfium| {
        let document = pdfium.load_pdf_from_byte_vec(data.to_vec(), None).ok()?;
        let mut lines = Vec::new();
        for page in document.pages().iter() {
            lines.extend(raster::positioned_page_lines(&page));
        }
        Some(lines)
    })
    .flatten()
}

/// Renders page bitmaps through a dynamically loaded pdfium library.
#[cfg(feature = "raster-pdf")]
mod raster {
    use circuitfabric_plugin_api::{
        DocumentOpenerOutcome, DocumentOpenerRequest, DocumentRasterPage, DocumentViewBody,
    };
    use pdfium_render::prelude::{PdfPage, Pdfium, PdfiumLibraryBindings};

    use super::view;
    use crate::datasheet::{Line, Word};

    /// Target bitmap width; the preview pane shows pages around this resolution.
    const TARGET_WIDTH: i32 = 900;
    /// Guard against pathological page aspect ratios producing enormous bitmaps.
    const MAX_HEIGHT: i32 = 3200;
    /// Pages rasterized by `open`, so the first screen appears quickly; hosts render the
    /// rest on demand with `render_pdf_pages`.
    const INITIAL_RASTER_PAGES: usize = 2;

    /// Runs the raster pipeline when a pdfium library is available, `None` otherwise (the
    /// caller falls back to text extraction).
    pub(super) fn open(request: &DocumentOpenerRequest) -> Option<DocumentOpenerOutcome> {
        with_pdfium(|pdfium| render(pdfium, request))
    }

    /// Whether a pdfium library was located on this machine.
    #[cfg(test)]
    pub(super) fn pdfium_available() -> bool {
        with_pdfium(|_| ()).is_some()
    }

    /// Binds pdfium for the duration of one call.
    ///
    /// pdfium-render's `thread_safe` bindings take a process-wide lock in `Pdfium::new`
    /// and release it only when the instance drops. The instance therefore must not
    /// outlive the call: a long-lived (e.g. thread-local) one would hold the lock for the
    /// thread's lifetime and block opens on every other thread indefinitely.
    pub(super) fn with_pdfium<T>(run: impl FnOnce(&Pdfium) -> T) -> Option<T> {
        let pdfium = Pdfium::new(bind_pdfium()?);
        Some(run(&pdfium))
    }

    /// Extracts one page's text as positioned lines: words grouped by y proximity and
    /// split on character gaps, in reading order.
    ///
    /// Positions preserve table columns, which the plain extraction fallback loses.
    pub(super) fn positioned_page_lines(page: &PdfPage<'_>) -> Vec<Line> {
        let Ok(text) = page.text() else {
            return Vec::new();
        };
        let mut lines: Vec<Line> = Vec::new();
        let mut current: Option<Line> = None;
        let mut current_top = 0.0_f32;
        let mut previous_right: Option<f32> = None;
        for character in text.chars().iter() {
            let Some(glyph) = character.unicode_char() else { continue };
            if glyph.is_whitespace() {
                // An explicit space always ends the current word.
                previous_right = None;
                continue;
            }
            let Ok(bounds) = character.loose_bounds() else { continue };
            let (left, right, top) =
                (bounds.left().value, bounds.right().value, bounds.top().value);
            let starts_line = current.as_ref().is_none_or(|_| (top - current_top).abs() > 2.5);
            if starts_line {
                if let Some(finished) = current.take() {
                    lines.push(finished);
                }
                current = Some(Line { words: Vec::new() });
                current_top = top;
                previous_right = None;
            }
            let line = current.as_mut().expect("a line was just started");
            let breaks_word = previous_right.is_none_or(|previous| left - previous > 1.2);
            if breaks_word {
                line.words.push(Word { text: glyph.to_string(), left, right });
            } else if let Some(word) = line.words.last_mut() {
                word.text.push(glyph);
                word.right = right;
            }
            previous_right = Some(right);
        }
        if let Some(finished) = current.take() {
            lines.push(finished);
        }
        lines
    }

    /// Search order documented in `native/README.md`: the environment override, the
    /// executable directory, `native/<platform>` beside nearby workspace ancestors, then
    /// the system library path.
    fn bind_pdfium() -> Option<Box<dyn PdfiumLibraryBindings>> {
        for candidate in library_candidates() {
            if let Ok(bindings) = Pdfium::bind_to_library(&candidate) {
                return Some(bindings);
            }
        }
        Pdfium::bind_to_system_library().ok()
    }

    fn library_candidates() -> Vec<std::path::PathBuf> {
        let mut candidates = Vec::new();
        if let Ok(from_env) = std::env::var("CIRCUITFABRIC_PDFIUM_PATH") {
            candidates.push(std::path::PathBuf::from(from_env));
        }
        if let Ok(executable) = std::env::current_exe()
            && let Some(directory) = executable.parent()
        {
            candidates.push(directory.join(library_file_name()));
            let mut ancestor = Some(directory);
            for _ in 0..4 {
                let Some(current) = ancestor else { break };
                candidates.push(
                    current.join("native").join(platform_directory()).join(library_file_name()),
                );
                ancestor = current.parent();
            }
        }
        candidates
    }

    #[cfg(windows)]
    fn library_file_name() -> &'static str {
        "pdfium.dll"
    }

    #[cfg(windows)]
    fn platform_directory() -> &'static str {
        "windows-x64"
    }

    #[cfg(not(windows))]
    fn library_file_name() -> &'static str {
        "libpdfium.so"
    }

    #[cfg(not(windows))]
    fn platform_directory() -> &'static str {
        "linux-x64"
    }

    fn render(pdfium: &Pdfium, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let document =
            match pdfium.load_pdf_from_byte_vec(request.managed_copy.data().to_vec(), None) {
                Ok(document) => document,
                Err(error) => {
                    return DocumentOpenerOutcome::Failed {
                        reason: format!("the PDF could not be parsed: {error}"),
                    };
                }
            };
        let page_count = usize::from(document.pages().len());
        let truncated = page_count > INITIAL_RASTER_PAGES;
        let mut pages = Vec::new();
        for (index, page) in document.pages().iter().enumerate().take(INITIAL_RASTER_PAGES) {
            match render_page(&page, index) {
                Ok(page) => pages.push(page),
                Err(reason) => {
                    return DocumentOpenerOutcome::Failed { reason };
                }
            }
        }
        if pages.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the PDF contains no renderable pages".to_owned(),
            };
        }
        DocumentOpenerOutcome::Loaded {
            view: view(
                "builtin-pdf",
                request,
                DocumentViewBody::RasterPages { pages, page_count, truncated },
            ),
        }
    }

    pub(super) fn page_sizes(pdfium: &Pdfium, data: &[u8]) -> Result<Vec<(f32, f32)>, String> {
        let document = pdfium
            .load_pdf_from_byte_slice(data, None)
            .map_err(|error| format!("the PDF could not be parsed: {error}"))?;
        let sizes = document
            .pages()
            .page_sizes()
            .map_err(|error| format!("page sizes could not be read: {error}"))?;
        Ok(sizes
            .iter()
            .map(|rect| (rect.width().value.max(1.0), rect.height().value.max(1.0)))
            .collect())
    }

    pub(super) fn render_numbers(
        pdfium: &Pdfium,
        data: &[u8],
        numbers: &[u32],
    ) -> Result<Vec<DocumentRasterPage>, String> {
        let document = pdfium
            .load_pdf_from_byte_slice(data, None)
            .map_err(|error| format!("the PDF could not be parsed: {error}"))?;
        let pages = document.pages();
        numbers
            .iter()
            .map(|&number| {
                let index = number
                    .checked_sub(1)
                    .and_then(|index| u16::try_from(index).ok())
                    .ok_or_else(|| format!("page {number} does not exist"))?;
                let page = pages.get(index).map_err(|_| format!("page {number} does not exist"))?;
                render_page(&page, usize::from(index))
            })
            .collect()
    }

    // The casts below are provably safe: the target width is a small constant and the
    // height is rounded and clamped to [1, MAX_HEIGHT] before converting.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_page(page: &PdfPage<'_>, index: usize) -> Result<DocumentRasterPage, String> {
        let width_points = page.width().value.max(1.0);
        let height_points = page.height().value.max(1.0);
        let scale = TARGET_WIDTH as f32 / width_points;
        let target_height =
            (height_points * scale).round().clamp(1.0, MAX_HEIGHT as f32).max(1.0) as i32;
        let bitmap = page
            .render(TARGET_WIDTH, target_height, None)
            .map_err(|error| format!("page {} could not be rendered: {error}", index + 1))?;
        let width = u32::try_from(bitmap.width()).unwrap_or(u32::MAX);
        let height = u32::try_from(bitmap.height()).unwrap_or(u32::MAX);
        let rgba = bitmap.as_rgba_bytes();
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(format!("page {} produced an unexpected bitmap layout", index + 1));
        }
        Ok(DocumentRasterPage {
            number: u32::try_from(index + 1).unwrap_or(u32::MAX),
            width,
            height,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{DocumentLoadState, DocumentOpenerOutcome, DocumentViewBody};

    use super::*;
    use crate::testing;

    #[test]
    fn a_valid_pdf_loads_as_raster_or_text() {
        let bytes = testing::minimal_pdf(&["LM317 voltage regulator", "Requires a 1uF capacitor"]);
        let request = testing::request_for("lm317.pdf", &bytes);

        let outcome = PdfOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = &outcome else {
            panic!("a valid PDF must load, got {outcome:?}");
        };
        assert_eq!(view.opener_id, "builtin-pdf");
        assert_eq!(view.title, "lm317");
        assert_eq!(view.document_id, "doc-test");
        assert!(view.content_hash.starts_with("sha256:"));
        assert_eq!(view.source_locator, "source/lm317.pdf");
        match &view.body {
            DocumentViewBody::RasterPages { pages, page_count, truncated } => {
                assert!(!truncated);
                assert_eq!(*page_count, 1);
                let page = &pages[0];
                assert_eq!(page.number, 1);
                assert!(page.width > 0 && page.height > page.width, "portrait page");
                assert_eq!(page.rgba.len(), page.width as usize * page.height as usize * 4);
            }
            DocumentViewBody::PagedText { pages, truncated } => {
                assert!(!truncated);
                let joined: String = pages.iter().map(|page| page.text.as_str()).collect();
                assert!(joined.contains("LM317"), "extracted: {joined:?}");
            }
            other => panic!("unexpected body for a PDF: {other:?}"),
        }
    }

    #[cfg(feature = "raster-pdf")]
    #[test]
    fn when_pdfium_is_available_pages_are_rasterized() {
        if !raster::pdfium_available() {
            eprintln!("pdfium library not located; running the text-fallback path only");
            return;
        }
        let bytes = testing::minimal_pdf(&["LM317 voltage regulator"]);
        let request = testing::request_for("lm317.pdf", &bytes);

        let outcome = PdfOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("a valid PDF must load, got {outcome:?}");
        };
        let DocumentViewBody::RasterPages { pages, .. } = view.body else {
            panic!("pdfium was available, so pages must rasterize");
        };
        assert_eq!(pages.len(), 1);
        assert!(!pages[0].rgba.is_empty());
    }

    #[cfg(feature = "raster-pdf")]
    #[test]
    fn opens_on_different_threads_do_not_block_each_other() {
        // Mirrors a worker pool: the first thread stays alive after its open while a
        // second thread opens another document.
        let bytes = testing::minimal_pdf(&["LM317 voltage regulator"]);
        let (release, parked) = std::sync::mpsc::channel::<()>();
        let mut parked = Some(parked);
        for _ in 0..2 {
            let bytes = bytes.clone();
            let park = parked.take();
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let request = testing::request_for("lm317.pdf", &bytes);
                let _ = sender.send(PdfOpener.open(&request).load_state());
                if let Some(park) = park {
                    let _ = park.recv();
                }
            });
            let state = receiver
                .recv_timeout(std::time::Duration::from_secs(30))
                .expect("a PDF open on a fresh thread must not block on another thread's pdfium");
            assert_eq!(state, DocumentLoadState::Loaded);
        }
        drop(release);
    }

    #[cfg(feature = "raster-pdf")]
    #[test]
    fn pages_render_on_demand_by_number() {
        if !raster::pdfium_available() {
            return;
        }
        let bytes = testing::minimal_pdf(&["LM317 voltage regulator"]);

        let sizes = pdf_page_sizes(&bytes).expect("pdfium").expect("sizes");
        assert_eq!(sizes.len(), 1);
        assert!(sizes[0].1 > sizes[0].0, "portrait page: {sizes:?}");

        let pages = render_pdf_pages(&bytes, &[1]).expect("pdfium").expect("page 1");
        assert_eq!(pages[0].number, 1);
        assert_eq!(pages[0].rgba.len(), pages[0].width as usize * pages[0].height as usize * 4);
        let missing = render_pdf_pages(&bytes, &[2]).expect("pdfium");
        assert!(missing.is_err_and(|reason| reason.contains("page 2")));
        assert!(render_pdf_pages(&bytes, &[0]).expect("pdfium").is_err());
    }

    #[test]
    fn the_text_extraction_fallback_stays_available() {
        let bytes = testing::minimal_pdf(&["Requires a 1uF capacitor"]);
        let request = testing::request_for("lm317.pdf", &bytes);

        let outcome = open_text(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("the text path must load, got {outcome:?}");
        };
        let DocumentViewBody::PagedText { pages, .. } = view.body else {
            panic!("the text path always yields paged text");
        };
        let joined: String = pages.iter().map(|page| page.text.as_str()).collect();
        assert!(joined.contains("1uF"), "extracted: {joined:?}");
    }

    #[test]
    fn a_corrupt_pdf_fails_explicitly() {
        let request = testing::request_for("broken.pdf", b"%PDF-1.4 this is not really a pdf");

        let outcome = PdfOpener.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Failed);
        let DocumentOpenerOutcome::Failed { reason } = outcome else {
            panic!("expected a failure, got {outcome:?}");
        };
        assert!(reason.contains("could not be parsed"), "{reason}");
    }
}
