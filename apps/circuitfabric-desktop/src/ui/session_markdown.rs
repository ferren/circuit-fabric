//! Keep audit event envelopes out of the rich-text list hierarchy.
//!
//! The Markdown viewer virtualizes top-level blocks. Rendering the persisted audit
//! as one list makes every message a descendant of one enormous visible block,
//! so every scrollbar move lays out the entire conversation again.

/// Build a read-only presentation copy once when opening a replay. Stored audit
/// bytes, event locators and runtime history remain unchanged.
pub(super) fn presentation_source(body: &str) -> String {
    let mut source = String::with_capacity(body.len());
    let mut multiline_message = false;
    for line in body.lines() {
        if let Some((timestamp, actor, message)) = event_header(line) {
            multiline_message = message == "消息";
            source.push_str("\n\n### ");
            source.push_str(timestamp);
            source.push_str(" · ");
            source.push_str(actor);
            source.push_str("\n\n");
            if !multiline_message {
                source.push_str(message);
                source.push_str("\n\n");
            }
        } else {
            // Only remove storage's envelope indentation; preserve nested lists,
            // block quotes and fenced code inside the original message.
            source.push_str(if multiline_message {
                line.strip_prefix("    ").unwrap_or(line)
            } else {
                line
            });
            source.push('\n');
        }
    }
    bound_plain_paragraphs(&source)
}

/// Legacy prompts and JSON replies can contain tens of thousands of characters
/// in one paragraph. The viewer virtualizes blocks, not individual text lines.
/// Insert presentation-only paragraph breaks outside inline formatting so a
/// visible paragraph cannot force layout of an entire flattened document.
fn bound_plain_paragraphs(source: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    const TARGET_BYTES: usize = 768;
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut cuts = Vec::new();
    let mut depth = 0_usize;
    let mut root_paragraph = false;
    let mut start = 0;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 && matches!(tag, Tag::Paragraph) {
                    root_paragraph = true;
                    start = range.start;
                }
                depth += 1;
            }
            Event::End(tag) => {
                if depth == 1 && matches!(tag, TagEnd::Paragraph) {
                    root_paragraph = false;
                }
                depth = depth.saturating_sub(1);
            }
            Event::Text(_) if root_paragraph && depth == 1 => {
                for (offset, ch) in source[range.clone()].char_indices() {
                    let end = range.start + offset + ch.len_utf8();
                    if end < range.end
                        && end.saturating_sub(start) >= TARGET_BYTES
                        && (ch.is_whitespace() || end.saturating_sub(start) >= TARGET_BYTES + 256)
                        && ch != '\\'
                        && safe_paragraph_start(&source[end..range.end])
                    {
                        cuts.push(end);
                        start = end;
                    }
                }
            }
            _ => {}
        }
    }
    let mut result = String::with_capacity(source.len() + cuts.len() * 2);
    let mut previous = 0;
    for end in cuts {
        result.push_str(&source[previous..end]);
        result.push_str("\n\n");
        previous = end;
    }
    result.push_str(&source[previous..]);
    result
}

fn safe_paragraph_start(source: &str) -> bool {
    let source = source.trim_start();
    !source.starts_with(['#', '>', '`', '~'])
        && !source.starts_with("- ")
        && !source.starts_with("+ ")
        && !source.starts_with("* ")
        && !source.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}

fn event_header(line: &str) -> Option<(&str, &str, &str)> {
    let (timestamp, rest) = line.strip_prefix("- **")?.split_once("** · ")?;
    // Only top-level storage-generated event envelopes are transformed.
    if timestamp.len() != 20 || timestamp.as_bytes()[10] != b'T' || timestamp.as_bytes()[19] != b'Z'
    {
        return None;
    }
    let (actor, message) = rest.split_once(" · ")?;
    Some((timestamp, actor, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_messages_become_independent_blocks_without_changing_their_markdown() {
        let body = "## 轮次与工具调用\n\n- **1970-01-01T00:00:00Z** · 用户 · Start\n- **1970-01-01T00:00:01Z** · 智能体 · 消息\n\n    # Result\n\n    - First\n      - Nested\n\n    ```json\n    {\"pins\": []}\n    ```\n\n    | Name | Value |\n    | --- | --- |\n    | V | 3.3 |\n\n    - **1970-01-01T00:00:02Z** · 审批 · quoted event\n\n- **1970-01-01T00:00:03Z** · 状态 · running → completed\n";
        let source = presentation_source(body);
        assert!(source.contains("### 1970-01-01T00:00:00Z · 用户\n\nStart"));
        assert!(source.contains("\n# Result\n"));
        assert!(source.contains("\n- First\n  - Nested\n"));
        assert!(source.contains("\n```json\n{\"pins\": []}\n```\n"));
        assert!(source.contains("| Name | Value |\n| --- | --- |\n| V | 3.3 |"));
        assert!(source.contains("- **1970-01-01T00:00:02Z** · 审批 · quoted event"));
        assert!(!source.contains("### 1970-01-01T00:00:02Z"));
        assert!(source.contains("### 1970-01-01T00:00:03Z · 状态"));
        assert_eq!(
            presentation_source("# Plain\n\n- first\n- second\n"),
            "# Plain\n\n- first\n- second\n"
        );
    }

    #[test]
    fn flattened_paragraphs_are_bounded_without_losing_unicode_or_splitting_markdown_containers() {
        let plain = "Long readable 中文 paragraph. ".repeat(400);
        let bounded = bound_plain_paragraphs(&plain);
        assert!(bounded.split("\n\n").all(|block| block.len() <= 1050));
        assert_eq!(bounded.replace("\n\n", ""), plain);
        let dense = "连续中文没有空格".repeat(400);
        assert_eq!(bound_plain_paragraphs(&dense).replace("\n\n", ""), dense);
        let bold = plain.trim_end();
        let markdown = format!(
            "**{bold}**\n\n```json\n{plain}\n```\n\n| Value |\n| --- |\n| {plain} |\n\n- {plain}\n"
        );
        assert!(
            bound_plain_paragraphs(&markdown) == markdown,
            "inline marks, code, tables and nested lists must stay intact"
        );
    }
}
