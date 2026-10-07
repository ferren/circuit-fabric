use super::project_data::*;
use circuitfabric_contracts::{LogicalCircuitSnapshot, Project, SnapshotAuthority};
use circuitfabric_project::{DocumentCategory, ProjectStorage};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cf-read-model-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn storage(&self, id: &str) -> ProjectStorage {
        fs::create_dir_all(self.0.join(id)).unwrap();
        ProjectStorage::create(
            self.0.join(id),
            Project { id: id.into(), name: id.into(), description: None },
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn late_load_failure_preserves_complete_previous_projection() {
    let fixture = Fixture::new();
    let storage = fixture.storage("first");
    let snapshot_path = storage.root().join("logic/snapshots/current.json");
    fs::write(
        &snapshot_path,
        serde_json::to_vec(&LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed)).unwrap(),
    )
    .unwrap();
    let mut cache = BTreeMap::new();
    let stores = BTreeMap::from([("first".to_owned(), storage)]);
    refresh_project_data(&stores, &mut cache, "first").unwrap();
    fs::write(
        snapshot_path,
        serde_json::to_vec(&LogicalCircuitSnapshot::empty(SnapshotAuthority::Verified)).unwrap(),
    )
    .unwrap();
    let bad_changeset = stores["first"].root().join("logic/changesets/broken.json");
    fs::write(&bad_changeset, "invalid json").unwrap();

    assert!(refresh_project_data(&stores, &mut cache, "first").is_err());
    assert_eq!(cache["first"].semantic_snapshots[0].authority, SnapshotAuthority::Observed);
    fs::remove_file(bad_changeset).unwrap();
    refresh_project_data(&stores, &mut cache, "first").unwrap();
    assert_eq!(cache["first"].semantic_snapshots[0].authority, SnapshotAuthority::Verified);
}

#[test]
fn refresh_is_project_scoped_and_never_invents_missing_projects() {
    let fixture = Fixture::new();
    let stores = BTreeMap::from([
        ("first".to_owned(), fixture.storage("first")),
        ("second".to_owned(), fixture.storage("second")),
    ]);
    let source = fixture.0.join("evidence.md");
    fs::write(&source, "# Hardware evidence").unwrap();
    stores["first"].import_document(&source, DocumentCategory::Datasheet, "fixture").unwrap();
    let mut cache = BTreeMap::new();
    refresh_project_data(&stores, &mut cache, "first").unwrap();
    refresh_project_data(&stores, &mut cache, "second").unwrap();
    assert_eq!(cache["first"].documents.len(), 1);
    assert!(cache["first"].document_integrity.values().all(|verified| *verified));
    assert!(cache["second"].documents.is_empty());
    assert!(refresh_project_data(&stores, &mut cache, "missing").is_err());
    assert_eq!(cache.len(), 2);
}

#[test]
fn persisted_usage_and_all_audit_sources_survive_reopen_filter_pagination_and_export() {
    use super::usage_audit::{AuditKindFilter, UsageAuditModel, UsagePeriod, audit_page_range};
    use circuitfabric_project::{
        ChangeSetAuditEntry, ChangeSetExecutionReport, ChangeSetStage, ChangeSetStageStatus,
        SessionEvent, SessionEventKind, SessionSeed, SessionStatus, SessionUsage, StoredChangeSet,
    };
    let fixture = Fixture::new();
    let storage = fixture.storage("p");
    for (id, counters) in [
        ("reported", Some(SessionUsage { input_tokens: 120, output_tokens: 30 })),
        ("zero", Some(SessionUsage::default())),
        ("unavailable", None),
    ] {
        let metadata = storage
            .start_session(SessionSeed {
                session_id: id.into(),
                runtime_profile_id: "provider".into(),
                backend_id: Some("codex".into()),
            })
            .unwrap();
        storage
            .append_session_event(
                id,
                &SessionEvent {
                    timestamp_unix_seconds: metadata.started_at_unix_seconds,
                    kind: counters.map_or_else(
                        || SessionEventKind::Note { text: "Token usage unavailable".into() },
                        |usage| SessionEventKind::UsageRecorded { usage },
                    ),
                },
            )
            .unwrap();
        storage
            .complete_session(
                id,
                counters.unwrap_or_default(),
                Vec::new(),
                SessionStatus::Completed,
            )
            .unwrap();
    }
    let metadata = storage.load_session("reported").unwrap().metadata;
    for index in 0..205 {
        storage
            .append_session_event(
                "reported",
                &SessionEvent {
                    timestamp_unix_seconds: metadata.started_at_unix_seconds,
                    kind: SessionEventKind::Approval { decision: format!("approval-{index}") },
                },
            )
            .unwrap();
    }
    let cs = StoredChangeSet {
        schema_version: 1,
        id: "cs".into(),
        base_snapshot_hash: "base".into(),
        target_snapshot_hash: "target".into(),
        plan_hash: "plan".into(),
        ir_diff: Vec::new(),
        evidence: Vec::new(),
        observed_snapshot_hash: Some("base".into()),
        rollback_handle: Some("handle".into()),
        execution: ChangeSetExecutionReport {
            write: ChangeSetStage {
                status: ChangeSetStageStatus::Passed,
                detail: "write passed".into(),
            },
            verification: ChangeSetStage {
                status: ChangeSetStageStatus::Passed,
                detail: "verified".into(),
            },
            ..Default::default()
        },
        audit: vec![ChangeSetAuditEntry {
            timestamp_unix_seconds: metadata.started_at_unix_seconds,
            actor: "backend".into(),
            decision: "rollback_executed".into(),
            reason: "restored base".into(),
            observed_snapshot_hash: Some("base".into()),
            rollback_handle: Some("handle".into()),
        }],
    };
    storage.save_change_set(&cs).unwrap();
    let reopened = ProjectStorage::open(storage.root()).unwrap();
    let data = ProjectWorkspaceData::load(&reopened).unwrap();
    let session_paths: Vec<_> = data
        .session_listing
        .sessions
        .iter()
        .map(|row| reopened.root().join("sessions").join(&row.file_name))
        .collect();
    let mut paths = session_paths;
    paths.push(reopened.root().join("logic/changesets/cs.json"));
    let before: Vec<_> = paths.iter().map(|path| fs::read(path).unwrap()).collect();
    let model = build_usage_audit_model(
        &BTreeMap::from([("p".into(), data)]),
        &BTreeMap::from([("p".into(), reopened)]),
    );
    assert!(model.diagnostics.is_empty());
    assert!(model.usage.iter().find(|row| row.session_id == "reported").unwrap().usage_reported);
    assert!(model.usage.iter().find(|row| row.session_id == "zero").unwrap().usage_reported);
    assert!(
        !model.usage.iter().find(|row| row.session_id == "unavailable").unwrap().usage_reported
    );
    let now = UsageAuditModel::now_unix_seconds();
    let usage = model.filtered_usage(UsagePeriod::Last7Days, "p", now);
    assert_eq!(usage.iter().map(|row| row.total_tokens()).sum::<u64>(), 150);
    let csv = UsageAuditModel::usage_csv(&usage);
    assert_eq!(csv.lines().count(), 4);
    assert!(csv.contains("#front-matter:usage"));
    let approvals =
        model.filtered_audit(UsagePeriod::All, AuditKindFilter::Approvals, "reported", now);
    assert_eq!(approvals.len(), 205);
    assert_eq!(audit_page_range(approvals.len(), 4), 200..205);
    let csv = UsageAuditModel::audit_csv(&approvals);
    assert_eq!(csv.lines().count(), 206);
    assert!(csv.contains("approval-204"));
    assert!(csv.contains("sessions/"));
    assert!(csv.contains("#body-line:"));
    let rollbacks =
        model.filtered_audit(UsagePeriod::Last7Days, AuditKindFilter::Rollbacks, "cs", now);
    assert_eq!(rollbacks.len(), 1);
    assert!(rollbacks[0].source.locator.ends_with("#/audit/0"));
    assert_eq!(
        model.filtered_audit(UsagePeriod::All, AuditKindFilter::Materializations, "cs", now).len(),
        1
    );
    assert_eq!(
        model.filtered_audit(UsagePeriod::All, AuditKindFilter::Verifications, "cs", now).len(),
        1
    );
    assert_eq!(
        before,
        paths.iter().map(|path| fs::read(path).unwrap()).collect::<Vec<_>>(),
        "read-only projection and export must not rewrite source files"
    );
}

#[test]
fn unreadable_replays_keep_usage_and_report_incomplete_audit() {
    use circuitfabric_project::SessionSeed;
    let fixture = Fixture::new();
    let storage = fixture.storage("missing");
    storage
        .start_session(SessionSeed {
            session_id: "s".into(),
            runtime_profile_id: "provider".into(),
            backend_id: None,
        })
        .unwrap();
    let data = ProjectWorkspaceData::load(&storage).unwrap();
    let file = storage.root().join("sessions").join(&data.session_listing.sessions[0].file_name);
    fs::remove_file(file).unwrap();
    let model = build_usage_audit_model(
        &BTreeMap::from([("missing".into(), data)]),
        &BTreeMap::from([("missing".into(), storage)]),
    );
    assert_eq!(model.usage.len(), 1);
    assert!(model.audit.is_empty());
    assert_eq!(model.diagnostics.len(), 1);
    assert!(model.diagnostics[0].contains("会话 s"));
}
