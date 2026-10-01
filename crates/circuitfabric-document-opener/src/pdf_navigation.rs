//! Text matching shared by the PDF text view and the raster highlight overlay.

use std::ops::Range;

/// A highlight rectangle in fractions of the rendered page, with a top-left origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfHighlightRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// Folds text for matching: case-insensitive, common PDF micro/minus variants normalized,
/// and ALL whitespace removed. Removing whitespace entirely (rather than collapsing it)
/// matters because search terms are cut from one extractor's text while the highlight runs
/// over another's — `vo ltage` and `voltage` must find each other. Every folded byte maps
/// back to its source range so highlights land on the original text.
fn searchable(text: &str) -> (String, Vec<Range<usize>>) {
    let mut folded = String::new();
    let mut offsets = Vec::new();
    for (start, ch) in text.char_indices() {
        if ch.is_whitespace() {
            continue;
        }
        let source = start..start + ch.len_utf8();
        let ch = match ch {
            'µ' => 'μ',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            other => other,
        };
        for lower in ch.to_lowercase() {
            folded.push(lower);
            offsets.extend(std::iter::repeat_n(source.clone(), lower.len_utf8()));
        }
    }
    (folded, offsets)
}

/// Find search terms in original PDF text, preserving UTF-8 boundaries while folding
/// case, whitespace and common PDF micro/minus variants. Overlapping matches are merged.
#[must_use]
pub fn pdf_text_highlight_ranges(text: &str, terms: &[String]) -> Vec<Range<usize>> {
    let (folded, offsets) = searchable(text);
    let mut ranges = Vec::new();
    for term in terms.iter().take(32) {
        let (needle, _) = searchable(term);
        if needle.is_empty() {
            continue;
        }
        for (start, _) in folded.match_indices(&needle).take(200) {
            ranges.push(offsets[start].start..offsets[start + needle.len() - 1].end);
        }
    }
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut().filter(|previous| range.start <= previous.end) {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_preserve_unicode_and_multiline_source_ranges() {
        let text = "输入 VDD −0.3 V\n  10 µA and İN";
        let ranges =
            pdf_text_highlight_ranges(text, &["vdd -0.3".into(), "10 μa".into(), "i̇n".into()]);
        let matches = ranges.iter().map(|range| &text[range.clone()]).collect::<Vec<_>>();
        assert_eq!(matches, ["VDD −0.3", "10 µA", "İN"]);
        let ranges = pdf_text_highlight_ranges(text, &["V 10".into()]);
        assert_eq!(&text[ranges[0].clone()], "V\n  10");
    }

    #[test]
    fn overlaps_and_empty_terms_do_not_duplicate_highlights() {
        assert_eq!(
            pdf_text_highlight_ranges("VIN VIN", &[String::new(), "vin".into(), "in".into()]),
            vec![0..3, 4..7]
        );
    }

    #[test]
    fn spacing_differences_between_extractors_still_match() {
        // The search term was cut from one extractor's text, the highlight runs over
        // another's: arbitrary spacing on either side must not break the match.
        let text = "VIN Input voltage 6 V";
        let ranges = pdf_text_highlight_ranges(text, &["vo ltage".into(), "i np u t".into()]);
        let matches = ranges.iter().map(|range| &text[range.clone()]).collect::<Vec<_>>();
        assert_eq!(matches, ["Input", "voltage"]);

        let text = "V  I N   v o l t a g e";
        let ranges = pdf_text_highlight_ranges(text, &["voltage".into()]);
        assert_eq!(&text[ranges[0].clone()], "v o l t a g e");
    }
}
