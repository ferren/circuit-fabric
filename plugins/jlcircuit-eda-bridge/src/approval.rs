//! 审批数据模型：高风险写入的计划批准/拒绝记录，绑定 observed baseline
//! 快照哈希与计划哈希。
//!
//! 审批记录持久化到 `<项目根>/observations/approval-<planHash 去前缀>.json`；
//! 同一计划的后续 Bridge 决定覆盖旧记录。这是 Bridge 审批的查询副本；
//! 桌面端与 Bridge 共用的 ChangeSet 审计才是审批事实源。apply 必须通过
//! `review::authorize_apply` 校验最新决定、计划绑定与当前观测，不能仅凭本文件放行。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::observation::{observations_dir, write_atomic};

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("审批记录写入失败（{path}）：{source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("审批记录提交失败（{from} -> {to}）：{source}")]
    Commit { from: PathBuf, to: PathBuf, source: std::io::Error },
}

/// 审批决定。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

impl ApprovalDecision {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }
}

/// 一次计划审批决定（serde camelCase，直接持久化）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRecord {
    pub schema_version: u32,
    /// 恒为 `approval`。
    pub kind: String,
    /// 被审批的物化计划哈希。
    pub plan_hash: String,
    /// 审批时绑定的 observed baseline 快照哈希；执行前必须与当前最新观测一致。
    pub baseline_snapshot_hash: String,
    pub decision: ApprovalDecision,
    /// 决定时间（RFC 3339 毫秒）。
    pub decided_at: String,
    pub session_id: String,
    pub project_id: String,
}

impl ApprovalRecord {
    #[must_use]
    pub fn new(
        plan_hash: &str,
        baseline_snapshot_hash: &str,
        decision: ApprovalDecision,
        decided_at: String,
        session_id: &str,
        project_id: &str,
    ) -> Self {
        Self {
            schema_version: 1,
            kind: "approval".to_owned(),
            plan_hash: plan_hash.to_owned(),
            baseline_snapshot_hash: baseline_snapshot_hash.to_owned(),
            decision,
            decided_at,
            session_id: session_id.to_owned(),
            project_id: project_id.to_owned(),
        }
    }
}

/// 审批记录路径 `<项目根>/observations/approval-<planHash 去前缀>.json`。
#[must_use]
pub fn approval_path(project_root: &Path, plan_hash: &str) -> PathBuf {
    let file_name =
        format!("approval-{}.json", plan_hash.strip_prefix("sha256:").unwrap_or(plan_hash));
    observations_dir(project_root).join(file_name)
}

/// 原子写入审批记录（覆盖同计划旧决定），返回写入路径。
///
/// # Errors
///
/// 返回 [`ApprovalError`] 当目录创建、写入或提交失败。
pub fn persist_approval(
    project_root: &Path,
    record: &ApprovalRecord,
) -> Result<PathBuf, ApprovalError> {
    let path = approval_path(project_root, &record.plan_hash);
    let bytes = serde_json::to_vec_pretty(record).expect("审批记录序列化不应失败");
    write_atomic(&path, &bytes).map_err(|error| match error {
        crate::observation::ObservationError::CreateDirectory { path, source }
        | crate::observation::ObservationError::Write { path, source } => {
            ApprovalError::Write { path, source }
        }
        crate::observation::ObservationError::Commit { from, to, source } => {
            ApprovalError::Commit { from, to, source }
        }
        other => ApprovalError::Write {
            path: path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::Other, other.to_string()),
        },
    })?;
    Ok(path)
}

/// 查询计划的最新审批记录；无记录或文件损坏时返回 `None`。
#[must_use]
pub fn approval_for(project_root: &Path, plan_hash: &str) -> Option<ApprovalRecord> {
    let path = approval_path(project_root, plan_hash);
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_roundtrip_and_latest_wins() {
        let dir = std::env::temp_dir().join(format!(
            "circuitfabric-approval-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let root = dir.join("project");

        assert!(approval_for(&root, "sha256:p1").is_none());

        let approved = ApprovalRecord::new(
            "sha256:p1",
            "sha256:baseline",
            ApprovalDecision::Approved,
            "2026-10-22T08:00:00.000Z".to_owned(),
            "s1",
            "power-supply",
        );
        let path = persist_approval(&root, &approved).expect("落盘审批");
        assert!(path.is_file());
        let parsed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(parsed["kind"], "approval");
        assert_eq!(parsed["decision"], "approved");

        let loaded = approval_for(&root, "sha256:p1").expect("审批存在");
        assert_eq!(loaded.decision, ApprovalDecision::Approved);
        assert_eq!(loaded.baseline_snapshot_hash, "sha256:baseline");
        assert_eq!(loaded.plan_hash, "sha256:p1");
        assert_eq!(loaded.session_id, "s1");
        assert_eq!(loaded.project_id, "power-supply");

        // 同一计划后续拒绝覆盖旧批准：最新决定生效。
        let rejected = ApprovalRecord::new(
            "sha256:p1",
            "sha256:baseline",
            ApprovalDecision::Rejected,
            "2026-10-22T08:05:00.000Z".to_owned(),
            "s1",
            "power-supply",
        );
        persist_approval(&root, &rejected).expect("落盘审批");
        let loaded = approval_for(&root, "sha256:p1").expect("审批存在");
        assert_eq!(loaded.decision, ApprovalDecision::Rejected);

        std::fs::remove_dir_all(&dir).ok();
    }
}
