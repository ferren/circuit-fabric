//! Typed, read-only semantic search projections; no GPUI dependency.
use circuitfabric_contracts::LogicalCircuitSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SemanticQueryScope {
    Components,
    Pins,
    Nets,
    Constraints,
    Evidence,
}
#[derive(Clone)]
pub(crate) struct SemanticQueryHit {
    pub(crate) kind: &'static str,
    pub(crate) subject: String,
    pub(crate) detail: String,
    pub(crate) evidence: String,
}
pub(crate) fn semantic_query_hits(
    snapshot: &LogicalCircuitSnapshot,
    scope: SemanticQueryScope,
    query: &str,
) -> Vec<SemanticQueryHit> {
    let query = query.trim().to_lowercase();
    let matches = |values: &[String]| {
        query.is_empty() || values.iter().any(|value| value.to_lowercase().contains(&query))
    };
    match scope {
        SemanticQueryScope::Components => snapshot
            .components
            .iter()
            .filter_map(|component| {
                let values = vec![
                    component.id.clone(),
                    component.reference.clone(),
                    component.value.clone().unwrap_or_default(),
                ];
                matches(&values).then(|| SemanticQueryHit {
                    kind: "component",
                    subject: format!("{} ({})", component.reference, component.id),
                    detail: component.value.clone().unwrap_or_else(|| "No value".to_owned()),
                    evidence: component.evidence.len().to_string(),
                })
            })
            .collect(),
        SemanticQueryScope::Pins => snapshot
            .components
            .iter()
            .flat_map(|component| {
                component.pins.iter().filter_map(move |pin| {
                    let values = vec![
                        component.reference.clone(),
                        component.id.clone(),
                        pin.id.clone(),
                        pin.name.clone(),
                    ];
                    matches(&values).then(|| SemanticQueryHit {
                        kind: "pin",
                        subject: format!("{}:{} ({})", component.reference, pin.id, pin.name),
                        detail: component.id.clone(),
                        evidence: component.evidence.len().to_string(),
                    })
                })
            })
            .collect(),
        SemanticQueryScope::Nets => snapshot
            .nets
            .iter()
            .filter_map(|net| {
                let values = vec![net.id.clone(), net.name.clone().unwrap_or_default()];
                matches(&values).then(|| SemanticQueryHit {
                    kind: "net",
                    subject: net.name.clone().unwrap_or_else(|| net.id.clone()),
                    detail: format!("{} pins · {}", net.pins.len(), net.id),
                    evidence: "snapshot".to_owned(),
                })
            })
            .collect(),
        SemanticQueryScope::Constraints => snapshot
            .constraints
            .iter()
            .filter_map(|constraint| {
                let values = vec![
                    constraint.constraint_id.clone(),
                    constraint.layer.clone(),
                    format!("{:?}", constraint.status),
                    constraint.explanation.clone(),
                ];
                matches(&values).then(|| SemanticQueryHit {
                    kind: "constraint",
                    subject: constraint.constraint_id.clone(),
                    detail: format!("{} · {:?}", constraint.layer, constraint.status),
                    evidence: constraint.evidence_refs.len().to_string(),
                })
            })
            .collect(),
        SemanticQueryScope::Evidence => snapshot
            .evidence
            .iter()
            .chain(snapshot.components.iter().flat_map(|component| component.evidence.iter()))
            .chain(
                snapshot.constraints.iter().flat_map(|constraint| constraint.evidence_refs.iter()),
            )
            .filter_map(|evidence| {
                let values = vec![
                    evidence.document_id.clone(),
                    evidence.content_hash.clone(),
                    evidence.locator.clone(),
                    evidence.excerpt.clone().unwrap_or_default(),
                ];
                matches(&values).then(|| SemanticQueryHit {
                    kind: "evidence",
                    subject: evidence.document_id.clone(),
                    detail: evidence.locator.clone(),
                    evidence: short_hash(&evidence.content_hash),
                })
            })
            .collect(),
    }
}
pub(crate) fn short_hash(value: &str) -> String {
    const PREFIX_LENGTH: usize = 18;
    if value.chars().count() > PREFIX_LENGTH {
        format!("{}…", value.chars().take(PREFIX_LENGTH).collect::<String>())
    } else if value.is_empty() {
        "—".to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuitfabric_contracts::{Component, EvidenceRef, Pin, SnapshotAuthority};

    #[test]
    fn typed_search_is_case_insensitive_and_does_not_mutate_facts() {
        let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        snapshot.components.push(Component {
            id: "chip".into(),
            reference: "U1".into(),
            value: Some("Amplifier".into()),
            pins: vec![Pin { id: "1".into(), name: "IN+".into() }],
            evidence: vec![EvidenceRef {
                document_id: "datasheet".into(),
                content_hash: "abc".into(),
                locator: "page=3".into(),
                excerpt: None,
            }],
        });
        let original = snapshot.clone();
        let hits = semantic_query_hits(&snapshot, SemanticQueryScope::Components, " amplIFIER ");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "component");
        assert!(semantic_query_hits(&snapshot, SemanticQueryScope::Nets, "u1").is_empty());
        assert_eq!(semantic_query_hits(&snapshot, SemanticQueryScope::Pins, "in+").len(), 1);
        let evidence = semantic_query_hits(&snapshot, SemanticQueryScope::Evidence, "datasheet");
        assert_eq!(evidence[0].detail, "page=3");
        assert_eq!(snapshot, original);
    }

    #[test]
    fn imported_non_ascii_hashes_do_not_panic_when_abbreviated() {
        assert_eq!(short_hash(""), "—");
        assert_eq!(short_hash("abc"), "abc");
        assert_eq!(short_hash(&"证".repeat(20)), format!("{}…", "证".repeat(18)));
    }
}
