//! HTML opener: block extraction through a small in-process tokenizer.
//!
//! Active-content policy mirrors the Markdown opener: `<script>`, `<style>`, and
//! `<template>` contents are skipped without being read, `<head>` content is dropped,
//! and link/image destinations never surface — only their inner text and image `alt`
//! text stay readable. The tokenizer is deliberately tolerant: unclosed tags, stray
//! `<`, and malformed attributes degrade to readable text instead of failing the
//! preview. It never builds a DOM, resolves references, or evaluates anything. Legacy
//! encodings are not detected; the managed copy must be UTF-8.

use circuitfabric_plugin_api::{
    DocumentBlock, DocumentBlockKind, DocumentOpener, DocumentOpenerOutcome, DocumentOpenerRequest,
    DocumentSpanStyle, DocumentViewBody, MAX_DOCUMENT_VIEW_BLOCKS, OpenerCapability,
};

use crate::{Spans, capped_text, view};

/// Opens HTML managed copies as a block-oriented, read-only view.
#[derive(Clone, Copy, Debug, Default)]
pub struct HtmlOpener;

impl DocumentOpener for HtmlOpener {
    fn capability(&self) -> OpenerCapability {
        OpenerCapability {
            opener_id: "builtin-html".to_owned(),
            label: "HTML document".to_owned(),
            extensions: vec!["html".to_owned(), "htm".to_owned()],
            mime_types: vec!["text/html".to_owned(), "application/xhtml+xml".to_owned()],
        }
    }

    fn open(&self, request: &DocumentOpenerRequest) -> DocumentOpenerOutcome {
        let Ok(text) = String::from_utf8(request.managed_copy.data().to_vec()) else {
            return DocumentOpenerOutcome::Failed {
                reason: "the HTML document is not valid UTF-8".to_owned(),
            };
        };
        let blocks = render_blocks(&text);
        if blocks.is_empty() {
            return DocumentOpenerOutcome::Failed {
                reason: "the HTML document contains no readable body text".to_owned(),
            };
        }
        let truncated = blocks.len() > MAX_DOCUMENT_VIEW_BLOCKS;
        let blocks = blocks.into_iter().take(MAX_DOCUMENT_VIEW_BLOCKS).collect();
        DocumentOpenerOutcome::Loaded {
            view: view("builtin-html", request, DocumentViewBody::Blocks { blocks, truncated }),
        }
    }
}

/// One lexical item the tokenizer can surface.
enum Token {
    /// A start tag; `alt` is the decoded `alt` attribute when present.
    Start { name: String, alt: Option<String> },
    /// An end tag.
    End { name: String },
    /// A start tag whose raw contents must be skipped entirely.
    RawText { name: String },
    /// A comment, doctype, or processing instruction — consumed, never surfaced.
    Ignored,
}

/// Parses one `<...>` construct at the start of `input`; `None` means the `<` begins
/// no construct and should stay literal text.
fn next_token(input: &str) -> Option<(Token, &str)> {
    debug_assert!(input.starts_with('<'));
    let after_lt = &input[1..];
    let first = after_lt.chars().next()?;
    if first == '!' {
        if after_lt.starts_with("<!--") {
            let rest = skip_through(after_lt, "-->");
            return Some((Token::Ignored, rest));
        }
        return Some((Token::Ignored, skip_through(after_lt, ">")));
    }
    if first == '?' {
        return Some((Token::Ignored, skip_through(after_lt, ">")));
    }
    if first == '/' {
        let (name, rest) = take_tag_name(&after_lt[1..]);
        let rest = skip_through(rest, ">");
        let token = if name.is_empty() {
            Token::Ignored
        } else {
            Token::End { name: name.to_ascii_lowercase() }
        };
        return Some((token, rest));
    }
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let (name, rest) = take_tag_name(after_lt);
    let lowered = name.to_ascii_lowercase();
    if matches!(lowered.as_str(), "script" | "style" | "template") {
        // Closing quotes inside raw-text elements are plain characters, so attributes
        // are not parsed at all — the skip starts right after the tag name's `>`.
        let rest = skip_through(rest, ">");
        return Some((Token::RawText { name: lowered }, rest));
    }
    let (alt, rest) = take_attributes(rest);
    Some((Token::Start { name: lowered, alt }, rest))
}

/// Splits an ASCII tag name off the front; HTML names are alphanumerics plus `-` and `:`
/// (namespaced Office/Word markup emits tags like `o:p`).
fn take_tag_name(input: &str) -> (&str, &str) {
    let end = input
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == ':'))
        .unwrap_or(input.len());
    input.split_at(end)
}

/// Consumes through the next occurrence of `marker`, or to the end when unterminated.
fn skip_through<'input>(input: &'input str, marker: &str) -> &'input str {
    match input.find(marker) {
        Some(index) => &input[index + marker.len()..],
        None => "",
    }
}

/// Reads a tag's attributes, returning the decoded `alt` value when present and the
/// remainder after the closing `>`. Quoted values may contain `>` and `<`.
fn take_attributes(input: &str) -> (Option<String>, &str) {
    let mut alt = None;
    let mut rest = input;
    loop {
        rest = rest.trim_start();
        let Some(first) = rest.chars().next() else {
            return (alt, "");
        };
        if first == '>' {
            return (alt, &rest[1..]);
        }
        if first == '/' {
            rest = &rest[1..];
            continue;
        }
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let (name, after_name) = rest.split_at(end);
        rest = after_name;
        let after_ws = rest.trim_start();
        if let Some(value_start) = after_ws.strip_prefix('=') {
            rest = value_start.trim_start();
            let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'');
            let (value, after_value) = if let Some(quote) = quote
                && let Some(close) = rest[1..].find(quote)
            {
                (&rest[1..=close], &rest[2 + close..])
            } else {
                let end =
                    rest.find(|c: char| c.is_ascii_whitespace() || c == '>').unwrap_or(rest.len());
                rest.split_at(end)
            };
            if name.eq_ignore_ascii_case("alt") && alt.is_none() {
                alt = Some(decode_entities(value));
            }
            rest = after_value;
        }
    }
}

/// Advances past the closing tag of a raw-text element, case-insensitively. `closing`
/// is the full `</name` needle.
fn skip_raw_text<'input>(rest: &'input str, closing: &str) -> &'input str {
    let mut remaining = rest;
    while let Some(index) = find_ignore_case(remaining, closing) {
        let after = &remaining[index + closing.len()..];
        let boundary =
            after.chars().next().is_some_and(|c| c.is_ascii_whitespace() || c == '>' || c == '/');
        if boundary {
            return skip_through(after, ">");
        }
        remaining = &remaining[index + 1..];
    }
    ""
}

/// Case-insensitive search for an ASCII needle; a match always starts at an ASCII
/// byte, so returned indices are char boundaries.
fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
}

/// One open block being accumulated, plus the inline state around it.
struct BlockState {
    blocks: Vec<DocumentBlock>,
    spans: Spans,
    kind: DocumentBlockKind,
    list_depth: u8,
    quote_depth: u8,
    strong_depth: u32,
    emphasis_depth: u32,
    strikethrough_depth: u32,
    code_depth: u32,
    pre_depth: u32,
    table_row: Vec<String>,
}

impl BlockState {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            spans: Spans::default(),
            kind: DocumentBlockKind::Paragraph,
            list_depth: 0,
            quote_depth: 0,
            strong_depth: 0,
            emphasis_depth: 0,
            strikethrough_depth: 0,
            code_depth: 0,
            pre_depth: 0,
            table_row: Vec::new(),
        }
    }
}

impl BlockState {
    fn flush(&mut self) {
        let kind = if self.quote_depth > 0 && self.kind == DocumentBlockKind::Paragraph {
            DocumentBlockKind::Quote
        } else {
            self.kind.clone()
        };
        self.spans.trim_end();
        if self.spans.is_blank() {
            self.spans.clear();
        } else {
            self.blocks.push(self.spans.take_block(&kind));
        }
        self.kind = DocumentBlockKind::Paragraph;
    }

    /// Emits a flattened table row, matching the Word and Markdown table convention.
    fn flush_row(&mut self) {
        let text = self.table_row.join(" | ");
        self.table_row.clear();
        self.spans.clear();
        if !text.trim().is_empty() {
            self.blocks.push(DocumentBlock {
                kind: DocumentBlockKind::Paragraph,
                text: capped_text(&text),
                spans: Vec::new(),
            });
        }
    }

    fn push_cell(&mut self) {
        let cell = self.spans.take_text();
        self.spans.clear();
        self.table_row.push(cell.trim().to_owned());
    }

    fn push_text(&mut self, chunk: &str) {
        let decoded = decode_entities(chunk);
        if self.pre_depth > 0 {
            self.spans.push(self.inline_style(), &decoded);
            return;
        }
        let collapsed = collapse_whitespace(&decoded);
        let collapsed = if self.spans.is_empty() { collapsed.trim_start() } else { &collapsed };
        self.spans.push(self.inline_style(), collapsed);
    }

    fn inline_style(&self) -> DocumentSpanStyle {
        if self.strong_depth > 0 {
            DocumentSpanStyle::Strong
        } else if self.emphasis_depth > 0 {
            DocumentSpanStyle::Emphasis
        } else if self.strikethrough_depth > 0 {
            DocumentSpanStyle::Strikethrough
        } else if self.code_depth > 0 {
            DocumentSpanStyle::Code
        } else {
            DocumentSpanStyle::Plain
        }
    }

    fn start_element(&mut self, name: &str, alt: Option<String>) {
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                let level = name[1..].parse::<u8>().unwrap_or(1);
                self.kind = DocumentBlockKind::Heading { level };
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "aside"
            | "nav" | "figure" | "figcaption" | "address" | "details" | "summary" | "hgroup"
            | "dl" | "dt" | "dd" | "hr" | "form" | "fieldset" | "legend" | "table" | "thead"
            | "tbody" | "tfoot" | "tr" => self.flush(),
            "ul" | "ol" => {
                self.flush();
                self.list_depth = self.list_depth.saturating_add(1);
            }
            "li" => {
                self.flush();
                self.kind =
                    DocumentBlockKind::ListItem { depth: self.list_depth.saturating_sub(1) };
            }
            "blockquote" => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_add(1);
            }
            "pre" => {
                self.flush();
                self.pre_depth = self.pre_depth.saturating_add(1);
                self.kind = DocumentBlockKind::Code { language: None };
            }
            "td" | "th" => self.spans.clear(),
            "br" => self.spans.push(DocumentSpanStyle::Plain, " "),
            "img" => {
                if let Some(alt) = alt
                    && !alt.trim().is_empty()
                {
                    self.push_text(alt.trim());
                }
            }
            "b" | "strong" => self.strong_depth = self.strong_depth.saturating_add(1),
            "i" | "em" => self.emphasis_depth = self.emphasis_depth.saturating_add(1),
            "s" | "strike" | "del" => {
                self.strikethrough_depth = self.strikethrough_depth.saturating_add(1);
            }
            "code" | "kbd" | "samp" | "var" => {
                self.code_depth = self.code_depth.saturating_add(1);
            }
            // Inline containers (`a`, `span`, `u`, `small`, …) contribute their text
            // through the surrounding flow; their attributes are never read.
            _ => {}
        }
    }

    fn end_element(&mut self, name: &str) {
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "li" | "dt" | "dd" | "hr" | "form"
            | "fieldset" | "legend" | "div" | "section" | "article" | "header" | "footer"
            | "main" | "aside" | "nav" | "figure" | "figcaption" | "address" | "details"
            | "summary" | "hgroup" | "dl" | "thead" | "tbody" | "tfoot" | "body" => self.flush(),
            "table" => {
                self.flush();
                self.table_row.clear();
            }
            "tr" => self.flush_row(),
            "td" | "th" => self.push_cell(),
            "ul" | "ol" => {
                self.flush();
                self.list_depth = self.list_depth.saturating_sub(1);
            }
            "blockquote" => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            "pre" => {
                self.flush();
                self.pre_depth = self.pre_depth.saturating_sub(1);
            }
            "b" | "strong" => self.strong_depth = self.strong_depth.saturating_sub(1),
            "i" | "em" => self.emphasis_depth = self.emphasis_depth.saturating_sub(1),
            "s" | "strike" | "del" => {
                self.strikethrough_depth = self.strikethrough_depth.saturating_sub(1);
            }
            "code" | "kbd" | "samp" | "var" => {
                self.code_depth = self.code_depth.saturating_sub(1);
            }
            _ => {}
        }
    }
}

/// Converts HTML source into read-only blocks; parsing stops once the block cap is
/// exceeded so a hostile document cannot burn unbounded CPU.
fn render_blocks(source: &str) -> Vec<DocumentBlock> {
    let mut state = BlockState::new();
    let mut rest = source;
    let mut in_head = false;
    loop {
        if state.blocks.len() > MAX_DOCUMENT_VIEW_BLOCKS {
            break;
        }
        let (chunk, tail) = if let Some(index) = rest.find('<') {
            rest.split_at(index)
        } else {
            if !in_head {
                state.push_text(rest);
            }
            break;
        };
        if !in_head {
            state.push_text(chunk);
        }
        match next_token(tail) {
            None => {
                if !in_head {
                    state.spans.push(DocumentSpanStyle::Plain, "<");
                }
                rest = &tail[1..];
            }
            Some((Token::Ignored, remainder)) => rest = remainder,
            Some((Token::RawText { name }, remainder)) => {
                rest = skip_raw_text(remainder, &format!("</{name}"));
            }
            Some((Token::Start { name, alt }, remainder)) => {
                rest = remainder;
                if in_head {
                    if name == "body" {
                        in_head = false;
                    }
                    continue;
                }
                match name.as_str() {
                    "head" => in_head = true,
                    "body" => {}
                    _ => state.start_element(&name, alt),
                }
            }
            Some((Token::End { name }, remainder)) => {
                rest = remainder;
                if in_head {
                    if name == "head" {
                        in_head = false;
                    }
                    continue;
                }
                state.end_element(&name);
            }
        }
    }
    if !in_head {
        state.flush();
    }
    state.blocks
}

/// Collapses ASCII whitespace runs to single spaces, the way HTML rendering does.
fn collapse_whitespace(text: &str) -> String {
    if !text.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return text.to_owned();
    }
    let mut collapsed = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_whitespace() {
            if !collapsed.ends_with(' ') {
                collapsed.push(' ');
            }
        } else {
            collapsed.push(character);
        }
    }
    collapsed
}

/// Decodes character references: named entities from the table below plus numeric
/// (`&#65;`, `&#x41;`) forms. Anything else stays literal.
fn decode_entities(text: &str) -> String {
    let Some(first) = text.find('&') else {
        return text.to_owned();
    };
    let mut decoded = String::with_capacity(text.len());
    decoded.push_str(&text[..first]);
    let mut rest = &text[first..];
    while let Some(ampersand) = rest.find('&') {
        decoded.push_str(&rest[..ampersand]);
        let after = &rest[ampersand + 1..];
        let entity = after.find(';').and_then(|end| {
            let candidate = after.get(..end)?;
            let name = candidate.strip_prefix('#').unwrap_or(candidate);
            (end <= 32 && !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()))
                .then_some(candidate)
        });
        if let Some(name) = entity {
            let character = match name.strip_prefix('#') {
                Some(numeric) => {
                    let (digits, radix) =
                        match numeric.strip_prefix('x').or_else(|| numeric.strip_prefix('X')) {
                            Some(hexadecimal) => (hexadecimal, 16_u32),
                            None => (numeric, 10_u32),
                        };
                    u32::from_str_radix(digits, radix)
                        .ok()
                        .and_then(char::from_u32)
                        .unwrap_or(char::REPLACEMENT_CHARACTER)
                }
                None => named_entity(name).unwrap_or('\0'),
            };
            if character != '\0' {
                decoded.push(character);
                rest = &after[name.len() + 1..];
                continue;
            }
            decoded.push('&');
            decoded.push_str(name);
            decoded.push(';');
            rest = &after[name.len() + 1..];
        } else {
            decoded.push('&');
            rest = after;
        }
    }
    decoded.push_str(rest);
    decoded
}

/// The named entities worth decoding in engineering documents: the markup and quote
/// escapes, the full Latin-1 set, common typography and math symbols, and Greek
/// letters (µ, Ω, and friends appear constantly in component HTML).
#[allow(clippy::too_many_lines)]
fn named_entity(name: &str) -> Option<char> {
    let character = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        // Latin-1.
        "iexcl" => '\u{a1}',
        "cent" => '\u{a2}',
        "pound" => '\u{a3}',
        "curren" => '\u{a4}',
        "yen" => '\u{a5}',
        "brvbar" => '\u{a6}',
        "sect" => '\u{a7}',
        "uml" => '\u{a8}',
        "copy" => '\u{a9}',
        "ordf" => '\u{aa}',
        "laquo" => '\u{ab}',
        "not" => '\u{ac}',
        "shy" => '\u{ad}',
        "reg" => '\u{ae}',
        "macr" => '\u{af}',
        "deg" => '\u{b0}',
        "plusmn" => '\u{b1}',
        "sup2" => '\u{b2}',
        "sup3" => '\u{b3}',
        "acute" => '\u{b4}',
        "micro" => '\u{b5}',
        "para" => '\u{b6}',
        "middot" => '\u{b7}',
        "cedil" => '\u{b8}',
        "sup1" => '\u{b9}',
        "ordm" => '\u{ba}',
        "raquo" => '\u{bb}',
        "frac14" => '\u{bc}',
        "frac12" => '\u{bd}',
        "frac34" => '\u{be}',
        "iquest" => '\u{bf}',
        "Agrave" => '\u{c0}',
        "Aacute" => '\u{c1}',
        "Acirc" => '\u{c2}',
        "Atilde" => '\u{c3}',
        "Auml" => '\u{c4}',
        "Aring" => '\u{c5}',
        "AElig" => '\u{c6}',
        "Ccedil" => '\u{c7}',
        "Egrave" => '\u{c8}',
        "Eacute" => '\u{c9}',
        "Ecirc" => '\u{ca}',
        "Euml" => '\u{cb}',
        "Igrave" => '\u{cc}',
        "Iacute" => '\u{cd}',
        "Icirc" => '\u{ce}',
        "Iuml" => '\u{cf}',
        "ETH" => '\u{d0}',
        "Ntilde" => '\u{d1}',
        "Ograve" => '\u{d2}',
        "Oacute" => '\u{d3}',
        "Ocirc" => '\u{d4}',
        "Otilde" => '\u{d5}',
        "Ouml" => '\u{d6}',
        "times" => '\u{d7}',
        "Oslash" => '\u{d8}',
        "Ugrave" => '\u{d9}',
        "Uacute" => '\u{da}',
        "Ucirc" => '\u{db}',
        "Uuml" => '\u{dc}',
        "Yacute" => '\u{dd}',
        "THORN" => '\u{de}',
        "szlig" => '\u{df}',
        "agrave" => '\u{e0}',
        "aacute" => '\u{e1}',
        "acirc" => '\u{e2}',
        "atilde" => '\u{e3}',
        "auml" => '\u{e4}',
        "aring" => '\u{e5}',
        "aelig" => '\u{e6}',
        "ccedil" => '\u{e7}',
        "egrave" => '\u{e8}',
        "eacute" => '\u{e9}',
        "ecirc" => '\u{ea}',
        "euml" => '\u{eb}',
        "igrave" => '\u{ec}',
        "iacute" => '\u{ed}',
        "icirc" => '\u{ee}',
        "iuml" => '\u{ef}',
        "eth" => '\u{f0}',
        "ntilde" => '\u{f1}',
        "ograve" => '\u{f2}',
        "oacute" => '\u{f3}',
        "ocirc" => '\u{f4}',
        "otilde" => '\u{f5}',
        "ouml" => '\u{f6}',
        "divide" => '\u{f7}',
        "oslash" => '\u{f8}',
        "ugrave" => '\u{f9}',
        "uacute" => '\u{fa}',
        "ucirc" => '\u{fb}',
        "uuml" => '\u{fc}',
        "yacute" => '\u{fd}',
        "thorn" => '\u{fe}',
        "yuml" => '\u{ff}',
        // Typography and math.
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "sbquo" => '\u{201a}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "bdquo" => '\u{201e}',
        "dagger" => '\u{2020}',
        "Dagger" => '\u{2021}',
        "bull" => '\u{2022}',
        "hellip" => '\u{2026}',
        "prime" => '\u{2032}',
        "Prime" => '\u{2033}',
        "permil" => '\u{2030}',
        "lsaquo" => '\u{2039}',
        "rsaquo" => '\u{203a}',
        "oline" => '\u{203e}',
        "frasl" => '\u{2044}',
        "euro" => '\u{20ac}',
        "trade" => '\u{2122}',
        "minus" => '\u{2212}',
        "radic" => '\u{221a}',
        "infin" => '\u{221e}',
        "asymp" => '\u{2248}',
        "equiv" => '\u{2261}',
        "ne" => '\u{2260}',
        "le" => '\u{2264}',
        "ge" => '\u{2265}',
        "sum" => '\u{2211}',
        "prod" => '\u{220f}',
        "part" => '\u{2202}',
        "int" => '\u{222b}',
        "larr" => '\u{2190}',
        "uarr" => '\u{2191}',
        "rarr" => '\u{2192}',
        "darr" => '\u{2193}',
        "harr" => '\u{2194}',
        "crarr" => '\u{21b5}',
        // Greek letters.
        "alpha" => '\u{3b1}',
        "beta" => '\u{3b2}',
        "gamma" => '\u{3b3}',
        "delta" => '\u{3b4}',
        "epsilon" => '\u{3b5}',
        "zeta" => '\u{3b6}',
        "eta" => '\u{3b7}',
        "theta" => '\u{3b8}',
        "iota" => '\u{3b9}',
        "kappa" => '\u{3ba}',
        "lambda" => '\u{3bb}',
        "mu" => '\u{3bc}',
        "nu" => '\u{3bd}',
        "xi" => '\u{3be}',
        "omicron" => '\u{3bf}',
        "pi" => '\u{3c0}',
        "rho" => '\u{3c1}',
        "sigmaf" => '\u{3c2}',
        "sigma" => '\u{3c3}',
        "tau" => '\u{3c4}',
        "upsilon" => '\u{3c5}',
        "phi" => '\u{3c6}',
        "chi" => '\u{3c7}',
        "psi" => '\u{3c8}',
        "omega" => '\u{3c9}',
        "Alpha" => '\u{391}',
        "Beta" => '\u{392}',
        "Gamma" => '\u{393}',
        "Delta" => '\u{394}',
        "Epsilon" => '\u{395}',
        "Zeta" => '\u{396}',
        "Eta" => '\u{397}',
        "Theta" => '\u{398}',
        "Iota" => '\u{399}',
        "Kappa" => '\u{39a}',
        "Lambda" => '\u{39b}',
        "Mu" => '\u{39c}',
        "Nu" => '\u{39d}',
        "Xi" => '\u{39e}',
        "Omicron" => '\u{39f}',
        "Pi" => '\u{3a0}',
        "Rho" => '\u{3a1}',
        "Sigma" => '\u{3a3}',
        "Tau" => '\u{3a4}',
        "Upsilon" => '\u{3a5}',
        "Phi" => '\u{3a6}',
        "Chi" => '\u{3a7}',
        "Psi" => '\u{3a8}',
        "Omega" | "ohm" => '\u{3a9}',
        _ => return None,
    };
    Some(character)
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{
        DocumentBlockKind, DocumentLoadState, DocumentOpenerOutcome, DocumentViewBody,
    };

    use super::*;
    use crate::testing;

    #[test]
    fn an_html_document_loads_headings_lists_and_tables() {
        let source = concat!(
            "<!DOCTYPE html><html><head><title>Ignored title</title>",
            "<script>steal()</script><style>body{color:red}</style></head>",
            "<body><h1>Power stages</h1>",
            "<p>Input <b>12&nbsp;V</b> at <i>2 A</i>.</p>",
            "<ul><li>Check dropout<ul><li>at 500 mA</li></ul></li></ul>",
            "<blockquote>See the note</blockquote>",
            "<table><tr><th>Part</th><th>Value</th></tr>",
            "<tr><td>R7</td><td>10k</td></tr></table>",
            "<p><a href=\"https://evil.example/docs\">visit the docs</a> ",
            "<img alt=\"pinout diagram\" src=\"https://evil.example/x.png\"></p>",
            "</body></html>",
        );
        let request = testing::request_for("report.html", source.as_bytes());

        let outcome = HtmlOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("valid html must load, got {outcome:?}");
        };
        assert_eq!(view.title, "report");
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("html renders as blocks");
        };
        assert!(blocks.iter().any(|block| block.kind == DocumentBlockKind::Heading { level: 1 }
            && block.text == "Power stages"));
        let paragraph = blocks
            .iter()
            .find(|block| block.text.starts_with("Input"))
            .expect("styled paragraph survives");
        assert!(
            paragraph.spans.iter().any(|span| {
                span.style == circuitfabric_plugin_api::DocumentSpanStyle::Strong
                    && span.text == "12\u{a0}V"
            }),
            "bold runs become strong spans with entities decoded: {:?}",
            paragraph.spans
        );
        assert!(
            paragraph
                .spans
                .iter()
                .any(|span| span.style == circuitfabric_plugin_api::DocumentSpanStyle::Emphasis),
            "italic survives"
        );
        assert!(blocks.iter().any(|block| block.kind == DocumentBlockKind::ListItem { depth: 0 }
            && block.text == "Check dropout"));
        assert!(blocks.iter().any(|block| block.kind == DocumentBlockKind::ListItem { depth: 1 }));
        assert!(blocks.iter().any(|block| block.kind == DocumentBlockKind::Quote));
        assert!(blocks.iter().any(|block| block.text == "Part | Value"));
        assert!(blocks.iter().any(|block| block.text == "R7 | 10k"));

        let rendered: String = blocks
            .iter()
            .flat_map(|block| {
                let mut parts = vec![block.text.clone(), format!("{:?}", block.kind)];
                parts.extend(block.spans.iter().map(|span| span.text.clone()));
                parts
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!rendered.contains("steal"), "script contents are dropped: {rendered}");
        assert!(!rendered.contains("color:red"), "style contents are dropped: {rendered}");
        assert!(!rendered.contains("Ignored title"), "head contents are dropped: {rendered}");
        assert!(!rendered.contains("evil.example"), "no URL survives: {rendered}");
        assert!(rendered.contains("visit the docs"), "link text stays readable");
        assert!(rendered.contains("pinout diagram"), "image alt text stays readable");
    }

    #[test]
    fn unclosed_and_malformed_markup_still_extracts_text() {
        let source = concat!(
            "<H3>Upper &amp; unclosed</H3>",
            "<p>First paragraph\n<p>Second &#65;&#x42; items &lt; 5",
            "<div class=\"truncated at eof",
        );
        let request = testing::request_for("messy.htm", source.as_bytes());

        let outcome = HtmlOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("tolerant parsing must load, got {outcome:?}");
        };
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("html renders as blocks");
        };
        assert!(
            blocks.iter().any(|block| block.kind == DocumentBlockKind::Heading { level: 3 }
                && block.text == "Upper & unclosed"),
            "uppercase tags and entities work: {blocks:?}"
        );
        assert!(blocks.iter().any(|block| block.text == "First paragraph"));
        assert!(
            blocks.iter().any(|block| block.text == "Second AB items < 5"),
            "numeric entities and stray comparisons decode: {blocks:?}"
        );
    }

    #[test]
    fn raw_text_elements_are_never_surfaced() {
        let source = concat!(
            "<body><p>keep me</p>",
            "<script>var x = '<div>not text</div>';</script>",
            "<style>.a { content: 'x' }</style>",
            "<template><p>template text</p></template>",
            "<p>also keep</p></body>"
        );
        let request = testing::request_for("raw.html", source.as_bytes());

        let outcome = HtmlOpener.open(&request);

        let DocumentOpenerOutcome::Loaded { view } = outcome else {
            panic!("the document itself is valid html");
        };
        let DocumentViewBody::Blocks { blocks, .. } = view.body else {
            panic!("html renders as blocks");
        };
        let rendered: String = blocks.iter().map(|block| block.text.as_str()).collect();
        assert!(rendered.contains("keep me") && rendered.contains("also keep"));
        assert!(!rendered.contains("not text"), "script raw text dropped: {rendered}");
        assert!(!rendered.contains("var x"), "script raw text dropped: {rendered}");
        assert!(!rendered.contains(".a"), "style raw text dropped: {rendered}");
        assert!(!rendered.contains("template text"), "template raw text dropped: {rendered}");
    }

    #[test]
    fn non_utf8_html_fails_explicitly() {
        let request = testing::request_for("broken.html", &[0xff, 0xfe, 0x00, 0x01]);

        let outcome = HtmlOpener.open(&request);

        assert_eq!(outcome.load_state(), DocumentLoadState::Failed);
    }
}
