//! Selection geometry from the PDF text layer, independent of any GUI toolkit.

use std::ops::Range;

use crate::PdfHighlightRect;

/// One Unicode character and its geometry in fractions of the rendered, rotated page.
#[derive(Clone, Debug)]
pub struct PdfTextCharacter {
    pub bytes: Range<usize>,
    pub bounds: PdfHighlightRect,
    /// Caret positions before and after this character, including page rotation.
    pub leading: (f32, f32),
    pub trailing: (f32, f32),
}

/// A page's Unicode text and exact glyph positions. Whitespace stays in the copy text;
/// only characters with visible bounds participate in pointer hit testing.
#[derive(Clone, Debug, Default)]
pub struct PdfPageText {
    pub number: u32,
    pub text: String,
    pub characters: Vec<PdfTextCharacter>,
}

impl PdfPageText {
    #[must_use]
    pub fn hit_test(&self, point: (f32, f32)) -> bool {
        self.characters.iter().any(|character| {
            let rect = character.bounds;
            point.0 >= rect.left
                && point.0 <= rect.left + rect.width
                && point.1 >= rect.top
                && point.1 <= rect.top + rect.height
        })
    }

    /// Resolves a pointer to a UTF-8 insertion boundary, also outside the text bounds
    /// during an ongoing drag. Caret geometry rotates together with the page.
    #[must_use]
    pub fn nearest_boundary(&self, point: (f32, f32)) -> usize {
        let Some(character) = self.characters.iter().min_by(|left, right| {
            distance_to_rect(point, left.bounds).total_cmp(&distance_to_rect(point, right.bounds))
        }) else {
            return 0;
        };
        let distance =
            |caret: (f32, f32)| (point.0 - caret.0).powi(2) + (point.1 - caret.1).powi(2);
        if distance(character.leading) <= distance(character.trailing) {
            character.bytes.start
        } else {
            character.bytes.end
        }
    }

    #[must_use]
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<PdfHighlightRect> {
        self.characters
            .iter()
            .filter(|character| {
                character.bytes.start < range.end && character.bytes.end > range.start
            })
            .map(|character| character.bounds)
            .collect()
    }
}

fn distance_to_rect(point: (f32, f32), rect: PdfHighlightRect) -> f32 {
    let x = (rect.left - point.0).max(0.).max(point.0 - rect.left - rect.width);
    let y = (rect.top - point.1).max(0.).max(point.1 - rect.top - rect.height);
    x * x + 4. * y * y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_selection_uses_utf8_boundaries_and_preserves_whitespace() {
        let text = "A 中\nµB";
        let characters = text
            .char_indices()
            .filter(|(_, ch)| !ch.is_whitespace())
            .enumerate()
            .map(|(index, (start, ch))| {
                let x = index as f32 / 10.;
                PdfTextCharacter {
                    bytes: start..start + ch.len_utf8(),
                    bounds: PdfHighlightRect { left: x, top: 0.1, width: 0.1, height: 0.1 },
                    leading: (x, 0.15),
                    trailing: (x + 0.1, 0.15),
                }
            })
            .collect();
        let page = PdfPageText { text: text.into(), characters, number: 1 };
        let first = page.nearest_boundary((0.101, 0.15));
        let last = page.nearest_boundary((0.299, 0.15));
        assert_eq!(&page.text[first..last], "中\nµ");
        assert_eq!(page.selection_rects(first..last).len(), 2);
        assert!(page.hit_test((0.12, 0.15)));
        assert!(!page.hit_test((0.12, 0.8)));
        assert_eq!(PdfPageText::default().nearest_boundary((0.5, 0.5)), 0);
    }

    #[test]
    fn rotated_caret_direction_is_respected() {
        let page = PdfPageText {
            number: 1,
            text: "µ".into(),
            characters: vec![PdfTextCharacter {
                bytes: 0..2,
                bounds: PdfHighlightRect { left: 0.2, top: 0.2, width: 0.1, height: 0.2 },
                leading: (0.25, 0.4),
                trailing: (0.25, 0.2),
            }],
        };
        assert_eq!(page.nearest_boundary((0.25, 0.39)), 0);
        assert_eq!(page.nearest_boundary((0.25, 0.21)), 2);
    }
}
