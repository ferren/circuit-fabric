use super::*;
use circuitfabric_contracts::{ConstraintResult, Project, Severity, SnapshotAuthority};
use circuitfabric_project::{
    ChangeSetAuditEntry, ChangeSetExecutionReport, DocumentCategory, ProjectStorage, SessionSeed,
    SessionUsage, StoredChangeSet,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cf-overview-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn storage(&self, id: &str) -> ProjectStorage {
        let root = self.0.join(id);
        fs::create_dir_all(&root).unwrap();
        ProjectStorage::create(
            root,
            Project { id: id.into(), name: format!("项目 {id}"), description: None },
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(hash: &str, parent: Option<&str>) -> LogicalCircuitSnapshot {
    let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
    snapshot.snapshot_hash = hash.into();
    snapshot.parent_snapshot_hash = parent.map(str::to_owned);
    for (index, status) in
        [FactStatus::Passed, FactStatus::Failed, FactStatus::Inconclusive, FactStatus::NotRun]
            .into_iter()
            .enumerate()
    {
        snapshot.constraints.push(ConstraintResult {
            constraint_id: format!("rule-{index}"),
            layer: "logic".into(),
            subject_refs: vec![],
            status,
            severity: Severity::Warning,
            evidence_refs: vec![],
            snapshot_hash: hash.into(),
            explanation: "真实来源中的验证说明".into(),
            recommendation: None,
        });
    }
    snapshot
}
fn change(id: &str, decision: Option<&str>) -> StoredChangeSet {
    StoredChangeSet {
        schema_version: 1,
        id: id.into(),
        base_snapshot_hash: "old".into(),
        target_snapshot_hash: "tip".into(),
        plan_hash: "plan".into(),
        ir_diff: vec![],
        evidence: vec![],
        execution: ChangeSetExecutionReport::default(),
        observed_snapshot_hash: None,
        rollback_handle: None,
        audit: decision
            .into_iter()
            .map(|decision| ChangeSetAuditEntry {
                timestamp_unix_seconds: 1,
                actor: "fixture".into(),
                decision: decision.into(),
                reason: "review".into(),
                observed_snapshot_hash: None,
                rollback_handle: None,
            })
            .collect(),
    }
}
fn projects(ids: &[&str]) -> BTreeMap<String, String> {
    ids.iter().map(|id| (id.to_string(), format!("项目 {id}"))).collect()
}

#[test]
fn persisted_multi_project_kpis_charts_todos_and_reopen_share_sources_without_writes() {
    let fixture = Fixture::new();
    let a = fixture.storage("a");
    let b = fixture.storage("b");
    let source = fixture.0.join("evidence.md");
    fs::write(&source, "datasheet").unwrap();
    a.import_document(&source, DocumentCategory::Datasheet, "fixture").unwrap();
    for (id, decision) in [
        ("pending", None),
        ("approved", Some("approved")),
        ("rejected", Some("rejected")),
        ("cancelled", Some("cancelled")),
        ("awaiting", Some("awaiting_approval")),
    ] {
        a.save_change_set(&change(id, decision)).unwrap();
    }
    for (file, data) in [
        ("old", snapshot("old", None)),
        ("tip", snapshot("tip", Some("old"))),
        ("duplicate", snapshot("old", None)),
    ] {
        fs::write(
            a.root().join(format!("logic/snapshots/{file}.json")),
            serde_json::to_vec(&data).unwrap(),
        )
        .unwrap();
    }
    for (store, id, status) in
        [(&a, "same", SessionStatus::Completed), (&b, "same", SessionStatus::Failed)]
    {
        store
            .start_session(SessionSeed {
                session_id: id.into(),
                runtime_profile_id: "p".into(),
                backend_id: Some("codex".into()),
            })
            .unwrap();
        store.complete_session(id, SessionUsage::default(), vec![], status).unwrap();
    }
    let read = |store: &ProjectStorage| {
        ProjectWorkspaceData::load(&ProjectStorage::open(store.root()).unwrap()).unwrap()
    };
    let data = BTreeMap::from([("a".into(), read(&a)), ("b".into(), read(&b))]);
    let before = fs::read(a.root().join("logic/changesets/pending.json")).unwrap();
    let model = OverviewModel::build(&projects(&["a", "b"]), &data, &BTreeMap::new(), u64::MAX);
    assert!(model.complete());
    assert_eq!(model.documents, 1);
    assert_eq!(model.pending, 2);
    assert_eq!(
        model.facts,
        [1, 1, 1, 1],
        "historical/duplicate snapshots never duplicate current facts"
    );
    assert_eq!(model.resources[0].counts, Some([1, 1, 3]));
    assert_eq!(model.resources[1].counts, Some([0, 1, 0]));
    assert_eq!(
        model.sessions.iter().map(|s| (&*s.project_id, &*s.id)).collect::<Vec<_>>(),
        [("a", "same"), ("b", "same")]
    );
    assert_eq!(model.todos.len(), 5);
    assert!(model.todos.iter().any(|todo| matches!(&todo.target, TodoTarget::Constraint { snapshot_hash, .. } if snapshot_hash == "tip")));
    assert_eq!(before, fs::read(a.root().join("logic/changesets/pending.json")).unwrap());
}

#[test]
fn empty_zero_unloaded_partial_stale_and_removed_projects_are_distinct() {
    let empty =
        OverviewModel::build(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 10 * 86_400);
    assert!(empty.complete());
    assert_eq!(empty.daily_sessions.len(), 7);
    assert!(empty.daily_sessions.iter().all(|(_, count)| *count == 0));
    let mut data = BTreeMap::from([("a".into(), ProjectWorkspaceData::default())]);
    let model = OverviewModel::build(&projects(&["a", "b"]), &data, &BTreeMap::new(), 10 * 86_400);
    assert!(!model.complete());
    assert_eq!(model.resources[0].counts, Some([0; 3]));
    assert_eq!(model.resources[1].counts, None);
    let errors = BTreeMap::from([("a".into(), "corrupt source".into())]);
    let stale = OverviewModel::build(&projects(&["a", "b"]), &data, &errors, 10 * 86_400);
    assert_eq!(stale.loaded_projects, 0);
    assert_eq!(stale.resources[0].counts, None);
    data.insert("b".into(), ProjectWorkspaceData::default());
    let removed = OverviewModel::build(&projects(&["b"]), &data, &BTreeMap::new(), 10 * 86_400);
    assert_eq!(removed.resources.len(), 1);
    assert_eq!(removed.resources[0].id, "b");
    assert!(removed.complete());
}

#[test]
fn snapshot_branch_cycle_conflict_and_input_order_do_not_invent_current_facts() {
    let old = snapshot("z-old", None);
    let tip = snapshot("a-tip", Some("z-old"));
    for history in [vec![tip.clone(), old.clone()], vec![old.clone(), tip.clone(), old.clone()]] {
        assert_eq!(current_snapshot(&history).unwrap().unwrap().snapshot_hash, "a-tip");
    }
    assert!(current_snapshot(&[old.clone(), snapshot("branch", None)]).is_err());
    assert!(
        current_snapshot(&[snapshot("cycle1", Some("cycle2")), snapshot("cycle2", Some("cycle1"))])
            .is_err()
    );
    let mut conflict = old.clone();
    conflict.constraints.clear();
    assert!(current_snapshot(&[old, conflict]).is_err());
}

#[test]
fn true_daily_time_spacing_excludes_zero_future_and_out_of_range_times() {
    use circuitfabric_project::{SessionMetadata, SessionSummary};
    let now = 100 * 86_400 + 200;
    let mut data = ProjectWorkspaceData::default();
    for (index, timestamp) in
        [now, now - 2 * 86_400, now - 7 * 86_400, 0, now + 1].into_iter().enumerate()
    {
        data.session_listing.sessions.push(SessionSummary {
            file_name: format!("{index}.md"),
            byte_size: 1,
            metadata: SessionMetadata {
                session_id: index.to_string(),
                project_id: "p".into(),
                runtime_profile_id: "codex".into(),
                backend_id: None,
                started_at_unix_seconds: timestamp,
                completed_at_unix_seconds: None,
                status: SessionStatus::Running,
                usage: SessionUsage::default(),
                citations: vec![],
            },
        });
    }
    let model = OverviewModel::build(
        &projects(&["p"]),
        &BTreeMap::from([("p".into(), data)]),
        &BTreeMap::new(),
        now,
    );
    assert_eq!(
        model.daily_sessions.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
        [0, 0, 0, 0, 1, 0, 1]
    );
    assert_eq!(model.unreliable_session_times, 2);
    assert_eq!(model.sessions.len(), 5);
}

#[test]
fn cache_rebuilds_on_refresh_error_recovery_day_and_project_removal() {
    let mut cache = OverviewCache::default();
    let projects = projects(&["p"]);
    let mut data = BTreeMap::from([("p".into(), ProjectWorkspaceData::default())]);
    assert!(cache.get(&projects, &data, &BTreeMap::new(), 1_000_000).complete());
    let errors = BTreeMap::from([("p".into(), "failed".into())]);
    assert!(!cache.get(&projects, &data, &errors, 1_000_000).complete());
    data.get_mut("p").unwrap().change_sets.push(change("new", None));
    data.get_mut("p").unwrap().loaded_at_unix_nanos = 1;
    assert_eq!(cache.get(&projects, &data, &BTreeMap::new(), 1_000_000).pending, 1);
    assert!(cache.get(&BTreeMap::new(), &data, &BTreeMap::new(), 2_000_000).resources.is_empty());
}

#[test]
fn corrupt_source_recovery_restarts_and_authorization_change_are_project_scoped() {
    use super::super::project_data::refresh_project_data;
    let fixture = Fixture::new();
    let store = fixture.storage("p");
    let source = fixture.0.join("doc.md");
    fs::write(&source, "doc").unwrap();
    store.import_document(&source, DocumentCategory::Datasheet, "fixture").unwrap();
    let stores = BTreeMap::from([("p".into(), store.clone())]);
    let mut data = BTreeMap::new();
    refresh_project_data(&stores, &mut data, "p").unwrap();
    let doc_path = store.document_index_path();
    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&doc_path).unwrap()).unwrap();
    index["documents"][0]["authorized"] = false.into();
    fs::write(&doc_path, serde_json::to_vec(&index).unwrap()).unwrap();
    refresh_project_data(&stores, &mut data, "p").unwrap();
    assert_eq!(OverviewModel::build(&projects(&["p"]), &data, &BTreeMap::new(), 1).documents, 0);
    let bad = store.root().join("logic/changesets/broken.json");
    fs::write(&bad, "broken").unwrap();
    let error = refresh_project_data(&stores, &mut data, "p").unwrap_err();
    let failed =
        OverviewModel::build(&projects(&["p"]), &data, &BTreeMap::from([("p".into(), error)]), 1);
    assert_eq!(failed.resources[0].counts, None);
    assert!(failed.resources[0].loaded_at > 0);
    fs::remove_file(bad).unwrap();
    store.save_change_set(&change("recover", None)).unwrap();
    refresh_project_data(&stores, &mut data, "p").unwrap();
    assert_eq!(OverviewModel::build(&projects(&["p"]), &data, &BTreeMap::new(), 1).pending, 1);
}

#[test]
fn many_projects_remain_identifiable_and_all_zero() {
    let projects: BTreeMap<_, _> =
        (0..500).map(|i| (format!("p{i:03}"), "同名项目".into())).collect();
    let data = projects.keys().map(|id| (id.clone(), ProjectWorkspaceData::default())).collect();
    let model = OverviewModel::build(&projects, &data, &BTreeMap::new(), 1_000_000);
    assert_eq!(model.resources.len(), 500);
    assert!(model.complete());
    assert!(model.resources.iter().all(|row| row.counts == Some([0; 3])));
}

#[test]
fn unknown_approval_and_corrupt_fact_identity_never_claim_pending_or_success() {
    let mut data = ProjectWorkspaceData::default();
    data.change_sets.push(change("unknown", Some("unrecognized")));
    let mut bad = snapshot("tip", None);
    bad.constraints[0].snapshot_hash = "another".into();
    data.semantic_snapshots.push(bad);
    let model = OverviewModel::build(
        &projects(&["p"]),
        &BTreeMap::from([("p".into(), data)]),
        &BTreeMap::new(),
        100,
    );
    assert_eq!(model.pending, 0);
    assert_eq!(model.unknown_approval_states, 1);
    assert_eq!(model.facts, [0; 4]);
    assert_eq!(model.ambiguous_projects, 1);
    assert_eq!(model.fact_projects, 0);
    assert!(!model.diagnostics.is_empty());
}

#[test]
fn duplicate_persisted_session_identity_is_a_failed_source() {
    let fixture = Fixture::new();
    let store = fixture.storage("p");
    store
        .start_session(SessionSeed {
            session_id: "s".into(),
            runtime_profile_id: "local".into(),
            backend_id: None,
        })
        .unwrap();
    let file = store.list_sessions().unwrap().sessions[0].file_name.clone();
    fs::copy(
        store.root().join("sessions").join(&file),
        store.root().join("sessions/duplicate--s.md"),
    )
    .unwrap();
    let error =
        ProjectWorkspaceData::load(&store).err().expect("duplicate identity must not be summed");
    assert!(error.contains("重复"));
}
