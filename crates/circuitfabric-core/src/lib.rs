//! Deterministic operations over the logical circuit model.

use std::collections::BTreeSet;

use circuitfabric_contracts::{
    ConstraintResult, FactStatus, LogicalCircuitSnapshot, Severity, SnapshotHash,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("failed to serialize a logical circuit snapshot: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Serializes a snapshot deterministically because all maps in the contract are ordered maps.
///
/// # Errors
///
/// Returns [`CoreError::Serialization`] when the snapshot cannot be serialized.
pub fn canonical_snapshot_bytes(snapshot: &LogicalCircuitSnapshot) -> Result<Vec<u8>, CoreError> {
    Ok(serde_json::to_vec(snapshot)?)
}

/// Computes a content address while excluding the self-referential snapshot hash.
///
/// # Errors
///
/// Returns [`CoreError::Serialization`] when the snapshot cannot be serialized.
pub fn snapshot_content_hash(snapshot: &LogicalCircuitSnapshot) -> Result<SnapshotHash, CoreError> {
    let mut projection = snapshot.clone();
    projection.snapshot_hash.clear();
    Ok(digest(&canonical_snapshot_bytes(&projection)?))
}

/// Computes the logical hash without physical observation fields or content-address fields.
///
/// # Errors
///
/// Returns [`CoreError::Serialization`] when the snapshot cannot be serialized.
pub fn logical_content_hash(snapshot: &LogicalCircuitSnapshot) -> Result<String, CoreError> {
    let mut projection = snapshot.clone();
    projection.snapshot_hash.clear();
    projection.logical_hash.clear();
    projection.physical_hash = None;
    Ok(digest(&canonical_snapshot_bytes(&projection)?))
}

/// Returns a copy carrying deterministic logical and snapshot hashes.
///
/// # Errors
///
/// Returns [`CoreError::Serialization`] when the snapshot cannot be serialized.
pub fn stamp_snapshot(
    snapshot: &LogicalCircuitSnapshot,
) -> Result<LogicalCircuitSnapshot, CoreError> {
    let mut stamped = snapshot.clone();
    stamped.logical_hash = logical_content_hash(&stamped)?;
    stamped.snapshot_hash = snapshot_content_hash(&stamped)?;
    Ok(stamped)
}

/// Performs the first deterministic schema, identity, and topology checks.
#[must_use]
pub fn validate_snapshot(snapshot: &LogicalCircuitSnapshot) -> Vec<ConstraintResult> {
    let mut results = Vec::new();
    let mut component_ids = BTreeSet::new();
    let mut pin_ids = BTreeSet::new();

    for component in &snapshot.components {
        if !component_ids.insert(component.id.clone()) {
            results.push(failure(snapshot, "identity.unique_component_id", &component.id));
        }

        for pin in &component.pins {
            let pin_key = format!("{}:{}", component.id, pin.id);
            if !pin_ids.insert(pin_key.clone()) {
                results.push(failure(snapshot, "identity.unique_pin_id", &pin_key));
            }
        }
    }

    let mut net_ids = BTreeSet::new();
    for net in &snapshot.nets {
        if !net_ids.insert(net.id.clone()) {
            results.push(failure(snapshot, "identity.unique_net_id", &net.id));
        }

        for pin in &net.pins {
            let pin_key = format!("{}:{}", pin.component_id, pin.pin_id);
            if !pin_ids.contains(&pin_key) {
                results.push(failure(snapshot, "topology.pin_reference_exists", &pin_key));
            }
        }
    }

    if results.is_empty() {
        results.push(ConstraintResult {
            constraint_id: "schema.identity.topology".to_owned(),
            layer: "deterministic".to_owned(),
            subject_refs: Vec::new(),
            status: FactStatus::Passed,
            severity: Severity::Info,
            evidence_refs: Vec::new(),
            snapshot_hash: snapshot.snapshot_hash.clone(),
            explanation: "Schema, identity, and topology checks passed.".to_owned(),
            recommendation: None,
        });
    }

    results
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn failure(
    snapshot: &LogicalCircuitSnapshot,
    constraint_id: &str,
    subject_ref: &str,
) -> ConstraintResult {
    ConstraintResult {
        constraint_id: constraint_id.to_owned(),
        layer: "deterministic".to_owned(),
        subject_refs: vec![subject_ref.to_owned()],
        status: FactStatus::Failed,
        severity: Severity::Error,
        evidence_refs: Vec::new(),
        snapshot_hash: snapshot.snapshot_hash.clone(),
        explanation: format!(
            "The referenced object `{subject_ref}` is not valid in this snapshot."
        ),
        recommendation: Some("Correct the snapshot before planning materialization.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::{
        Component, LogicalCircuitSnapshot, Net, Pin, PinRef, SnapshotAuthority,
    };

    use super::*;

    fn fixture() -> LogicalCircuitSnapshot {
        let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        snapshot.components.push(Component {
            id: "component:1".to_owned(),
            reference: "R1".to_owned(),
            value: Some("10k".to_owned()),
            pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
            evidence: Vec::new(),
        });
        snapshot.nets.push(Net {
            id: "net:1".to_owned(),
            name: Some("SENSE".to_owned()),
            pins: vec![PinRef { component_id: "component:1".to_owned(), pin_id: "1".to_owned() }],
        });
        snapshot
    }

    #[test]
    fn stamping_is_deterministic() {
        let first = stamp_snapshot(&fixture()).expect("fixture serializes");
        let second = stamp_snapshot(&fixture()).expect("fixture serializes");

        assert_eq!(first.snapshot_hash, second.snapshot_hash);
        assert_eq!(first.logical_hash, second.logical_hash);
    }

    #[test]
    fn missing_pin_reference_fails_validation() {
        let mut snapshot = fixture();
        snapshot.nets[0].pins[0].pin_id = "missing".to_owned();

        let results = validate_snapshot(&snapshot);

        assert_eq!(results[0].status, FactStatus::Failed);
        assert_eq!(results[0].constraint_id, "topology.pin_reference_exists");
    }
}
