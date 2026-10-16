//! Keep audit event envelopes out of the rich-text list hierarchy.
//!
//! The Markdown viewer virtualizes top-level blocks. Rendering the persisted audit
//! as one list makes every message a descendant of one enormous visible block,
//! so every scrollbar move lays out the entire conversation again.

use circuitfabric_project::SessionActor;

/// One row of the chat-style conversation view, parsed from the persisted audit body.
///
/// A turn is split into several rows when its message is long: every chunk becomes its
/// own virtualized list item, so dragging the scrollbar never lays out more than the
/// few chunks on screen. Only the `first` chunk of a turn carries the actor header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ConversationRow {
    Reasoning {
        timestamp: String,
        text: String,
    },
    Turn {
        actor: SessionActor,
        timestamp: String,
        first: bool,
        text: String,
    },
    /// Non-message audit events: tool calls, approvals, evidence, usage, status, notes.
    System {
        timestamp: String,
        label: String,
        detail: String,
    },
}

/// Parse the stored audit body into chat rows. Storage bytes stay untouched; this is a
/// presentation projection built when a replay opens.
pub(super) fn conversation_rows(body: &str) -> Vec<ConversationRow> {
    let mut rows = Vec::new();
    // A multiline turn being accumulated: (timestamp, actor, message lines so far).
    let mut pending: Option<(String, String, String)> = None;
    for line in body.lines() {
        if let Some((timestamp, actor, rest)) = event_header(line) {
            if let Some((timestamp, actor, message)) = pending.take() {
                push_event_rows(&mut rows, &timestamp, &actor, &message);
            }
            match actor {
                "用户" | "智能体" | "思考摘要" => {
                    if rest == "消息" {
                        pending = Some((timestamp.to_owned(), actor.to_owned(), String::new()));
                    } else {
                        push_event_rows(&mut rows, timestamp, actor, rest);
                    }
                }
                _ => rows.push(ConversationRow::System {
                    timestamp: timestamp.to_owned(),
                    label: actor.to_owned(),
                    detail: rest.to_owned(),
                }),
            }
        } else if let Some((_, _, message)) = &mut pending {
            // Only remove storage's envelope indentation; nested lists, quotes and
            // fenced code keep their own leading whitespace.
            message.push_str(line.strip_prefix("    ").unwrap_or(line));
            message.push('\n');
        }
        // Headings and blank lines outside events carry no conversation content.
    }
    if let Some((timestamp, actor, message)) = pending.take() {
        push_event_rows(&mut rows, &timestamp, &actor, &message);
    }
    rows
}

fn push_event_rows(rows: &mut Vec<ConversationRow>, timestamp: &str, label: &str, text: &str) {
    if label == "思考摘要" {
        rows.push(ConversationRow::Reasoning {
            timestamp: timestamp.into(),
            text: text.trim().into(),
        });
    } else {
        push_turn_rows(
            rows,
            timestamp,
            if label == "用户" { SessionActor::User } else { SessionActor::Agent },
            text,
        );
    }
}

fn push_turn_rows(
    rows: &mut Vec<ConversationRow>,
    timestamp: &str,
    actor: SessionActor,
    message: &str,
) {
    let readable;
    let message = if actor == SessionActor::Agent {
        readable = crate::application::datasheet_dialogue::readable_reply(message);
        readable.as_str()
    } else {
        message
    };
    for (index, chunk) in chunk_message(message).into_iter().enumerate() {
        rows.push(ConversationRow::Turn {
            actor,
            timestamp: timestamp.to_owned(),
            first: index == 0,
            text: chunk,
        });
    }
}

pub(super) fn live_rows(answer: &str, reasoning: &str) -> Vec<ConversationRow> {
    let mut rows = Vec::new();
    if !reasoning.is_empty() {
        rows.push(ConversationRow::Reasoning {
            timestamp: "正在思考".into(),
            text: reasoning.into(),
        });
    }
    if !answer.is_empty() {
        push_turn_rows(&mut rows, "正在回复", SessionActor::Agent, answer);
    } else {
        rows.push(ConversationRow::System {
            timestamp: String::new(),
            label: "智能体".into(),
            detail: "正在等待回复…".into(),
        });
    }
    rows
}

/// Split one message into virtualization-friendly chunks: paragraphs are first bounded
/// the same way as the raw view, then top-level Markdown blocks are grouped under a
/// byte target. Fenced code, tables and lists stay whole inside one chunk.
fn chunk_message(message: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Options, Parser};
    /// Roughly two viewportfuls of body text; matched against UTF-8 bytes.
    const CHUNK_TARGET_BYTES: usize = 1600;
    let bounded = bound_plain_paragraphs(message.trim());
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut blocks: Vec<&str> = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0_usize;
    for (event, range) in Parser::new_ext(&bounded, options).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    start = range.start;
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                // Offset ranges may include one trailing newline; blank-line
                // separation between blocks is re-added when chunks are joined.
                let block = bounded[start..range.end].trim();
                if depth == 0 && !block.is_empty() {
                    blocks.push(block);
                }
            }
            _ => {}
        }
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for block in blocks {
        if !current.is_empty() && current.len() + block.len() > CHUNK_TARGET_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(block);
    }
    if !current.is_empty() || chunks.is_empty() {
        chunks.push(current);
    }
    chunks
}

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
pub(super) fn bound_plain_paragraphs(source: &str) -> String {
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
    fn reasoning_summaries_keep_full_markdown_and_stay_separate_from_answers() {
        let rows = conversation_rows(
            "- **1970-01-01T00:00:01Z** · 思考摘要 · 消息\n\n    **检查文档**\n\n    第二段。\n- **1970-01-01T00:00:02Z** · 智能体 · 完成。",
        );
        assert_eq!(
            rows[0],
            ConversationRow::Reasoning {
                timestamp: "1970-01-01T00:00:01Z".into(),
                text: "**检查文档**\n\n第二段。".into()
            }
        );
        assert!(matches!(&rows[1], ConversationRow::Turn { text, .. } if text == "完成。"));
        let live = live_rows("回复内容", "运行时摘要");
        assert!(
            matches!(&live[0], ConversationRow::Reasoning { text, .. } if text == "运行时摘要")
        );
        assert!(matches!(&live[1], ConversationRow::Turn { text, .. } if text == "回复内容"));
        assert!(
            !live_rows("完成", "").iter().any(|r| matches!(r, ConversationRow::Reasoning { .. }))
        );
    }

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
    fn conversation_rows_group_turns_system_events_and_split_long_messages() {
        let body = "## 会话摘要\n\n## 轮次与工具调用\n\n\
            - **1970-01-01T00:00:00Z** · 用户 · 提取这份手册\n\
            - **1970-01-01T00:00:01Z** · 智能体 · 消息\n\n    \
            # Result\n\n    \
            ```json\n    \
            {\"pins\": []}\n    \
            ```\n\n    \
            完成。\n\
            - **1970-01-01T00:00:02Z** · 工具调用 · jev.evaluate\n\
            - **1970-01-01T00:00:03Z** · 状态 · running → completed\n";
        let rows = conversation_rows(body);
        assert_eq!(
            rows[0],
            ConversationRow::Turn {
                actor: SessionActor::User,
                timestamp: "1970-01-01T00:00:00Z".into(),
                first: true,
                text: "提取这份手册".into(),
            }
        );
        match &rows[1] {
            ConversationRow::Turn { actor, first, text, .. } => {
                assert_eq!(*actor, SessionActor::Agent);
                assert!(*first);
                assert!(text.contains("# Result"));
                assert!(text.contains("```json\n{\"pins\": []}\n```"));
                assert!(text.contains("完成。"));
            }
            other => panic!("expected agent turn, got {other:?}"),
        }
        assert_eq!(
            rows[2],
            ConversationRow::System {
                timestamp: "1970-01-01T00:00:02Z".into(),
                label: "工具调用".into(),
                detail: "jev.evaluate".into(),
            }
        );
        assert!(matches!(&rows[3], ConversationRow::System { label, .. } if label == "状态"));

        // A long message becomes several rows; only the first carries the header.
        let long = format!(
            "- **1970-01-01T00:00:04Z** · 用户 · 消息\n\n    {}",
            "一段较长的中文说明。".repeat(400).replace('\n', "\n    ")
        );
        let rows = conversation_rows(&long);
        assert!(rows.len() > 1, "long messages split into chunk rows: {}", rows.len());
        assert!(rows.iter().all(|row| matches!(row, ConversationRow::Turn { .. })));
        let headers =
            rows.iter().filter(|row| matches!(row, ConversationRow::Turn { first, .. } if *first));
        assert_eq!(headers.count(), 1);
        let reassembled: String = rows
            .iter()
            .map(|row| match row {
                ConversationRow::Turn { text, .. } => text.as_str(),
                ConversationRow::System { .. } | ConversationRow::Reasoning { .. } => {
                    unreachable!()
                }
            })
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(reassembled.replace("\n\n", ""), "一段较长的中文说明。".repeat(400));

        // Empty bodies and fresh session skeletons produce no rows.
        assert!(conversation_rows("").is_empty());
        assert!(conversation_rows("## 会话摘要\n\n## 轮次与工具调用\n").is_empty());
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
