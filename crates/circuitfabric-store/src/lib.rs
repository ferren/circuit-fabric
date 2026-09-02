//! Persistence boundary for immutable logical circuit snapshots.

use std::collections::BTreeMap;

use circuitfabric_contracts::{LogicalCircuitSnapshot, SnapshotHash};
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum StoreError {
    #[error("a snapshot must have a content hash before it can be persisted")]
    MissingHash,
    #[error("snapshot `{0}` already exists and immutable snapshots cannot be overwritten")]
    AlreadyExists(SnapshotHash),
}

pub trait SnapshotStore {
    /// Persists a new immutable snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::MissingHash`] when the snapshot is not content-addressed, or
    /// [`StoreError::AlreadyExists`] when the immutable hash is already present.
    fn put(&mut self, snapshot: LogicalCircuitSnapshot) -> Result<SnapshotHash, StoreError>;
    fn get(&self, hash: &str) -> Option<&LogicalCircuitSnapshot>;
}

#[derive(Default)]
pub struct InMemorySnapshotStore {
    snapshots: BTreeMap<SnapshotHash, LogicalCircuitSnapshot>,
}

impl SnapshotStore for InMemorySnapshotStore {
    fn put(&mut self, snapshot: LogicalCircuitSnapshot) -> Result<SnapshotHash, StoreError> {
        if snapshot.snapshot_hash.is_empty() {
            return Err(StoreError::MissingHash);
        }

        let hash = snapshot.snapshot_hash.clone();
        if self.snapshots.contains_key(&hash) {
            return Err(StoreError::AlreadyExists(hash));
        }

        self.snapshots.insert(hash.clone(), snapshot);
        Ok(hash)
    }

    fn get(&self, hash: &str) -> Option<&LogicalCircuitSnapshot> {
        self.snapshots.get(hash)
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_contracts::{LogicalCircuitSnapshot, SnapshotAuthority};

    use super::*;

    #[test]
    fn snapshots_cannot_be_overwritten() {
        let mut store = InMemorySnapshotStore::default();
        let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        snapshot.snapshot_hash = "sha256:fixture".to_owned();

        assert!(store.put(snapshot.clone()).is_ok());
        assert_eq!(
            store.put(snapshot),
            Err(StoreError::AlreadyExists("sha256:fixture".to_owned()))
        );
    }
}
