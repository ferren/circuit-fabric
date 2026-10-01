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
    render_pdf_pages_at_width(data, numbers, 900)
}

/// Re-renders PDF vectors and text at the requested physical pixel width. Page aspect
/// ratio is preserved; dimensions are bounded to 8192 pixels and 24 million pixels per
/// page. Hosts should request only visible pages at large sizes. `None` means no pdfium.
#[cfg(feature = "raster-pdf")]
#[must_use]
pub fn render_pdf_pages_at_width(
    data: &[u8],
    numbers: &[u32],
    width: u32,
) -> Option<Result<Vec<circuitfabric_plugin_api::DocumentRasterPage>, String>> {
    raster::with_pdfium(|pdfium| raster::render_numbers(pdfium, data, numbers, width))
}

/// Locate matching text on a page without rendering another bitmap. Coordinates follow
/// the same page rotation and dimensions as the preview renderer. `None` means pdfium
/// is unavailable; a successful empty result means the page has no matching text layer.
#[cfg(feature = "raster-pdf")]
#[must_use]
pub fn pdf_highlight_regions(
    data: &[u8],
    number: u32,
    terms: &[String],
) -> Option<Result<Vec<crate::PdfHighlightRect>, String>> {
    raster::with_pdfium(|pdfium| raster::highlight_regions(pdfium, data, number, terms))
}

/// Locates the search terms starting from a preferred page and highlights them there; when
/// that page's text layer does not contain the terms, nearby and then remaining pages are
/// tried, and the page the terms were actually found on is returned. `None` when pdfium is
/// unavailable; a successful result with empty regions means no page matched.
#[cfg(feature = "raster-pdf")]
#[must_use]
pub fn pdf_locate_highlight(
    data: &[u8],
    preferred: u32,
    terms: &[String],
) -> Option<Result<(u32, Vec<crate::PdfHighlightRect>), String>> {
    raster::with_pdfium(|pdfium| raster::locate_highlight(pdfium, data, preferred, terms))
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
        width: u32,
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
                render_page_at_width(&page, usize::from(index), width)
            })
            .collect()
    }

    pub(super) fn highlight_regions(
        pdfium: &Pdfium,
        data: &[u8],
        number: u32,
        terms: &[String],
    ) -> Result<Vec<crate::PdfHighlightRect>, String> {
        let document = pdfium.load_pdf_from_byte_slice(data, None).map_err(|e| e.to_string())?;
        let index = number
            .checked_sub(1)
            .and_then(|index| u16::try_from(index).ok())
            .ok_or_else(|| format!("page {number} does not exist"))?;
        let page = document.pages().get(index).map_err(|e| e.to_string())?;
        regions_on_page(&page, terms)
    }

    /// Locates the terms near a preferred page first, then across the whole document, so a
    /// page-number disagreement between the text extractor that indexed the document and
    /// pdfium's own pagination cannot leave the hit unhighlighted. Returns the page the
    /// regions were found on; empty regions mean no page's text layer matched.
    pub(super) fn locate_highlight(
        pdfium: &Pdfium,
        data: &[u8],
        preferred: u32,
        terms: &[String],
    ) -> Result<(u32, Vec<crate::PdfHighlightRect>), String> {
        const SCAN_RADIUS: u32 = 3;
        const MAX_SCANNED_PAGES: u32 = 300;
        let document = pdfium.load_pdf_from_byte_slice(data, None).map_err(|e| e.to_string())?;
        let count = u32::from(document.pages().len());
        if count == 0 {
            return Ok((preferred, Vec::new()));
        }
        let page_at = |number: u32| -> Option<PdfPage<'_>> {
            number
                .checked_sub(1)
                .and_then(|index| u16::try_from(index).ok())
                .filter(|_| number <= count)
                .and_then(|index| document.pages().get(index).ok())
        };
        let mut order: Vec<u32> = Vec::new();
        let push = |number: u32, order: &mut Vec<u32>| {
            if (1..=count).contains(&number) && !order.contains(&number) {
                order.push(number);
            }
        };
        push(preferred, &mut order);
        for offset in 1..=SCAN_RADIUS {
            push(preferred.saturating_add(offset), &mut order);
            push(preferred.saturating_sub(offset).max(1), &mut order);
        }
        for number in 1..=count.min(MAX_SCANNED_PAGES) {
            push(number, &mut order);
        }
        for number in order {
            let Some(page) = page_at(number) else { continue };
            let regions = regions_on_page(&page, terms)?;
            if !regions.is_empty() {
                return Ok((number, regions));
            }
        }
        Ok((preferred.clamp(1, count), Vec::new()))
    }

    fn regions_on_page(
        page: &PdfPage<'_>,
        terms: &[String],
    ) -> Result<Vec<crate::PdfHighlightRect>, String> {
        let text = page.text().map_err(|e| e.to_string())?;
        let mut source = String::new();
        let mut chars = Vec::new();
        for character in text.chars().iter() {
            if let Some(ch) = character.unicode_char() {
                let start = source.len();
                source.push(ch);
                chars.push((start..source.len(), character.index()));
            }
        }
        let (width, height) = preview_dimensions(page);
        let config = pdfium_render::prelude::PdfRenderConfig::new()
            .set_target_width(width)
            .set_target_height(height);
        let mut regions = Vec::new();
        for range in crate::pdf_text_highlight_ranges(&source, terms).iter().take(200) {
            let mut matching = chars
                .iter()
                .filter(|(bytes, _)| bytes.start < range.end && bytes.end > range.start);
            let Some((_, first)) = matching.next() else { continue };
            let last = matching.next_back().map_or(*first, |(_, index)| *index);
            for segment in text.segments_subset(*first, last - *first + 1).iter() {
                let bounds = segment.bounds();
                let corners = [
                    (bounds.left(), bounds.top()),
                    (bounds.right(), bounds.top()),
                    (bounds.left(), bounds.bottom()),
                    (bounds.right(), bounds.bottom()),
                ]
                .into_iter()
                .map(|(x, y)| page.points_to_pixels(x, y, &config))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
                #[allow(clippy::cast_precision_loss)]
                let region = {
                    let left = corners.iter().map(|(x, _)| *x).min().unwrap_or(0).max(0) as f32
                        / width as f32;
                    let top = corners.iter().map(|(_, y)| *y).min().unwrap_or(0).max(0) as f32
                        / height as f32;
                    let right = corners.iter().map(|(x, _)| *x).max().unwrap_or(0).min(width)
                        as f32
                        / width as f32;
                    let bottom = corners.iter().map(|(_, y)| *y).max().unwrap_or(0).min(height)
                        as f32
                        / height as f32;
                    crate::PdfHighlightRect { left, top, width: right - left, height: bottom - top }
                };
                if region.width > 0. && region.height > 0. && !regions.contains(&region) {
                    regions.push(region);
                }
            }
        }
        Ok(regions)
    }

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn preview_dimensions(page: &PdfPage<'_>) -> (i32, i32) {
        raster_dimensions(page.width().value, page.height().value, 900)
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn raster_dimensions(width: f32, height: f32, requested_width: u32) -> (i32, i32) {
        let width = f64::from(width).max(1.0);
        let height = f64::from(height).max(1.0);
        let scale = (f64::from(requested_width.max(1)) / width)
            .min(8192.0 / width)
            .min(8192.0 / height)
            .min((24_000_000.0 / (width * height)).sqrt());
        ((width * scale).floor().max(1.0) as i32, (height * scale).floor().max(1.0) as i32)
    }

    fn render_page(page: &PdfPage<'_>, index: usize) -> Result<DocumentRasterPage, String> {
        render_page_at_width(page, index, u32::try_from(TARGET_WIDTH).unwrap_or(900))
    }

    fn render_page_at_width(
        page: &PdfPage<'_>,
        index: usize,
        requested_width: u32,
    ) -> Result<DocumentRasterPage, String> {
        let (width, height) =
            raster_dimensions(page.width().value, page.height().value, requested_width);
        let bitmap = page
            .render(width, height, None)
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
    fn requested_resolution_preserves_aspect_and_bounds_memory() {
        assert_eq!(raster::raster_dimensions(612.0, 792.0, 1800).0, 1800);
        assert_eq!(raster::raster_dimensions(612.0, 792.0, 900).0, 900);
        for (page_width, page_height) in [(612.0, 792.0), (792.0, 612.0), (10.0, 10000.0)] {
            let (width, height) = raster::raster_dimensions(page_width, page_height, u32::MAX);
            assert!((1..=8192).contains(&width) && (1..=8192).contains(&height));
            assert!(i64::from(width) * i64::from(height) <= 24_000_000);
        }
    }

    #[cfg(feature = "raster-pdf")]
    #[test]
    fn zoom_rerenders_the_pdf_at_a_higher_pixel_density() {
        if !raster::pdfium_available() {
            assert!(
                std::env::var_os("CIRCUITFABRIC_PDFIUM_PATH").is_none(),
                "configured pdfium failed to load"
            );
            eprintln!("pdfium unavailable; skipping raster resolution test");
            return;
        }
        let bytes = testing::minimal_pdf(&["VIN supply input", "VOUT output"]);
        let low = render_pdf_pages(&bytes, &[1]).unwrap().unwrap().remove(0);
        let high = render_pdf_pages_at_width(&bytes, &[1], 1800).unwrap().unwrap().remove(0);
        assert_eq!(high.width, low.width * 2);
        assert!((i64::from(high.height) - i64::from(low.height) * 2).abs() <= 1);
        assert_eq!(high.rgba.len(), high.width as usize * high.height as usize * 4);
        // Newly rasterized glyph edges have subpixel detail inside a low-resolution
        // pixel's 2x2 footprint; simply enlarging the old bitmap cannot produce it.
        let stride = high.width as usize * 4;
        assert!(high.rgba.chunks_exact(stride * 2).any(|rows| {
            (0..stride).step_by(8).any(|x| rows[x] != rows[x + 4] || rows[x] != rows[stride + x])
        }));
        assert!(render_pdf_pages_at_width(&bytes, &[0], 1800).unwrap().is_err());
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
    fn locate_highlight_recovers_when_the_preferred_page_drifts() {
        if !raster::pdfium_available() {
            eprintln!("pdfium library not located; running nothing for this test");
            return;
        }
        let bytes = testing::minimal_pdf(&["LM317 voltage regulator"]);
        let terms = vec!["voltage".to_owned(), "regulator".to_owned()];

        // The hit's page number claims page 7 of a one-page document — the drift case
        // between the indexing extractor and pdfium. The scan must find page 1 and
        // highlight the keywords there.
        let Some(Ok((page, regions))) = pdf_locate_highlight(&bytes, 7, &terms) else {
            panic!("pdfium is available, so locate must return a result");
        };
        assert_eq!(page, 1, "the scan falls back to the page holding the text");
        assert!(!regions.is_empty(), "keywords must be highlighted on the found page");

        // Terms that appear on no page come back anchored to the preferred page, empty.
        let Some(Ok((page, regions))) =
            pdf_locate_highlight(&bytes, 1, &["nonexistent".to_owned()])
        else {
            panic!("pdfium is available, so locate must return a result");
        };
        assert_eq!(page, 1);
        assert!(regions.is_empty());
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

    #[cfg(feature = "raster-pdf")]
    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn highlight_rectangles_cover_text_pixels_in_normal_and_rotated_pages() {
        use pdfium_render::prelude::PdfPageRenderRotation;
        if !raster::pdfium_available() {
            return;
        }
        let original = testing::minimal_pdf(&["VIN supply input", "VOUT output", "VIN repeated"]);
        for rotated in [false, true] {
            let bytes = if rotated {
                raster::with_pdfium(|pdfium| {
                    let doc = pdfium.load_pdf_from_byte_slice(&original, None).unwrap();
                    doc.pages().get(0).unwrap().set_rotation(PdfPageRenderRotation::Degrees90);
                    doc.save_to_bytes().unwrap()
                })
                .unwrap()
            } else {
                original.clone()
            };
            let regions = pdf_highlight_regions(&bytes, 1, &["vin".to_owned()]).unwrap().unwrap();
            assert_eq!(regions.len(), 2);
            for requested_width in [900, 2700] {
                let pages =
                    render_pdf_pages_at_width(&bytes, &[1], requested_width).unwrap().unwrap();
                let bitmap = &pages[0];
                for region in &regions {
                    assert!(region.left >= 0. && region.top >= 0.);
                    assert!(
                        region.left + region.width <= 1.001 && region.top + region.height <= 1.001
                    );
                    let x1 = (region.left * bitmap.width as f32).floor() as usize;
                    let x2 = ((region.left + region.width) * bitmap.width as f32).ceil() as usize;
                    let y1 = (region.top * bitmap.height as f32).floor() as usize;
                    let y2 = ((region.top + region.height) * bitmap.height as f32).ceil() as usize;
                    let dark = (y1..y2.min(bitmap.height as usize)).any(|y| {
                        (x1..x2.min(bitmap.width as usize))
                            .any(|x| bitmap.rgba[(y * bitmap.width as usize + x) * 4] < 128)
                    });
                    assert!(dark, "highlight misses glyphs, rotated={rotated}: {region:?}");
                }
            }
            assert!(
                pdf_highlight_regions(&bytes, 1, &["absent".into()]).unwrap().unwrap().is_empty()
            );
            assert!(pdf_highlight_regions(&bytes, 0, &["VIN".into()]).unwrap().is_err());
        }
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
