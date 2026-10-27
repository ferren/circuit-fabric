//! Persisted, auditable `ChangeSet` records used by materialization backends and the desktop UI.

use serde::{Deserialize, Serialize};

/// One separately reported materialization stage.  A stage is never inferred from the next one:
/// successful writing does not claim that readback or verification also succeeded.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetStageStatus {
    #[default]
    NotRun,
    Inconclusive,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetStage {
    pub status: ChangeSetStageStatus,
    #[serde(default)]
    pub detail: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetExecutionReport {
    #[serde(default)]
    pub write: ChangeSetStage,
    #[serde(default)]
    pub readback: ChangeSetStage,
    #[serde(default)]
    pub verification: ChangeSetStage,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetEvidenceRef {
    pub document_id: String,
    pub content_hash: String,
    pub locator: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetAuditEntry {
    pub timestamp_unix_seconds: u64,
    pub actor: String,
    pub decision: String,
    pub reason: String,
    pub observed_snapshot_hash: Option<String>,
    pub rollback_handle: Option<String>,
}

/// The on-disk representation is deliberately self-contained: it can be reviewed without
/// replaying a runtime session, and it retains the exact hashes that an approval referred to.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredChangeSet {
    pub schema_version: u32,
    pub id: String,
    pub base_snapshot_hash: String,
    pub target_snapshot_hash: String,
    pub plan_hash: String,
    #[serde(default)]
    pub ir_diff: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<ChangeSetEvidenceRef>,
    /// Preview validation is separate from post-write verification.
    #[serde(default)]
    pub preview_validation: ChangeSetStage,
    #[serde(default)]
    pub execution: ChangeSetExecutionReport,
    /// The hash returned by the last materializer readback, if there was one.
    pub observed_snapshot_hash: Option<String>,
    pub rollback_handle: Option<String>,
    #[serde(default)]
    pub audit: Vec<ChangeSetAuditEntry>,
}

impl StoredChangeSet {
    /// Approval requires a fresh observed state that matches the baseline and a completed,
    /// successful preview validation. Absence is unsafe; post-write verification cannot
    /// substitute for preview validation, and a started write cannot be reapproved.
    #[must_use]
    pub fn approval_allowed(&self, current_observation: Option<&str>) -> bool {
        current_observation == Some(self.base_snapshot_hash.as_str())
            && self.observed_snapshot_hash.as_deref() == current_observation
            && self.preview_validation.status == ChangeSetStageStatus::Passed
            && self.execution.write.status == ChangeSetStageStatus::NotRun
    }

    #[must_use]
    pub fn decision(&self) -> Option<&ChangeSetAuditEntry> {
        self.audit.last()
    }

    /// A write acknowledgement alone never establishes a successful change.
    #[must_use]
    pub fn verified(&self) -> bool {
        self.execution.write.status == ChangeSetStageStatus::Passed
            && self.execution.readback.status == ChangeSetStageStatus::Passed
            && self.execution.verification.status == ChangeSetStageStatus::Passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change_set() -> StoredChangeSet {
        StoredChangeSet {
            schema_version: 1,
            id: "cs-1".into(),
            base_snapshot_hash: "base".into(),
            target_snapshot_hash: "target".into(),
            plan_hash: "plan".into(),
            ir_diff: Vec::new(),
            evidence: Vec::new(),
            preview_validation: ChangeSetStage {
                status: ChangeSetStageStatus::Passed,
                detail: String::new(),
            },
            execution: ChangeSetExecutionReport {
                verification: ChangeSetStage {
                    status: ChangeSetStageStatus::Passed,
                    detail: String::new(),
                },
                ..ChangeSetExecutionReport::default()
            },
            observed_snapshot_hash: Some("base".into()),
            rollback_handle: None,
            audit: Vec::new(),
        }
    }

    #[test]
    fn approval_requires_current_matching_observation() {
        let mut record = change_set();
        assert!(record.approval_allowed(Some("base")));
        assert!(!record.approval_allowed(Some("other")));
        assert!(!record.approval_allowed(None));
        record.preview_validation.status = ChangeSetStageStatus::NotRun;
        assert!(
            !record.approval_allowed(Some("base")),
            "verification cannot impersonate preview validation"
        );
        record.preview_validation.status = ChangeSetStageStatus::Passed;
        record.execution.write.status = ChangeSetStageStatus::Passed;
        assert!(!record.approval_allowed(Some("base")), "a written plan cannot be reapproved");
    }
}
