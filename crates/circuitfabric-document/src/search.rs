//! Ranked evidence search over a point-in-time snapshot of one project's fragments.
//!
//! [`EvidenceCorpus`] shares the indexed fragments by `Arc`, so taking a snapshot is cheap
//! and the search itself can run off the UI thread.

use std::{collections::BTreeSet, ops::Range, sync::Arc};

use circuitfabric_contracts::DocumentFragment;

/// Which kind of citable fragment a search covers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EvidenceScope {
    #[default]
    All,
    /// Full-text lines of indexed documents.
    FullText,
    /// Individually verified datasheet rows.
    VerifiedData,
}

/// The position a fragment's locator points to, parsed from the part after `#`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FragmentAnchor {
    /// `#line=<n>` in a text document.
    Line {
        line: u32,
    },
    /// `#page=<n>&line=<m>` in a paged (PDF) document; `line` counts within the page.
    PageLine {
        page: u32,
        line: u32,
    },
    /// `#datasheet=<section>/<row>`: a verified extraction row, `row` counted from 1.
    DatasheetRow {
        section: String,
        row: usize,
    },
    Unknown,
}

impl FragmentAnchor {
    #[must_use]
    pub fn parse(locator: &str) -> Self {
        let Some((_, anchor)) = locator.rsplit_once('#') else {
            return Self::Unknown;
        };
        if let Some(row) = anchor.strip_prefix("datasheet=") {
            return row
                .rsplit_once('/')
                .and_then(|(section, row)| {
                    Some(Self::DatasheetRow { section: section.to_owned(), row: row.parse().ok()? })
                })
                .unwrap_or(Self::Unknown);
        }
        let (mut page, mut line) = (None, None);
        for pair in anchor.split('&') {
            match pair.split_once('=') {
                Some(("page", value)) => page = value.parse().ok(),
                Some(("line", value)) => line = value.parse().ok(),
                _ => return Self::Unknown,
            }
        }
        match (page, line) {
            (Some(page), Some(line)) => Self::PageLine { page, line },
            (None, Some(line)) => Self::Line { line },
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub const fn is_verified_data(&self) -> bool {
        matches!(self, Self::DatasheetRow { .. })
    }
}

/// One matching fragment with the byte ranges of `fragment.text` that matched a term.
#[derive(Clone, Debug)]
pub struct EvidenceHit {
    pub fragment: DocumentFragment,
    pub anchor: FragmentAnchor,
    pub highlights: Vec<Range<usize>>,
    pub score: i64,
}

/// Ranked hits (best first, at most the requested limit) plus totals over all matches.
#[derive(Clone, Debug, Default)]
pub struct EvidenceSearch {
    pub query: String,
    pub terms: Vec<String>,
    pub hits: Vec<EvidenceHit>,
    /// Matching fragments before the limit was applied.
    pub total: usize,
    /// Distinct documents among all matching fragments.
    pub documents: usize,
}

/// A point-in-time copy of one project's citable fragments.
#[derive(Clone, Debug, Default)]
pub struct EvidenceCorpus {
    pub(crate) shards: Vec<Arc<[DocumentFragment]>>,
}

impl EvidenceCorpus {
    #[must_use]
    pub fn fragment_count(&self) -> usize {
        self.shards.iter().map(|shard| shard.len()).sum()
    }

    /// Finds fragments containing every term of `query`, ranked by relevance.
    ///
    /// Terms are whitespace-separated; `"double quotes"` keep a phrase together. Matching
    /// ignores case, collapses whitespace, and treats `µ`/`μ` and dash/minus variants as
    /// equal. `admit` can exclude fragments (for example of documents that failed
    /// integrity verification) before they are counted.
    #[must_use]
    pub fn search(
        &self,
        query: &str,
        scope: EvidenceScope,
        admit: impl Fn(&DocumentFragment) -> bool,
        limit: usize,
    ) -> EvidenceSearch {
        let terms = query_terms(query);
        let mut search = EvidenceSearch { query: query.to_owned(), ..EvidenceSearch::default() };
        if terms.is_empty() {
            return search;
        }
        let phrase = (terms.len() > 1).then(|| terms.join(" "));
        let mut documents = BTreeSet::new();
        let mut hits = Vec::new();
        for fragment in self.shards.iter().flat_map(|shard| shard.iter()) {
            let anchor = FragmentAnchor::parse(&fragment.locator);
            let in_scope = match scope {
                EvidenceScope::All => true,
                EvidenceScope::FullText => !anchor.is_verified_data(),
                EvidenceScope::VerifiedData => anchor.is_verified_data(),
            };
            if !in_scope || !admit(fragment) {
                continue;
            }
            let Some((highlights, mut relevance)) = match_terms(&fragment.text, &terms) else {
                continue;
            };
            if phrase.as_ref().is_some_and(|whole| fold(&fragment.text).text.contains(whole)) {
                relevance += 25;
            }
            if anchor.is_verified_data() {
                relevance += 15;
            }
            relevance -= i64::try_from(fragment.text.len() / 60).unwrap_or(i64::MAX);
            documents.insert(fragment.document_id.as_str());
            hits.push(EvidenceHit {
                fragment: fragment.clone(),
                anchor,
                highlights,
                score: relevance,
            });
        }
        search.total = hits.len();
        search.documents = documents.len();
        // Stable: equal scores keep document and line order.
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        hits.truncate(limit);
        search.hits = hits;
        search.terms = terms;
        search
    }
}

/// Splits a query into folded terms; `"quoted text"` stays one term.
#[must_use]
pub fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for (index, part) in query.split('"').enumerate() {
        let candidates: Vec<&str> =
            if index % 2 == 1 { vec![part] } else { part.split_whitespace().collect() };
        for candidate in candidates {
            let term = fold(candidate).text;
            if !term.is_empty() && !terms.contains(&term) {
                terms.push(term);
            }
        }
    }
    terms
}

/// Highlight ranges and base score when every term occurs in `text`.
fn match_terms(text: &str, terms: &[String]) -> Option<(Vec<Range<usize>>, i64)> {
    let folded = fold(text);
    let mut highlights = Vec::new();
    let mut score = 0;
    for term in terms {
        let mut count = 0;
        let mut at_word_start = false;
        for (position, _) in folded.text.match_indices(term.as_str()) {
            count += 1;
            at_word_start |= folded.text[..position]
                .chars()
                .next_back()
                .is_none_or(|previous| !previous.is_alphanumeric());
            highlights
                .push(folded.origin[position].start..folded.origin[position + term.len() - 1].end);
        }
        if count == 0 {
            return None;
        }
        score += 10 * count.min(3) + if at_word_start { 4 } else { 0 };
    }
    highlights.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(highlights.len());
    for range in highlights {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    Some((merged, score))
}

/// Text normalized for matching, with the source byte range of every folded byte.
struct Folded {
    text: String,
    origin: Vec<Range<usize>>,
}

fn fold(text: &str) -> Folded {
    let mut folded = Folded { text: String::with_capacity(text.len()), origin: Vec::new() };
    let mut pending_space = false;
    for (start, character) in text.char_indices() {
        if character.is_whitespace() {
            pending_space = !folded.text.is_empty();
            continue;
        }
        if pending_space {
            folded.text.push(' ');
            folded.origin.push(start..start);
            pending_space = false;
        }
        let end = start + character.len_utf8();
        let mapped = match character {
            'µ' => 'μ',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            other => other,
        };
        for lower in mapped.to_lowercase() {
            let before = folded.text.len();
            folded.text.push(lower);
            folded.origin.extend((before..folded.text.len()).map(|_| start..end));
        }
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fragment(document_id: &str, locator: &str, text: &str) -> DocumentFragment {
        DocumentFragment {
            document_id: document_id.to_owned(),
            content_hash: "sha256:test".to_owned(),
            locator: locator.to_owned(),
            text: text.to_owned(),
        }
    }

    fn corpus(fragments: Vec<DocumentFragment>) -> EvidenceCorpus {
        EvidenceCorpus { shards: vec![fragments.into()] }
    }

    #[test]
    fn anchors_parse_every_locator_form() {
        assert_eq!(FragmentAnchor::parse("a.md#line=12"), FragmentAnchor::Line { line: 12 });
        assert_eq!(
            FragmentAnchor::parse("docs/a.pdf#page=3&line=7"),
            FragmentAnchor::PageLine { page: 3, line: 7 }
        );
        assert_eq!(
            FragmentAnchor::parse("a.pdf#datasheet=electricalCharacteristics/4"),
            FragmentAnchor::DatasheetRow {
                section: "electricalCharacteristics".to_owned(),
                row: 4
            }
        );
        assert_eq!(FragmentAnchor::parse("a.pdf"), FragmentAnchor::Unknown);
        assert_eq!(FragmentAnchor::parse("a.pdf#zoom=2"), FragmentAnchor::Unknown);
    }

    #[test]
    fn every_term_must_match_and_quotes_keep_phrases() {
        let corpus = corpus(vec![
            fragment("a", "a.pdf#page=1&line=1", "VIN  Input voltage  6 V"),
            fragment("a", "a.pdf#page=1&line=2", "Output voltage accuracy"),
            fragment("b", "b.md#line=1", "input capacitor 10 µF"),
        ]);
        let found = corpus.search("input voltage", EvidenceScope::All, |_| true, 10);
        assert_eq!(found.total, 1);
        assert_eq!(found.hits[0].fragment.text, "VIN  Input voltage  6 V");

        assert_eq!(corpus.search("\"input voltage\"", EvidenceScope::All, |_| true, 10).total, 1);
        assert_eq!(corpus.search("\"voltage input\"", EvidenceScope::All, |_| true, 10).total, 0);
        assert_eq!(query_terms("  VIN \"Input  Voltage\" vin "), vec!["vin", "input voltage"]);
    }

    #[test]
    fn matching_folds_case_whitespace_and_look_alikes_and_highlights_the_source() {
        let corpus = corpus(vec![fragment("b", "b.md#line=1", "Cin 10 μF, VDD −0.3 V")]);
        let found = corpus.search("10 µf \"vdd -0.3\"", EvidenceScope::All, |_| true, 10);
        assert_eq!(found.total, 1);
        let text = &found.hits[0].fragment.text;
        let highlighted: Vec<&str> =
            found.hits[0].highlights.iter().map(|range| &text[range.clone()]).collect();
        assert_eq!(highlighted, vec!["10", "μF", "VDD −0.3"]);
    }

    #[test]
    fn verified_rows_and_phrases_rank_first_and_scope_filters() {
        let corpus = corpus(vec![
            fragment("a", "a.pdf#page=2&line=9", "the input voltage range is wide"),
            fragment("a", "a.pdf#page=1&line=1", "VIN Input voltage 2.7 5.5 V"),
            fragment("a", "a.pdf#datasheet=operatingConditions/1", "VIN Input voltage 2.7 5.5 V"),
            fragment("c", "c.md#line=4", "voltage at the input"),
        ]);
        let all = corpus.search("input voltage", EvidenceScope::All, |_| true, 2);
        assert_eq!(all.total, 4);
        assert_eq!(all.documents, 2);
        assert_eq!(all.hits.len(), 2);
        assert!(all.hits[0].anchor.is_verified_data());

        let verified = corpus.search("input voltage", EvidenceScope::VerifiedData, |_| true, 10);
        assert_eq!(verified.total, 1);
        let full_text = corpus.search("input voltage", EvidenceScope::FullText, |_| true, 10);
        assert_eq!(full_text.total, 3);
        assert_eq!(full_text.hits.last().unwrap().fragment.document_id, "c");

        let admitted =
            corpus.search("input voltage", EvidenceScope::All, |f| f.document_id == "c", 10);
        assert_eq!((admitted.total, admitted.documents), (1, 1));
        assert_eq!(corpus.search("   ", EvidenceScope::All, |_| true, 10).total, 0);
    }
}
