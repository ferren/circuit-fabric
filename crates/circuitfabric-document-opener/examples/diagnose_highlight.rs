//! Reproduces the search-hit highlight pipeline against one real PDF on disk: prints the
//! indexing extractor's view of a page and line, pdfium's view of the same pages, and what
//! `pdf_locate_highlight` returns for the terms. Diagnostic tool, run manually:
//!
//! ```text
//! cargo run -p circuitfabric-document-opener --features raster-pdf --example diagnose_highlight -- <pdf> <page> <line>
//! ```

use std::path::Path;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(path) = arguments.next() else {
        eprintln!("usage: diagnose_highlight <pdf> [page] [line]");
        return;
    };
    let page_hint: u32 = arguments.next().and_then(|value| value.parse().ok()).unwrap_or(1);
    let line_hint: usize = arguments.next().and_then(|value| value.parse().ok()).unwrap_or(1);
    let data = std::fs::read(&path).expect("read pdf");
    println!("file: {} ({} bytes)", Path::new(&path).display(), data.len());

    let pages = pdf_extract::extract_text_from_mem_by_pages(&data).expect("pdf-extract pages");
    println!("pdf-extract pages: {}", pages.len());
    // Line numbering matches registration: every line, empty ones included.
    let line_text = pages
        .get(page_hint as usize - 1)
        .and_then(|page| page.lines().nth(line_hint - 1))
        .unwrap_or_default()
        .to_owned();
    println!("pdf-extract page {page_hint} line {line_hint}: {line_text:?}");
    if line_text.trim().is_empty() {
        let nearby: Vec<&str> = pages
            .get(page_hint as usize - 1)
            .map(|page| page.lines().collect())
            .unwrap_or_default();
        let from = line_hint.saturating_sub(3);
        println!(
            "  context lines {}..{}: {:?}",
            from + 1,
            (from + 6).min(nearby.len()),
            &nearby[from..(from + 6).min(nearby.len())]
        );
    }
    let terms: Vec<String> = line_text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(ToOwned::to_owned)
        .collect();
    println!("line terms: {terms:?}");

    pdfium_view(&data, page_hint, &terms);
    #[cfg(feature = "raster-pdf")]
    if let Some(Ok((page, regions))) =
        circuitfabric_document_opener::pdf_locate_highlight(&data, page_hint, &terms)
    {
        println!("pdf_locate_highlight -> page {page}, {} regions", regions.len());
        for region in regions.iter().take(8) {
            println!("  region: {region:?}");
        }
    } else {
        println!("pdf_locate_highlight -> unavailable/failed");
    }
}

#[cfg(not(feature = "raster-pdf"))]
fn pdfium_view(_data: &[u8], _page_hint: u32, _terms: &[String]) {
    println!("pdfium view needs --features raster-pdf");
}

#[cfg(feature = "raster-pdf")]
fn pdfium_view(data: &[u8], page_hint: u32, terms: &[String]) {
    let bindings = pdfium_render::prelude::Pdfium::bind_to_system_library().or_else(|_| {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .expect("locate the example binary");
        let candidate = dir
            .ancestors()
            .take(6)
            .map(|ancestor| ancestor.join("native/windows-x64/pdfium.dll"))
            .find(|candidate| candidate.exists())
            .unwrap_or_else(|| dir.join("pdfium.dll"));
        pdfium_render::prelude::Pdfium::bind_to_library(candidate)
    });
    let Ok(bindings) = bindings else {
        println!("pdfium: unavailable");
        return;
    };
    let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
    let Ok(document) = pdfium.load_pdf_from_byte_slice(data, None) else {
        println!("pdfium: load failed");
        return;
    };
    println!("pdfium pages: {}", document.pages().len());
    for number in page_hint.saturating_sub(1)..=page_hint + 1 {
        let Some(index) = (number as usize).checked_sub(1) else { continue };
        let Ok(page) = document.pages().get(u16::try_from(index).unwrap_or(0)) else { continue };
        let Ok(text) = page.text() else { continue };
        let mut source = String::new();
        for character in text.chars().iter() {
            if let Some(ch) = character.unicode_char() {
                source.push(ch);
            }
        }
        let flat: String = source.to_lowercase().chars().filter(|ch| !ch.is_whitespace()).collect();
        let per_term: Vec<(bool, usize)> = terms
            .iter()
            .map(|term| {
                let needle: String =
                    term.to_lowercase().chars().filter(|ch| !ch.is_whitespace()).collect();
                let occurrences = flat.matches(&needle).count();
                (occurrences > 0, occurrences)
            })
            .collect();
        println!(
            "pdfium page {number}: {} chars, term hits (found, count): {per_term:?}",
            source.chars().count()
        );
        if number == page_hint {
            let start = terms
                .first()
                .map(|term| {
                    term.to_lowercase().chars().filter(|ch| !ch.is_whitespace()).collect::<String>()
                })
                .filter(|needle| !needle.is_empty())
                .and_then(|needle| flat.find(&needle))
                .unwrap_or(0);
            let from = start.saturating_sub(40);
            println!(
                "  context around first term: {:?}",
                &flat[from..(from + 160).min(flat.len())]
            );
        }
    }
}
