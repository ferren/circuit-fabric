//! Append-only observation timeline over content-addressed logical snapshots.
//!
//! `logic/snapshots/<hash>.json` answers "what the circuit is": one file per content
//! identity, with no lineage inside. `logic/timeline/<seq>.json` answers "when it became
//! so": an event is appended only when the observed identity differs from the current one.
//! A revert (A → B → A) is therefore three events that reference two snapshot files, and
//! the "returned to #1" fact is derived from the timeline instead of being stored twice.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use circuitfabric_contracts::{LogicalCircuitSnapshot, SnapshotHash};
use serde::{Deserialize, Serialize};

use crate::{ProjectStorage, ProjectStorageError};

/// Version of each JSON record in `logic/timeline`.
pub const SNAPSHOT_TIMELINE_SCHEMA_VERSION: u32 = 1;

const TIMELINE_DIRECTORY: &str = "logic/timeline";

/// What produced a timeline event.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSource {
    /// An explicit EDA inspect.
    Inspect,
    /// The readback that follows a materialization.
    Readback,
    /// Synthesized from snapshot files written before the timeline existed.
    Legacy,
}

/// One observed change of the project's current logical snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTimelineEvent {
    pub schema_version: u32,
    /// 1-based, gap-free order of observation; the only clock the timeline trusts.
    pub seq: u64,
    pub snapshot_hash: SnapshotHash,
    pub previous_snapshot_hash: Option<SnapshotHash>,
    pub observed_at_unix_millis: Option<u64>,
    pub source: TimelineSource,
    pub session_id: Option<String>,
    pub change_set_id: Option<String>,
}

/// Provenance of one observation handed to [`ProjectStorage::record_observed_snapshot`].
#[derive(Clone, Copy, Debug)]
pub struct TimelineObservation<'a> {
    pub source: TimelineSource,
    pub observed_at_unix_millis: u64,
    pub session_id: Option<&'a str>,
    pub change_set_id: Option<&'a str>,
}

impl ProjectStorage {
    /// Returns the observation timeline in order. Projects that predate the timeline get a
    /// read-only synthesized history: snapshot files in identifier order, consecutive
    /// duplicates collapsed.
    ///
    /// # Errors
    ///
    /// Returns an error when timeline or snapshot files cannot be read or parsed, or the
    /// persisted sequence is not gap-free and chained.
    pub fn list_snapshot_timeline(
        &self,
    ) -> Result<Vec<SnapshotTimelineEvent>, ProjectStorageError> {
        let events = self.read_timeline_events()?;
        if events.is_empty() { self.legacy_timeline() } else { Ok(events) }
    }

    /// Stores an observed snapshot by content and appends a timeline event when its identity
    /// differs from the current one. Returns the appended event, or `None` when the circuit
    /// did not change. A legacy history is persisted first so the new event extends it.
    ///
    /// # Errors
    ///
    /// Returns an error when the hash cannot address a file, the timeline is unreadable or
    /// invalid, or a snapshot or event cannot be written atomically.
    pub fn record_observed_snapshot(
        &self,
        snapshot: &LogicalCircuitSnapshot,
        observation: TimelineObservation<'_>,
    ) -> Result<Option<SnapshotTimelineEvent>, ProjectStorageError> {
        let snapshot_path = self.snapshot_path(&snapshot.snapshot_hash)?;
        let mut events = self.read_timeline_events()?;
        if events.is_empty() {
            events = self.legacy_timeline()?;
            for event in &events {
                self.write_timeline_event(event)?;
            }
        }
        if !snapshot_path.exists() {
            let mut stored = snapshot.clone();
            stored.parent_snapshot_hash = None;
            Self::write_json_atomically(&snapshot_path, &stored)?;
        }
        let previous = events.last();
        if previous.is_some_and(|event| event.snapshot_hash == snapshot.snapshot_hash) {
            return Ok(None);
        }
        let event = SnapshotTimelineEvent {
            schema_version: SNAPSHOT_TIMELINE_SCHEMA_VERSION,
            seq: previous.map_or(1, |event| event.seq + 1),
            snapshot_hash: snapshot.snapshot_hash.clone(),
            previous_snapshot_hash: previous.map(|event| event.snapshot_hash.clone()),
            observed_at_unix_millis: Some(observation.observed_at_unix_millis),
            source: observation.source,
            session_id: observation.session_id.map(str::to_owned),
            change_set_id: observation.change_set_id.map(str::to_owned),
        };
        self.write_timeline_event(&event)?;
        Ok(Some(event))
    }

    fn snapshot_path(&self, hash: &str) -> Result<PathBuf, ProjectStorageError> {
        let name = hash.strip_prefix("sha256:").unwrap_or(hash);
        if name.is_empty()
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(ProjectStorageError::InvalidSnapshotHash { hash: hash.to_owned() });
        }
        self.resolve_relative_path(Path::new("logic/snapshots").join(format!("{name}.json")))
    }

    fn write_timeline_event(
        &self,
        event: &SnapshotTimelineEvent,
    ) -> Result<(), ProjectStorageError> {
        let directory = self.resolve_relative_path(TIMELINE_DIRECTORY)?;
        fs::create_dir_all(&directory).map_err(|source| ProjectStorageError::Io {
            action: "create snapshot timeline directory",
            path: directory.clone(),
            source,
        })?;
        let path = directory.join(format!("{:010}.json", event.seq));
        if path.exists() {
            return Err(ProjectStorageError::TimelineEventExists { path });
        }
        Self::write_json_atomically(&path, event)
    }

    fn read_timeline_events(&self) -> Result<Vec<SnapshotTimelineEvent>, ProjectStorageError> {
        let directory = self.resolve_relative_path(TIMELINE_DIRECTORY)?;
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(ProjectStorageError::Io {
                    action: "list snapshot timeline",
                    path: directory,
                    source,
                });
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|source| ProjectStorageError::Io {
                    action: "read snapshot timeline entry",
                    path: directory.clone(),
                    source,
                })?
                .path();
            if path.extension().is_some_and(|extension| extension == "json") {
                paths.push(path);
            }
        }
        paths.sort();
        let mut events: Vec<SnapshotTimelineEvent> = Vec::with_capacity(paths.len());
        for path in paths {
            let raw = fs::read_to_string(&path).map_err(|source| {
                ProjectStorageError::ReadTimelineEvent { path: path.clone(), source }
            })?;
            let event: SnapshotTimelineEvent = serde_json::from_str(&raw).map_err(|source| {
                ProjectStorageError::ParseTimelineEvent { path: path.clone(), source }
            })?;
            let previous = events.last();
            let reason = if event.schema_version != SNAPSHOT_TIMELINE_SCHEMA_VERSION {
                Some("unsupported schema version")
            } else if event.seq != previous.map_or(1, |e| e.seq + 1) {
                Some("sequence is not gap-free")
            } else if event.previous_snapshot_hash.as_ref() != previous.map(|e| &e.snapshot_hash) {
                Some("previous snapshot does not match the preceding event")
            } else if previous.is_some_and(|e| e.snapshot_hash == event.snapshot_hash) {
                Some("event repeats the current snapshot")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(ProjectStorageError::InvalidTimeline { path, reason });
            }
            events.push(event);
        }
        Ok(events)
    }

    fn legacy_timeline(&self) -> Result<Vec<SnapshotTimelineEvent>, ProjectStorageError> {
        let mut events: Vec<SnapshotTimelineEvent> = Vec::new();
        for (path, snapshot) in self.read_logical_snapshot_files()? {
            let previous = events.last();
            if previous.is_some_and(|event| event.snapshot_hash == snapshot.snapshot_hash) {
                continue;
            }
            events.push(SnapshotTimelineEvent {
                schema_version: SNAPSHOT_TIMELINE_SCHEMA_VERSION,
                seq: previous.map_or(1, |event| event.seq + 1),
                previous_snapshot_hash: previous.map(|event| event.snapshot_hash.clone()),
                snapshot_hash: snapshot.snapshot_hash,
                observed_at_unix_millis: legacy_observed_at(&path),
                source: TimelineSource::Legacy,
                session_id: None,
                change_set_id: None,
            });
        }
        Ok(events)
    }
}

/// `observed-<millis>-<hash>.json` is the only legacy name that carries a clock.
fn legacy_observed_at(path: &Path) -> Option<u64> {
    let name = path.file_stem()?.to_str()?;
    name.strip_prefix("observed-")?.split('-').next()?.parse().ok()
}

/// For each event, the `seq` of the earliest earlier event that observed the same snapshot:
/// a non-`None` entry means the circuit returned to that earlier state.
#[must_use]
pub fn timeline_revisits(events: &[SnapshotTimelineEvent]) -> Vec<Option<u64>> {
    let mut first_seen = BTreeMap::new();
    events
        .iter()
        .map(|event| {
            let first = *first_seen.entry(&event.snapshot_hash).or_insert(event.seq);
            (first != event.seq).then_some(first)
        })
        .collect()
}

/// Identity-keyed component and net differences between two snapshots.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SnapshotChangeSummary {
    pub components_added: usize,
    pub components_removed: usize,
    pub components_changed: usize,
    pub nets_added: usize,
    pub nets_removed: usize,
    pub nets_changed: usize,
}

impl SnapshotChangeSummary {
    #[must_use]
    pub fn between(before: &LogicalCircuitSnapshot, after: &LogicalCircuitSnapshot) -> Self {
        let (components_added, components_removed, components_changed) = diff_by_id(
            before.components.iter().map(|c| (&c.id, c)),
            after.components.iter().map(|c| (&c.id, c)),
        );
        let (nets_added, nets_removed, nets_changed) = diff_by_id(
            before.nets.iter().map(|n| (&n.id, n)),
            after.nets.iter().map(|n| (&n.id, n)),
        );
        Self {
            components_added,
            components_removed,
            components_changed,
            nets_added,
            nets_removed,
            nets_changed,
        }
    }

    /// No component or net differs; other fields (constraints, semantics) may still.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.components_added == 0
            && self.components_removed == 0
            && self.components_changed == 0
            && self.nets_added == 0
            && self.nets_removed == 0
            && self.nets_changed == 0
    }
}

fn diff_by_id<'a, T: PartialEq + 'a>(
    before: impl Iterator<Item = (&'a String, &'a T)>,
    after: impl Iterator<Item = (&'a String, &'a T)>,
) -> (usize, usize, usize) {
    let before: BTreeMap<_, _> = before.collect();
    let after: BTreeMap<_, _> = after.collect();
    let added = after.keys().filter(|id| !before.contains_key(*id)).count();
    let removed = before.keys().filter(|id| !after.contains_key(*id)).count();
    let changed =
        after.iter().filter(|(id, item)| before.get(*id).is_some_and(|old| old != *item)).count();
    (added, removed, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuitfabric_contracts::{Component, Project, SnapshotAuthority};

    fn storage(label: &str) -> (PathBuf, ProjectStorage) {
        let root = std::env::temp_dir()
            .join(format!("circuitfabric-timeline-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let project = Project { id: label.into(), name: label.into(), description: None };
        let storage = ProjectStorage::create(&root, project).unwrap();
        (root, storage)
    }

    fn snapshot(hash: &str, components: &[&str]) -> LogicalCircuitSnapshot {
        let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
        snapshot.snapshot_hash = format!("sha256:{hash}");
        snapshot.parent_snapshot_hash = Some("sha256:ignored".into());
        snapshot.components = components
            .iter()
            .map(|id| Component {
                id: (*id).into(),
                reference: (*id).into(),
                value: None,
                pins: vec![],
                evidence: vec![],
            })
            .collect();
        snapshot
    }

    fn at(millis: u64) -> TimelineObservation<'static> {
        TimelineObservation {
            source: TimelineSource::Inspect,
            observed_at_unix_millis: millis,
            session_id: Some("s"),
            change_set_id: None,
        }
    }

    #[test]
    fn unchanged_is_not_an_event_and_revert_is_one_that_reuses_the_snapshot() {
        let (root, storage) = storage("revert");
        let a = snapshot("aa", &["R1"]);
        let b = snapshot("bb", &["R1", "C1"]);
        assert_eq!(storage.record_observed_snapshot(&a, at(1)).unwrap().unwrap().seq, 1);
        assert!(storage.record_observed_snapshot(&a, at(2)).unwrap().is_none());
        assert_eq!(storage.record_observed_snapshot(&b, at(3)).unwrap().unwrap().seq, 2);
        let back = storage.record_observed_snapshot(&a, at(4)).unwrap().unwrap();
        assert_eq!((back.seq, back.previous_snapshot_hash.as_deref()), (3, Some("sha256:bb")));

        let timeline = storage.list_snapshot_timeline().unwrap();
        assert_eq!(timeline.len(), 3);
        assert_eq!(timeline_revisits(&timeline), [None, None, Some(1)]);
        let snapshots = storage.list_logical_snapshots().unwrap();
        assert_eq!(snapshots.len(), 2, "one file per content identity");
        assert!(snapshots.iter().all(|s| s.parent_snapshot_hash.is_none()));
        let summary = SnapshotChangeSummary::between(&a, &b);
        assert_eq!((summary.components_added, summary.is_empty()), (1, false));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn legacy_files_become_the_persisted_prefix_of_the_timeline() {
        let (root, storage) = storage("legacy");
        for (name, hash) in [
            ("observed-00000000000000000010-aa", "aa"),
            ("observed-00000000000000000020-aa", "aa"),
            ("observed-00000000000000000030-bb", "bb"),
        ] {
            fs::write(
                root.join(format!("logic/snapshots/{name}.json")),
                serde_json::to_vec(&snapshot(hash, &[])).unwrap(),
            )
            .unwrap();
        }
        let synthesized = storage.list_snapshot_timeline().unwrap();
        assert_eq!(synthesized.len(), 2);
        assert_eq!(synthesized[0].observed_at_unix_millis, Some(10));
        assert!(!root.join(TIMELINE_DIRECTORY).exists(), "reading never writes");

        let event = storage.record_observed_snapshot(&snapshot("aa", &[]), at(40)).unwrap();
        assert_eq!(event.unwrap().seq, 3);
        let timeline = storage.list_snapshot_timeline().unwrap();
        assert_eq!(timeline[..2], synthesized[..]);
        assert_eq!(timeline_revisits(&timeline), [None, None, Some(1)]);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unsafe_hashes_and_broken_chains_are_rejected() {
        let (root, storage) = storage("invalid");
        let mut bad = snapshot("aa", &[]);
        bad.snapshot_hash = "sha256:../escape".into();
        assert!(matches!(
            storage.record_observed_snapshot(&bad, at(1)),
            Err(ProjectStorageError::InvalidSnapshotHash { .. })
        ));
        storage.record_observed_snapshot(&snapshot("aa", &[]), at(1)).unwrap();
        storage.record_observed_snapshot(&snapshot("bb", &[]), at(2)).unwrap();
        fs::remove_file(root.join("logic/timeline/0000000001.json")).unwrap();
        assert!(matches!(
            storage.list_snapshot_timeline(),
            Err(ProjectStorageError::InvalidTimeline { .. })
        ));
        fs::remove_dir_all(root).ok();
    }
}
