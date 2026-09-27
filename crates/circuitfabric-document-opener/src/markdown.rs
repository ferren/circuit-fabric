//! Markdown opener: block extraction via `pulldown-cmark`.
//!
//! Active-content policy: raw HTML blocks and inline HTML are dropped rather than rendered,
//! link/image destinations are never surfaced (only their inner text), so a view consumer
//! cannot navigate anywhere from the rendered data. Inline emphasis (bold, italic, strike,
//! inline code) survives as [`DocumentSpan`] runs so hosts can render real formatting.

use circuitfabric_plugin_api::{
    DocumentBlock, DocumentBlockKind, DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest,
    DocumentSpanStyle, DocumentViewBody, MAX_DOCUMENT_VIEW_BLOCKS, OpenerCapability,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{Spans, view};

/// Opens Markdown managed copies as a block-oriented, read-only view.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarkdownOpener;

impl DocumentOpener for MarkdownOpener {
    fn capability(&self) -> OpenerCapability {
        OpenerCapability {
            opener_id: "builtin-markdown".to_owned(),
            label: "Markdown document".to_owned(),
            extensions: vec!["md".to_owned(), "markdown".to_owned()],
            mime_types: vec!["text/markdown".to_owned(), "text/x-markdown".to_owned()],
        }
    }

    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let Ok(text) = String::from_utf8(request.managed_copy.data().to_vec()) else {
            return DocumentOpenerOutcome::Failed {
                reason: "the Markdown document is not valid UTF-8".to_owned(),
            };
        };
        let blocks = render_blocks(&text);
        if blocks.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the Markdown document contains no readable content".to_owned(),
            };
        }
        let truncated = blocks.len() > MAX_DOCUMENT_VIEW_BLOCKS;
        let blocks = blocks.into_iter().take(MAX_DOCUMENT_VIEW_BLOCKS).collect();
        DocumentOpenerOutcome::Loaded {
            view: view("builtin-markdown", request, DocumentViewBody::Blocks { blocks, truncated }),
        }
    }
}

/// Emits the accumulated spans as one block (when non-blank) and resets the buffer.
fn flush_block(blocks: &mut Vec<DocumentBlock>, kind: &DocumentBlockKind, spans: &mut Spans) {
    if !spans.is_blank() {
        blocks.push(spans.take_block(kind));
    }
    spans.clear();
}

/// Converts one Markdown source into read-only blocks with inline styling runs.
// One flat event-dispatch match: pulling arms out would trade line count for indirection
// without making the mapping clearer.
#[allow(clippy::too_many_lines)]
fn render_blocks(source: &str) -> Vec<DocumentBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut blocks = Vec::new();
    let mut pending_kind = DocumentBlockKind::Paragraph;
    let mut spans = Spans::default();
    let mut list_depth: u8 = 0;
    let mut quote_depth: u8 = 0;
    let mut code_language: Option<String> = None;
    let mut table_row: Vec<String> = Vec::new();
    // Inline emphasis nesting; the innermost active style wins.
    let mut strong_depth: u32 = 0;
    let mut emphasis_depth: u32 = 0;
    let mut strikethrough_depth: u32 = 0;
    let inline_style = |strong: u32, emphasis: u32, strike: u32| {
        if strong > 0 {
            DocumentSpanStyle::Strong
        } else if emphasis > 0 {
            DocumentSpanStyle::Emphasis
        } else if strike > 0 {
            DocumentSpanStyle::Strikethrough
        } else {
            DocumentSpanStyle::Plain
        }
    };

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_block(&mut blocks, &pending_kind, &mut spans);
                pending_kind = DocumentBlockKind::Heading { level: level as u8 };
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_block(&mut blocks, &pending_kind, &mut spans);
                pending_kind = DocumentBlockKind::Paragraph;
            }
            Event::Start(Tag::Paragraph) => {
                flush_block(&mut blocks, &pending_kind, &mut spans);
                pending_kind = if quote_depth > 0 {
                    DocumentBlockKind::Quote
                } else {
                    DocumentBlockKind::Paragraph
                };
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_block(&mut blocks, &pending_kind, &mut spans);
                code_language = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(crate::capped_text)
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                let kind = DocumentBlockKind::Code { language: code_language.take() };
                flush_block(&mut blocks, &kind, &mut spans);
            }
            Event::Start(Tag::List(_)) => list_depth = list_depth.saturating_add(1),
            Event::End(TagEnd::List(_)) => list_depth = list_depth.saturating_sub(1),
            Event::Start(Tag::Item) => {
                flush_block(&mut blocks, &pending_kind, &mut spans);
                pending_kind = DocumentBlockKind::ListItem { depth: list_depth.saturating_sub(1) };
            }
            Event::Start(Tag::BlockQuote(_)) => quote_depth = quote_depth.saturating_add(1),
            Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
            Event::Start(Tag::Table(_) | Tag::TableRow | Tag::TableCell)
            | Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::Table | TagEnd::TableHead) => {
                flush_block(&mut blocks, &pending_kind, &mut spans);
            }
            Event::End(TagEnd::TableRow) => {
                let text = table_row.join(" | ");
                table_row.clear();
                if !text.trim().is_empty() {
                    blocks.push(DocumentBlock {
                        kind: DocumentBlockKind::Paragraph,
                        text: crate::capped_text(&text),
                        spans: Vec::new(),
                    });
                }
            }
            Event::End(TagEnd::TableCell) => {
                table_row.push(spans.take_text().trim().to_owned());
                spans.clear();
            }
            Event::Text(chunk) => {
                spans.push(inline_style(strong_depth, emphasis_depth, strikethrough_depth), &chunk);
            }
            Event::Code(chunk) => spans.push(DocumentSpanStyle::Code, &chunk),
            Event::SoftBreak | Event::HardBreak => spans.push(DocumentSpanStyle::Plain, " "),
            Event::Start(Tag::Strong) => strong_depth = strong_depth.saturating_add(1),
            Event::End(TagEnd::Strong) => strong_depth = strong_depth.saturating_sub(1),
            Event::Start(Tag::Emphasis) => emphasis_depth = emphasis_depth.saturating_add(1),
            Event::End(TagEnd::Emphasis) => emphasis_depth = emphasis_depth.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => {
                strikethrough_depth = strikethrough_depth.saturating_add(1);
            }
            Event::End(TagEnd::Strikethrough) => {
                strikethrough_depth = strikethrough_depth.saturating_sub(1);
            }
            // Active content is dropped, never rendered or passed through: raw HTML
            // blocks, link/image destinations, math source, footnotes, and any other
            // structural container. Only readable inner text survives the events above.
            Event::Html(_)
            | Event::InlineHtml(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
            | Event::Rule
            | Event::TaskListMarker(_)
            | Event::Start(_)
            | Event::End(_) => {}
        }
    }
    flush_block(&mut blocks, &pending_kind, &mut spans);
    blocks
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{
        DocumentBlockKind, DocumentLoadState, DocumentOpenerOutcome, DocumentSpanStyle,
    };

    use super::*;
    use crate::testing;

    #[test]
    fn markdown_loads_headings_lists_code_and_tables() {
        let source = "# LM317 notes\n\nRegulator checklist:\n\n- check dropout\n  - at 500 mA\n\n\
                      `SET_PIN` is pin 1.\n\n```rust\nlet v = 1.25;\n```\n\n\
                      | Part | Value |\n| --- | --- |\n| R1 | 240R |\n";
        let request = testing::request_for("notes.md", source.as_bytes());

        let outcome = MarkdownOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("valid markdown must load, got {outcome:?}");
        };
        assert_eq!(view.title, "notes");
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("markdown renders as blocks");
        };
        assert!(blocks.iter().any(|block| block.kind == DocumentBlockKind::Heading { level: 1 }
            && block.text == "LM317 notes"));
        assert!(blocks.iter().any(|block| block.kind == DocumentBlockKind::ListItem { depth: 1 }));
        assert!(blocks.iter().any(|block| matches!(block.kind, DocumentBlockKind::Code { .. })));
        assert!(blocks.iter().any(|block| block.text.contains("Part | Value")));
        assert!(blocks.iter().any(|block| block.text.contains("240R")));
    }

    #[test]
    fn inline_emphasis_survives_as_styled_spans() {
        let source = "Use a **1uF capacitor** and *mind the polarity*, ~~old value~~, `SET_PIN`.\n";
        let request = testing::request_for("spans.md", source.as_bytes());

        let outcome = MarkdownOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("valid markdown must load, got {outcome:?}");
        };
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("markdown renders as blocks");
        };
        let paragraph = blocks
            .iter()
            .find(|block| block.kind == DocumentBlockKind::Paragraph)
            .expect("one paragraph block");
        let styles: Vec<(&str, DocumentSpanStyle)> =
            paragraph.spans.iter().map(|span| (span.text.as_str(), span.style)).collect();
        assert!(
            styles.iter().any(|(text, style)| {
                *style == DocumentSpanStyle::Strong && text.contains("1uF capacitor")
            }),
            "bold text becomes a strong span: {styles:?}"
        );
        assert!(
            styles.iter().any(|(_, style)| *style == DocumentSpanStyle::Emphasis),
            "italic survives"
        );
        assert!(
            styles.iter().any(|(text, style)| {
                *style == DocumentSpanStyle::Strikethrough && text.contains("old value")
            }),
            "strikethrough survives: {styles:?}"
        );
        assert!(
            styles
                .iter()
                .any(|(text, style)| *style == DocumentSpanStyle::Code && *text == "SET_PIN")
        );
        // The plain text is the exact concatenation of the spans.
        let joined: String = paragraph.spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(paragraph.text, joined);
    }

    #[test]
    fn html_and_link_destinations_are_never_surfaced() {
        let source = "# Title\n\n<script>alert('active')</script>\n\n\
                      [visit the docs](https://evil.example/path)\n\n\
                      ![pinout](https://evil.example/pinout.png)\n";
        let request = testing::request_for("evil.md", source.as_bytes());

        let outcome = MarkdownOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("the document itself is valid markdown");
        };
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("markdown renders as blocks");
        };
        let rendered: String = blocks
            .iter()
            .flat_map(|block| {
                let mut parts = vec![block.text.clone(), format!("{:?}", block.kind)];
                parts.extend(block.spans.iter().map(|span| span.text.clone()));
                parts
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!rendered.contains("script"), "raw HTML is dropped: {rendered}");
        assert!(!rendered.contains("evil.example"), "no URL survives: {rendered}");
        assert!(rendered.contains("visit the docs"), "link text stays readable");
        assert!(rendered.contains("pinout"), "image alt text stays readable");
    }

    #[test]
    fn non_utf8_markdown_fails_explicitly() {
        let request = testing::request_for("broken.md", &[0xff, 0xfe, 0x00, 0x01]);

        let outcome = MarkdownOpener.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Failed);
    }
}
